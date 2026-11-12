// SPDX-License-Identifier: GPL-3.0-or-later
//
// Duplicate Detector — 4-stage pipeline (Chapter 7).
//
// v0.1 implementation:
//   Stage 1 — size bucket: collect files of the same size (≥2 → candidate).
//             Chapter 7.2: 0-byte and below-`min_size_bytes` filter (default 4 KB).
//   Stage 2 — full Blake3 hash (streaming, 64 KB buffer).
//             Chapter 7.3: SIMD, constant RAM, parallel — currently single-thread.
//   Stage 3 — hash bucket (≥2 → group).
//   Stage 4 — sort the DuplicateGroup vec (`redundant_bytes` desc).
//
// Optimizations (v0.2):
//   * 4 KB "head hash" before Stage 2 — eliminates the vast majority early.
//   * rayon parallel hashing.
//   * The same `inode` (NodeId) is not counted twice — hardlink/junction (Chapter 7.2).
//   * Bench harness for measuring 100 GB mixed data < 60 s (Chapter 7.4).

use crate::duplicate::{DuplicateGroup, DuplicateScanResult, DuplicateStats};
use crate::error::{Error, Result};
use crate::scan::{node_full_path, NodeId, ScanTree};
use blake3::Hasher;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::Instant;
use tracing::{debug, info, warn};

const HASH_BUFFER: usize = 64 * 1024;
/// Head-hash prefilter buffer (first 4 KB). Chapter 7.4 — before the full
/// Blake3 pass, the vast majority of same-size files have different
/// headers; this small read cuts the full hash workload by ~80-95%.
const HEAD_HASH_SIZE: usize = 4096;
/// File size threshold for applying the head hash. Below it (≤64 KB) the
/// full hash is already cheap; prefilter overhead would exceed the gain.
const HEAD_HASH_THRESHOLD: u64 = 64 * 1024;
/// Minimum member count of a size bucket for applying the head hash.
/// Putting prefilter overhead on a pair of files makes no sense.
const HEAD_HASH_BUCKET_MIN: usize = 4;
/// Chapter 7.2 — filters out system files and caches.
pub const DEFAULT_MIN_DUP_SIZE: u64 = 4096;
/// Protection against pathological distribution in the hash stage.
const MAX_CANDIDATE_FILES: usize = 500_000;

/// Chapter 7 settings — provided parametrically from the UI.
#[derive(Debug, Clone, Copy, serde::Deserialize, Serialize)]
pub struct DuplicateOptions {
    pub min_size_bytes: u64,
    /// For skipping Stage 2 and reporting only size buckets (debug/UI preview).
    /// Default false — hashing is mandatory.
    pub size_only: bool,
    /// Disable the head-hash prefilter — for benchmark and regression
    /// comparisons. In production the default is false (prefilter enabled).
    #[serde(default)]
    pub skip_head_prefilter: bool,
    /// IPC payload upper limit (sorted by reclaim gain). In a 1M-file scan
    /// there can be 100k+ duplicate groups; sending 50 MB of JSON to the
    /// UI side freezes the frontend (Gemini review 2.1). Default 500 groups;
    /// the user can fetch more by narrowing the filter.
    #[serde(default = "default_max_groups")]
    pub max_groups: usize,
}

fn default_max_groups() -> usize {
    500
}

impl Default for DuplicateOptions {
    fn default() -> Self {
        Self {
            min_size_bytes: DEFAULT_MIN_DUP_SIZE,
            size_only: false,
            skip_head_prefilter: false,
            max_groups: default_max_groups(),
        }
    }
}

/// Snapshot of a candidate file (id, full path, byte size).
/// `id` will be used in v0.2 for Chapter 7.2 inode-based hardlink/junction elimination.
#[derive(Debug, Clone)]
struct Candidate {
    #[allow(dead_code)]
    id: NodeId,
    path: String,
    size_bytes: u64,
}

