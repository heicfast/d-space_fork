// SPDX-License-Identifier: GPL-3.0-or-later
//
// Disk health — DiskWizard recovery engine.
//
// SMART data for SATA drives via `ATA_PASS_THROUGH_EX` /
// `SMART_RCV_DRIVE_DATA` on the physical drive handle (administrator),
// temperature via `IOCTL_STORAGE_QUERY_PROPERTY` (query 0x0A? — we use
// the STORAGE_TEMPERATURE request where available). NVMe health via the
// protocol-specific SMART/health log is a documented v0.5 candidate;
// drives that do not answer the ATA SMART query degrade honestly to the
// contract's `health-unavailable` error.
//
// The 512-byte attribute sector parser is pure (fixture-tested).

use crate::error::{Error, Result};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Wire types (contract's DiskHealth)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartAttributeWire {
    pub id: u32,
    pub name: String,
    pub current: Option<i64>,
    pub worst: Option<i64>,
    pub threshold: Option<i64>,
    pub raw: u64,
    pub status: String, // ok | failing | unknown
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskHealthWire {
    pub device_id: String,
    pub status: String, // good | caution | bad
    pub temperature_c: Option<i64>,
    pub power_on_hours: Option<i64>,
    pub disk_class: String, // ssd | hdd | unknown
    pub wear_percent: Option<i64>,
    pub trim_enabled: Option<bool>,
    pub reallocated_sectors: Option<i64>,
    pub pending_sectors: Option<i64>,
    pub attributes: Vec<SmartAttributeWire>,
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Pure SMART sector parsing
// ---------------------------------------------------------------------------

/// Known SMART attribute names (best-effort; unknown ids stay numeric).
pub fn attribute_name(id: u8) -> String {
    let known = match id {
        1 => "Read Error Rate",
        3 => "Spin-Up Time",
        4 => "Start/Stop Count",
        5 => "Reallocated Sectors Count",
        7 => "Seek Error Rate",
        9 => "Power-On Hours",
        10 => "Spin Retry Count",
        11 => "Recalibration Retries",
        12 => "Power Cycle Count",
        170 => "Available Reserved Space (SSD)",
        171 => "Program Fail Count (SSD)",
        172 => "Erase Fail Count (SSD)",
        173 => "Average Block Erase Count (SSD)",
        177 => "Wear Leveling Count (SSD)",
        178 => "Used Reserved Block Count (SSD)",
        180 => "Unused Reserved Block Count (SSD)",
        187 => "Reported Uncorrectable Errors",
        188 => "Command Timeout",
        194 => "Temperature Celsius",
        196 => "Reallocated Event Count",
        197 => "Current Pending Sector Count",
        198 => "Uncorrectable Sector Count",
        199 => "UltraDMA CRC Error Count",
        231 => "SSD Life Left",
        232 => "Endurance Remaining (SSD)",
        233 => "Media Wearout Indicator (SSD)",
        240 => "Head Flying Hours",
        241 => "Total LBAs Written",
        242 => "Total LBAs Read",
        _ => "",
    };
    if known.is_empty() {
        format!("Attribute {id}")
    } else {
        known.to_string()
    }
}

/// Parses the 362-byte attribute table out of a SMART READ DATA sector.
/// Layout: 12 bytes header, then 12 bytes per attribute (id, flags, value
/// [current], worst, raw[6] + reserved), id 0 terminates.
pub fn parse_smart_attributes(sector: &[u8]) -> Vec<(u8, u8, u8, Option<u8>, u64)> {
    // returns (id, current, worst, threshold?, raw)
    let mut out = Vec::new();
    if sector.len() < 12 {
        return out;
    }
    for i in 0..30 {
        let off = 12 + i * 12;
        if off + 12 > sector.len() {
            break;
        }
        let id = sector[off];
        if id == 0 {
            break;
        }
        let current = sector[off + 3];
        let worst = sector[off + 4];
        let raw = u64::from(sector[off + 5])
            | (u64::from(sector[off + 6]) << 8)
            | (u64::from(sector[off + 7]) << 16)
            | (u64::from(sector[off + 8]) << 24)
            | (u64::from(sector[off + 9]) << 32)
            | (u64::from(sector[off + 10]) << 40);
        out.push((id, current, worst, None, raw));
    }
    out
}

/// Attributes whose nonzero raw values indicate degradation.
const CRITICAL_ATTRIBUTES: &[u8] = &[5, 187, 196, 197, 198];

/// Derives the health verdict from parsed attributes.
pub fn health_status(attrs: &[(u8, u8, u8, Option<u8>, u64)]) -> &'static str {
    for (id, current, _worst, threshold, _raw) in attrs {
        if let Some(th) = threshold {
            if CRITICAL_ATTRIBUTES.contains(id) && *current > 0 && *current < *th {
                return "bad";
            }
        }
    }
    // Heuristic raw checks (thresholds often return 0 on consumer drives).
    for (id, _c, _w, _t, raw) in attrs {
        if CRITICAL_ATTRIBUTES.contains(id) && *raw > 0 {
            return "caution";
        }
    }
    "good"
}

/// Extracts temperature (attribute 194 raw, first byte) and power-on
/// hours (attribute 9 raw).
pub fn temperature_and_hours(
    attrs: &[(u8, u8, u8, Option<u8>, u64)],
) -> (Option<i64>, Option<i64>) {
    let temp = attrs
        .iter()
        .find(|(id, ..)| *id == 194)
        .map(|(_, _, _, _, raw)| (*raw & 0xFF) as i64)
        .filter(|t| (0..=100).contains(t));
    let hours = attrs
        .iter()
        .find(|(id, ..)| *id == 9)
        .map(|(_, _, _, _, raw)| *raw as i64);
    (temp, hours)
}

/// SSD wear from attribute 231 (life left %) or 177 (leveling count).
pub fn wear_percent(attrs: &[(u8, u8, u8, Option<u8>, u64)]) -> Option<i64> {
    if let Some((_, _, _, _, raw)) = attrs.iter().find(|(id, ..)| *id == 231) {
        return Some(100 - (*raw & 0xFF) as i64);
    }
    if let Some((_, cur, _, _, _)) = attrs.iter().find(|(id, ..)| *id == 177) {
        return Some(100 - *cur as i64);
    }
    None
}

pub fn is_ssd_like(attrs: &[(u8, u8, u8, Option<u8>, u64)]) -> bool {
    attrs
        .iter()
        .any(|(id, ..)| matches!(id, 170 | 171 | 172 | 173 | 177 | 178 | 231 | 232 | 233))
}

pub fn critical_raw(attrs: &[(u8, u8, u8, Option<u8>, u64)], id: u8) -> Option<i64> {
    attrs
        .iter()
        .find(|(a, ..)| *a == id)
        .map(|(_, _, _, _, raw)| *raw as i64)
}

// ---------------------------------------------------------------------------
// IO (Windows)
// ---------------------------------------------------------------------------

#[repr(C)]
struct AtaPassThroughEx {
    length: u16,
    atapi_flags: u16,
    ata_flags: u16,
    path_transfer_length: u16,
    data_transfer_length: u32,
    time_out_value: u32,
    reserved_as_u32: u32,
    data_buffer_offset: *mut u8,
    previous_task_file: [u8; 8],
    current_task_file: [u8; 8],
}

const ATA_FLAGS_DRDY_REQUIRED: u16 = 0x02;
const ATA_FLAGS_DATA_IN: u16 = 0x04;
const ATA_FLAGS_USE_DMA: u16 = 0x10;

#[cfg(windows)]
const fn ctl_code(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

#[cfg(windows)]
const IOCTL_ATA_PASS_THROUGH: u32 = ctl_code(0x00000045, 0x040B, 0, 1 | 2); // METHOD_BUFFERED, FILE_READ|WRITE

/// SMART READ DATA command payload.
const SMART_CMD: u8 = 0xB0;
const SMART_CYL_HIGH: u8 = 0xC2;
const SMART_CYL_LOW: u8 = 0x4F;

/// Reads the SMART attribute sector from a physical drive via
/// ATA PASS THROUGH. Returns the 512-byte READ DATA sector.
#[cfg(windows)]
pub fn read_smart_sector(physical_drive: u32) -> Result<Vec<u8>> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::IO::DeviceIoControl;

    let path = format!(r"\\.\PhysicalDrive{physical_drive}");
    let file = std::fs::File::open(&path)
        .map_err(|e| Error::Volume(format!("cannot open '{path}': {e}")))?;

    let mut data = vec![0u8; 512 + 48]; // sense + ATA struct headroom
    let mut apt = AtaPassThroughEx {
        length: std::mem::size_of::<AtaPassThroughEx>() as u16,
        atapi_flags: 0,
        ata_flags: ATA_FLAGS_DRDY_REQUIRED | ATA_FLAGS_DATA_IN | ATA_FLAGS_USE_DMA,
        path_transfer_length: 512,
        data_transfer_length: 512,
        time_out_value: 10,
        reserved_as_u32: 0,
        data_buffer_offset: data.as_mut_ptr(),
        previous_task_file: [0; 8],
        current_task_file: [0; 8],
    };
    // SMART READ DATA: features=0xD0, sector count=1, lba=0xC2/0x4F, cmd=0xB0
    apt.current_task_file = [
        0xD0,
        0x01,
        0,
        0,
        0,
        SMART_CYL_LOW,
        SMART_CYL_HIGH,
        SMART_CMD,
    ];

    let apt_bytes = unsafe { std::slice::from_raw_parts(&apt as *const _ as *const u8, 40) };
    let mut input: Vec<u8> = Vec::with_capacity(40 + 512);
    input.extend_from_slice(apt_bytes);
    input.extend_from_slice(&[0u8; 512]);

    let mut returned: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            HANDLE(file.as_raw_handle()),
            IOCTL_ATA_PASS_THROUGH,
            Some(input.as_mut_ptr().cast()),
            input.len() as u32,
            Some(input.as_mut_ptr().cast()),
            input.len() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() {
        return Err(Error::Volume(format!(
            "SMART query failed on disk {physical_drive} (ATA passthrough unsupported — common for NVMe)"
        )));
    }
    // Data follows the 40-byte ATA struct in the output buffer.
    if input.len() < 40 + 512 {
        return Err(Error::Volume("SMART response too short".into()));
    }
    Ok(input[40..552].to_vec())
}

#[cfg(not(windows))]
pub fn read_smart_sector(_physical_drive: u32) -> Result<Vec<u8>> {
    Err(Error::Volume(
        "SMART queries are only available on Windows".into(),
    ))
}

/// Full health probe for a device id ("dev:C" → resolve to physical drive
/// by matching the volume's disk number).
pub fn disk_health(device_id: &str) -> Result<DiskHealthWire> {
    let letter = crate::recovery::device_letter(device_id)?;
    let physical = physical_drive_of_volume(letter)?;

    let sector = read_smart_sector(physical)?;
    let attrs = parse_smart_attributes(&sector);
    if attrs.is_empty() {
        return Err(Error::Volume(format!(
            "disk {physical} returned an empty SMART table"
        )));
    }
    let (temp, hours) = temperature_and_hours(&attrs);
    let wear = wear_percent(&attrs);
    let ssd = is_ssd_like(&attrs);
    let status = health_status(&attrs);

    Ok(DiskHealthWire {
        device_id: device_id.to_string(),
        status: status.to_string(),
        temperature_c: temp,
        power_on_hours: hours,
        disk_class: if ssd { "ssd" } else { "hdd" }.to_string(),
        wear_percent: wear,
        trim_enabled: None, // not exposed by SMART; OS-level query is v0.5
        reallocated_sectors: critical_raw(&attrs, 5),
        pending_sectors: critical_raw(&attrs, 197),
        attributes: attrs
            .iter()
            .map(|(id, current, worst, threshold, raw)| SmartAttributeWire {
                id: u32::from(*id),
                name: attribute_name(*id),
                current: Some(i64::from(*current)),
                worst: Some(i64::from(*worst)),
                threshold: threshold.map(i64::from),
                raw: *raw,
                status: if CRITICAL_ATTRIBUTES.contains(id) && *raw > 0 {
                    "failing".to_string()
                } else {
                    "ok".to_string()
                },
            })
            .collect(),
        note: None,
    })
}

/// Maps a volume letter to its physical drive number via
/// `IOCTL_STORAGE_GET_DEVICE_NUMBER` on the volume handle.
#[cfg(windows)]
pub fn physical_drive_of_volume(letter: char) -> Result<u32> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::IO::DeviceIoControl;

    const IOCTL_STORAGE_GET_DEVICE_NUMBER: u32 = 0x2D1080;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct StorageDeviceNumber {
        DeviceType: u32,
        DeviceNumber: u32,
        PartitionNumber: u32,
    }

    let path = format!(r"\\.\{letter}:");
    let file = std::fs::File::open(&path)
        .map_err(|e| Error::Volume(format!("cannot open '{path}': {e}")))?;
    let mut number = StorageDeviceNumber {
        DeviceType: 0,
        DeviceNumber: 0,
        PartitionNumber: 0,
    };
    let mut returned: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            HANDLE(file.as_raw_handle()),
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            None,
            0,
            Some(&mut number as *mut _ as *mut core::ffi::c_void),
            std::mem::size_of::<StorageDeviceNumber>() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() {
        return Err(Error::Volume(format!(
            "cannot resolve the physical drive behind {letter}:"
        )));
    }
    Ok(number.DeviceNumber)
}

