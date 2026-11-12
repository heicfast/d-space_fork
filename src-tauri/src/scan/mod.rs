// SPDX-License-Identifier: GPL-3.0-or-later
//
// Scan engine — Master architecture Chapter 5 (MFT) + Chapter 9.6 (lazy
// viewport query) + Chapter 33 (external/network drive layers).
//
// Architectural principles:
//   * Single Source of Truth (Chapter 4.4): `Arc<ScanTree>` has a single
//     owner on the Rust side. Vue only holds `NodeId` references.
//   * Three-layer privilege strategy (Chapter 5.2A): MFT Service → admin raw
//     volume → FindFirstFile fallback.
//   * Hot-path isolation: never runs during a VSS scan
//     (see `locked_file` Chapter 34.5.1).

pub mod cloud;
pub mod find_first;
pub mod mft;
pub mod privilege;
pub mod tree;
pub mod walk;

pub use cloud::{probe_cloud_state, CloudPlaceholderState, CloudProbe};

pub use find_first::{scan_find_first, scan_find_first_with_progress};
pub use mft::{probe_ntfs, MftProbe};
pub use privilege::is_elevated;
pub use tree::{
    build_partial_view, build_tree, build_tree_with_user_rules, node_full_path, node_path,
    scan_to_tree, scan_to_tree_with_progress, scan_to_tree_with_user_rules, top_consumers,
    window_query, Node, PartialNode, ScanSummary, ScanTree, ScanTreeState, SortKey, WindowResult,
};
pub use walk::{
    collect_mft_entries, walk_mft, MftEntries, MftWalkStats, ProgressCb, RawMftEntry, ScanProgress,
};

use serde::Serialize;

/// MFT record number — used as the node identity (Chapter 4.4).
pub type NodeId = u64;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum ScanStrategy {
    /// Chapter 5.2A Layer 3 — one-shot installed Windows service (v2 stub).
    MftService,
    /// Chapter 5.2A Layer 1 — process elevated, `\\.\X:` raw read.
    DirectRawVolume,
    /// Chapter 5.2A Layer 2 — standard mode, FindFirstFile/FindNextFile.
    FindFirstFileFallback,
}

/// Chapter 5.2A — scan strategy selection based on privilege.
/// MFT Service is a v2 stub; for now it looks at the elevation flag.
pub fn pick_strategy() -> ScanStrategy {
    if is_elevated() {
        ScanStrategy::DirectRawVolume
    } else {
        ScanStrategy::FindFirstFileFallback
    }
}
