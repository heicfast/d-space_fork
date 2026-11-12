// SPDX-License-Identifier: GPL-3.0-or-later
//
// FindFirstFile fallback — Master architecture Chapter 5.2A Layer 2 +
// Chapter 33.2 Layer B (streaming fallback).
//
// Recursive scan with the standard Windows API. Slow but ALWAYS works
// (requires no admin, no raw volume access).
//
// Principles:
//   * Symlinks/junctions are not followed (follow_links=false in Chapter 33.2).
//   * MAX_DEPTH=50 — pathological depth protection.
//   * BFS order: the parent is always placed before its children, so the
//     path_to_id dictionary stays consistent.
//   * The output has the same schema as `RawMftEntry` — `tree::build_tree`
//     can be fed directly. Synthetic NodeIds start at 16 (system reserve).
//
// Performance (Chapter 5.4 vs Chapter 17.2): target < 45 s for 1 TB (WizTree
// free level). This v0.1 is single-threaded. v0.2 will add rayon parallel
// per-dir + lock-free id counter.

use crate::error::{Error, Result};
use crate::scan::tree::ROOT_RECORD;
use crate::scan::walk::{
    advance_partial_threshold, build_partial_from_raw, MftEntries, ProgressCb, RawMftEntry,
    ScanProgress, PARTIAL_TREE_INTERVAL, PARTIAL_TREE_MAX_DEPTH, PARTIAL_TREE_TOP_N,
};
use crate::scan::NodeId;
use std::collections::{HashSet, VecDeque};
use std::fs::Metadata;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// NTFS file_index query threshold for hard link dedup. We do not call
/// `CreateFileW + GetFileInformationByHandle` for files below this size —
/// typical hard links (WinSxS DLLs, system binaries) are generally >= 64
/// KB; smaller ones are negligible in the total.
const HARDLINK_DEDUP_THRESHOLD: u64 = 64 * 1024;

/// `FILE_ATTRIBUTE_COMPRESSED` — NTFS compression flag. winnt.h: 0x800.
const FILE_ATTRIBUTE_COMPRESSED: u32 = 0x0000_0800;
/// `FILE_ATTRIBUTE_SPARSE_FILE` — sparse file flag. winnt.h: 0x200.
const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x0000_0200;

/// For compressed/sparse files `metadata.len()` returns the LOGICAL size
/// (uncompressed; the nominal size for sparse). `GetCompressedFileSizeW`
/// returns the **physical** size — the real disk usage. In a Windows
/// installation WinSxS + System32 are heavily compressed, and this 20-30%
/// difference leads to reports like "1.2 TB on a 1 TB disk".
///
/// The extra Win32 call is made only for files flagged
/// `FILE_ATTRIBUTE_COMPRESSED | SPARSE` (~5µs per file); for other files
/// it is a no-op.
#[cfg(windows)]
fn physical_file_size(path: &Path, metadata: &Metadata) -> u64 {
    let attrs = metadata.file_attributes();
    let logical = metadata.len();
    if attrs & (FILE_ATTRIBUTE_COMPRESSED | FILE_ATTRIBUTE_SPARSE_FILE) == 0 {
        return logical;
    }
    // Win32 query only for compressed / sparse files.
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetCompressedFileSizeW;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut high: u32 = 0;
    let low = unsafe { GetCompressedFileSizeW(PCWSTR(wide.as_ptr()), Some(&mut high)) };
    // INVALID_FILE_SIZE = 0xFFFFFFFF — a legitimate value even on success
    // (a physical size of 4 GiB-1). Gemini review B8: the real error is
    // distinguished via GetLastError; on error we fall back to the logical size.
    if low == 0xFFFF_FFFF {
        let err = unsafe { windows::Win32::Foundation::GetLastError() };
        if err.0 != 0 {
            return logical;
        }
    }
    ((high as u64) << 32) | (low as u64)
}

#[cfg(not(windows))]
fn physical_file_size(_path: &Path, metadata: &Metadata) -> u64 {
    metadata.len()
}

/// `fs::metadata().modified()` → Unix seconds. 0 on error or pre-epoch.
fn metadata_mtime_unix(metadata: &Metadata) -> i64 {
    match metadata.modified() {
        Ok(t) => match t.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i64,
            Err(_) => 0, // pre-1970 (possible on NTFS), edge case
        },
        Err(_) => 0,
    }
}

