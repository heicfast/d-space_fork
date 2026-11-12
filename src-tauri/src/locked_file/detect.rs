// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lock Detection — CreateFileW share-violation probe.
// Master mimari Bölüm 34.1 + 34.5.3.
//
// İlke (Bölüm 34.5.1): scan-time hot path'te çağrılMAZ. Yalnızca on-demand
// drill-down ve user-facing diagnostic için. CreateFileW maliyeti ~0ms ama
// 1M dosyaya × ~0ms hâlâ kayda değer overhead — scan hız bütçesini bozar.
//
// Probe stratejisi:
//   * `dwShareMode = 0` (FILE_SHARE_NONE) ile açmayı dene → başka aktif
//     handle varsa ERROR_SHARING_VIOLATION.
//   * `dwDesiredAccess = FILE_READ_DATA` — Microsoft `FILE_READ_ATTRIBUTES`'ı
//     sharing kontrolünden muaf tutar ("queried without opening the file");
//     gerçek sharing violation tetikleyen minimal access FILE_READ_DATA.
//   * `FILE_FLAG_BACKUP_SEMANTICS` ile klasör de açılabilir (gelecek için).
//   * Açılıştan hemen sonra handle kapatılır — sadece state kontrolü.

use crate::error::Result;
use std::path::Path;
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LockState {
    /// Dosya açıktı, başka aktif handle yok.
    Free,
    /// ERROR_SHARING_VIOLATION — başka bir process tutuyor.
    Locked,
    /// ERROR_ACCESS_DENIED — ACL engelliyor (kilit ayrı bir kategori).
    AccessDenied,
    /// Dosya bulunamadı (ERROR_FILE_NOT_FOUND / ERROR_PATH_NOT_FOUND).
    NotFound,
    /// Diğer Win32 hatası — code raporlanır.
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
            // Gemini review B3 — windows-rs `CreateFileW` hata kodunu Result
            // oluştururken zaten yakalar; `GetLastError()` sonradan çağrılırsa
            // değer error-path tarafından kirletilmiş olabilir. Güvenilir
            // kaynak: `e.code()` (HRESULT). FACILITY_WIN32 (0x0007) ise alt
            // 16 bit gerçek Win32 error kodudur.
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

/// Non-Windows test/derleme yolu — desteklenmez.
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

        // dwShareMode=0 ile aç — başkalarına lock görünür.
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
