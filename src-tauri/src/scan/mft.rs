// SPDX-License-Identifier: GPL-3.0-or-later
//
// MFT probe — Bölüm 5.1–5.3.
//
// Bu sprint: volume handle açma + NTFS boot sector parse + temel metadata.
// Hedef: ntfs crate'iyle entegrasyonun çalıştığını ve raw volume read
// pipeline'ının ayakta olduğunu kanıtlamak.
//
// Sonraki sprint: MFT entry iteration + FileNode listesi + rayon paralel
// aggregation (Bölüm 4.3, 9.6).

use crate::error::{Error, Result};
use ntfs::Ntfs;
use serde::Serialize;
use std::fs::File;
use std::time::Instant;
use tracing::{debug, info};

/// NTFS boot sector'ünden okunan temel metadata.
#[derive(Debug, Clone, Serialize)]
pub struct MftProbe {
    pub drive: String,
    pub volume_path: String,
    pub volume_serial: u64,
    pub cluster_size: u32,
    pub sector_size: u32,
    pub file_record_size: u32,
    pub elapsed_ms: u64,
}

/// "C", "c:", "C:\", "C:/Users" → r"\\.\C:" — katı doğrulama (UNC reddedilir,
/// Gemini review B2).
fn normalize_volume_path(drive: &str) -> Result<String> {
    let letter = crate::volume::parse_drive_letter(drive)
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| {
            Error::Scan(format!(
                "Geçersiz sürücü parametresi: '{}' (raw volume yolu UNC kabul etmez)",
                drive
            ))
        })?;
    Ok(format!(r"\\.\{}:", letter))
}

/// Volume handle açar, NTFS boot sector'ünü parse eder, temel metadata döner.
///
/// Yönetici yetkisi gerekir (`\\.\X:` raw volume access). Eğer
/// [`crate::scan::is_elevated`] false dönerse bu fonksiyon büyük olasılıkla
/// `Error::Scan` (ERROR_ACCESS_DENIED) verir — Bölüm 5.2A Katman 2 fallback'i
/// devreye girer.
pub fn probe_ntfs(drive: &str) -> Result<MftProbe> {
    let start = Instant::now();
    let volume_path = normalize_volume_path(drive)?;

    debug!(volume = %volume_path, "raw volume açılıyor");
    let mut handle = File::open(&volume_path).map_err(|e| {
        Error::Scan(format!(
            "Volume açılamadı '{}': {} (yönetici izni gerekli olabilir)",
            volume_path, e
        ))
    })?;

    debug!("NTFS boot sector parse ediliyor");
    let ntfs =
        Ntfs::new(&mut handle).map_err(|e| Error::Scan(format!("NTFS parse hatası: {:?}", e)))?;

    let probe = MftProbe {
        drive: drive.to_string(),
        volume_path,
        volume_serial: ntfs.serial_number(),
        cluster_size: ntfs.cluster_size(),
        sector_size: ntfs.sector_size() as u32,
        file_record_size: ntfs.file_record_size(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    info!(
        serial = format!("{:x}", probe.volume_serial),
        cluster = probe.cluster_size,
        record = probe.file_record_size,
        elapsed_ms = probe.elapsed_ms,
        "MFT probe başarılı"
    );

    Ok(probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_drive_letters() {
        assert_eq!(normalize_volume_path("C").unwrap(), r"\\.\C:");
        assert_eq!(normalize_volume_path("c:").unwrap(), r"\\.\C:");
        assert_eq!(normalize_volume_path("C:\\").unwrap(), r"\\.\C:");
        assert_eq!(normalize_volume_path("d:\\Users").unwrap(), r"\\.\D:");
        assert!(normalize_volume_path("").is_err());
        assert!(normalize_volume_path("123").is_err());
    }
}
