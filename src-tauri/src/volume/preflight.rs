// SPDX-License-Identifier: GPL-3.0-or-later
//
// Pre-flight volume check — Chapter 33.2 Layer 0.
//
// Called BEFORE a scan is attempted. It never opens the raw volume
// (`\\.\X:` requires admin); instead it queries volume metadata via
// user-space Win32 APIs.
//
// BitLocker detection is a TODO in v0.1 — WMI Win32_EncryptableVolume or
// manage-bde parsing arrives in a later sprint. For now: a locked
// BitLocker volume makes `GetVolumeInformationW` fail and gets flagged
// `AccessDenied` (the UI warns the user it may be BitLocker).

use crate::error::{Error, Result};
use crate::volume::{DriveKind, VolumeInfo, VolumeStatus};
use std::time::Instant;
use tracing::{debug, info};

#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn normalize_drive_letter(drive: &str) -> Result<char> {
    crate::volume::parse_drive_letter(drive)
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| {
            Error::Volume(format!(
                "Invalid drive parameter: '{}' (UNC/path not supported here)",
                drive
            ))
        })
}

#[cfg(windows)]
pub fn pre_flight_check(drive: &str) -> Result<VolumeInfo> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDiskFreeSpaceW, GetDriveTypeW, GetVolumeInformationW,
    };

    let start = Instant::now();
    let letter = normalize_drive_letter(drive)?;
    let drive_letter = format!("{}:", letter);
    let root_path = format!(r"{}:\", letter);
    let root_w = to_wide(&root_path);
    let root_pcwstr = PCWSTR(root_w.as_ptr());

    // 1. Drive type
    let dtype_code = unsafe { GetDriveTypeW(root_pcwstr) };
    let drive_kind = DriveKind::from_win32(dtype_code);
    debug!(
        drive = %drive_letter,
        kind = ?drive_kind,
        code = dtype_code,
        "drive type queried"
    );

    if matches!(drive_kind, DriveKind::NoRootDir | DriveKind::Unknown) {
        return Ok(VolumeInfo {
            drive_letter,
            root_path,
            file_system: String::new(),
            volume_label: String::new(),
            volume_serial: 0,
            drive_kind,
            total_bytes: 0,
            free_bytes: 0,
            sector_size: 0,
            status: VolumeStatus::NotMounted,
            elapsed_ms: start.elapsed().as_millis() as u64,
        });
    }

    // 2. Volume information (FS name, label, serial)
    let mut label_buf = [0u16; 256];
    let mut fs_buf = [0u16; 64];
    let mut serial: u32 = 0;
    let mut max_component: u32 = 0;
    let mut flags: u32 = 0;

    let info_result = unsafe {
        GetVolumeInformationW(
            root_pcwstr,
            Some(&mut label_buf),
            Some(&mut serial as *mut _),
            Some(&mut max_component as *mut _),
            Some(&mut flags as *mut _),
            Some(&mut fs_buf),
        )
    };

    let (file_system, volume_label, volume_serial, status) = match info_result {
        Ok(_) => {
            let fs = wide_to_string(&fs_buf);
            let label = wide_to_string(&label_buf);
            let status = if fs.is_empty() {
                VolumeStatus::NotFormatted
            } else {
                VolumeStatus::Ready
            };
            (fs, label, serial, status)
        }
        Err(e) => {
            // BitLocker-locked volumes return access denied here. The app
            // runs elevated (requireAdministrator), so a metadata access
            // denial on a fixed/removable volume means the volume is
            // locked — surfaced honestly as BitLockerLocked so the UI can
            // show its lock state instead of a misleading permission
            // error.
            debug!(?e, "GetVolumeInformationW failed");
            let locked = matches!(drive_kind, DriveKind::Fixed | DriveKind::Removable);
            let status = if locked {
                VolumeStatus::BitLockerLocked
            } else {
                VolumeStatus::AccessDenied
            };
            (String::new(), String::new(), 0, status)
        }
    };

    // 3. Disk free space
    let mut available_to_caller: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    let free_result = unsafe {
        GetDiskFreeSpaceExW(
            root_pcwstr,
            Some(&mut available_to_caller),
            Some(&mut total),
            Some(&mut total_free),
        )
    };
    let (total_bytes, free_bytes) = match free_result {
        Ok(_) => (total, total_free),
        Err(e) => {
            debug!(?e, "GetDiskFreeSpaceExW failed");
            (0, 0)
        }
    };

    // 4. Sector size (physical geometry for the health page + raw math).
    let mut sectors_per_cluster: u32 = 0;
    let mut bytes_per_sector: u32 = 0;
    let mut free_clusters: u32 = 0;
    let mut total_clusters: u32 = 0;
    let sector_size = if unsafe {
        GetDiskFreeSpaceW(
            root_pcwstr,
            Some(&mut sectors_per_cluster),
            Some(&mut bytes_per_sector),
            Some(&mut free_clusters),
            Some(&mut total_clusters),
        )
    }
    .is_ok()
    {
        bytes_per_sector
    } else {
        0
    };

    let info = VolumeInfo {
        drive_letter,
        root_path,
        file_system,
        volume_label,
        volume_serial,
        drive_kind,
        total_bytes,
        free_bytes,
        sector_size,
        status,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    info!(
        drive = %info.drive_letter,
        fs = %info.file_system,
        total_gb = info.total_bytes / 1_073_741_824,
        free_gb = info.free_bytes / 1_073_741_824,
        status = ?info.status,
        elapsed_ms = info.elapsed_ms,
        "pre-flight complete"
    );

    Ok(info)
}

#[cfg(not(windows))]
pub fn pre_flight_check(_drive: &str) -> Result<VolumeInfo> {
    Err(Error::NotImplemented(
        "pre_flight_check is only available on Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_letter_normalize() {
        assert_eq!(normalize_drive_letter("C").unwrap(), 'C');
        assert_eq!(normalize_drive_letter("c:").unwrap(), 'C');
        assert_eq!(normalize_drive_letter("d:\\").unwrap(), 'D');
        assert!(normalize_drive_letter("").is_err());
    }

    #[test]
    fn unc_input_rejected() {
        // Gemini review B2 — UNC input must not silently become a drive letter.
        assert!(normalize_drive_letter(r"\\fileserver\public").is_err());
        assert!(normalize_drive_letter("//server/share").is_err());
        assert!(normalize_drive_letter(r"\rooted").is_err());
        assert!(normalize_drive_letter("12").is_err());
    }

    #[test]
    fn drive_kind_mapping() {
        assert!(matches!(DriveKind::from_win32(0), DriveKind::Unknown));
        assert!(matches!(DriveKind::from_win32(2), DriveKind::Removable));
        assert!(matches!(DriveKind::from_win32(3), DriveKind::Fixed));
        assert!(matches!(DriveKind::from_win32(99), DriveKind::Unknown));
    }
}
