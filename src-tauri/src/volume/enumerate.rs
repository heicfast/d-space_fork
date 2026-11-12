// SPDX-License-Identifier: GPL-3.0-or-later
//
// Volume Enumeration — sistemdeki tüm mount edilmiş sürücüleri listeler.
// Win32 `GetLogicalDrives` bitmask döner (bit 0 = A:, bit 25 = Z:). Her bit
// için `pre_flight_check` çağrılır; sonuç bir araya getirilir. Master spec
// Bölüm 33.2 Katman 0 — admin gerektirmez.

use crate::error::Result;
use crate::volume::{pre_flight_check, VolumeInfo};
use tracing::{debug, info};

/// Win32 `GetLogicalDrives` ile mount edilmiş sürücü harflerini döner.
/// Sonuç sıralı: A, B, ..., Z. Hiç sürücü yoksa boş Vec.
#[cfg(windows)]
pub fn enumerate_drive_letters() -> Vec<char> {
    use windows::Win32::Storage::FileSystem::GetLogicalDrives;
    let mask = unsafe { GetLogicalDrives() };
    debug!(mask = format!("{:#034b}", mask), "GetLogicalDrives bitmask");
    let mut out = Vec::new();
    for i in 0..26u32 {
        if (mask & (1u32 << i)) != 0 {
            let letter = char::from(b'A' + i as u8);
            out.push(letter);
        }
    }
    out
}

#[cfg(not(windows))]
pub fn enumerate_drive_letters() -> Vec<char> {
    Vec::new()
}

/// Sistemdeki tüm sürücülerin pre-flight bilgisini döner. Tek bir sürücüde
/// pre_flight_check hata verirse o sürücü atlanır (genel listeyi bozmamak
/// için). Hata satırı tracing'e düşer.
pub fn list_drives() -> Result<Vec<VolumeInfo>> {
    let letters = enumerate_drive_letters();
    info!(count = letters.len(), "list_drives: enumeration started");
    let mut out = Vec::with_capacity(letters.len());
    for letter in letters {
        let drive = format!("{}:", letter);
        match pre_flight_check(&drive) {
            Ok(info) => out.push(info),
            Err(e) => {
                debug!(drive = %drive, ?e, "pre_flight_check skipped");
            }
        }
    }
    info!(returned = out.len(), "list_drives complete");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn enumerate_returns_at_least_one_letter() {
        // Test makinesinde en az bir mount edilmiş sürücü olmalı (C:).
        let letters = enumerate_drive_letters();
        assert!(!letters.is_empty(), "at least one mounted drive expected");
        // Tümü ASCII büyük harf.
        for c in &letters {
            assert!(c.is_ascii_uppercase());
        }
        // Sıralı (artarak) ve tekrarsız.
        let mut sorted = letters.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, letters);
    }

    #[cfg(windows)]
    #[test]
    fn list_drives_returns_valid_info() {
        let drives = list_drives().expect("list_drives should succeed");
        // En az bir VolumeInfo olmalı (C:).
        assert!(!drives.is_empty(), "at least one drive entry expected");
        for info in &drives {
            assert!(!info.drive_letter.is_empty());
            assert!(info.drive_letter.ends_with(':'));
            assert!(info.root_path.ends_with('\\'));
        }
    }
}
