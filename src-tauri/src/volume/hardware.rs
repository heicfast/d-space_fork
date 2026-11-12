// SPDX-License-Identifier: GPL-3.0-or-later
//
// Drive hardware profile — Chapter 33 appendix. The summary the user wants
// to see in the sidebar: media type (SSD/HDD), bus (NVMe/SATA/USB), vendor,
// model, serial, typical read speed.
//
// Flow (Windows):
//   1) Open a `\\.\X:` volume handle (no-access is enough — metadata query).
//   2) `IOCTL_STORAGE_QUERY_PROPERTY` (StorageDeviceProperty) →
//      `STORAGE_DEVICE_DESCRIPTOR` → bus_type + vendor/product/serial
//      offsets + the Removable flag.
//   3) `IOCTL_STORAGE_QUERY_PROPERTY` (StorageDeviceSeekPenaltyProperty)
//      → `DEVICE_SEEK_PENALTY_DESCRIPTOR.IncursSeekPenalty` (TRUE=HDD,
//      FALSE=SSD). If the query is unsupported, guess from bus_type (NVMe →
//      SSD, ATA without info → no HDD assumption, "Unknown").
//   4) The typical read speed is mapped from bus + SSD/HDD. This is NOT a
//      BENCHMARK — only an estimate from the spec. Real speed requires
//      S.M.A.R.T. or a separate benchmark (v0.3+).
//
// Form factor (M.2 vs mSATA vs 2.5"): the Windows API does not expose it
// as a separate field. In practice almost all NVMe drives are M.2 PCIe; SATA
// SSDs can be 2.5" / M.2 SATA / mSATA, Windows sees them alike. In the UI,
// "NVMe SSD" (implies M.2) and "SATA SSD" (form ambiguous) labels suffice.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// Drive hardware summary — shown in the sidebar by the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriveHardware {
    pub drive_letter: String,
    /// IDENTIFY/SCSI Vendor Identification — whitespace trimmed; e.g. "Samsung".
    pub vendor: Option<String>,
    /// Product/Model string — e.g. "SSD 970 EVO Plus 1TB".
    pub product: Option<String>,
    /// Serial number (hex or ASCII; trimmed). Empty on some drives.
    pub serial: Option<String>,
    /// `STORAGE_BUS_TYPE` human-readable label: "NVMe" | "SATA" | "USB" |
    /// "SAS" | "SCSI" | "SCM" | "Unknown".
    pub bus_label: String,
    /// Media type: "SSD" | "HDD" | "Unknown" (when the seek penalty query
    /// fails + bus_type gave no hint).
    pub media_label: String,
    pub is_ssd: bool,
    pub removable: bool,
    /// Typical read speed (MB/s) — derived from bus+media, **not a benchmark**.
    /// 0 = unknown.
    pub typical_read_mbps: u32,
    /// Short summary: e.g. "Samsung NVMe SSD ~3500 MB/s".
    pub summary: String,
}

/// Human-readable label + typical MB/s from the raw `STORAGE_BUS_TYPE` value.
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
        14 | 15 => ("Virtual", 400),
        _ => ("Unknown", 0),
    }
}

fn build_summary(
    vendor: &Option<String>,
    bus_label: &str,
    media_label: &str,
    typical_read_mbps: u32,
) -> String {
    let kind = if bus_label == "Unknown" {
        media_label.to_string()
    } else {
        format!("{} {}", bus_label, media_label)
    };
    let head = match vendor {
        Some(v) if !v.is_empty() => format!("{} {}", v, kind),
        _ => kind,
    };
    if typical_read_mbps > 0 {
        format!("{} · ~{} MB/s", head, typical_read_mbps)
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

    // No-access open — enough for the metadata query, does not require admin.
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
            "cannot open volume '{}' for the hardware query: {:?}",
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
            // base struct + the strings accessed via offsets.
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

    // 2) SeekPenalty — the SSD/HDD distinction.
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
        // Query unsupported → infer from bus_type.
        // NVMe / SCM → SSD, USB / SATA → unknown (an assumption close to HDD).
        match bus_raw {
            17 | 18 => (true, true), // NVMe/SCM definitely SSD
            _ => (false, false),     // Unknown — we return false but media_known=false
        }
    };

    let (bus_label_str, bus_speed) = bus_label_and_speed(bus_raw, is_ssd);
    // Gemini review B14 — make no speed claim while the media type is unknown
    // ("SATA Unknown · ~150 MB/s" would be misleading); build_summary skips 0.
    let typical_read_mbps = if media_known { bus_speed } else { 0 };
    let bus_label = bus_label_str.to_string();
    let media_label = if media_known {
        if is_ssd { "SSD" } else { "HDD" }.to_string()
    } else {
        "Unknown".to_string()
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
        "drive hardware probe is only supported on Windows targets ({})",
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
        assert_eq!(label, "Unknown");
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
        assert_eq!(s, "SATA SSD · ~550 MB/s");
    }

    #[test]
    fn summary_no_speed_omits_mbps() {
        let s = build_summary(&Some("WD".into()), "SATA", "HDD", 0);
        assert_eq!(s, "WD SATA HDD");
    }
}
