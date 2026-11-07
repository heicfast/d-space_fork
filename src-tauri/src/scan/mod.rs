// SPDX-License-Identifier: GPL-3.0-or-later
//
// Tarama motoru — Master mimari Bölüm 5 (MFT) + Bölüm 9.6 (lazy viewport
// query) + Bölüm 33 (external/network drive katmanları).
//
// Mimari ilkeler:
//   * Single Source of Truth (Bölüm 4.4): `Arc<ScanTree>` Rust tarafında
//     tek sahip. Vue sadece `NodeId` referansı tutar.
//   * Üç katmanlı yetki stratejisi (Bölüm 5.2A): MFT Service → admin raw
//     volume → FindFirstFile fallback.
//   * Hot-path izolasyonu: VSS scan sırasında ASLA çalışmaz
//     (bkz. `locked_file` Bölüm 34.5.1).

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

/// MFT record numarası — düğüm kimliği olarak kullanılır (Bölüm 4.4).
pub type NodeId = u64;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum ScanStrategy {
    /// Bölüm 5.2A Katman 3 — Tek seferlik kurulu Windows servisi (v2 stub).
    MftService,
    /// Bölüm 5.2A Katman 1 — Process elevated, `\\.\X:` raw read.
    DirectRawVolume,
    /// Bölüm 5.2A Katman 2 — Standart mod, FindFirstFile/FindNextFile.
    FindFirstFileFallback,
}

/// Bölüm 5.2A — yetkiye göre tarama stratejisi seçimi.
/// MFT Service v2 stub, şimdilik elevation flag'ine bakar.
pub fn pick_strategy() -> ScanStrategy {
    if is_elevated() {
        ScanStrategy::DirectRawVolume
    } else {
        ScanStrategy::FindFirstFileFallback
    }
}
