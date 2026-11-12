// SPDX-License-Identifier: GPL-3.0-or-later
//
// MFT probe — Chapter 5.1–5.3.
//
// This sprint: open the volume handle + NTFS boot sector parse + basic metadata.
// Goal: prove the ntfs crate integration works and the raw volume read
// pipeline is up and running.
//
// Next sprint: MFT entry iteration + FileNode list + rayon parallel
// aggregation (Chapter 4.3, 9.6).

use crate::error::{Error, Result};
use ntfs::Ntfs;
use serde::Serialize;
use std::fs::File;
use std::time::Instant;
use tracing::{debug, info};

/// Basic metadata read from the NTFS boot sector.
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

/// "C", "c:", "C:\", "C:/Users" → r"\\.\C:" — strict validation (UNC is
/// rejected, Gemini review B2).
fn normalize_volume_path(drive: &str) -> Result<String> {
    let letter = crate::volume::parse_drive_letter(drive)
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| {
            Error::Scan(format!(
                "Invalid drive parameter: '{}' (the raw volume path does not accept UNC)",
                drive
            ))
        })?;
    Ok(format!(r"\\.\{}:", letter))
}

/// Opens the volume handle, parses the NTFS boot sector, returns basic metadata.
///
/// Requires administrator privileges (`\\.\X:` raw volume access). If
/// [`crate::scan::is_elevated`] returns false, this function most likely
/// yields `Error::Scan` (ERROR_ACCESS_DENIED) — the Chapter 5.2A Layer 2
/// fallback kicks in.
pub fn probe_ntfs(drive: &str) -> Result<MftProbe> {
    let start = Instant::now();
    let volume_path = normalize_volume_path(drive)?;

    debug!(volume = %volume_path, "opening raw volume");
    let mut handle = File::open(&volume_path).map_err(|e| {
        Error::Scan(format!(
            "cannot open volume '{}': {} (administrator privileges may be required)",
            volume_path, e
        ))
    })?;

    debug!("parsing NTFS boot sector");
    let ntfs =
        Ntfs::new(&mut handle).map_err(|e| Error::Scan(format!("NTFS parse error: {:?}", e)))?;

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
        "MFT probe ok"
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
