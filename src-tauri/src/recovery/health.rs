// SPDX-License-Identifier: GPL-3.0-or-later
//
// Disk health — DiskWizard recovery engine.
//
// SMART data through three graded paths, tried in order:
//   1. NVMe health log page via `IOCTL_STORAGE_QUERY_PROPERTY`
//      (StorageDeviceProtocolSpecificProperty) — the native path for
//      modern NVMe drives, which by design cannot answer ATA passthrough.
//   2. SATA/ATA `SMART READ DATA` (+ `READ ATTRIBUTE THRESHOLDS`) via a
//      correct `ATA_PASS_THROUGH_EX` on the physical drive handle.
//   3. `IOCTL_STORAGE_PREDICT_FAILURE` — many drivers (notably USB
//      bridges that block passthrough) still return the 512-byte SMART
//      sector in `VendorStatus`.
// Temperature additionally falls back to `StorageTemperatureProperty`,
// TRIM to `StorageDeviceTrimProperty`. Drives that answer none of these
// degrade honestly to the contract's `health-unavailable` error.
//
// The pure parsers (attribute sector, threshold sector, NVMe health log)
// are fixture-tested and host-independent.

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

/// Attribute tuple: (id, current, worst, threshold, raw).
pub type SmartAttr = (u8, u8, u8, Option<u8>, u64);

/// Parses the 362-byte attribute table out of a SMART READ DATA sector.
/// Layout: 12 bytes header, then 12 bytes per attribute (id, flags, value
/// [current], worst, raw[6] + reserved), id 0 terminates.
pub fn parse_smart_attributes(sector: &[u8]) -> Vec<SmartAttr> {
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

/// Parses a SMART READ ATTRIBUTE THRESHOLDS sector (the 0xD1 reply):
/// 12-byte header, then 12 bytes per entry `(id, threshold, [10 reserved])`.
pub fn parse_smart_thresholds(sector: &[u8]) -> Vec<(u8, u8)> {
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
        out.push((id, sector[off + 1]));
    }
    out
}

/// Merges a threshold table into parsed attributes (id-matched).
pub fn merge_thresholds(mut attrs: Vec<SmartAttr>, thresholds: &[(u8, u8)]) -> Vec<SmartAttr> {
    for attr in attrs.iter_mut() {
        if let Some((_, th)) = thresholds.iter().find(|(id, _)| *id == attr.0) {
            attr.3 = Some(*th);
        }
    }
    attrs
}

/// Attributes whose nonzero raw values indicate degradation.
const CRITICAL_ATTRIBUTES: &[u8] = &[5, 187, 196, 197, 198];

/// Derives the health verdict from parsed attributes.
pub fn health_status(attrs: &[SmartAttr]) -> &'static str {
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
pub fn temperature_and_hours(attrs: &[SmartAttr]) -> (Option<i64>, Option<i64>) {
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
pub fn wear_percent(attrs: &[SmartAttr]) -> Option<i64> {
    if let Some((_, _, _, _, raw)) = attrs.iter().find(|(id, ..)| *id == 231) {
        return Some(100 - (*raw & 0xFF) as i64);
    }
    if let Some((_, cur, _, _, _)) = attrs.iter().find(|(id, ..)| *id == 177) {
        return Some(100 - *cur as i64);
    }
    None
}

pub fn is_ssd_like(attrs: &[SmartAttr]) -> bool {
    attrs
        .iter()
        .any(|(id, ..)| matches!(id, 170 | 171 | 172 | 173 | 177 | 178 | 231 | 232 | 233))
}

pub fn critical_raw(attrs: &[SmartAttr], id: u8) -> Option<i64> {
    attrs
        .iter()
        .find(|(a, ..)| *a == id)
        .map(|(_, _, _, _, raw)| *raw as i64)
}

// ---------------------------------------------------------------------------
// NVMe health log (pure parsing)
// ---------------------------------------------------------------------------

/// Parsed NVMe SMART/Health log page (the 512-byte page 0x02).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NvmeHealth {
    pub critical_warning: u8,
    pub composite_temp_k: u16,
    pub available_spare_pct: u8,
    pub available_spare_threshold_pct: u8,
    pub percentage_used: u8,
    pub data_units_read: u64,
    pub data_units_written: u64,
    pub power_on_hours: u64,
    pub power_cycles: u64,
    pub unsafe_shutdowns: u64,
    pub media_errors: u64,
    pub error_log_entries: u64,
}

