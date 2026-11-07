// SPDX-License-Identifier: GPL-3.0-or-later
//
// Pre-flight volume check — Bölüm 33.2 Katman 0.
//
// Tarama denemesinden ÖNCE çağrılır. Hiçbir zaman raw volume açmaz
// (`\\.\X:` admin gerektirir), bunun yerine kullanıcı-uzayı Win32
// API'leriyle volume metadata'sını sorgular.
//
// BitLocker detection v0.1'de TODO — WMI Win32_EncryptableVolume veya
// manage-bde parse'ı sonraki sprint'te eklenir. Şu an: kilitli BitLocker
// volume `GetVolumeInformationW` çağrısında hata döner ve `AccessDenied`
// olarak işaretlenir (kullanıcıya BitLocker olabilir uyarısı UI'da).

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
                "Geçersiz sürücü parametresi: '{}' (UNC/path burada desteklenmez)",
                drive
            ))
        })
}

#[cfg(windows)]
pub fn pre_flight_check(drive: &str) -> Result<VolumeInfo> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW,
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
        "drive type sorgulandı"
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
            // BitLocker locked volumes typically return access denied here.
            // v0.1'de tek tip işliyoruz; v0.2'de WMI ile BitLocker ayırt edilecek.
            debug!(?e, "GetVolumeInformationW hatası");
            (String::new(), String::new(), 0, VolumeStatus::AccessDenied)
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
            debug!(?e, "GetDiskFreeSpaceExW hatası");
            (0, 0)
        }
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
        "pre-flight tamamlandı"
    );

    Ok(info)
}

#[cfg(not(windows))]
pub fn pre_flight_check(_drive: &str) -> Result<VolumeInfo> {
    Err(Error::NotImplemented("pre_flight_check sadece Windows"))
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
        // Gemini review B2 — UNC girdisi sessizce sürücü harfine dönüşmemeli.
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
