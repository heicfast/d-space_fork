// SPDX-License-Identifier: GPL-3.0-or-later
//
// Volume Pre-Flight Status Check — Master architecture Chapter 33.2 (v1.4 fix).
//
// The volume status is queried BEFORE any scan ATTEMPT is made. Added as
// Layer 0 so that Layers A/B/C do not retry endlessly on a BitLocker-locked
// drive (v1.4 race condition fix).

pub mod enumerate;
pub mod hardware;
pub mod preflight;

pub use enumerate::{enumerate_drive_letters, list_drives};
pub use hardware::{probe_drive_hardware, DriveHardware};
pub use preflight::pre_flight_check;

use serde::Serialize;

/// Strict drive-letter parser — accepts "C", "c:", "C:\\" (and "C:\\alt\\path")
/// but rejects UNC (`\\\\server\\share`), rooted (`\\foo`), empty, or non-letter
/// inputs. Gemini review: the old `chars().find(alphabetic)` pattern silently
/// interpreted `\\\\fileserver\\public` input as `F:` (risk of scanning the
/// wrong volume's MFT). This strict validation is used at every command
/// boundary.
pub fn parse_drive_letter(input: &str) -> Option<char> {
    let trimmed = input.trim();
    let b = trimmed.as_bytes();
    if b.is_empty() {
        return None;
    }
    // UNC or rooted path — never a drive letter.
    if b[0] == b'\\' || b[0] == b'/' {
        return None;
    }
    if !b[0].is_ascii_alphabetic() {
        return None;
    }
    match b.len() {
        1 => Some(b[0] as char),
        _ if b[1] == b':' => Some(b[0] as char), // "C:" / "C:\\..."
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum VolumeStatus {
    Ready,
    BitLockerLocked,
    BitLockerSuspended,
    Encrypted { method: String },
    NotMounted,
    AccessDenied,
    NotFormatted,
    UnsupportedDriveType { drive_type: u32 },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum DriveKind {
    Unknown,
    NoRootDir,
    Removable,
    Fixed,
    Remote,
    CdRom,
    RamDisk,
}

impl DriveKind {
    /// Maps the Win32 GetDriveTypeW return value.
    pub fn from_win32(code: u32) -> Self {
        match code {
            0 => Self::Unknown,
            1 => Self::NoRootDir,
            2 => Self::Removable,
            3 => Self::Fixed,
            4 => Self::Remote,
            5 => Self::CdRom,
            6 => Self::RamDisk,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VolumeInfo {
    pub drive_letter: String,
    pub root_path: String,
    pub file_system: String,
    pub volume_label: String,
    pub volume_serial: u32,
    pub drive_kind: DriveKind,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub status: VolumeStatus,
    pub elapsed_ms: u64,
}
