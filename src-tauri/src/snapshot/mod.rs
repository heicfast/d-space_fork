// SPDX-License-Identifier: GPL-3.0-or-later
//
// Time Machine — daily metadata snapshot system.
// Master architecture Chapter 8.
//
// Principles:
//   * ~1 MB per snapshot (compressed), 90-day default history.
//   * Delta storage: for the same path, only changed fields are kept.
//   * Retention policy (Chapter 8.4): free 7 days, premium 365 days.
//   * Streaming delta loader (Chapter 9.6.5): delivered to the UI as
//     chunk events, not two full trees.

pub mod capture;
pub mod delta;

pub use capture::{
    capture_snapshot, list_snapshots, purge_old_snapshots, SnapshotMeta, DEFAULT_RETAIN_DAYS,
    MIN_RETAIN_DAYS,
};
pub use delta::{compute_delta, DeltaEntry, DeltaResult, PathEntry};

/// Matches the snapshot table's PRIMARY KEY type — i64.
pub type SnapshotId = i64;
