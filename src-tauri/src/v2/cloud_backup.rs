// SPDX-License-Identifier: GPL-3.0-or-later
//
// 26.5 Cloud Backup Integration — v2 stub.
//
// Phase 2: optional cloud backup before deleting from the staging folder.
// The "delete but back it up to the cloud first" flow is a premium feature.
// Current staging (Chapter 12) is local disk; cloud backup is an extra layer.
//
// Principle (Chapter 22.6): cloud backup is opt-in, never a dark pattern
// you cannot opt out of. If the license is cancelled, the data stays in
// the user's hands (90-day grace period including download).

use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CloudProvider {
    /// D-Space's own backup endpoint (premium).
    DSpaceCloud,
    /// Bring-your-own: S3-compatible (Backblaze B2, Wasabi, etc.).
    S3Compatible,
    /// Copy into the user's OneDrive folder (optional, free).
    OneDriveFolder,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudUploadResult {
    pub staging_id: i64,
    pub provider: CloudProvider,
    pub remote_id: String,
    pub uploaded_bytes: u64,
    pub elapsed_ms: u64,
}

/// Chapter 26.5 — cloud backup trait. Streaming upload, hash verify.
pub trait CloudBackupIntegration: Send + Sync {
    fn provider(&self) -> CloudProvider;

    /// Back up a staging item to the cloud. The local staging file is copied
    /// to the cloud and `remote_id` is returned. The subsequent permanent_delete
    /// also writes this remote_id to the forensic ledger (Chapter 12.5).
    fn upload_staged(&self, staging_id: i64, staged_path: &str) -> Result<CloudUploadResult>;

    /// Restore from the cloud. The user can still find an item whose 24h
    /// staging window has passed in the cloud (during the license grace period).
    fn restore_from_cloud(&self, remote_id: &str, dest_path: &str) -> Result<u64>;

    /// Delete cloud traces (forensic-aware; our ledger is preserved).
    fn delete_remote(&self, remote_id: &str) -> Result<()>;
}
