// SPDX-License-Identifier: GPL-3.0-or-later
//
// Staging + Undo + Cross-Volume Two-Phase Commit
// Master architecture Chapter 12 (12.1–12.5).
//
// Principles:
//   * No direct deletion. Every deletion is a MOVE → staging folder.
//   * Undo window defaults to 24 hours, user-customizable.
//   * Cross-volume copy: .dspace_tmp + WAL + Blake3 hash verify +
//     atomic rename (Chapter 12.3 Layer C, v1.4 fix).
//   * Lazy expiry: NO automatic permanent deletion in the background
//     (Chapter 12.2.3, Chapter 22.6 no dark patterns).
//   * Conflict resolution (Chapter 12.2.4): overwrite / new name /
//     keep both / cancel.

pub mod cross_volume;
pub mod expiry;
pub mod ops;
pub mod permanent;
pub mod wal;
pub mod wipe;

pub use cross_volume::{blake3_file, cross_volume_stage_file};
pub use expiry::{cleanup_expired, list_expired, CleanupReport, ExpiredItem};
pub use ops::{
    list_pending, stage, undo, undo_with_resolution, ConflictResolution, FileSnapshot, StagedItem,
    UndoOutcome, STAGING_TTL_SECS,
};
pub use permanent::{permanent_delete, PermanentDeleteResult};
pub use wal::{recover_wal, WalRecoveryReport};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub enum FallbackTier {
    Normal,
    RecycleBin,
    CrossVolume,
}

#[derive(Debug, Serialize)]
pub enum WalState {
    Begin,
    Committed,
    Aborted,
}
