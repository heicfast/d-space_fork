// SPDX-License-Identifier: GPL-3.0-or-later
//
// Volume Pre-Flight Status Check — Master mimari Bölüm 33.2 (v1.4 fix).
//
// Tarama DENEMESİ yapılmadan ÖNCE volume statüsü sorgulanır. BitLocker
// kilitli sürücüde Katman A/B/C sonsuz recursive deneme yapmasın diye
// Katman 0 olarak eklendi (v1.4 race condition kapatma).

pub mod enumerate;
pub mod hardware;
pub mod preflight;

pub use enumerate::{enumerate_drive_letters, list_drives};
pub use hardware::{probe_drive_hardware, DriveHardware};
pub use preflight::pre_flight_check;

use serde::Serialize;

/// Katı sürücü harfi ayrıştırıcı — "C", "c:", "C:\\" (ve "C:\\alt\\path")
/// kabul eder; UNC (`\\\\server\\share`), rootlu (`\\foo`), boş veya harf-
/// dışı girdileri reddeder. Gemini review: eski `chars().find(alphabetic)`
/// deseni `\\\\fileserver\\public` girdisini sessizce `F:` olarak yorumluyordu
/// (yanlış volume MFT taraması riski). Tüm komut sınırlarında bu katı
/// doğrulama kullanılır.
pub fn parse_drive_letter(input: &str) -> Option<char> {
    let trimmed = input.trim();
    let b = trimmed.as_bytes();
    if b.is_empty() {
        return None;
    }
    // UNC veya rootlu path — asla drive harfi değil.
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
    /// Win32 GetDriveTypeW dönüş değerini eşler.
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