fn u128_le_at(b: &[u8], o: usize) -> u64 {
    // NVMe 128-bit counters are little-endian; 64 bits cover any real drive.
    b.get(o..o + 8)
        .map(|s| u64::from_le_bytes(s.try_into().unwrap_or([0; 8])))
        .unwrap_or(0)
}

/// Parses the 512-byte NVMe health log page.
pub fn parse_nvme_health(log: &[u8]) -> Option<NvmeHealth> {
    if log.len() < 192 {
        return None;
    }
    Some(NvmeHealth {
        critical_warning: log[0],
        composite_temp_k: u16::from_le_bytes([log[1], log[2]]),
        available_spare_pct: log[3],
        available_spare_threshold_pct: log[4],
        percentage_used: log[5],
        data_units_read: u128_le_at(log, 32),
        data_units_written: u128_le_at(log, 48),
        power_on_hours: u128_le_at(log, 128),
        power_cycles: u128_le_at(log, 112),
        unsafe_shutdowns: u128_le_at(log, 144),
        media_errors: u128_le_at(log, 160),
        error_log_entries: u128_le_at(log, 176),
    })
}

impl NvmeHealth {
    /// Composite temperature in °C (0 when the sensor reads 0 K = absent).
    pub fn temperature_c(&self) -> Option<i64> {
        if self.composite_temp_k == 0 {
            return None;
        }
        let c = i64::from(self.composite_temp_k) - 273;
        (0..=127).contains(&c).then_some(c)
    }

    /// Verdict from the spec's critical-warning bits + spare accounting.
    pub fn status(&self) -> &'static str {
        // bit 0: available spare below threshold; bit 2: media read-only.
        if self.critical_warning & 0x05 != 0 {
            return "bad";
        }
        // bit 1: temperature over threshold; bit 3: backup device failed;
        // bit 4: media errors present.
        if self.critical_warning & 0x1A != 0 {
            return "caution";
        }
        if self.available_spare_pct < self.available_spare_threshold_pct {
            return "bad";
        }
        if self.percentage_used >= 100 {
            return "caution";
        }
        "good"
    }

    /// Synthesizes the SMART-shaped attribute table for the existing UI.
    /// Synthetic ids live in a distinct range (0x100+) so they cannot be
    /// confused with ATA attribute ids (which are u8).
    pub fn to_attribute_wires(&self) -> Vec<SmartAttributeWire> {
        let entry = |id: u32, current: Option<i64>, raw: u64, failing: bool| SmartAttributeWire {
            id,
            name: nvme_attribute_name(id)
                .map(str::to_string)
                .unwrap_or_else(|| format!("Attribute {id:#04x}")),
            current,
            worst: None,
            threshold: None,
            raw,
            status: if failing { "failing" } else { "ok" }.to_string(),
        };
        vec![
            entry(
                0x100,
                Some(i64::from(self.available_spare_pct)),
                u64::from(self.available_spare_pct),
                self.available_spare_pct < self.available_spare_threshold_pct,
            ),
            entry(
                0x101,
                Some(i64::from(self.available_spare_threshold_pct)),
                u64::from(self.available_spare_threshold_pct),
                false,
            ),
            entry(
                0x102,
                Some(i64::from(self.percentage_used)),
                u64::from(self.percentage_used),
                self.percentage_used >= 100,
            ),
            entry(0x103, None, self.data_units_read, false),
            entry(0x104, None, self.data_units_written, false),
            entry(
                0x105,
                Some(self.power_on_hours as i64),
                self.power_on_hours,
                false,
            ),
            entry(
                0x106,
                Some(self.power_cycles as i64),
                self.power_cycles,
                false,
            ),
            entry(0x107, None, self.unsafe_shutdowns, false),
            entry(0x108, None, self.media_errors, self.media_errors > 0),
            entry(
                0x109,
                None,
                self.error_log_entries,
                self.error_log_entries > 0,
            ),
        ]
    }
}

