// SPDX-License-Identifier: GPL-3.0-or-later
//
// Duplicate Detector — four-stage pipeline.
// Master architecture Chapter 7.
//
// Principles:
//   * 0-byte and <4 KB filter (Chapter 7.2): user `min_size_bytes`.
//   * Hash: Blake3 (Chapter 7.3) — parallel, SIMD, the fastest balanced choice.
//   * Symlink/junction/hardlink by inode (Chapter 7.2): a NodeId is never
//     counted twice (ScanTree HashMap guarantee — explicit symlink/reparse elimination in v0.2).
//   * Performance target: <60 s on 100 GB of mixed data (Chapter 7.4).
//
// v0.1 limit: single-threaded hashing. v0.2 adds rayon + head-hash prefilter.

pub mod scan;

pub use scan::{find_duplicates, DuplicateOptions, DEFAULT_MIN_DUP_SIZE};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    /// Full Blake3 hash hex (64 chars) or "size:<N>" in `size_only` mode.
    pub hash_hex: String,
    pub size_bytes: u64,
    pub paths: Vec<String>,
    /// Modification times (unix seconds), parallel to `paths` — lets the
    /// UI pick the newest copy honestly instead of guessing by order.
    pub modified_unix: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateStats {
    /// TOTAL number of groups found (before truncation). The `groups` vec
    /// returned to the UI may be capped by `DuplicateOptions.max_groups`;
    /// the user is shown the "X more groups remain, narrow the filter"
    /// message based on this difference.
    pub group_count: u64,
    pub redundant_bytes: u64,
    pub elapsed_ms: u64,
}

/// Chapter 7 — response of the `find_duplicates` Tauri command.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateScanResult {
    pub drive_letter: String,
    pub scanned_files: u64,
    pub filtered_small: u64,
    pub candidate_pairs: u64,
    pub hash_errors: u64,
    pub groups: Vec<DuplicateGroup>,
    pub stats: DuplicateStats,
}
