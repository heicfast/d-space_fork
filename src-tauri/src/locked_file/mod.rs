// SPDX-License-Identifier: GPL-3.0-or-later
//
// Locked File Handling — Master architecture Chapter 34.
//
// Principles:
//   * Chapter 34.5.1 VSS is OUTSIDE the scan critical path. During a scan
//     a locked file is merely flagged, its content is skipped. This
//     module's API is called on demand (drill-down, hash-time) — never
//     per-record.
//   * Chapter 34.5.6 Reference counting + lease renewal (v1.4 fix): a
//     snapshot is NEVER evicted during an active read. **v0.2 sprint.**
//   * Chapter 34.5.2 Three-path strategy: scan-time (no VSS), hash-time
//     (global snapshot pool), user drill-down (on-demand). v0.1 only has
//     scan-time + drill-down probe; the pool is v0.2.
//
// Current scope (v0.1):
//   * `probe_lock_state` — CreateFileW share-violation probe (Chapter 34.5.3).
//   * `find_lock_owners` — Windows Restart Manager (Chapter 34.4 alternative).
//   * `probe_file` — high-level API: state + owners + recommended action.

pub mod detect;
pub mod owner;

// Chapter 34.5.4 — the VSS pool only exists on Windows + with the `vss`
// feature enabled. Default OFF (Discovery Log #002 → Plan A: winapi 0.3.9).
#[cfg(all(windows, feature = "vss"))]
pub mod vss;
#[cfg(all(windows, feature = "vss"))]
pub mod vss_pool;

pub use detect::{probe_lock_state, LockState};
pub use owner::{find_lock_owners, LockOwner};

use crate::error::Result;
use serde::Serialize;
use std::path::Path;
use tracing::info;

/// Chapter 34.1 — recommended action per locked-file category.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LockedFileAction {
    /// The file was free; normal processing can proceed.
    Proceed,
    /// Content is skipped, only metadata is used (scan-time default).
    SkipContent,
    /// Read via a VSS snapshot, read-only (v0.2 hash-time path).
    SnapshotRead,
    /// Present options to the user (drill-down dialog).
    UserDrillDown,
    /// System file — hard pass, do not attempt.
    HardPass,
}

/// Chapter 34.3 — top-level probe result for user-facing diagnostics.
#[derive(Debug, Clone, Serialize)]
pub struct LockedFileProbe {
    pub path: String,
    pub state: LockState,
    pub owners: Vec<LockOwner>,
    pub recommended_action: LockedFileAction,
    /// Becomes active in v0.2 — currently always false.
    pub vss_available: bool,
    pub probe_elapsed_ms: u64,
}

/// High-level probe for a single file:
///   1) Lock state via the share-violation probe.
///   2) If locked, the owner list via Restart Manager.
///   3) Pick the recommended action per the spec's Chapter 34.1 table.
///
/// **No VSS** (Chapter 34.5.1). Hash-time snapshot pool in the v0.2 sprint.
pub fn probe_file(path: &Path) -> Result<LockedFileProbe> {
    let start = std::time::Instant::now();
    let state = probe_lock_state(path)?;

    let owners = if matches!(state, LockState::Locked) {
        find_lock_owners(path).unwrap_or_else(|e| {
            tracing::warn!(?e, path = %path.display(), "lock owner lookup failed");
            Vec::new()
        })
    } else {
        Vec::new()
    };

    let recommended_action = recommend_action(&state, &owners, path);
    let vss_available = vss_available_for(path);

    let elapsed_ms = start.elapsed().as_millis() as u64;
    info!(
        path = %path.display(),
        ?state,
        owners = owners.len(),
        ?recommended_action,
        vss_available,
        elapsed_ms,
        "locked file probe"
    );

    Ok(LockedFileProbe {
        path: path.display().to_string(),
        state,
        owners,
        recommended_action,
        vss_available,
        probe_elapsed_ms: elapsed_ms,
    })
}

/// Chapter 34.5.4 — is a VSS snapshot usable for the given path?
/// v0.2 first cut: true when the feature is enabled (admin/NTFS deep-check
/// in the next sprint). Always false while the feature is off.
#[cfg(all(windows, feature = "vss"))]
pub fn vss_available_for(_path: &Path) -> bool {
    // First iteration: assume true as long as the pool worker is ready.
    // The is_volume_supported probe (admin + NTFS) lands in the next
    // sprint; calling a deep probe now would cost COM init on every
    // drill-down.
    true
}

#[cfg(not(all(windows, feature = "vss")))]
pub fn vss_available_for(_path: &Path) -> bool {
    false
}

/// Chapter 34.1 — action recommendation by category.
/// v0.1 heuristic: locked + system path → HardPass, otherwise SnapshotRead.
fn recommend_action(state: &LockState, _owners: &[LockOwner], path: &Path) -> LockedFileAction {
    match state {
        LockState::Free => LockedFileAction::Proceed,
        LockState::NotFound | LockState::AccessDenied => LockedFileAction::HardPass,
        LockState::OtherError(_) => LockedFileAction::SkipContent,
        LockState::Locked => {
            let lc = path.to_string_lossy().to_ascii_lowercase();
            // System files → hard pass (Chapter 34.1 "Hard pass — not attempted")
            let system_marker = [
                "\\windows\\system32\\",
                "\\windows\\winsxs\\",
                "\\$recycle.bin\\",
                "\\system volume information\\",
                "pagefile.sys",
                "hiberfil.sys",
                "swapfile.sys",
            ];
            if system_marker.iter().any(|m| lc.contains(m)) {
                LockedFileAction::HardPass
            } else {
                // Chapter 34.5.4 — SnapshotRead if the VSS pool is active, else UserDrillDown.
                #[cfg(all(windows, feature = "vss"))]
                {
                    LockedFileAction::SnapshotRead
                }
                #[cfg(not(all(windows, feature = "vss")))]
                {
                    LockedFileAction::UserDrillDown
                }
            }
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn free_file_recommends_proceed() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("free.bin");
        fs::write(&p, b"x").unwrap();
        let probe = probe_file(&p).unwrap();
        assert!(matches!(probe.state, LockState::Free));
        assert!(matches!(
            probe.recommended_action,
            LockedFileAction::Proceed
        ));
        assert!(probe.owners.is_empty());
        // `vss_available` depends on the feature; just verify the probe
        // ran. The detailed test lives under `vss_pool::tests`.
        #[cfg(not(feature = "vss"))]
        assert!(!probe.vss_available);
    }

    #[test]
    fn missing_file_recommends_hard_pass() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("ghost.bin");
        let probe = probe_file(&p).unwrap();
        assert!(matches!(probe.state, LockState::NotFound));
        assert!(matches!(
            probe.recommended_action,
            LockedFileAction::HardPass
        ));
    }
}