/// Name for a synthetic NVMe attribute id (0x100+ range).
pub fn nvme_attribute_name(id: u32) -> Option<&'static str> {
    Some(match id {
        0x100 => "Available Spare (NVMe)",
        0x101 => "Available Spare Threshold (NVMe)",
        0x102 => "Percentage Used (NVMe)",
        0x103 => "Data Units Read (NVMe)",
        0x104 => "Data Units Written (NVMe)",
        0x105 => "Power On Hours (NVMe)",
        0x106 => "Power Cycles (NVMe)",
        0x107 => "Unsafe Shutdowns (NVMe)",
        0x108 => "Media and Data Integrity Errors (NVMe)",
        0x109 => "Error Information Log Entries (NVMe)",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// ATA PASS THROUGH (correct layout)
// ---------------------------------------------------------------------------

/// `ATA_PASS_THROUGH_EX` exactly as winioctl.h defines it on x64
/// (48 bytes). The previous hand-rolled struct had the flag fields in the
/// wrong slots, a misaligned u32, `DataBufferOffset` set to an absolute
/// pointer instead of a struct-relative offset, and only 40 of the 48
/// bytes were copied into the IOCTL buffer — the SMART command registers
/// never reached the driver, so every ATA query failed.
#[repr(C)]
struct AtaPassThroughEx {
    length: u16,                 // 0
    ata_flags: u16,              // 2
    path_id: u16,                // 4
    target_id: u16,              // 6
    lun: u16,                    // 8
    _pad: u16,                   // 10
    data_transfer_length: u32,   // 12
    time_out_value: u32,         // 16
    reserved_as_ulong: u32,      // 20
    data_buffer_offset: usize,   // 24 (ULONG_PTR, struct-relative)
    previous_task_file: [u8; 8], // 32
    current_task_file: [u8; 8],  // 40
}

const _: () = assert!(std::mem::size_of::<AtaPassThroughEx>() == 48);

// ntddscsi.h flag values.
const ATA_FLAGS_DRDY_REQUIRED: u16 = 0x01;
const ATA_FLAGS_DATA_IN: u16 = 0x02;
/// `IOCTL_ATA_PASS_THROUGH` — CTL_CODE(FILE_DEVICE_CONTROLLER=4, 0x040B,
/// METHOD_BUFFERED, FILE_READ_ACCESS | FILE_WRITE_ACCESS).
const IOCTL_ATA_PASS_THROUGH: u32 = 0x0004D02C;

/// SMART command bytes: features=0xD0 (READ DATA) / 0xD1 (THRESHOLDS).
const SMART_CMD: u8 = 0xB0;
const SMART_READ_DATA: u8 = 0xD0;
const SMART_READ_THRESHOLDS: u8 = 0xD1;
const SMART_CYL_LOW: u8 = 0x4F;
const SMART_CYL_HIGH: u8 = 0xC2;

/// Builds the 8-byte current task file for a SMART command.
/// Register order (ATA registers in `CurrentTaskFile`): features,
/// sector count, LBA low, LBA mid, LBA high, device, command, reserved.
pub fn smart_task_file(features: u8) -> [u8; 8] {
    [
        features,
        0x01, // sector count
        0x00, // LBA low
        SMART_CYL_LOW,
        SMART_CYL_HIGH,
        0xA0, // device: master, LBA addressing
        SMART_CMD,
        0x00,
    ]
}

/// Issues a SMART subcommand (0xD0 / 0xD1) through ATA passthrough on the
/// physical drive and returns its 512-byte data sector. `data_buffer_offset`
/// is the offset of the data area from the start of the IOCTL buffer
/// (right after the 48-byte struct), as the API requires.
#[cfg(windows)]
fn ata_smart_read(physical_drive: u32, features: u8) -> Result<Vec<u8>> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::IO::DeviceIoControl;

    let path = format!(r"\\.\PhysicalDrive{physical_drive}");
    let file = std::fs::File::open(&path)
        .map_err(|e| Error::Volume(format!("cannot open '{path}': {e}")))?;

    const STRUCT_LEN: usize = std::mem::size_of::<AtaPassThroughEx>();
    let mut buffer = vec![0u8; STRUCT_LEN + 512];
    let apt = AtaPassThroughEx {
        length: STRUCT_LEN as u16,
        ata_flags: ATA_FLAGS_DRDY_REQUIRED | ATA_FLAGS_DATA_IN,
        path_id: 0,
        target_id: 0,
        lun: 0,
        _pad: 0,
        data_transfer_length: 512,
        time_out_value: 10,
        reserved_as_ulong: 0,
        data_buffer_offset: STRUCT_LEN, // offset from buffer start, NOT a pointer
        previous_task_file: [0; 8],
        current_task_file: smart_task_file(features),
    };
    buffer[..STRUCT_LEN].copy_from_slice(unsafe {
        std::slice::from_raw_parts(&apt as *const AtaPassThroughEx as *const u8, STRUCT_LEN)
    });

    let mut returned: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            HANDLE(file.as_raw_handle()),
            IOCTL_ATA_PASS_THROUGH,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() {
        return Err(Error::Volume(format!(
            "ATA passthrough failed on disk {physical_drive}"
        )));
    }
    Ok(buffer[STRUCT_LEN..].to_vec())
}

// ---------------------------------------------------------------------------
// NVMe / storage property queries
// ---------------------------------------------------------------------------

/// `IOCTL_STORAGE_QUERY_PROPERTY` — CTL_CODE(0x2D, 0x0500, BUFFERED, ANY).
#[cfg(windows)]
const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D1400;
/// `IOCTL_STORAGE_PREDICT_FAILURE` — CTL_CODE(0x2D, 0x0000, BUFFERED, ANY).
#[cfg(windows)]
const IOCTL_STORAGE_PREDICT_FAILURE: u32 = 0x002D0000;

#[cfg(windows)]
const STORAGE_PROPERTY_ID_DEVICE_PROTOCOL_SPECIFIC: u32 = 49; // StorageDeviceProtocolSpecificProperty
#[cfg(windows)]
const STORAGE_PROPERTY_ID_TEMPERATURE: u32 = 71; // StorageTemperatureProperty
#[cfg(windows)]
const STORAGE_PROPERTY_ID_TRIM: u32 = 8; // StorageDeviceTrimProperty
#[cfg(windows)]
const PROTOCOL_TYPE_NVME: u32 = 3;
#[cfg(windows)]
const NVME_DATA_TYPE_LOG_PAGE: u32 = 3;
#[cfg(windows)]
const NVME_LOG_PAGE_HEALTH_INFO: u32 = 0x02;

/// Runs a STORAGE_PROPERTY_QUERY whose additional parameters are a
/// `STORAGE_PROTOCOL_SPECIFIC_DATA` header (NVMe path). Returns the
/// protocol data area.
#[cfg(windows)]
fn nvme_query_log(handle: &std::fs::File, log_page: u32, data_len: usize) -> Result<Vec<u8>> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::IO::DeviceIoControl;

    // Layout: [PropertyId(4)][QueryType(4)][ProtocolSpecificData(40)][data]
    const PD_OFF: usize = 8;
    const PD_LEN: usize = 40;
    let mut buffer = vec![0u8; PD_OFF + PD_LEN + data_len];
    buffer[0..4].copy_from_slice(&STORAGE_PROPERTY_ID_DEVICE_PROTOCOL_SPECIFIC.to_le_bytes());
    buffer[4..8].copy_from_slice(&0u32.to_le_bytes()); // PropertyStandardQuery
    let pd = PD_OFF;
    buffer[pd..pd + 4].copy_from_slice(&PROTOCOL_TYPE_NVME.to_le_bytes());
    buffer[pd + 4..pd + 8].copy_from_slice(&NVME_DATA_TYPE_LOG_PAGE.to_le_bytes());
    buffer[pd + 8..pd + 12].copy_from_slice(&log_page.to_le_bytes());
    buffer[pd + 12..pd + 16].copy_from_slice(&0u32.to_le_bytes()); // sub value
    buffer[pd + 16..pd + 20].copy_from_slice(&(PD_LEN as u32).to_le_bytes()); // data offset (from struct start)
    buffer[pd + 20..pd + 24].copy_from_slice(&(data_len as u32).to_le_bytes());

    let mut returned: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            HANDLE(handle.as_raw_handle()),
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() {
        return Err(Error::Volume("NVMe protocol query failed".into()));
    }
    Ok(buffer[PD_OFF + PD_LEN..].to_vec())
}

