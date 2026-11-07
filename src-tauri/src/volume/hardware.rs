// SPDX-License-Identifier: GPL-3.0-or-later
//
// Sürücü donanım profili — Bölüm 33 ek. Kullanıcının sidebar'da görmek
// istediği özet: medya tipi (SSD/HDD), bus (NVMe/SATA/USB), üretici,
// model, seri, tipik okuma hızı.
//
// Akış (Windows):
//   1) `\\.\X:` volume handle aç (no-access yeterli — metadata sorgu).
//   2) `IOCTL_STORAGE_QUERY_PROPERTY` (StorageDeviceProperty) →
//      `STORAGE_DEVICE_DESCRIPTOR` → bus_type + vendor/product/serial
//      offset'leri + Removable bayrağı.
//   3) `IOCTL_STORAGE_QUERY_PROPERTY` (StorageDeviceSeekPenaltyProperty)
//      → `DEVICE_SEEK_PENALTY_DESCRIPTOR.IncursSeekPenalty` (TRUE=HDD,
//      FALSE=SSD). Sorgu desteklenmiyorsa bus_type'tan tahmin (NVMe →
//      SSD, ATA without info → HDD varsayım yok, "Unknown").
//   4) Tipik okuma hızı bus + SSD/HDD'ye göre map'lenir. Bu BENCHMARK
//      değil — sadece spec'ten tahmin. Gerçek hız S.M.A.R.T. veya
//      ayrı benchmark gerektirir (v0.3+).
//
// Form faktörü (M.2 vs mSATA vs 2.5"): Windows API bunu ayrı bir alan
// olarak vermez. NVMe sürücülerin pratikte hepsi M.2 PCIe; SATA SSD'ler
// 2.5" / M.2 SATA / mSATA olabilir, Windows aynı görür. UI'da "NVMe SSD"
// (M.2 ima eder) ve "SATA SSD" (form belirsiz) etiketleri yeterli.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// Sürücü donanım özeti — UI tarafından sidebar'da gösterilir.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriveHardware {
    pub drive_letter: String,
    /// IDENTIFY/SCSI Vendor Identification — boşluklar trim'lenmiş; örn. "Samsung".
    pub vendor: Option<String>,
    /// Product/Model dizesi — örn. "SSD 970 EVO Plus 1TB".
    pub product: Option<String>,
    /// Seri numarası (hex veya ASCII; trim'li). Bazı sürücülerde boş.
    pub serial: Option<String>,
    /// `STORAGE_BUS_TYPE` insan-okur etiketi: "NVMe" | "SATA" | "USB" |
    /// "SAS" | "SCSI" | "SCM" | "Unknown".
    pub bus_label: String,
    /// Medya tipi: "SSD" | "HDD" | "Unknown" (seek penalty sorgu başarısız
    /// + bus_type ipucu vermediyse).
    pub media_label: String,
    pub is_ssd: bool,
    pub removable: bool,
    /// Tipik okuma hızı (MB/s) — bus+medya'dan türetilir, **benchmark değil**.
    /// 0 = bilinmiyor.
    pub typical_read_mbps: u32,
    /// Kısa özet: "Samsung NVMe SSD ~3500 MB/s" gibi.
    pub summary: String,
}

/// `STORAGE_BUS_TYPE` raw değerinden insan okur etiket + tipik MB/s.
fn bus_label_and_speed(bus_raw: i32, is_ssd: bool) -> (&'static str, u32) {
    // Windows DDK winioctl.h STORAGE_BUS_TYPE enum:
    //  1=SCSI, 2=ATAPI, 3=ATA, 4=1394, 5=SSA, 6=Fibre, 7=USB, 8=RAID,
    //  9=iSCSI, 10=SAS, 11=SATA, 12=SD, 13=MMC, 14=Virtual, 15=FileBackedVirtual,
    //  16=Spaces, 17=Nvme, 18=SCM, 19=Ufs.
    match bus_raw {
        17 => ("NVMe", if is_ssd { 3500 } else { 0 }),
        11 | 3 => ("SATA", if is_ssd { 550 } else { 150 }),
        7 => ("USB", if is_ssd { 500 } else { 120 }),
        10 => ("SAS", if is_ssd { 1200 } else { 200 }),
        1 => ("SCSI", if is_ssd { 600 } else { 150 }),
        18 => ("SCM", 7000),
        16 => ("Storage Spaces", if is_ssd { 800 } else { 200 }),
        8 => ("RAID", if is_ssd { 1500 } else { 250 }),
        14 | 15 => ("Sanal", 400),
        _ => ("Bilinmeyen", 0),
    }
}

fn build_summary(
    vendor: &Option<String>,
    bus_label: &str,
    media_label: &str,
    typical_read_mbps: u32,
) -> String {
    let kind = if bus_label == "Bilinmeyen" {
        media_label.to_string()
    } else {
        format!("{} {}", bus_label, media_label)
    };
    let head = match vendor {
        Some(v) if !v.is_empty() => format!("{} {}", v, kind),
        _ => kind,
    };
    if typical_read_mbps > 0 {
        format!("{} · ~{} MB/sn", head, typical_read_mbps)
    } else {
        head
    }
}

