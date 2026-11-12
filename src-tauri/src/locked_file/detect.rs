// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lock Detection — CreateFileW share-violation probe.
// Master architecture Chapter 34.1 + 34.5.3.
//
// Principle (Chapter 34.5.1): must NOT be called on the scan-time hot path.
// Only for on-demand drill-down and user-facing diagnostics. CreateFileW
// costs ~0ms but 1M files × ~0ms is still real overhead — it breaks the scan speed budget.
//
// Probe strategy:
//   * try to open with `dwShareMode = 0` (FILE_SHARE_NONE) → if another
//     active handle exists, ERROR_SHARING_VIOLATION.
//   * `dwDesiredAccess = FILE_READ_DATA` — Microsoft exempts `FILE_READ_ATTRIBUTES`
//     from the sharing check ("queried without opening the file"); the
//     minimal access that actually triggers a sharing violation is FILE_READ_DATA.
//   * with `FILE_FLAG_BACKUP_SEMANTICS` folders can also be opened (future use).
//   * The handle is closed immediately after opening — state check only.

use crate::error::Result;
use std::path::Path;
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LockState {
    /// File opened; no other active handles.
    Free,
    /// ERROR_SHARING_VIOLATION — another process holds it.
    Locked,
    /// ERROR_ACCESS_DENIED — an ACL is blocking (lock is a separate category).
    AccessDenied,
    /// File not found (ERROR_FILE_NOT_FOUND / ERROR_PATH_NOT_FOUND).
    NotFound,
    /// Other Win32 error — the code is reported.
    OtherError(u32),
}

#[cfg(windows)]
pub fn probe_lock_state(path: &Path) -> Result<LockState> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND,
        ERROR_SHARING_VIOLATION,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_DATA,
        FILE_SHARE_MODE, OPEN_EXISTING,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let pcwstr = PCWSTR(wide.as_ptr());
    let share = FILE_SHARE_MODE(0); // FILE_SHARE_NONE → lock probe

    let result = unsafe {
        CreateFileW(
            pcwstr,
            FILE_READ_DATA.0,
            share,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    };

    match result {
        Ok(handle) => {
            let _ = unsafe { CloseHandle(handle) };
            debug!(path = %path.display(), "lock probe = Free");
            Ok(LockState::Free)
        }
        Err(e) => {
            // Gemini review B3 — windows-rs `CreateFileW` already captures the
            // error code when building the Result; if `GetLastError()` is
            // called later, the value may have been clobbered by the error
            // path. Reliable source: `e.code()` (HRESULT). If FACILITY_WIN32
            // (0x0007), the low 16 bits are the real Win32 error code.
            let hr = e.code().0 as u32;
            let code = if (hr & 0xFFFF_0000) == 0x0007_0000 || (hr & 0xFFFF_0000) == 0x8007_0000 {
                hr & 0xFFFF
            } else {
                hr
            };
            let state = match code {
                c if c == ERROR_SHARING_VIOLATION.0 => LockState::Locked,
                c if c == ERROR_ACCESS_DENIED.0 => LockState::AccessDenied,
                c if c == ERROR_FILE_NOT_FOUND.0 || c == ERROR_PATH_NOT_FOUND.0 => {
                    LockState::NotFound
                }
                other => LockState::OtherError(other),
            };
            debug!(path = %path.display(), ?state, code, "lock probe result");
            Ok(state)
        }
    }
}

/// Non-Windows test/build path — not supported.
#[cfg(not(windows))]
pub fn probe_lock_state(_path: &Path) -> Result<LockState> {
    Err(crate::error::Error::LockedFile(
        "lock detection is only supported on Windows targets".into(),
    ))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::windows::fs::OpenOptionsExt;

    #[test]
    fn free_file_is_free() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("free.bin");
        fs::write(&p, b"hello").unwrap();
        assert_eq!(probe_lock_state(&p).unwrap(), LockState::Free);
    }

    #[test]
    fn exclusive_open_is_locked() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("busy.bin");
        fs::write(&p, b"busy").unwrap();

        // Open with dwShareMode=0 — the lock is visible to others.
        let _holder = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&p)
            .expect("exclusive open");

        assert_eq!(probe_lock_state(&p).unwrap(), LockState::Locked);
    }

    #[test]
    fn missing_path_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("ghost.bin");
        assert_eq!(probe_lock_state(&p).unwrap(), LockState::NotFound);
    }
}