/// `IOCTL_STORAGE_PREDICT_FAILURE` fallback: the driver returns
/// `{ PredictFailure: u32, VendorStatus: [u8; 512] }` where many stacks
/// (USB bridges especially) fill VendorStatus with the SMART READ DATA
/// sector. `Ok(None)` means the query worked and the drive is healthy,
/// `Ok(Some(sector))` means the sector was returned.
#[cfg(windows)]
fn predict_failure_smart(handle: &std::fs::File) -> Result<Option<(bool, Vec<u8>)>> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::IO::DeviceIoControl;

    let mut buffer = vec![0u8; 4 + 512];
    let mut returned: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            HANDLE(handle.as_raw_handle()),
            IOCTL_STORAGE_PREDICT_FAILURE,
            None,
            0,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() {
        return Err(Error::Volume("predict-failure query failed".into()));
    }
    let predict = u32::from_le_bytes(buffer[0..4].try_into().unwrap_or([0; 4]));
    Ok(Some((predict != 0, buffer[4..].to_vec())))
}

/// Temperature (°C) through `StorageTemperatureProperty` — works for NVMe
/// and SAT-translated SATA when the SMART attribute table has none.
#[cfg(windows)]
fn query_temperature_c(handle: &std::fs::File) -> Option<i16> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::IO::DeviceIoControl;

    let mut buffer = vec![0u8; 64];
    buffer[0..4].copy_from_slice(&STORAGE_PROPERTY_ID_TEMPERATURE.to_le_bytes());
    buffer[4..8].copy_from_slice(&0u32.to_le_bytes());
    let mut returned: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            HANDLE(handle.as_raw_handle()),
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(buffer.as_mut_ptr().cast()),
            8,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() {
        return None;
    }
    // STORAGE_TEMPERATURE_DATA_DESCRIPTOR: Version(4) Size(4)
    // CriticalTemperature(2) WarningTemperature(2) Reserved(2) pad(2)
    // then STORAGE_TEMPERATURE_INFO[] where info.Temperature is at +2.
    let temp = i16::from_le_bytes([buffer.get(18).copied()?, buffer.get(19).copied()?]);
    (-60..=150).contains(&temp).then_some(temp)
}