#[cfg(windows)]
pub fn probe_drive_hardware(drive_letter: char) -> Result<DriveHardware> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceProperty, StorageDeviceSeekPenaltyProperty,
        DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR,
        STORAGE_PROPERTY_QUERY,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    let upper = drive_letter.to_ascii_uppercase();
    let volume_path = format!(r"\\.\{}:", upper);
    let wide: Vec<u16> = OsStr::new(&volume_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let pcwstr = PCWSTR(wide.as_ptr());

    // No-access open — metadata sorgusu için yeterli, admin gerektirmez.
    let handle = unsafe {
        CreateFileW(
            pcwstr,
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| {
        Error::Volume(format!(
            "donanım sorgusu için volume açılamadı '{}': {:?}",
            volume_path, e
        ))
    })?;

    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let _guard = HandleGuard(handle);

    // 1) StorageDeviceProperty — vendor/product/serial + bus_type.
    let mut query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut buf = [0u8; 1024];
    let mut returned: u32 = 0;
    let device_ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const _ as *const _),
            std::mem::size_of_val(&query) as u32,
            Some(buf.as_mut_ptr() as *mut _),
            buf.len() as u32,
            Some(&mut returned),
            None,
        )
    }
    .is_ok();

    let (vendor, product, serial, removable, bus_raw) =
        if device_ok && (returned as usize) >= std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
            // SAFETY: layout is repr(C), buf is 1024 bytes which fits the
            // base struct + offset'lerle erişilen string'ler.
            let desc: &STORAGE_DEVICE_DESCRIPTOR =
                unsafe { &*(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };

            let read_str = |offset: u32| -> Option<String> {
                if offset == 0 {
                    return None;
                }
                let off = offset as usize;
                if off >= buf.len() {
                    return None;
                }
                let mut end = off;
                while end < buf.len() && buf[end] != 0 {
                    end += 1;
                }
                let bytes = &buf[off..end];
                std::str::from_utf8(bytes)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            };

            (
                read_str(desc.VendorIdOffset),
                read_str(desc.ProductIdOffset),
                read_str(desc.SerialNumberOffset),
                desc.RemovableMedia,
                desc.BusType.0,
            )
        } else {
            (None, None, None, false, 0)
        };

    // 2) SeekPenalty — SSD/HDD ayrımı.
    query.PropertyId = StorageDeviceSeekPenaltyProperty;
    let mut sp_buf = [0u8; 32];
    let mut sp_returned: u32 = 0;
    let sp_ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const _ as *const _),
            std::mem::size_of_val(&query) as u32,
            Some(sp_buf.as_mut_ptr() as *mut _),
            sp_buf.len() as u32,
            Some(&mut sp_returned),
            None,
        )
    }
    .is_ok();

    let (is_ssd, media_known) = if sp_ok
        && (sp_returned as usize) >= std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>()
    {
        let sp: &DEVICE_SEEK_PENALTY_DESCRIPTOR =
            unsafe { &*(sp_buf.as_ptr() as *const DEVICE_SEEK_PENALTY_DESCRIPTOR) };
        (!sp.IncursSeekPenalty, true)
    } else {
        // Sorgu desteklenmiyor → bus_type'tan tahmin yürüt.
        // NVMe / SCM → SSD, USB / SATA → bilinmiyor (HDD'ye yakın varsayım).
        match bus_raw {
            17 | 18 => (true, true), // NVMe/SCM kesin SSD
            _ => (false, false),     // Bilinmiyor — false döneriz ama media_known=false
        }
    };

    let (bus_label_str, bus_speed) = bus_label_and_speed(bus_raw, is_ssd);
    // Gemini review B14 — medya tipi bilinmiyorken hız iddiasında bulunma
    // ("SATA Bilinmeyen · ~150 MB/sn" yanıltıcı); build_summary 0'ı atlar.
    let typical_read_mbps = if media_known { bus_speed } else { 0 };
    let bus_label = bus_label_str.to_string();
    let media_label = if media_known {
        if is_ssd { "SSD" } else { "HDD" }.to_string()
    } else {
        "Bilinmeyen".to_string()
    };
    let summary = build_summary(&vendor, &bus_label, &media_label, typical_read_mbps);

    Ok(DriveHardware {
        drive_letter: upper.to_string(),
        vendor,
        product,
        serial,
        bus_label,
        media_label,
        is_ssd,
        removable,
        typical_read_mbps,
        summary,
    })
}

#[cfg(not(windows))]
pub fn probe_drive_hardware(drive_letter: char) -> Result<DriveHardware> {
    Err(Error::Volume(format!(
        "drive hardware probe yalnız Windows hedefinde desteklenir ({})",
        drive_letter
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_label_nvme_ssd() {
        let (label, speed) = bus_label_and_speed(17, true);
        assert_eq!(label, "NVMe");
        assert!(speed >= 1000);
    }

    #[test]
    fn bus_label_sata_hdd() {
        let (label, speed) = bus_label_and_speed(11, false);
        assert_eq!(label, "SATA");
        assert!(speed > 50 && speed < 400);
    }

    #[test]
    fn bus_label_usb_ssd() {
        let (label, speed) = bus_label_and_speed(7, true);
        assert_eq!(label, "USB");
        assert!(speed > 100);
    }

    #[test]
    fn bus_label_unknown() {
        let (label, _) = bus_label_and_speed(0, false);
        assert_eq!(label, "Bilinmeyen");
    }

    #[test]
    fn summary_with_vendor_and_speed() {
        let s = build_summary(&Some("Samsung".into()), "NVMe", "SSD", 3500);
        assert!(s.contains("Samsung"));
        assert!(s.contains("NVMe SSD"));
        assert!(s.contains("3500"));
    }

    #[test]
    fn summary_no_vendor_falls_back() {
        let s = build_summary(&None, "SATA", "SSD", 550);
        assert_eq!(s, "SATA SSD · ~550 MB/sn");
    }

    #[test]
    fn summary_no_speed_omits_mbps() {
        let s = build_summary(&Some("WD".into()), "SATA", "HDD", 0);
        assert_eq!(s, "WD SATA HDD");
    }
}