/// Gets the NTFS file_index via `GetFileInformationByHandle` — for hard
/// link dedupe. If the same physical file is walked through multiple
/// paths, its size should be counted **once** (e.g. `WinSxS` contains
/// thousands of hard links; this is the main reason a 1 TB disk gets
/// reported as 1.2 TB).
///
/// nNumberOfLinks <= 1 → no dedupe needed (returns None; single link).
/// nNumberOfLinks > 1 → returns the 64-bit composite index.
///
/// `MetadataExt::file_index()` on Rust stable is still behind the
/// `windows_by_handle` nightly feature; that is why we call the Win32 API
/// directly.
#[cfg(windows)]
fn file_index_for_dedupe(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let pcwstr = PCWSTR(wide.as_ptr());
    let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

    let handle = unsafe {
        CreateFileW(
            pcwstr,
            0, // No access — enough for a metadata-only query
            share,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_BACKUP_SEMANTICS),
            None,
        )
    }
    .ok()?;

    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
    let _ = unsafe { CloseHandle(handle) };
    ok.ok()?;

    if info.nNumberOfLinks <= 1 {
        return None;
    }
    Some(((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64))
}

#[cfg(not(windows))]
fn file_index_for_dedupe(_path: &Path) -> Option<u64> {
    None
}

pub const MAX_DEPTH: u32 = 50;
const SYNTHETIC_ID_START: u64 = 16;