/// TRIM state through `StorageDeviceTrimProperty`.
#[cfg(windows)]
fn query_trim_enabled(handle: &std::fs::File) -> Option<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::IO::DeviceIoControl;

    let mut buffer = vec![0u8; 16];
    buffer[0..4].copy_from_slice(&STORAGE_PROPERTY_ID_TRIM.to_le_bytes());
    buffer[4..8].copy_from_slice(&0u32.to_le_bytes());
    let mut returned: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            HANDLE(handle.as_raw_handle()),
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(buffer.as_mut_ptr().cast()),
            8,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() {
        return None;
    }
    // STORAGE_DEVICE_TRIM_DESCRIPTOR { Version(4), Size(4), bTrimEnabled(u8) }
    Some(buffer.get(8).copied().unwrap_or(0) != 0)
}

// ---------------------------------------------------------------------------
// Health assembly
// ---------------------------------------------------------------------------

/// Full health probe for a device id ("dev:C" → resolve to physical drive
/// by matching the volume's disk number). Tries NVMe first, then ATA
/// passthrough, then the predict-failure fallback; a successful
/// temperature query alone is never enough to claim health data.
pub fn disk_health(device_id: &str) -> Result<DiskHealthWire> {
    let letter = crate::recovery::device_letter(device_id)?;
    let physical = physical_drive_of_volume(letter)?;

    let path = format!(r"\\.\PhysicalDrive{physical}");
    let handle = std::fs::File::open(&path)
        .map_err(|e| Error::Volume(format!("cannot open '{path}': {e}")))?;

    // --- Path 1: NVMe health log.
    if let Ok(log) = nvme_query_log(&handle, NVME_LOG_PAGE_HEALTH_INFO, 512) {
        if let Some(nvme) = parse_nvme_health(&log) {
            let temp = nvme.temperature_c();
            let status = nvme.status();
            let wear = i64::from(nvme.percentage_used);
            let trim = query_trim_enabled(&handle);
            return Ok(DiskHealthWire {
                device_id: device_id.to_string(),
                status: status.to_string(),
                temperature_c: temp,
                power_on_hours: Some(nvme.power_on_hours as i64),
                disk_class: "ssd".to_string(),
                wear_percent: Some(wear.min(100)),
                trim_enabled: trim,
                reallocated_sectors: None,
                pending_sectors: Some(nvme.media_errors as i64),
                attributes: nvme.to_attribute_wires(),
                note: Some("NVMe SMART/Health log (controller-reported)".to_string()),
            });
        }
    }

    // --- Path 2: ATA passthrough (data + thresholds).
    let ata = ata_smart_read(physical, SMART_READ_DATA).ok().map(|data| {
        let thresholds = ata_smart_read(physical, SMART_READ_THRESHOLDS).ok();
        (data, thresholds)
    });

    // --- Path 3: predict-failure (USB bridges).
    let (sector, thresholds, predict_failing, note) = match ata {
        Some((data, th)) => (data, th, false, None),
        None => match predict_failure_smart(&handle).ok().flatten() {
            Some((failing, vendor)) => (vendor, None, failing, Some("SMART data via IOCTL_STORAGE_PREDICT_FAILURE (USB/bridge path; thresholds unavailable)".to_string())),
            None => {
                return Err(Error::Volume(format!(
                    "no SMART data path succeeded for disk {physical} (NVMe log, ATA passthrough and predict-failure all unavailable)"
                )))
            }
        },
    };

    let mut attrs = parse_smart_attributes(&sector);
    if attrs.is_empty() {
        return Err(Error::Volume(format!(
            "disk {physical} returned an empty SMART table"
        )));
    }
    if let Some(th) = thresholds.as_deref() {
        attrs = merge_thresholds(attrs, &parse_smart_thresholds(th));
    }
    let (temp, hours) = temperature_and_hours(&attrs);
    let temp = temp.or_else(|| query_temperature_c(&handle).map(i64::from));
    let wear = wear_percent(&attrs);
    let ssd = is_ssd_like(&attrs);
    let trim = query_trim_enabled(&handle);
    let mut status = health_status(&attrs).to_string();
    if predict_failing && status != "bad" {
        status = "bad".to_string();
    }

    Ok(DiskHealthWire {
        device_id: device_id.to_string(),
        status,
        temperature_c: temp,
        power_on_hours: hours,
        disk_class: if ssd { "ssd" } else { "hdd" }.to_string(),
        wear_percent: wear,
        trim_enabled: trim,
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
        note,
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
            "cannot resolve the physical drive behind {letter}: {ok:?}"
        )));
    }
    Ok(number.DeviceNumber)
}

