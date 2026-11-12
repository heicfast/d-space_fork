// SPDX-License-Identifier: GPL-3.0-or-later
//
// MFT full walker — Chapter 5.1, Chapter 4.3 (Step 2: Rust reads the MFT sequentially).
//
// This sprint's goal: walk all MFT records in order, producing in-use /
// dir / file counts and sample file names. Hierarchy building and the
// `Arc<ScanTree>` build come in the next sprint (Chapter 4.3 Step 3 +
// rayon parallel aggregation).
//
// Performance note (Chapter 5.4): hitting the 1TB SSD < 5s target in real
// production will require bulk MFT reads (raw IOCTL) and rayon parallel
// parsing. This v0.1 walker is single-threaded and calls
// `ntfs::Ntfs::file()` per record — good enough, optimize next round.

use crate::error::{Error, Result};
use ntfs::structured_values::NtfsFileNamespace;
use ntfs::{KnownNtfsFileRecordNumber, Ntfs, NtfsFile, NtfsFileFlags};
use serde::Serialize;
use std::fs::File;
use std::time::Instant;
use tracing::{debug, info};

const SAMPLE_NAME_LIMIT: usize = 25;
const MAX_RECORD_HARD_LIMIT: u64 = 50_000_000; // safety ceiling

/// NTFS FILETIME (1601-01-01 epoch, 100-ns intervals) → Unix seconds.
/// The ntfs crate feeds the u64 value from `NtfsTime::nt_timestamp()`
/// into this function. A 0 input returns 0 (epoch placeholder).
pub(crate) fn nt_to_unix(nt_ts: u64) -> i64 {
    if nt_ts == 0 {
        return 0;
    }
    ((nt_ts / 10_000_000) as i64) - 11_644_473_600i64
}

/// Summary metadata for a single MFT record — sufficient for hierarchy +
/// Timeline (Chapter 9.1 module 4/4).
#[derive(Debug, Clone, Serialize)]
pub struct RawMftEntry {
    pub record_no: u64,
    pub parent_record_no: u64,
    pub name: String,
    pub data_size: u64,
    pub is_dir: bool,
    /// `$STANDARD_INFORMATION.modification_time` (updated on Windows writes).
    /// The `$FILE_NAME` fallback may lag — StandardInformation is canonical.
    pub modified_unix: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MftEntries {
    pub drive: String,
    pub volume_path: String,
    pub entries: Vec<RawMftEntry>,
    pub skipped_errors: u64,
    pub elapsed_ms: u64,
}

/// Chapter 9.6.5 — scan progress event. The backend emits it every N entries.
/// `partial_tree`: Sprint 3.7 — every 10k entries, a root + 2 levels +
/// top-200 node snapshot for the live sunburst. `None` on other events
/// (small payload).
#[derive(Debug, Clone, Serialize)]
pub struct ScanProgress {
    pub phase: &'static str,
    pub visited: u64,
    pub total_estimate: u64,
    pub in_use: u64,
    pub last_name: String,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_tree: Option<Vec<crate::scan::tree::PartialNode>>,
}

/// `Fn(&ScanProgress)` callback alias — to avoid heap-allocated closures.
pub type ProgressCb<'a> = &'a (dyn Fn(&ScanProgress) + Send + Sync);

const MFT_PROGRESS_INTERVAL: u64 = 5000;
/// Sprint 3.7 — initial interval for live sunburst snapshots. Gemini review
/// (partial-tree O(N²)): a fixed 10k interval with a full rebuild produced
/// quadratic cost on large volumes. Geometric growth (×4) is used now —
/// 10k, 40k, 160k, 640k, 2.5M, then every 2.5M. The total rebuild count
/// is ~O(log N), each O(N) → cost stays bounded at ~O(N log N).
pub(crate) const PARTIAL_TREE_INTERVAL: u64 = 10_000;
/// Geometric growth factor (interval × 4).
const PARTIAL_TREE_GROWTH: u64 = 4;
/// Cap on the geometric interval — a sane frequency even for huge scans.
const PARTIAL_TREE_MAX_INTERVAL: u64 = 2_500_000;
/// Sprint 3.7 — live sunburst max depth (root + 2 levels).
pub(crate) const PARTIAL_TREE_MAX_DEPTH: usize = 2;
/// Sprint 3.7 — live sunburst top-N nodes (root + 199).
pub(crate) const PARTIAL_TREE_TOP_N: usize = 200;

/// Advances the next partial-tree snapshot threshold geometrically.
#[inline]
pub(crate) fn advance_partial_threshold(current: u64) -> u64 {
    (current.saturating_mul(PARTIAL_TREE_GROWTH)).min(PARTIAL_TREE_MAX_INTERVAL)
}

#[derive(Debug, Clone, Serialize)]
pub struct MftWalkStats {
    pub drive: String,
    pub volume_path: String,
    pub total_records_estimate: u64,
    pub records_visited: u64,
    pub in_use_records: u64,
    pub directory_records: u64,
    pub file_records: u64,
    pub skipped_errors: u64,
    pub bytes_aggregate: u64,
    pub sample_names: Vec<String>,
    pub elapsed_ms: u64,
}

/// "C", "c:", "C:\Users" → r"\\.\C:" — strict validation (UNC is rejected).
pub(crate) fn normalize_volume_path(drive: &str) -> Result<String> {
    let letter = crate::volume::parse_drive_letter(drive)
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| {
            Error::Scan(format!(
                "Invalid drive parameter: '{}' (the MFT path does not accept UNC)",
                drive
            ))
        })?;
    Ok(format!(r"\\.\{}:", letter))
}

