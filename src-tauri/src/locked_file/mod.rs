// SPDX-License-Identifier: GPL-3.0-or-later
//
// Locked File Handling — Master mimari Bölüm 34.
//
// İlkeler:
//   * Bölüm 34.5.1 VSS scan critical path DIŞINDA. Scan sırasında kilitli
//     dosya sadece flag konur, içerik atlanır. Bu modülün API'si on-demand
//     (drill-down, hash-time) çağrılır — asla per-record.
//   * Bölüm 34.5.6 Reference counting + lease renewal (v1.4 fix): aktif
//     okuma sırasında snapshot ASLA evict edilmez. **v0.2 sprint'i.**
//   * Bölüm 34.5.2 Üç path stratejisi: scan-time (VSS yok), hash-time
//     (global snapshot pool), user drill-down (on-demand). v0.1 yalnızca
//     scan-time + drill-down probe; pool v0.2.
//
// Şu anki kapsam (v0.1):
//   * `probe_lock_state` — CreateFileW share-violation probe (Bölüm 34.5.3).
//   * `find_lock_owners` — Windows Restart Manager (Bölüm 34.4 alternatifi).
//   * `probe_file` — yüksek seviye API: state + owners + önerilen aksiyon.

pub mod detect;
pub mod owner;

// Bölüm 34.5.4 — VSS pool yalnızca Windows + `vss` feature açıkken.
// Default OFF (Discovery Log #002 → Plan A: winapi 0.3.9).
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

/// Bölüm 34.1 — locked dosya kategorilerine göre önerilen aksiyon.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LockedFileAction {
    /// Dosya boştu, normal işlem yapılabilir.
    Proceed,
    /// İçerik atlanır, sadece metadata kullanılır (scan-time default).
    SkipContent,
    /// VSS snapshot üzerinden read-only oku (v0.2 hash-time path).
    SnapshotRead,
    /// Kullanıcıya seçenekler sun (drill-down dialog).
    UserDrillDown,
    /// Sistem dosyası — hard pass, denemez.
    HardPass,
}

/// Bölüm 34.3 — user-facing diagnostic için top-level probe sonucu.
#[derive(Debug, Clone, Serialize)]
pub struct LockedFileProbe {
    pub path: String,
    pub state: LockState,
    pub owners: Vec<LockOwner>,
    pub recommended_action: LockedFileAction,
    /// v0.2'de aktif olacak — şu an her zaman false.
    pub vss_available: bool,
    pub probe_elapsed_ms: u64,
}

/// Tek dosya için yüksek seviye probe:
///   1) Share-violation probe ile lock state.
///   2) Locked ise Restart Manager ile owner listesi.
///   3) Spec Bölüm 34.1 tablosuna göre önerilen aksiyonu seç.
///
/// **VSS yok** (Bölüm 34.5.1). Hash-time snapshot pool v0.2 sprint'inde.
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

/// Bölüm 34.5.4 — verilen yol için VSS snapshot kullanılabilir mi?
/// v0.2 ilk hali: feature açıksa true (admin/NTFS deep-check sonraki sprint'te).
/// Feature kapalıyken her zaman false.
#[cfg(all(windows, feature = "vss"))]
pub fn vss_available_for(_path: &Path) -> bool {
    // İlk iterasyon: pool worker hazır olduğu sürece true varsayıyoruz.
    // is_volume_supported probe'u (admin + NTFS) sonraki sprint'te eklenecek;
    // şu an deep probe çağırmak her drill-down'da COM init maliyetini doğurur.
    true
}

#[cfg(not(all(windows, feature = "vss")))]
pub fn vss_available_for(_path: &Path) -> bool {
    false
}

/// Bölüm 34.1 — kategoriye göre aksiyon önerisi.
/// v0.1'de heuristik: locked + system path → HardPass, aksi SnapshotRead.
fn recommend_action(state: &LockState, _owners: &[LockOwner], path: &Path) -> LockedFileAction {
    match state {
        LockState::Free => LockedFileAction::Proceed,
        LockState::NotFound | LockState::AccessDenied => LockedFileAction::HardPass,
        LockState::OtherError(_) => LockedFileAction::SkipContent,
        LockState::Locked => {
            let lc = path.to_string_lossy().to_ascii_lowercase();
            // Sistem dosyaları → hard pass (Bölüm 34.1 "Hard pass — denenmez")
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
                // Bölüm 34.5.4 — VSS pool aktifse SnapshotRead, aksi UserDrillDown.
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
        // `vss_available` feature'a göre değişir; sadece probe çalıştığını
        // doğrula. Detay test'i `vss_pool::tests` altında.
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