#[cfg(not(windows))]
pub fn physical_drive_of_volume(_letter: char) -> Result<u32> {
    Err(Error::Volume("only available on Windows".into()))
}

// ---------------------------------------------------------------------------
// Tests — pure parsing with realistic fixtures
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
    fn thresholds_parse_and_merge() {
        // Threshold sector: header + (5 -> 36), (9 -> 0), (194 -> 0).
        let mut t = vec![0u8; 512];
        t[12] = 5;
        t[12 + 1] = 36;
        t[24] = 9;
        t[24 + 1] = 0;
        t[36] = 194;
        t[36 + 1] = 0;
        let ths = parse_smart_thresholds(&t);
        assert_eq!(ths, vec![(5, 36), (9, 0), (194, 0)]);

        let attrs = merge_thresholds(parse_smart_attributes(&fixture_sector()), &ths);
        assert_eq!(attrs[0].3, Some(36));
        assert_eq!(attrs[1].3, Some(0));
        assert_eq!(attrs[2].3, Some(0));
    }

    #[test]
    fn status_bad_when_below_threshold() {
        // Attribute 5 current 10 < threshold 36 → bad (previously dead
        // logic — thresholds were never read).
        let mut s = fixture_sector();
        s[12 + 3] = 10; // current
        s[12 + 5] = 0; // raw 0 (so the raw heuristic does not fire first)
        let attrs = merge_thresholds(parse_smart_attributes(&s), &[(5, 36)]);
        assert_eq!(health_status(&attrs), "bad");
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

    // ---- ATA layout ----

    #[test]
    fn ata_struct_layout_is_exact() {
        // Field offsets must match winioctl.h on x64.
        let apt = AtaPassThroughEx {
            length: 48,
            ata_flags: 0x0003,
            path_id: 1,
            target_id: 2,
            lun: 3,
            _pad: 0,
            data_transfer_length: 512,
            time_out_value: 10,
            reserved_as_ulong: 0,
            data_buffer_offset: 48,
            previous_task_file: [0xAA; 8],
            current_task_file: [0xBB; 8],
        };
        assert_eq!(std::mem::size_of::<AtaPassThroughEx>(), 48);
        // data_buffer_offset sits at byte 24; current_task_file at 40.
        let raw = unsafe {
            std::slice::from_raw_parts(
                &apt as *const AtaPassThroughEx as *const u8,
                std::mem::size_of::<AtaPassThroughEx>(),
            )
        };
        // data_buffer_offset = 48 (struct-relative) at bytes 24..32.
        assert_eq!(u64::from_le_bytes(raw[24..32].try_into().unwrap()), 48);
        assert_eq!(raw[40], 0xBB);
        assert_eq!(raw[47], 0xBB);
    }

    #[test]
    fn smart_task_file_register_order() {
        // The command byte 0xB0 must be in slot 6 (command register); the
        // old code put 0xC2 there and 0xB0 in the reserved slot.
        let tf = smart_task_file(SMART_READ_DATA);
        assert_eq!(tf[0], 0xD0); // features
        assert_eq!(tf[1], 0x01); // sector count
        assert_eq!(tf[3], 0x4F); // LBA mid (cyl low)
        assert_eq!(tf[4], 0xC2); // LBA high (cyl high)
        assert_eq!(tf[5], 0xA0); // device
        assert_eq!(tf[6], 0xB0); // command = SMART
        assert_eq!(tf[7], 0x00); // reserved
        assert_eq!(smart_task_file(SMART_READ_THRESHOLDS)[0], 0xD1);
    }

    // ---- NVMe health log ----

    fn nvme_log_fixture() -> [u8; 512] {
        let mut log = [0u8; 512];
        log[0] = 0x00; // no critical warnings
        log[1] = 303u16.to_le_bytes()[0]; // 303 K = 30°C
        log[2] = 303u16.to_le_bytes()[1];
        log[3] = 100; // available spare %
        log[4] = 10; // spare threshold %
        log[5] = 23; // percentage used
        log[32..40].copy_from_slice(&123_456u64.to_le_bytes()); // data units read
        log[48..56].copy_from_slice(&234_567u64.to_le_bytes()); // written
        log[112..120].copy_from_slice(&42u64.to_le_bytes()); // power cycles
        log[128..136].copy_from_slice(&8760u64.to_le_bytes()); // power on hours
        log[144..152].copy_from_slice(&7u64.to_le_bytes()); // unsafe shutdowns
        log[160..168].copy_from_slice(&0u64.to_le_bytes()); // media errors
        log[176..184].copy_from_slice(&3u64.to_le_bytes()); // error log entries
        log
    }

    #[test]
    fn nvme_health_parse() {
        let h = parse_nvme_health(&nvme_log_fixture()).unwrap();
        assert_eq!(h.temperature_c(), Some(30));
        assert_eq!(h.available_spare_pct, 100);
        assert_eq!(h.percentage_used, 23);
        assert_eq!(h.power_on_hours, 8760);
        assert_eq!(h.power_cycles, 42);
        assert_eq!(h.unsafe_shutdowns, 7);
        assert_eq!(h.media_errors, 0);
        assert_eq!(h.error_log_entries, 3);
        assert_eq!(h.status(), "good");
    }

    #[test]
    fn nvme_status_escalations() {
        let mut log = nvme_log_fixture();
        log[0] = 0x01; // available spare below threshold
        assert_eq!(parse_nvme_health(&log).unwrap().status(), "bad");
        log[0] = 0x02; // temperature over threshold
        assert_eq!(parse_nvme_health(&log).unwrap().status(), "caution");
        log[0] = 0x00;
        log[3] = 5; // spare 5% < threshold 10%
        assert_eq!(parse_nvme_health(&log).unwrap().status(), "bad");
        log[3] = 100;
        log[5] = 100; // worn out
        assert_eq!(parse_nvme_health(&log).unwrap().status(), "caution");
    }

    #[test]
    fn nvme_attributes_and_names() {
        let h = parse_nvme_health(&nvme_log_fixture()).unwrap();
        let attrs = h.to_attribute_wires();
        assert_eq!(attrs.len(), 10);
        assert!(attrs.iter().all(|a| a.id >= 0x100));
        assert_eq!(attrs[0].id, 0x100);
        assert_eq!(attrs[0].current, Some(100));
        assert_eq!(attrs[8].id, 0x108);
        assert_eq!(attrs[8].status, "ok");
        assert_eq!(
            nvme_attribute_name(0x108),
            Some("Media and Data Integrity Errors (NVMe)")
        );
        assert_eq!(nvme_attribute_name(5), None);
    }

    #[test]
    fn nvme_rejects_short_log() {
        assert!(parse_nvme_health(&[0u8; 100]).is_none());
    }
}