/// Estimates the total record count from the MFT's own `$DATA` size.
pub(crate) fn estimate_record_count<T: std::io::Read + std::io::Seek>(
    ntfs: &Ntfs,
    handle: &mut T,
) -> Result<u64> {
    let mft_file = ntfs
        .file(handle, KnownNtfsFileRecordNumber::MFT as u64)
        .map_err(|e| Error::Scan(format!("cannot read MFT record 0: {:?}", e)))?;

    let data_attr_item = mft_file
        .data(handle, "")
        .ok_or_else(|| Error::Scan("MFT $DATA not found".into()))?
        .map_err(|e| Error::Scan(format!("MFT $DATA error: {:?}", e)))?;

    let attr = data_attr_item
        .to_attribute()
        .map_err(|e| Error::Scan(format!("MFT $DATA attribute: {:?}", e)))?;

    let total_bytes = attr.value_length();
    let record_size = ntfs.file_record_size() as u64;
    if record_size == 0 {
        return Err(Error::Scan("file_record_size = 0".into()));
    }
    Ok(total_bytes / record_size)
}

/// Tries to extract a name for a single `NtfsFile`. The Win32 namespace is
/// preferred (to skip the secondary DOS 8.3 views).
fn extract_name<T: std::io::Read + std::io::Seek>(
    file: &NtfsFile,
    handle: &mut T,
) -> Option<String> {
    // Win32 namespace first
    if let Some(Ok(name)) = file.name(handle, Some(NtfsFileNamespace::Win32), None) {
        return Some(name.name().to_string_lossy());
    }
    // Fallback: Win32AndDos (3) — combined name
    if let Some(Ok(name)) = file.name(handle, None, None) {
        // Skip DOS
        if !matches!(name.namespace(), NtfsFileNamespace::Dos) {
            return Some(name.name().to_string_lossy());
        }
    }
    None
}

/// Extracts name + parent_ref + data_size + is_dir + mtime in one pass.
/// The Win32 namespace is preferred. For mtime, `$STANDARD_INFORMATION`
/// (updated on Windows writes — canonical), falling back to `$FILE_NAME`.
pub(crate) fn extract_full<T: std::io::Read + std::io::Seek>(
    file: &NtfsFile,
    handle: &mut T,
) -> Option<(String, u64, u64, bool, i64)> {
    let si_mtime = file
        .info()
        .ok()
        .map(|si| nt_to_unix(si.modification_time().nt_timestamp()));

    if let Some(Ok(fname)) = file.name(handle, Some(NtfsFileNamespace::Win32), None) {
        let mtime =
            si_mtime.unwrap_or_else(|| nt_to_unix(fname.modification_time().nt_timestamp()));
        return Some((
            fname.name().to_string_lossy(),
            fname.parent_directory_reference().file_record_number(),
            fname.data_size(),
            fname.is_directory(),
            mtime,
        ));
    }
    if let Some(Ok(fname)) = file.name(handle, None, None) {
        if !matches!(fname.namespace(), NtfsFileNamespace::Dos) {
            let mtime =
                si_mtime.unwrap_or_else(|| nt_to_unix(fname.modification_time().nt_timestamp()));
            return Some((
                fname.name().to_string_lossy(),
                fname.parent_directory_reference().file_record_number(),
                fname.data_size(),
                fname.is_directory(),
                mtime,
            ));
        }
    }
    None
}