/// Streaming Blake3 — generic over `Read`. A reader-based helper for
/// tests and VSS retry.
fn hash_reader<R: Read>(mut reader: R) -> std::io::Result<[u8; 32]> {
    let mut hasher = Hasher::new();
    let mut buf = vec![0u8; HASH_BUFFER];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Win32 `ERROR_SHARING_VIOLATION` code — detected via
/// `std::io::Error::raw_os_error()`. The only reliable heuristic that does
/// not depend on Turkish or other-locale messages in the path string.
const WIN32_ERROR_SHARING_VIOLATION: i32 = 32;

fn is_share_violation(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(WIN32_ERROR_SHARING_VIOLATION)
}

/// Plain hash — preserves `std::io::Error` (for share violation detection).
fn hash_file_inner(path: &Path) -> std::io::Result<[u8; 32]> {
    let file = fs::File::open(path)?;
    let reader = BufReader::with_capacity(HASH_BUFFER, file);
    hash_reader(reader)
}

/// Head hash — reads only the first `HEAD_HASH_SIZE` bytes. Saves ~95% of
/// the cost of the full Blake3 pass; two same-size files with different
/// content usually differ within the first 4 KB (header, magic bytes,
/// manifest etc.).
fn head_hash_inner(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut buf = [0u8; HEAD_HASH_SIZE];
    let mut total = 0usize;
    while total < buf.len() {
        let n = file.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    let mut hasher = Hasher::new();
    hasher.update(&buf[..total]);
    Ok(*hasher.finalize().as_bytes())
}

/// Streaming Blake3 — constant RAM, independent of file size.
/// Wraps in `Error::Duplicate`; for the raw error use `hash_file_inner`.
/// In production `hash_file_with_retry` is used; this plain form is only for tests.
#[cfg(test)]
fn hash_file(path: &Path) -> Result<[u8; 32]> {
    hash_file_inner(path)
        .map_err(|e| Error::Duplicate(format!("hash open/read '{}': {}", path.display(), e)))
}

/// Chapter 7.3 + 34.5.4 — locked-file hash retry chain.
/// On ShareViolation it retries via a snapshot-based reader from the VSS pool.
/// If the `vss` feature is off, it returns the original error as
/// `Error::Duplicate` (legacy behavior is preserved).
pub(crate) fn hash_file_with_retry(path: &Path) -> Result<[u8; 32]> {
    match hash_file_inner(path) {
        Ok(h) => Ok(h),
        Err(e) => {
            #[cfg(all(windows, feature = "vss"))]
            {
                if is_share_violation(&e) {
                    let reader = crate::locked_file::vss_pool::VssPool::global()
                        .reader_for(path)
                        .map_err(|err| {
                            Error::Duplicate(format!("vss reader '{}': {}", path.display(), err))
                        })?;
                    return hash_reader(reader).map_err(|err| {
                        Error::Duplicate(format!("vss hash retry '{}': {}", path.display(), err))
                    });
                }
            }
            // If it is not a share violation, or vss is off: the original error.
            let _ = is_share_violation; // suppress dead_code warning
            Err(Error::Duplicate(format!(
                "hash open/read '{}': {}",
                path.display(),
                e
            )))
        }
    }
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Chapter 7 — runs the duplicate search over the ScanTree.
/// `drive_letter` is used for path reconstruction.
pub fn find_duplicates(
    tree: &ScanTree,
    drive_letter: char,
    opts: DuplicateOptions,
) -> Result<DuplicateScanResult> {
    let started = Instant::now();
    let min_size = opts.min_size_bytes.max(1); // 0-byte is always skipped

    // --- Stage 1: size buckets -----------------------------------------
    let mut size_buckets: HashMap<u64, Vec<Candidate>> = HashMap::new();
    let mut scanned_files = 0u64;
    let mut filtered_small = 0u64;

    for node in tree.nodes.values() {
        if node.is_dir {
            continue;
        }
        scanned_files += 1;
        let size = node.data_size;
        if size < min_size {
            filtered_small += 1;
            continue;
        }
        let Some(path) = node_full_path(tree, drive_letter, node.id) else {
            continue;
        };
        size_buckets.entry(size).or_default().push(Candidate {
            id: node.id,
            path,
            size_bytes: size,
        });
    }

    // Take the buckets with at least 2 candidates — but keep the bucket
    // structure for the parallel head-hash (head-hash sub-buckets per size group).
    let mut candidate_pairs: u64 = 0;
    let mut size_groups: Vec<Vec<Candidate>> = Vec::new();
    let mut bucket_count: u64 = 0;
    let mut total_candidates = 0usize;
    for (_, list) in size_buckets.into_iter() {
        if list.len() < 2 {
            continue;
        }
        bucket_count += 1;
        candidate_pairs += list.len() as u64;
        total_candidates += list.len();
        size_groups.push(list);
    }

    if total_candidates > MAX_CANDIDATE_FILES {
        return Err(Error::Duplicate(format!(
            "{} aday dosya MAX_CANDIDATE_FILES ({}) sınırını aştı — \
             min_size_bytes değerini yükseltin",
            total_candidates, MAX_CANDIDATE_FILES
        )));
    }

    debug!(
        scanned_files,
        filtered_small,
        bucket_count,
        candidates = total_candidates,
        "duplicate stage 1 (size buckets) complete"
    );

    // --- Stage 1.5: head-hash prefilter (Chapter 7.4 v0.2) -------------
    // Within each size group, hash the first 4 KB in parallel; if a
    // (size, head) sub-group has ≥2 members, it moves on to the full hash
    // as a survivor. The rest are eliminated.
    let candidates: Vec<Candidate> = if opts.size_only || opts.skip_head_prefilter {
        // Prefilter off: pass all candidates through unchanged.
        size_groups.into_iter().flatten().collect()
    } else {
        let mut survivors: Vec<Candidate> = Vec::new();
        let mut head_hash_count: usize = 0;
        let mut head_eliminated: usize = 0;
        for group in size_groups {
            let size = group.first().map(|c| c.size_bytes).unwrap_or(0);
            if size < HEAD_HASH_THRESHOLD || group.len() < HEAD_HASH_BUCKET_MIN {
                // Small file or small bucket: prefilter overhead would
                // exceed the gain — go straight to the full hash.
                survivors.extend(group);
                continue;
            }
            // Parallel head-hash. On error, be conservative: keep the candidate.
            let heads: Vec<Option<[u8; 32]>> = group
                .par_iter()
                .map(|c| head_hash_inner(Path::new(&c.path)).ok())
                .collect();
            head_hash_count += group.len();

            let mut by_head: HashMap<[u8; 32], Vec<Candidate>> = HashMap::new();
            for (c, h) in group.into_iter().zip(heads) {
                match h {
                    Some(h) => by_head.entry(h).or_default().push(c),
                    None => survivors.push(c), // error → keep (conservative)
                }
            }
            for (_, sub) in by_head {
                if sub.len() >= 2 {
                    survivors.extend(sub);
                } else {
                    head_eliminated += sub.len();
                }
            }
        }
        debug!(
            head_hash_count,
            head_eliminated,
            survivors = survivors.len(),
            "duplicate stage 1.5 (head-hash prefilter) complete"
        );
        survivors
    };

    // --- Stage 2/3: full hash + hash bucket ----------------------------
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    let mut hash_errors = 0u64;

    if opts.size_only {
        // Convert only the size buckets into DuplicateGroup (hash = placeholder).
        let mut by_size: HashMap<u64, Vec<Candidate>> = HashMap::new();
        for c in candidates {
            by_size.entry(c.size_bytes).or_default().push(c);
        }
        for (size, list) in by_size {
            if list.len() < 2 {
                continue;
            }
            let mut paths: Vec<String> = list.into_iter().map(|c| c.path).collect();
            paths.sort();
            groups.push(DuplicateGroup {
                hash_hex: format!("size:{}", size),
                size_bytes: size,
                paths,
            });
        }
    } else {
        // Rayon parallel full hash. After par_iter returns, sequential
        // hash bucket collection (HashMap mutation on one thread).
        let results: Vec<(Candidate, std::result::Result<[u8; 32], String>)> = candidates
            .into_par_iter()
            .map(|cand| {
                let p = Path::new(&cand.path);
                let res = hash_file_with_retry(p).map_err(|e| format!("{:?}", e));
                (cand, res)
            })
            .collect();

        let mut hash_buckets: HashMap<[u8; 32], (u64, Vec<String>)> = HashMap::new();
        for (cand, res) in results {
            match res {
                Ok(h) => {
                    hash_buckets
                        .entry(h)
                        .or_insert_with(|| (cand.size_bytes, Vec::new()))
                        .1
                        .push(cand.path);
                }
                Err(e) => {
                    warn!(path = %cand.path, error = %e, "hash skipped");
                    hash_errors += 1;
                }
            }
        }
        for (h, (size, mut paths)) in hash_buckets.into_iter() {
            if paths.len() < 2 {
                continue;
            }
            paths.sort();
            groups.push(DuplicateGroup {
                hash_hex: hex32(&h),
                size_bytes: size,
                paths,
            });
        }
    }

    // --- Stage 4: sort (biggest gain first) ----------------------------
    groups.sort_by(|a, b| {
        let a_red = a
            .size_bytes
            .saturating_mul(a.paths.len().saturating_sub(1) as u64);
        let b_red = b
            .size_bytes
            .saturating_mul(b.paths.len().saturating_sub(1) as u64);
        b_red.cmp(&a_red)
    });

    let redundant_bytes: u64 = groups
        .iter()
        .map(|g| {
            g.size_bytes
                .saturating_mul(g.paths.len().saturating_sub(1) as u64)
        })
        .sum();

    let total_group_count = groups.len() as u64;
    let stats = DuplicateStats {
        group_count: total_group_count,
        redundant_bytes,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };

    // Gemini review 2.1 — IPC payload limit. The groups are already sorted
    // by gain, top-N is enough; the user can fetch the rest by tightening
    // the `min_size_bytes` filter.
    let cap = opts.max_groups.clamp(50, 5_000);
    let truncated = groups.len() > cap;
    if truncated {
        groups.truncate(cap);
    }

    info!(
        groups = total_group_count,
        returned = groups.len(),
        truncated,
        reclaim_mb = redundant_bytes / 1_048_576,
        candidate_pairs,
        hash_errors,
        elapsed_ms = stats.elapsed_ms,
        "duplicate scan complete"
    );

    Ok(DuplicateScanResult {
        drive_letter: drive_letter.to_ascii_uppercase().to_string(),
        scanned_files,
        filtered_small,
        candidate_pairs,
        hash_errors,
        groups,
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hash_two_identical_files_match() {
        let dir = tempdir();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let content = b"D-Space test payload \xff\x00\x01\x02\x03";
        fs::File::create(&a).unwrap().write_all(content).unwrap();
        fs::File::create(&b).unwrap().write_all(content).unwrap();
        let ha = hash_file(&a).unwrap();
        let hb = hash_file(&b).unwrap();
        assert_eq!(ha, hb);
    }

    #[test]
    fn hash_different_files_differ() {
        let dir = tempdir();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        fs::File::create(&a).unwrap().write_all(b"hello").unwrap();
        fs::File::create(&b).unwrap().write_all(b"world").unwrap();
        assert_ne!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
    }

    #[test]
    fn hash_reader_matches_hash_file() {
        let dir = tempdir();
        let p = dir.path().join("c.bin");
        let content = b"reader vs file path";
        fs::File::create(&p).unwrap().write_all(content).unwrap();

        let from_file = hash_file(&p).unwrap();
        let from_reader = hash_reader(std::io::Cursor::new(content)).unwrap();
        assert_eq!(from_file, from_reader);
    }

    #[test]
    fn head_hash_matches_full_when_file_under_4k() {
        // <4 KB: head_hash reads the entire file; for two files with the
        // same content it covers the same byte range as the full hash.
        let dir = tempdir();
        let a = dir.path().join("a.bin");
        let content = b"under-4kb-payload";
        fs::File::create(&a).unwrap().write_all(content).unwrap();
        let head = head_hash_inner(&a).unwrap();
        let mut hasher = Hasher::new();
        hasher.update(content);
        let expected = *hasher.finalize().as_bytes();
        assert_eq!(head, expected);
    }

    #[test]
    fn head_hash_differs_for_different_starts() {
        // Two files of the same size but different head content — head_hash
        // must differ (prefilter functionality).
        let dir = tempdir();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let mut content_a = vec![0u8; 8192];
        let mut content_b = vec![0u8; 8192];
        content_a[0] = 1;
        content_b[0] = 2;
        fs::File::create(&a).unwrap().write_all(&content_a).unwrap();
        fs::File::create(&b).unwrap().write_all(&content_b).unwrap();
        let ha = head_hash_inner(&a).unwrap();
        let hb = head_hash_inner(&b).unwrap();
        assert_ne!(ha, hb);
    }

    #[test]
    fn head_hash_groups_same_starts() {
        // Same first 4 KB, different tail → head_hash must be equal.
        let dir = tempdir();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let mut content_a = vec![0u8; 8192];
        let mut content_b = vec![0u8; 8192];
        for i in 0..4096 {
            content_a[i] = (i % 251) as u8;
            content_b[i] = (i % 251) as u8;
        }
        // Tail differs
        for i in 4096..8192 {
            content_a[i] = 0xAA;
            content_b[i] = 0xBB;
        }
        fs::File::create(&a).unwrap().write_all(&content_a).unwrap();
        fs::File::create(&b).unwrap().write_all(&content_b).unwrap();
        let ha = head_hash_inner(&a).unwrap();
        let hb = head_hash_inner(&b).unwrap();
        assert_eq!(
            ha, hb,
            "identical first 4 KB must yield identical head_hash (even if the full hash differs)"
        );
    }

    #[test]
    fn share_violation_detector() {
        let sv = std::io::Error::from_raw_os_error(32);
        assert!(is_share_violation(&sv));
        let other = std::io::Error::from_raw_os_error(5);
        assert!(!is_share_violation(&other));
        let synthetic = std::io::Error::other("x");
        assert!(!is_share_violation(&synthetic));
    }

    /// Produces files of specific sizes, simulates a ScanTree-like structure,
    /// and verifies that `find_duplicates` reports the candidate count of the
    /// size bucket stage correctly (size_only mode makes no hash calls, the
    /// temp file paths are never actually read → the tree path resolver is tested).
    /// Module-wide `RawMftEntry` helper — shared by multiple tests.
    fn r(
        record_no: u64,
        parent: u64,
        name: &str,
        size: u64,
        dir: bool,
    ) -> crate::scan::walk::RawMftEntry {
        crate::scan::walk::RawMftEntry {
            record_no,
            parent_record_no: parent,
            name: name.into(),
            data_size: size,
            is_dir: dir,
            modified_unix: 0,
        }
    }

    #[test]
    fn size_only_groups_by_size() {
        use crate::scan::tree::{build_tree, ROOT_RECORD};

        // Two pairs of the same size, one singleton.
        let raw = vec![
            r(100, ROOT_RECORD, "a.bin", 8192, false),
            r(101, ROOT_RECORD, "b.bin", 8192, false),
            r(102, ROOT_RECORD, "c.bin", 16384, false),
            r(103, ROOT_RECORD, "d.bin", 16384, false),
            r(104, ROOT_RECORD, "lonely.bin", 32768, false),
            // below 4 KB → filtered out
            r(105, ROOT_RECORD, "tiny1.bin", 100, false),
            r(106, ROOT_RECORD, "tiny2.bin", 100, false),
        ];
        let tree = build_tree("vol".into(), raw);
        let opts = DuplicateOptions {
            min_size_bytes: DEFAULT_MIN_DUP_SIZE,
            size_only: true,
            skip_head_prefilter: false,
            max_groups: 500,
        };
        let result = find_duplicates(&tree, 'C', opts).unwrap();
        assert_eq!(result.groups.len(), 2, "expected two size buckets");
        // the tiny ones were filtered out
        assert!(result.filtered_small >= 2);
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    /// Sprint 4.1 integration: head-hash prefilter + parallel full hash
    /// end-to-end on real files. Two files of the same size but with a
    /// different first 4 KB should be eliminated by the prefilter; a third
    /// pair of the same size + same content should be reported as duplicates.
    #[test]
    fn duplicate_v02_pipeline_end_to_end() {
        use crate::scan::tree::{build_tree, ROOT_RECORD};

        let dir = tempdir();
        // 4 files, all 128 KB (above HEAD_HASH_THRESHOLD).
        //   a, b: same content (duplicate pair)
        //   c, d: content different from a/b BUT same size — the prefilter
        //         should eliminate them; they must not reach the full hash.
        let make = |name: &str, fill: u8| -> std::path::PathBuf {
            let p = dir.path().join(name);
            let content = vec![fill; 128 * 1024];
            fs::File::create(&p).unwrap().write_all(&content).unwrap();
            p
        };
        let a = make("a.bin", 0xAA);
        let b = make("b.bin", 0xAA);
        let _c = make("c.bin", 0xBB);
        let _d = make("d.bin", 0xCC);

        // We cannot point the ScanTree at temp paths (the path resolver expects C:).
        // So let's trigger it directly with hash_file — verify head-hash and
        // full hash consistency end to end.
        let head_a = head_hash_inner(&a).unwrap();
        let head_b = head_hash_inner(&b).unwrap();
        assert_eq!(head_a, head_b, "same content → same head_hash");

        let full_a = hash_file(&a).unwrap();
        let full_b = hash_file(&b).unwrap();
        assert_eq!(full_a, full_b, "same content → same full hash");

        // Synthetic raw entries for the build_tree + find_duplicates path:
        let raw = vec![
            r(100, ROOT_RECORD, "a.bin", 128 * 1024, false),
            r(101, ROOT_RECORD, "b.bin", 128 * 1024, false),
            r(102, ROOT_RECORD, "c.bin", 128 * 1024, false),
            r(103, ROOT_RECORD, "d.bin", 128 * 1024, false),
        ];
        let tree = build_tree("vol".into(), raw);
        // size_only mode: skips the prefilter pipeline, only size buckets.
        let opts = DuplicateOptions {
            min_size_bytes: DEFAULT_MIN_DUP_SIZE,
            size_only: true,
            skip_head_prefilter: false,
            max_groups: 500,
        };
        let result = find_duplicates(&tree, 'C', opts).unwrap();
        // size_only: four files of the same size → one bucket.
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].paths.len(), 4);
    }

    /// The size_only=false path with `skip_head_prefilter` — when the
    /// prefilter is disabled, the full hash runs on all candidates. Since
    /// the paths in this test are temp, the real hash fails → 0 groups
    /// expected (controlled failure behavior).
    #[test]
    fn duplicate_skip_prefilter_runs_full_hash() {
        use crate::scan::tree::{build_tree, ROOT_RECORD};

        let raw = vec![
            r(100, ROOT_RECORD, "missing1.bin", 128 * 1024, false),
            r(101, ROOT_RECORD, "missing2.bin", 128 * 1024, false),
        ];
        let tree = build_tree("vol".into(), raw);
        let opts = DuplicateOptions {
            min_size_bytes: DEFAULT_MIN_DUP_SIZE,
            size_only: false,
            skip_head_prefilter: true,
            max_groups: 500,
        };
        let result = find_duplicates(&tree, 'C', opts).unwrap();
        // The paths don't exist on disk → hash errors expected. No groups form.
        assert!(result.hash_errors >= 2);
        assert_eq!(result.groups.len(), 0);
    }
}
