// SPDX-License-Identifier: GPL-3.0-or-later
//
// Privilege probe — Chapter 5.2A Three-Layer Privilege Strategy.
//
// is_elevated() queries the process token elevation flag. It does NOT
// open a UAC dialog; it only reports whether the current process has
// admin rights. This is the Chapter 5.2A Layer 1 principle: do not ask
// for UAC at first launch.

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use std::mem::size_of;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        let info_size = size_of::<TOKEN_ELEVATION>() as u32;

        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            info_size,
            &mut returned,
        );

        let _ = CloseHandle(token);

        result.is_ok() && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}
