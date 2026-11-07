// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lock Owner Detection — Windows Restart Manager (Bölüm 34.4 alternatifi).
//
// Spec NtQuerySystemInformation öneriyor; v0.1'de **Restart Manager** seçildi:
//   * Documented Win32 API (Microsoft'un kendi "What's using this file?"
//     diyalogları RM kullanır — Sysinternals Handle.exe'ye eşdeğer).
//   * Tek dosya için PID + process display name listesi döner.
//   * Admin yetkisi GEREKMEZ (NtQuerySystemInformation handle table tam
//     dump için elevated olmak ister).
//
// v0.2'de NtQuerySystemInformation ile derin handle table forensics
// (Bölüm 34.4) eklenebilir — RM yetersiz kalan kenarlar için.

use crate::error::{Error, Result};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LockOwner {
    pub pid: u32,
    pub process_name: String,
    pub service_short_name: Option<String>,
    pub restartable: bool,
}

#[cfg(windows)]
pub fn find_lock_owners(path: &std::path::Path) -> Result<Vec<LockOwner>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows::Win32::System::RestartManager::{
        RmEndSession, RmGetList, RmRegisterResources, RmStartSession, RM_PROCESS_INFO,
    };

    // 1. Session aç. strSessionKey 32 wchar + null.
    let mut session_handle: u32 = 0;
    let mut session_key: [u16; 33] = [0; 33];
    let start_rc = unsafe {
        RmStartSession(
            &mut session_handle,
            None,
            windows::core::PWSTR(session_key.as_mut_ptr()),
        )
    };
    if start_rc != ERROR_SUCCESS {
        return Err(Error::LockedFile(format!(
            "RmStartSession başarısız: Win32 code {}",
            start_rc.0
        )));
    }

    // RAII benzeri: session_handle bu fonksiyondan çıkmadan kapatılır.
    let result = (|| -> Result<Vec<LockOwner>> {
        // 2. Dosyayı kaydet.
        let wide_path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let file_array: [PCWSTR; 1] = [PCWSTR(wide_path.as_ptr())];
        let reg_rc = unsafe { RmRegisterResources(session_handle, Some(&file_array), None, None) };
        if reg_rc != ERROR_SUCCESS {
            return Err(Error::LockedFile(format!(
                "RmRegisterResources başarısız: Win32 code {}",
                reg_rc.0
            )));
        }

        // 3. RmGetList — iki turlu: önce boyut, sonra dolu liste.
        //    Gemini review B11 — probe ile fetch arasında process listesi
        //    büyürse fetch de ERROR_MORE_DATA dönebilir; sınırlı retry
        //    (3 deneme) ile toparlanır.
        let mut needed: u32 = 0;
        let mut count: u32 = 0;
        let mut reboot_reasons: u32 = 0;

        // İlk çağrı: pnProcInfo = 0 → MORE_DATA + pnProcInfoNeeded set olur.
        let probe_rc = unsafe {
            RmGetList(
                session_handle,
                &mut needed,
                &mut count,
                None,
                &mut reboot_reasons,
            )
        };
        if probe_rc != ERROR_SUCCESS && probe_rc != ERROR_MORE_DATA {
            return Err(Error::LockedFile(format!(
                "RmGetList (probe) başarısız: Win32 code {}",
                probe_rc.0
            )));
        }
        if needed == 0 {
            // Hiçbir process tutmuyor — temiz.
            return Ok(Vec::new());
        }

        let mut buffer: Vec<RM_PROCESS_INFO> = Vec::new();
        let mut fetch_rc = ERROR_SUCCESS;
        for _attempt in 0..3 {
            buffer = vec![unsafe { std::mem::zeroed() }; needed as usize];
            count = needed;
            fetch_rc = unsafe {
                RmGetList(
                    session_handle,
                    &mut needed,
                    &mut count,
                    Some(buffer.as_mut_ptr()),
                    &mut reboot_reasons,
                )
            };
            if fetch_rc == ERROR_SUCCESS {
                break;
            }
            if fetch_rc == ERROR_MORE_DATA && needed > 0 {
                // Liste yine büyüdü — yeni needed ile tekrar dene.
                continue;
            }
            return Err(Error::LockedFile(format!(
                "RmGetList (fetch) başarısız: Win32 code {}",
                fetch_rc.0
            )));
        }
        if fetch_rc != ERROR_SUCCESS {
            return Err(Error::LockedFile(format!(
                "RmGetList (fetch) retry tükendi: Win32 code {}",
                fetch_rc.0
            )));
        }

        buffer.truncate(count as usize);
        Ok(buffer.into_iter().map(rm_info_to_owner).collect())
    })();

    let _ = unsafe { RmEndSession(session_handle) };
    result
}

#[cfg(windows)]
fn rm_info_to_owner(info: windows::Win32::System::RestartManager::RM_PROCESS_INFO) -> LockOwner {
    let app_name = wide_to_string(&info.strAppName);
    let svc_name = wide_to_string(&info.strServiceShortName);
    LockOwner {
        pid: info.Process.dwProcessId,
        process_name: if app_name.is_empty() {
            format!("PID {}", info.Process.dwProcessId)
        } else {
            app_name
        },
        service_short_name: if svc_name.is_empty() {
            None
        } else {
            Some(svc_name)
        },
        restartable: info.bRestartable.as_bool(),
    }
}

#[cfg(windows)]
fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

#[cfg(not(windows))]
pub fn find_lock_owners(_path: &std::path::Path) -> Result<Vec<LockOwner>> {
    Err(Error::LockedFile(
        "lock owner detection yalnızca Windows hedefinde desteklenir".into(),
    ))
}
