// SPDX-License-Identifier: GPL-3.0-or-later
//
// Volume Enumeration — lists every mounted drive in the system.
// Win32 `GetLogicalDrives` returns a bitmask (bit 0 = A:, bit 25 = Z:). For
// each bit `pre_flight_check` is called; the results are combined. Master
// spec Chapter 33.2 Layer 0 — does not require admin.

use crate::error::Result;
use crate::volume::{pre_flight_check, VolumeInfo};
use tracing::{debug, info};

/// Returns the mounted drive letters via Win32 `GetLogicalDrives`.
/// The result is sorted: A, B, ..., Z. Empty Vec if there are no drives.
#[cfg(windows)]
pub fn enumerate_drive_letters() -> Vec<char> {
    use windows::Win32::Storage::FileSystem::GetLogicalDrives;
    let mask = unsafe { GetLogicalDrives() };
    debug!(mask = format!("{:#034b}", mask), "GetLogicalDrives bitmask");
    let mut out = Vec::new();
    for i in 0..26u32 {
        if (mask & (1u32 << i)) != 0 {
            let letter = char::from(b'A' + i as u8);
            out.push(letter);
        }
    }
    out
}

#[cfg(not(windows))]
pub fn enumerate_drive_letters() -> Vec<char> {
    Vec::new()
}

/// Returns pre-flight info for all drives in the system. If pre_flight_check
/// fails for one drive, that drive is skipped (so the overall list is not
/// broken). The error line goes to tracing.
pub fn list_drives() -> Result<Vec<VolumeInfo>> {
    let letters = enumerate_drive_letters();
    info!(count = letters.len(), "list_drives: enumeration started");
    let mut out = Vec::with_capacity(letters.len());
    for letter in letters {
        let drive = format!("{}:", letter);
        match pre_flight_check(&drive) {
            Ok(info) => out.push(info),
            Err(e) => {
                debug!(drive = %drive, ?e, "pre_flight_check skipped");
            }
        }
    }
    info!(returned = out.len(), "list_drives complete");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn enumerate_returns_at_least_one_letter() {
        // The test machine must have at least one mounted drive (C:).
        let letters = enumerate_drive_letters();
        assert!(!letters.is_empty(), "at least one mounted drive expected");
        // All are ASCII uppercase.
        for c in &letters {
            assert!(c.is_ascii_uppercase());
        }
        // Sorted (ascending) and duplicate-free.
        let mut sorted = letters.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, letters);
    }

    #[cfg(windows)]
    #[test]
    fn list_drives_returns_valid_info() {
        let drives = list_drives().expect("list_drives should succeed");
        // There must be at least one VolumeInfo (C:).
        assert!(!drives.is_empty(), "at least one drive entry expected");
        for info in &drives {
            assert!(!info.drive_letter.is_empty());
            assert!(info.drive_letter.ends_with(':'));
            assert!(info.root_path.ends_with('\\'));
        }
    }
}