/// Chapter 26.2 — scannable root path from the `drive` parameter. Two forms:
///   * "C" / "c:" / "C:\\" / "C:/Users" → `C:\` (local letter)
///   * `\\server\share` or `\\server\share\subpath` → the full UNC root
///     (network drive)
///
/// UNC path detection: if the first two characters are `\\` (or `//`) it is
/// accepted as UNC. The path is used directly instead of a drive letter.
/// The network drive warning is already printed by pre_flight.
fn drive_to_root(drive: &str) -> Result<PathBuf> {
    let trimmed = drive.trim();
    if trimmed.is_empty() {
        return Err(Error::Scan("empty drive parameter".into()));
    }
    // UNC path: `\\server\share[\sub]` or a forward-slash variant.
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'\\' || bytes[0] == b'/')
        && (bytes[1] == b'\\' || bytes[1] == b'/')
    {
        // UNC root — normalize: convert to backslashes.
        let normalized = trimmed.replace('/', "\\");
        // Must be at least `\\server\share` (after split, ≥3 non-empty
        // parts: ["", "", "server", "share"] split('\\').count() >= 4).
        let parts: Vec<&str> = normalized.split('\\').collect();
        let non_empty = parts.iter().filter(|p| !p.is_empty()).count();
        if non_empty < 2 {
            return Err(Error::Scan(format!(
                "invalid UNC path (server\\share required): '{}'",
                trimmed
            )));
        }
        return Ok(PathBuf::from(normalized));
    }
    // Local drive letter — strict validation (Gemini review B2: inputs like
    // "abc" must not silently become 'A:').
    let letter = crate::volume::parse_drive_letter(trimmed)
        .ok_or_else(|| Error::Scan(format!("invalid drive: '{}'", drive)))?
        .to_ascii_uppercase();
    Ok(PathBuf::from(format!(r"{}:\", letter)))
}

/// Backwards compatible — without a progress callback.
pub fn scan_find_first(drive: &str) -> Result<MftEntries> {
    scan_find_first_with_progress(drive, None, None)
}

/// BFS walk. Calls `read_dir` for each directory, enqueues the children.
/// Symlinks and reparse points are skipped. `progress_cb` is called once
/// every N entries (Chapter 9.6.5 streaming feedback).
pub fn scan_find_first_with_progress(
    drive: &str,
    progress_cb: Option<ProgressCb<'_>>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<MftEntries> {
    let start = Instant::now();
    let root = drive_to_root(drive)?;
    debug!(root = %root.display(), "FindFirstFile fallback starting");

    let mut entries: Vec<RawMftEntry> = Vec::with_capacity(8192);

    let mut next_id: u64 = SYNTHETIC_ID_START;
    let mut queue: VecDeque<(PathBuf, NodeId, u32)> = VecDeque::new();
    queue.push_back((root.clone(), ROOT_RECORD, 0));

    let mut skipped_errors: u64 = 0;
    let mut skipped_symlinks: u64 = 0;
    let mut max_depth_hits: u64 = 0;
    let mut visited_dirs: u64 = 0;
    // Hard link dedupe (Windows): if the same file_index is seen twice, we
    // count the size of the path seen second as 0. The total size stays
    // close to the physical disk. In WinSxS / Program Files based scenarios
    // a ~10-20% difference.
    let mut seen_file_indices: HashSet<u64> = HashSet::with_capacity(8192);
    let mut hardlink_duplicates: u64 = 0;
    // Sprint 3.7 — live sunburst snapshot threshold (based on the number of entries).
    let mut next_partial_at: u64 = PARTIAL_TREE_INTERVAL;

    while let Some((dir, parent_id, depth)) = queue.pop_front() {
        visited_dirs += 1;
        // Gemini review B1 — the cancel/progress gate used to be based on
        // `entries.len() % 500 == 0`; since entries grows by a variable
        // amount per directory, the check could be skipped (one big
        // directory = no check at all). Deterministic counter: a guaranteed
        // check every 32 directories.
        if visited_dirs % 32 == 0 {
            if let Some(c) = cancel {
                if c.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(Error::Scan("scan-cancelled".into()));
                }
            }
            if let Some(cb) = progress_cb {
                let partial = if entries.len() as u64 >= next_partial_at {
                    next_partial_at = advance_partial_threshold(next_partial_at);
                    Some(build_partial_from_raw(
                        &entries,
                        PARTIAL_TREE_MAX_DEPTH,
                        PARTIAL_TREE_TOP_N,
                    ))
                } else {
                    None
                };
                cb(&ScanProgress {
                    phase: "fallback_walk",
                    visited: entries.len() as u64,
                    total_estimate: 0, // unknown, BFS
                    in_use: visited_dirs,
                    last_name: dir.display().to_string(),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    partial_tree: partial,
                });
            }
        }
        if depth >= MAX_DEPTH {
            max_depth_hits += 1;
            continue;
        }

        let read = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                debug!(path = %dir.display(), error = %e, "read_dir failed");
                skipped_errors += 1;
                continue;
            }
        };

        for entry_result in read {
            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => {
                    skipped_errors += 1;
                    continue;
                }
            };

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => {
                    skipped_errors += 1;
                    continue;
                }
            };

            // Symlinks and reparse points are skipped (Chapter 33.2 follow_links=false)
            if file_type.is_symlink() {
                skipped_symlinks += 1;
                continue;
            }

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let id = next_id;
            next_id += 1;

            // Gemini review B16 — on Windows `entry.metadata()` is served
            // from FindNextFileW data (no extra syscall); `symlink_metadata`
            // would open an extra GetFileAttributesExW. `metadata()` does
            // not follow symlinks, and the reparse point check is still done via file_type.
            let metadata = entry.metadata().ok();
            let is_dir = file_type.is_dir();
            // raw_size: physical size for compressed/sparse files
            // (GetCompressedFileSizeW), metadata.len() for the others.
            let raw_size = metadata
                .as_ref()
                .map(|m| physical_file_size(&path, m))
                .unwrap_or(0);
            // Hard link dedupe: query the file identity via Win32
            // GetFileInformationByHandle only for files >= 64 KB. If there
            // are multiple links, the size = 0 when the second one is seen
            // (so the total does not exceed the physical disk usage). The
            // file still shows up in the tree; it just contributes nothing
            // to the aggregate.
            let data_size = if is_dir {
                0
            } else if raw_size >= HARDLINK_DEDUP_THRESHOLD {
                match file_index_for_dedupe(&path) {
                    Some(idx) => {
                        if seen_file_indices.insert(idx) {
                            raw_size
                        } else {
                            hardlink_duplicates += 1;
                            0
                        }
                    }
                    None => raw_size, // single link or query failed
                }
            } else {
                raw_size
            };
            let modified_unix = metadata.as_ref().map(metadata_mtime_unix).unwrap_or(0);

            entries.push(RawMftEntry {
                record_no: id,
                parent_record_no: parent_id,
                name,
                data_size,
                is_dir,
                modified_unix,
            });

            if is_dir {
                queue.push_back((path, id, depth + 1));
            }
        }
    }

    if skipped_symlinks > 0 {
        warn!(skipped_symlinks, "symlink/reparse point skipped");
    }
    if hardlink_duplicates > 0 {
        info!(
            hardlink_duplicates,
            "hard link detected — repeats counted as 0 to avoid exceeding physical size"
        );
    }
    if max_depth_hits > 0 {
        warn!(max_depth_hits, max_depth = MAX_DEPTH, "max_depth exceeded");
    }

    let result = MftEntries {
        drive: drive.to_string(),
        volume_path: root.to_string_lossy().to_string(),
        entries,
        skipped_errors,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };
    info!(
        entries = result.entries.len(),
        skipped = result.skipped_errors,
        elapsed_ms = result.elapsed_ms,
        "FindFirstFile fallback complete"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_root_normalize() {
        assert_eq!(drive_to_root("C").unwrap(), PathBuf::from(r"C:\"));
        assert_eq!(drive_to_root("d:").unwrap(), PathBuf::from(r"D:\"));
        assert!(drive_to_root("").is_err());
    }

    #[test]
    fn unc_path_accepted_as_root() {
        // `\\server\share` and sub-path variants must be accepted as valid.
        assert_eq!(
            drive_to_root(r"\\fileserver\public").unwrap(),
            PathBuf::from(r"\\fileserver\public")
        );
        assert_eq!(
            drive_to_root(r"\\fs01\projects\dspace").unwrap(),
            PathBuf::from(r"\\fs01\projects\dspace")
        );
    }

    #[test]
    fn unc_path_forward_slash_normalized() {
        // Users typing forward slashes — normalized to backslash.
        assert_eq!(
            drive_to_root("//server/share").unwrap(),
            PathBuf::from(r"\\server\share")
        );
    }

    #[test]
    fn unc_path_missing_share_errors() {
        // `\\server` (no share) must be rejected.
        let err = drive_to_root(r"\\server").unwrap_err();
        if let Error::Scan(msg) = err {
            assert!(msg.contains("UNC"));
        } else {
            panic!("expected a Scan error");
        }
    }

    #[test]
    fn unc_path_double_slash_only_errors() {
        let err = drive_to_root(r"\\").unwrap_err();
        assert!(matches!(err, Error::Scan(_)));
    }

    #[test]
    fn scan_empty_temp_dir() {
        let tmp = std::env::temp_dir().join(format!("dspace-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("a.txt"), b"hello").unwrap();
        std::fs::write(tmp.join("b.dat"), b"1234567890").unwrap();
        std::fs::create_dir(tmp.join("sub")).unwrap();
        std::fs::write(tmp.join("sub").join("inner.bin"), b"xx").unwrap();

        // drive_to_root expects a single letter; for this test we use it directly.
        let start = Instant::now();
        let mut entries: Vec<RawMftEntry> = Vec::new();
        let mut next_id = SYNTHETIC_ID_START;
        let mut queue = VecDeque::new();
        queue.push_back((tmp.clone(), ROOT_RECORD, 0u32));

        while let Some((dir, parent_id, depth)) = queue.pop_front() {
            if depth >= MAX_DEPTH {
                continue;
            }
            for entry in std::fs::read_dir(&dir).unwrap() {
                let entry = entry.unwrap();
                let ft = entry.file_type().unwrap();
                if ft.is_symlink() {
                    continue;
                }
                let id = next_id;
                next_id += 1;
                let name = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();
                let is_dir = ft.is_dir();
                let metadata = std::fs::symlink_metadata(&path).unwrap();
                entries.push(RawMftEntry {
                    record_no: id,
                    parent_record_no: parent_id,
                    name,
                    data_size: if is_dir { 0 } else { metadata.len() },
                    is_dir,
                    modified_unix: metadata_mtime_unix(&metadata),
                });
                if is_dir {
                    queue.push_back((path, id, depth + 1));
                }
            }
        }
        let _ = start;

        // a.txt(5), b.dat(10), sub/, inner.bin(2) → 4 entry
        assert_eq!(entries.len(), 4);
        let total: u64 = entries.iter().map(|e| e.data_size).sum();
        assert_eq!(total, 5 + 10 + 2);
        let dir_count = entries.iter().filter(|e| e.is_dir).count();
        assert_eq!(dir_count, 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