/// Backwards compatible — without a progress callback or cancel flag.
pub fn collect_mft_entries(drive: &str) -> Result<MftEntries> {
    collect_mft_entries_with_progress(drive, None, None)
}

/// Sprint 3.7 — builds a lightweight partial view from the raw entries for
/// the live sunburst. Gemini review (partial-tree perf): previously
/// `entries.to_vec()` + a full `build_tree` (rule matching included) cost
/// an extra O(N) copy + 63 rules × N nodes per snapshot. Now:
///   * NO clones — `build_tree_light` works on borrows,
///   * NO rule scoring — the partial view carries only size/name.
///
/// Together with the geometric interval (above), total cost is ~O(N log N).
pub(crate) fn build_partial_from_raw(
    entries: &[RawMftEntry],
    max_depth: usize,
    top_n: usize,
) -> Vec<crate::scan::tree::PartialNode> {
    let tree = crate::scan::tree::build_tree_light(entries);
    crate::scan::tree::build_partial_view(&tree, max_depth, top_n)
}

/// Full MFT entry collection — the feed source for `build_tree`.
/// System records (0-15) are skipped. The volume root is record 5 in NTFS.
/// `progress_cb` is called once every `MFT_PROGRESS_INTERVAL` entries (Chapter 9.6.5).
pub fn collect_mft_entries_with_progress(
    drive: &str,
    progress_cb: Option<ProgressCb<'_>>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<MftEntries> {
    let start = Instant::now();
    let volume_path = normalize_volume_path(drive)?;

    debug!(volume = %volume_path, "MFT entry collection starting");
    let mut handle = File::open(&volume_path).map_err(|e| {
        Error::Scan(format!(
            "cannot open volume '{}': {} (administrator privileges may be required)",
            volume_path, e
        ))
    })?;

    let ntfs =
        Ntfs::new(&mut handle).map_err(|e| Error::Scan(format!("NTFS parse error: {:?}", e)))?;

    let estimated = estimate_record_count(&ntfs, &mut handle)?;
    let cap = estimated.min(MAX_RECORD_HARD_LIMIT);
    if estimated > MAX_RECORD_HARD_LIMIT {
        tracing::warn!(
            estimated,
            hard_limit = MAX_RECORD_HARD_LIMIT,
            "MFT record estimate exceeded the hard limit — scan truncated at 50M records"
        );
    }
    info!(estimated, cap, "MFT entry koleksiyonu kapasitesi");

    // Gemini review B12 — a sane initial capacity instead of a huge upfront
    // reservation (~GBs); the Vec already grows as needed.
    let mut entries: Vec<RawMftEntry> =
        Vec::with_capacity(((cap as usize) / 2).clamp(1024, 1_000_000));
    let mut skipped = 0u64;
    let mut last_name = String::new();
    // Sprint 3.7 — produce a partial tree snapshot every
    // PARTIAL_TREE_INTERVAL entries. The count is over the absolute size
    // of entries — not the record count. Fires once per threshold crossed.
    let mut next_partial_at: u64 = PARTIAL_TREE_INTERVAL;

    // The root directory (record 5) is not added as a synthetic node —
    // during collection the user entries stay clean; build_tree produces
    // a synthetic root node if needed.
    for record_no in 16..cap {
        // Cancellation check — once every MFT_PROGRESS_INTERVAL is enough.
        if (record_no - 16) % MFT_PROGRESS_INTERVAL == 0 {
            if let Some(c) = cancel {
                if c.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(Error::Scan("scan-cancelled".into()));
                }
            }
        }
        // Progress event
        if let Some(cb) = progress_cb {
            if (record_no - 16) % MFT_PROGRESS_INTERVAL == 0 {
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
                    phase: "mft_walk",
                    visited: record_no - 16,
                    total_estimate: cap - 16,
                    in_use: entries.len() as u64,
                    last_name: last_name.clone(),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    partial_tree: partial,
                });
            }
        }

        let file = match ntfs.file(&mut handle, record_no) {
            Ok(f) => f,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if !file.flags().contains(NtfsFileFlags::IN_USE) {
            continue;
        }
        if let Some((name, parent, size, is_dir, mtime)) = extract_full(&file, &mut handle) {
            last_name = name.clone();
            entries.push(RawMftEntry {
                record_no,
                parent_record_no: parent,
                name,
                data_size: if is_dir { 0 } else { size },
                is_dir,
                modified_unix: mtime,
            });
        }
    }

    Ok(MftEntries {
        drive: drive.to_string(),
        volume_path,
        entries,
        skipped_errors: skipped,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

/// Walks all MFT records in order. The first 16 system files (records 0-15)
/// are excluded — those are `$MFT`, `$LogFile`, `$Volume`, `$AttrDef`, `$.`,
/// `$Bitmap`, `$Boot`, `$BadClus`, `$Secure`, `$UpCase`, `$Extend`, etc.
pub fn walk_mft(drive: &str) -> Result<MftWalkStats> {
    let start = Instant::now();
    let volume_path = normalize_volume_path(drive)?;

    debug!(volume = %volume_path, "MFT full walk starting");
    let mut handle = File::open(&volume_path).map_err(|e| {
        Error::Scan(format!(
            "cannot open volume '{}': {} (administrator privileges may be required)",
            volume_path, e
        ))
    })?;

    let ntfs =
        Ntfs::new(&mut handle).map_err(|e| Error::Scan(format!("NTFS parse error: {:?}", e)))?;

    let estimated = estimate_record_count(&ntfs, &mut handle)?;
    let cap = estimated.min(MAX_RECORD_HARD_LIMIT);
    info!(estimated, cap, "MFT record count estimate");

    let mut visited = 0u64;
    let mut in_use = 0u64;
    let mut dirs = 0u64;
    let mut files = 0u64;
    let mut skipped = 0u64;
    let mut total_bytes = 0u64;
    let mut samples: Vec<String> = Vec::with_capacity(SAMPLE_NAME_LIMIT);

    // System records (0-15) are skipped — real user content starts at 16.
    for record_no in 16..cap {
        visited += 1;

        let file = match ntfs.file(&mut handle, record_no) {
            Ok(f) => f,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let flags = file.flags();
        if !flags.contains(NtfsFileFlags::IN_USE) {
            continue;
        }
        in_use += 1;

        let is_dir = file.is_directory();
        if is_dir {
            dirs += 1;
        } else {
            files += 1;
            // Resident or non-resident $DATA size (Chapter 7 compliant size).
            // `data_size()` is u32 — for large files NtfsFileName.data_size()
            // gives 64 bits; that one is used in per-file metadata later.
            total_bytes = total_bytes.saturating_add(file.data_size() as u64);
        }

        if samples.len() < SAMPLE_NAME_LIMIT {
            if let Some(name) = extract_name(&file, &mut handle) {
                samples.push(name);
            }
        }
    }

    let stats = MftWalkStats {
        drive: drive.to_string(),
        volume_path,
        total_records_estimate: estimated,
        records_visited: visited,
        in_use_records: in_use,
        directory_records: dirs,
        file_records: files,
        skipped_errors: skipped,
        bytes_aggregate: total_bytes,
        sample_names: samples,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    info!(
        visited = stats.records_visited,
        in_use = stats.in_use_records,
        dirs = stats.directory_records,
        files = stats.file_records,
        gb = stats.bytes_aggregate / 1_073_741_824,
        skipped = stats.skipped_errors,
        elapsed_ms = stats.elapsed_ms,
        "MFT walk complete"
    );

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_drive_paths() {
        assert_eq!(normalize_volume_path("E").unwrap(), r"\\.\E:");
        assert_eq!(normalize_volume_path("d:").unwrap(), r"\\.\D:");
        assert!(normalize_volume_path("").is_err());
    }
}