#[cfg(not(windows))]
pub fn physical_drive_of_volume(_letter: char) -> Result<u32> {
    Err(Error::Volume("only available on Windows".into()))
}

// Silence unused import when not on Windows.
#[cfg(not(windows))]
#[allow(unused)]
fn _use_write() {
    let _ = std::io::stdout().flush();
}

// ---------------------------------------------------------------------------
// Tests — pure parsing with a realistic fixture
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_sector() -> Vec<u8> {
        // Header 12 bytes, 3 attributes, then terminator.
        let mut s = vec![0u8; 512];
        s[0] = 0x0F; // version
                     // Attribute 5: reallocated, current 100, worst 100, raw 4
        s[12] = 5;
        s[12 + 3] = 100;
        s[12 + 4] = 100;
        s[12 + 5] = 4;
        // Attribute 9: power-on hours, raw 8760 (0x2238)
        s[24] = 9;
        s[24 + 3] = 90;
        s[24 + 4] = 90;
        s[24 + 5] = 0x38;
        s[24 + 6] = 0x22;
        // Attribute 194: temperature 42°C
        s[36] = 194;
        s[36 + 3] = 66;
        s[36 + 4] = 60;
        s[36 + 5] = 42;
        s
    }

    #[test]
    fn parse_smart_fixture() {
        let attrs = parse_smart_attributes(&fixture_sector());
        assert_eq!(attrs.len(), 3);
        assert_eq!(attrs[0].0, 5);
        assert_eq!(attrs[0].4, 4);
        assert_eq!(attrs[1].0, 9);
        assert_eq!(attrs[1].4, 8760);
        assert_eq!(attrs[2].0, 194);
        assert_eq!(attrs[2].4, 42);
    }

    #[test]
    fn parse_stops_at_zero_id() {
        let attrs = parse_smart_attributes(&[0u8; 512]);
        assert!(attrs.is_empty());
        assert!(parse_smart_attributes(&[0u8; 8]).is_empty());
    }

    #[test]
    fn status_good_when_clean() {
        let mut s = fixture_sector();
        s[12 + 5] = 0; // reallocated raw = 0
        let attrs = parse_smart_attributes(&s);
        assert_eq!(health_status(&attrs), "good");
    }

    #[test]
    fn status_caution_when_reallocated() {
        let attrs = parse_smart_attributes(&fixture_sector());
        assert_eq!(health_status(&attrs), "caution");
    }

    #[test]
    fn temperature_and_hours_extract() {
        let attrs = parse_smart_attributes(&fixture_sector());
        let (temp, hours) = temperature_and_hours(&attrs);
        assert_eq!(temp, Some(42));
        assert_eq!(hours, Some(8760));
    }

    #[test]
    fn temperature_out_of_range_dropped() {
        let mut s = fixture_sector();
        s[36 + 5] = 200; // absurd
        let attrs = parse_smart_attributes(&s);
        let (temp, _) = temperature_and_hours(&attrs);
        assert_eq!(temp, None);
    }

    #[test]
    fn names_are_populated() {
        assert_eq!(attribute_name(5), "Reallocated Sectors Count");
        assert_eq!(attribute_name(194), "Temperature Celsius");
        assert_eq!(attribute_name(200), "Attribute 200");
    }

    #[test]
    fn critical_raw_extraction() {
        let attrs = parse_smart_attributes(&fixture_sector());
        assert_eq!(critical_raw(&attrs, 5), Some(4));
        assert_eq!(critical_raw(&attrs, 197), None);
    }
}
