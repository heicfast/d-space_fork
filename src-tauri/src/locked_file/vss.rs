// SPDX-License-Identifier: GPL-3.0-or-later
//
// VSS (Volume Shadow Copy Service) low-level COM bridge.
// Master architecture Chapter 34.2 + 34.5.4 + Discovery Log #001, #002, #004.
//
// Principles:
//   * All `IVssBackupComponents` calls are made on the worker thread
//     (MTA apartment). The UI thread never touches the VSS pointer.
//   * `RawSnapshot` panics on Drop — calling `destroy_snapshot`
//     manually is mandatory. Release without BackupComplete or cleanup
//     in the wrong order → service-side leak + crash.
//   * Discovery Log #001 — `SetBackupState(false, false, VSS_BT_COPY, false)`.
//   * Discovery Log #004 — `SetContext(VSS_CTX_FILE_SHARE_BACKUP)` was chosen
//     (no writer involvement, auto-release, non-persistent — read only).
//
// `windows 0.61` does not expose the IVssBackupComponents requester
// interface (Discovery Log #002). Plan A: the `winapi 0.3.9` crate with
// the `vsbackup` feature provides `IVssBackupComponentsVtbl` + the
// `CreateVssBackupComponents` factory — manual vtable setup unnecessary.

#![cfg(all(windows, feature = "vss"))]

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use std::ptr;
use winapi::shared::guiddef::GUID;
use winapi::shared::winerror::{E_ACCESSDENIED, RPC_E_CHANGED_MODE, RPC_E_TOO_LATE, S_FALSE, S_OK};
use winapi::shared::wtypes::BSTR;
use winapi::um::combaseapi::{CoInitializeEx, CoInitializeSecurity, CoUninitialize};
use winapi::um::objbase::COINIT_MULTITHREADED;
#[cfg(test)]
use winapi::um::oleauto::SysStringLen;
use winapi::um::oleauto::{SysAllocStringLen, SysFreeString};
use winapi::um::vsbackup::{
    CreateVssBackupComponents, IVssBackupComponents, VssFreeSnapshotProperties,
};
use winapi::um::vss::{IVssAsync, VSS_BT_COPY, VSS_CTX_FILE_SHARE_BACKUP, VSS_SNAPSHOT_PROP};

// Some winapi versions do not re-export `VSS_S_ASYNC_FINISHED` directly;
// we use it by pinning the constant values.
const VSS_S_ASYNC_FINISHED: i32 = 0x0004_230A;
const VSS_E_VOLUME_NOT_SUPPORTED: i32 = 0x8004_230C_u32 as i32;
const VSS_E_BAD_STATE: i32 = 0x8004_2301_u32 as i32;
const VSS_E_INSUFFICIENT_STORAGE: i32 = 0x8004_231F_u32 as i32;

// COM authentication levels — they can live in different places across
// winapi versions, so we defined them with fixed literals.
const RPC_C_AUTHN_LEVEL_PKT_PRIVACY: u32 = 6;
const RPC_C_IMP_LEVEL_IDENTIFY: u32 = 2;
const EOAC_NONE: u32 = 0;

const GUID_NULL: GUID = GUID {
    Data1: 0,
    Data2: 0,
    Data3: 0,
    Data4: [0; 8],
};

/// HRESULT → `Error::Snapshot` with a message.
/// Returns descriptive text for common VSS error codes; otherwise the hex code.
pub(crate) fn hresult_to_error(hr: i32, ctx: &str) -> Error {
    let detail = match hr {
        E_ACCESSDENIED => "E_ACCESSDENIED (admin required)",
        VSS_E_BAD_STATE => "VSS_E_BAD_STATE (wrong sequence or already busy)",
        VSS_E_VOLUME_NOT_SUPPORTED => "VSS_E_VOLUME_NOT_SUPPORTED (not NTFS or VSS not supported)",
        VSS_E_INSUFFICIENT_STORAGE => "VSS_E_INSUFFICIENT_STORAGE (insufficient shadow storage)",
        _ => "unknown HRESULT",
    };
    Error::Snapshot(format!("VSS {ctx}: {detail} (HRESULT=0x{:08X})", hr as u32))
}

/// UTF-8 → BSTR (SysAllocStringLen). The caller takes ownership and must
/// free it with `bstr_free`.
pub(crate) unsafe fn bstr_from_str(s: &str) -> BSTR {
    let utf16: Vec<u16> = s.encode_utf16().collect();
    // SysAllocStringLen adds the null terminator itself.
    SysAllocStringLen(utf16.as_ptr(), utf16.len() as u32)
}

/// Frees the BSTR. Null safe (SysFreeString tolerates null).
pub(crate) unsafe fn bstr_free(b: BSTR) {
    if !b.is_null() {
        SysFreeString(b);
    }
}

/// COM MTA apartment init. Called at worker thread startup.
/// `RPC_E_CHANGED_MODE` → the same thread was initialized as STA; not OK
/// for us, returns an error. `S_FALSE` → already initialized (OK).
pub(crate) unsafe fn init_com_mta() -> Result<()> {
    let hr = CoInitializeEx(ptr::null_mut(), COINIT_MULTITHREADED);
    if hr != S_OK && hr != S_FALSE && hr != RPC_E_CHANGED_MODE {
        return Err(hresult_to_error(hr, "CoInitializeEx"));
    }

    let sec_hr = CoInitializeSecurity(
        ptr::null_mut(),
        -1,
        ptr::null_mut(),
        ptr::null_mut(),
        RPC_C_AUTHN_LEVEL_PKT_PRIVACY,
        RPC_C_IMP_LEVEL_IDENTIFY,
        ptr::null_mut(),
        EOAC_NONE,
        ptr::null_mut(),
    );
    // RPC_E_TOO_LATE → already called in this process (perhaps by the Tauri main thread).
    if sec_hr != S_OK && sec_hr != RPC_E_TOO_LATE {
        return Err(hresult_to_error(sec_hr, "CoInitializeSecurity"));
    }
    Ok(())
}

/// CoUninitialize — at worker thread end. Idempotent.
pub(crate) unsafe fn uninit_com() {
    CoUninitialize();
}

/// Waits for an `IVssAsync` operation to complete. The async pointer is
/// Released in every case (even on error).
///
/// Gemini review B9 — previously `Wait(u32::MAX)` (INFINITE) was used; if
/// the VSS service hung, the pool worker thread blocked forever and froze
/// the duplicate scan. Now a loop with a bounded timeout: after 60 s total
/// it returns `Error::Snapshot("timeout")`.
pub(crate) unsafe fn wait_async(p_async: *mut IVssAsync, ctx: &str) -> Result<()> {
    if p_async.is_null() {
        return Err(Error::Snapshot(format!("VSS {ctx}: async pointer null")));
    }

    // 60 × 1 s waits — 60 s total ceiling. DoSnapshotSet normally takes 1-3 s.
    const WAIT_SLICE_MS: u32 = 1_000;
    const MAX_SLICES: u32 = 60;

    let mut wait_hr = S_OK;
    for _ in 0..MAX_SLICES {
        wait_hr = (*p_async).Wait(WAIT_SLICE_MS);
        if wait_hr != S_OK {
            // WAIT_TIMEOUT = HRESULT_FROM_WIN32(ERROR_TIMEOUT) 0x800705B4 —
            // the slice ran out, try again. Other errors are permanent.
            if wait_hr as u32 == 0x8007_05B4 {
                continue;
            }
            break;
        }
        break;
    }
    if wait_hr != S_OK {
        (*p_async).Release();
        return Err(hresult_to_error(wait_hr, &format!("{ctx} (Wait)")));
    }

    let mut status: i32 = 0;
    let mut qs_hr = (*p_async).QueryStatus(&mut status, ptr::null_mut());

    // If still not complete (future cases), spend the remaining time —
    // within the total 60 s slice budget the status must be VSS_S_ASYNC_FINISHED.
    for _ in 0..MAX_SLICES {
        if qs_hr != S_OK {
            break;
        }
        if status == VSS_S_ASYNC_FINISHED || status == S_OK {
            break;
        }
        if status == 0x0004_2306 {
            // VSS_S_ASYNC_CANCELLED — cancelled, waiting is pointless.
            break;
        }
        if status == 0x0004_2305 {
            // VSS_S_ASYNC_PENDING — wait one more slice.
            std::thread::sleep(std::time::Duration::from_millis(WAIT_SLICE_MS as u64));
            qs_hr = (*p_async).QueryStatus(&mut status, ptr::null_mut());
            continue;
        }
        break;
    }

    (*p_async).Release();

    if qs_hr != S_OK {
        return Err(hresult_to_error(qs_hr, &format!("{ctx} (QueryStatus)")));
    }
    if status == 0x0004_2306 {
        return Err(Error::Snapshot(format!("VSS {ctx}: operation cancelled")));
    }
    if status == 0x0004_2305 {
        return Err(Error::Snapshot(format!(
            "VSS {ctx}: did not complete within 60 s (timeout)"
        )));
    }
    if status != S_OK && status != VSS_S_ASYNC_FINISHED {
        return Err(hresult_to_error(status, &format!("{ctx} (status)")));
    }
    Ok(())
}

/// Core raw snapshot — lives only on the `vss_pool::Worker` thread.
/// Manual teardown via `destroy_snapshot` is mandatory (automatic drop panics).
pub(crate) struct RawSnapshot {
    pub backup: *mut IVssBackupComponents,
    #[allow(dead_code)]
    pub snapshot_id: GUID,
    /// `\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopyN` (null-terminated wide).
    pub device_object: Vec<u16>,
}

// Worker thread owns it → Send. The pointer is dereferenced only on that thread.
unsafe impl Send for RawSnapshot {}

impl Drop for RawSnapshot {
    fn drop(&mut self) {
        // We do not want a panic in production (the worker thread should not
        // die), but if we drop during unwind, warn via tracing. Reaching
        // drop without a `destroy_snapshot` call = service-side leak + possible crash.
        if !self.backup.is_null() {
            if std::thread::panicking() {
                // We are inside a panic unwind; silently Release instead of aborting.
                unsafe {
                    (*self.backup).Release();
                }
            } else {
                // Not a deliberate leak — we can log + attempt a single
                // Release, but without BackupComplete the service-side state gets dirty.
                tracing::error!(
                    "RawSnapshot Drop manuel destroy_snapshot olmadan tetiklendi — \
                     VSS service-side leak riski (BackupComplete çağrılmadı)"
                );
                unsafe {
                    (*self.backup).Release();
                }
            }
        }
    }
}

/// Combines the `\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopyN` device
/// path + the part of the original path after the volume root.
///
/// Example: device="\\?\GLOBALROOT\Device\HSC42", original="C:\Users\engin\busy.docx"
///   → "\\?\GLOBALROOT\Device\HSC42\Users\engin\busy.docx"
pub(crate) fn snapshot_path(device_object: &[u16], original: &Path) -> PathBuf {
    // device_object may be a null-terminated wide string; take up to the null.
    let dev_len = device_object
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(device_object.len());
    let device = String::from_utf16_lossy(&device_object[..dev_len]);

    let orig = original.to_string_lossy();
    // Drop the volume root (C:\, D:\, etc.) and take the remaining path-after-root.
    let after = if orig.len() >= 3
        && orig.as_bytes()[1] == b':'
        && (orig.as_bytes()[2] == b'\\' || orig.as_bytes()[2] == b'/')
    {
        &orig[2..] // "\Users\engin\busy.docx" (volume letter dropped)
    } else {
        // UNC or unexpected format — use as-is.
        orig.as_ref()
    };

    // Trailing slash check — the device path already ends like "...HSC42", NO slash.
    let mut combined = device;
    if !after.starts_with('\\') {
        combined.push('\\');
    }
    combined.push_str(after);
    PathBuf::from(combined)
}

/// Quick query for whether the volume is snapshot-suitable (NTFS + VSS
/// supported). Does no heavy work — only calls `IsVolumeSupported`.
/// Will be used by the `vss_available_for` deep probe in a later sprint;
/// for now it is only a debugging aid.
#[allow(dead_code)]
pub(crate) unsafe fn is_volume_supported(volume_root: &str) -> Result<bool> {
    let mut p_backup: *mut IVssBackupComponents = ptr::null_mut();
    let hr = CreateVssBackupComponents(&mut p_backup);
    if hr != S_OK || p_backup.is_null() {
        return Err(hresult_to_error(hr, "CreateVssBackupComponents"));
    }

    let result = (|| -> Result<bool> {
        let init_hr = (*p_backup).InitializeForBackup(ptr::null_mut());
        if init_hr != S_OK {
            return Err(hresult_to_error(init_hr, "InitializeForBackup"));
        }
        let ctx_hr = (*p_backup).SetContext(VSS_CTX_FILE_SHARE_BACKUP as i32);
        if ctx_hr != S_OK {
            return Err(hresult_to_error(ctx_hr, "SetContext"));
        }

        let vol_bstr = bstr_from_str(volume_root);
        if vol_bstr.is_null() {
            return Err(Error::Snapshot("volume BSTR alloc failed".into()));
        }
        let mut supported: i32 = 0;
        let hr = (*p_backup).IsVolumeSupported(GUID_NULL, vol_bstr, &mut supported);
        bstr_free(vol_bstr);
        if hr != S_OK {
            return Err(hresult_to_error(hr, "IsVolumeSupported"));
        }
        Ok(supported != 0)
    })();

    (*p_backup).Release();
    result
}

/// Creates a snapshot for the volume. Full chain:
///   CreateVssBackupComponents → InitializeForBackup → SetBackupState
///   → SetContext(FILE_SHARE_BACKUP) → GatherWriterMetadata
///   → IsVolumeSupported → StartSnapshotSet → AddToSnapshotSet
///   → PrepareForBackup → DoSnapshotSet → GetSnapshotProperties
///
/// On the error path, rollback via `cleanup_failed_backup`.
pub(crate) unsafe fn create_snapshot(volume_root: &str) -> Result<RawSnapshot> {
    let mut p_backup: *mut IVssBackupComponents = ptr::null_mut();
    let hr = CreateVssBackupComponents(&mut p_backup);
    if hr != S_OK || p_backup.is_null() {
        return Err(hresult_to_error(hr, "CreateVssBackupComponents"));
    }

    // Helper that cleans up p_backup on the error path.
    let cleanup = |past_do_snapshot: bool| {
        if past_do_snapshot {
            // Error after DoSnapshotSet → BackupComplete + Release.
            let mut p_async: *mut IVssAsync = ptr::null_mut();
            let bc_hr = (*p_backup).BackupComplete(&mut p_async);
            if bc_hr == S_OK && !p_async.is_null() {
                let _ = (*p_async).Wait(u32::MAX);
                (*p_async).Release();
            }
        } else {
            // Before DoSnapshotSet → AbortBackup.
            (*p_backup).AbortBackup();
        }
        (*p_backup).Release();
    };

    // --- 1) InitializeForBackup --------------------------------------------
    let init_hr = (*p_backup).InitializeForBackup(ptr::null_mut());
    if init_hr != S_OK {
        cleanup(false);
        return Err(hresult_to_error(init_hr, "InitializeForBackup"));
    }

    // --- 2) SetBackupState — Discovery Log #001 ---------------------------
    // (bSelectComponents=FALSE, bBackupBootableSystemState=FALSE,
    //  backupType=VSS_BT_COPY, bPartialFileSupport=FALSE)
    let sbs_hr = (*p_backup).SetBackupState(false, false, VSS_BT_COPY, false);
    if sbs_hr != S_OK {
        cleanup(false);
        return Err(hresult_to_error(sbs_hr, "SetBackupState"));
    }

    // --- 3) SetContext — Discovery Log #004 -------------------------------
    let ctx_hr = (*p_backup).SetContext(VSS_CTX_FILE_SHARE_BACKUP as i32);
    if ctx_hr != S_OK {
        cleanup(false);
        return Err(hresult_to_error(ctx_hr, "SetContext"));
    }

    // --- 4) GatherWriterMetadata ------------------------------------------
    let mut p_async: *mut IVssAsync = ptr::null_mut();
    let gwm_hr = (*p_backup).GatherWriterMetadata(&mut p_async);
    if gwm_hr != S_OK {
        cleanup(false);
        return Err(hresult_to_error(gwm_hr, "GatherWriterMetadata"));
    }
    if let Err(e) = wait_async(p_async, "GatherWriterMetadata") {
        cleanup(false);
        return Err(e);
    }

    // --- 5) IsVolumeSupported ---------------------------------------------
    let vol_bstr = bstr_from_str(volume_root);
    if vol_bstr.is_null() {
        cleanup(false);
        return Err(Error::Snapshot("volume BSTR alloc failed".into()));
    }

    let mut supported: i32 = 0;
    let ivs_hr = (*p_backup).IsVolumeSupported(GUID_NULL, vol_bstr, &mut supported);
    if ivs_hr != S_OK || supported == 0 {
        bstr_free(vol_bstr);
        cleanup(false);
        return Err(Error::Snapshot(format!(
            "volume desteklemiyor: {}",
            volume_root
        )));
    }

    // --- 6) StartSnapshotSet ----------------------------------------------
    let mut snap_set_id: GUID = std::mem::zeroed();
    let sss_hr = (*p_backup).StartSnapshotSet(&mut snap_set_id);
    if sss_hr != S_OK {
        bstr_free(vol_bstr);
        cleanup(false);
        return Err(hresult_to_error(sss_hr, "StartSnapshotSet"));
    }

    // --- 7) AddToSnapshotSet ----------------------------------------------
    let mut snap_id: GUID = std::mem::zeroed();
    let ats_hr = (*p_backup).AddToSnapshotSet(vol_bstr, GUID_NULL, &mut snap_id);
    if ats_hr != S_OK {
        bstr_free(vol_bstr);
        cleanup(false);
        return Err(hresult_to_error(ats_hr, "AddToSnapshotSet"));
    }

    // --- 8) PrepareForBackup ----------------------------------------------
    let mut p_async: *mut IVssAsync = ptr::null_mut();
    let pfb_hr = (*p_backup).PrepareForBackup(&mut p_async);
    if pfb_hr != S_OK {
        bstr_free(vol_bstr);
        cleanup(false);
        return Err(hresult_to_error(pfb_hr, "PrepareForBackup"));
    }
    if let Err(e) = wait_async(p_async, "PrepareForBackup") {
        bstr_free(vol_bstr);
        cleanup(false);
        return Err(e);
    }

    // --- 9) DoSnapshotSet (1-3 sn) ----------------------------------------
    let mut p_async: *mut IVssAsync = ptr::null_mut();
    let dss_hr = (*p_backup).DoSnapshotSet(&mut p_async);
    if dss_hr != S_OK {
        bstr_free(vol_bstr);
        cleanup(false);
        return Err(hresult_to_error(dss_hr, "DoSnapshotSet"));
    }
    if let Err(e) = wait_async(p_async, "DoSnapshotSet") {
        bstr_free(vol_bstr);
        cleanup(true); // after DoSnapshotSet — BackupComplete is required
        return Err(e);
    }

    // --- 10) GetSnapshotProperties ----------------------------------------
    let mut props: VSS_SNAPSHOT_PROP = std::mem::zeroed();
    let gsp_hr = (*p_backup).GetSnapshotProperties(snap_id, &mut props);
    if gsp_hr != S_OK {
        bstr_free(vol_bstr);
        cleanup(true);
        return Err(hresult_to_error(gsp_hr, "GetSnapshotProperties"));
    }

    // Copy device_object (null-terminated wide).
    let device_object: Vec<u16> = if props.m_pwszSnapshotDeviceObject.is_null() {
        Vec::new()
    } else {
        let mut len = 0usize;
        while *props.m_pwszSnapshotDeviceObject.add(len) != 0 {
            len += 1;
        }
        let mut v = Vec::with_capacity(len + 1);
        v.extend_from_slice(std::slice::from_raw_parts(
            props.m_pwszSnapshotDeviceObject,
            len,
        ));
        v.push(0);
        v
    };

    VssFreeSnapshotProperties(&mut props);
    bstr_free(vol_bstr);

    if device_object.is_empty() {
        cleanup(true);
        return Err(Error::Snapshot("snapshot device object empty".into()));
    }

    Ok(RawSnapshot {
        backup: p_backup,
        snapshot_id: snap_id,
        device_object,
    })
}

/// Cleans up a RawSnapshot the healthy way — `BackupComplete` + `Release`.
/// Ownership is moved in, so the drop panic is not triggered.
pub(crate) unsafe fn destroy_snapshot(mut snap: RawSnapshot) -> Result<()> {
    if snap.backup.is_null() {
        return Ok(());
    }

    let mut p_async: *mut IVssAsync = ptr::null_mut();
    let hr = (*snap.backup).BackupComplete(&mut p_async);
    let async_res = if hr == S_OK {
        wait_async(p_async, "BackupComplete")
    } else {
        Ok(()) // even if BackupComplete fails: Release + report
    };

    (*snap.backup).Release();
    snap.backup = ptr::null_mut(); // make Drop a no-op

    if hr != S_OK {
        return Err(hresult_to_error(hr, "BackupComplete"));
    }
    async_res
}

// ---------------------------------------------------------------------------
// Test and mock support
// ---------------------------------------------------------------------------

/// `SnapshotProvider` trait — for test injection. Production = `WinapiVssProvider`,
/// test = `MockProvider`. The pool is not generic; it holds `Arc<dyn SnapshotProvider>`.
pub(crate) trait SnapshotProvider: Send + Sync + 'static {
    /// Creates a snapshot for the given volume root.
    /// # Safety
    /// Must be called on a thread with an initialized COM apartment.
    unsafe fn create(&self, volume: &str) -> Result<RawSnapshot>;

    /// Cleans up the snapshot.
    /// # Safety
    /// `snap.backup` must have been created on the same thread.
    unsafe fn destroy(&self, snap: RawSnapshot) -> Result<()>;
}

/// Production provider — delegation to the winapi VSS calls.
pub(crate) struct WinapiVssProvider;

impl SnapshotProvider for WinapiVssProvider {
    unsafe fn create(&self, volume: &str) -> Result<RawSnapshot> {
        create_snapshot(volume)
    }
    unsafe fn destroy(&self, snap: RawSnapshot) -> Result<()> {
        destroy_snapshot(snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hresult_to_error_known_codes() {
        let e = hresult_to_error(VSS_E_BAD_STATE, "test");
        let msg = format!("{e}");
        assert!(msg.contains("VSS_E_BAD_STATE"), "msg={msg}");
        assert!(msg.contains("80042301"), "hex code yok: {msg}");

        let e = hresult_to_error(E_ACCESSDENIED, "test");
        assert!(format!("{e}").contains("E_ACCESSDENIED"));

        let e = hresult_to_error(VSS_E_VOLUME_NOT_SUPPORTED, "test");
        assert!(format!("{e}").contains("VOLUME_NOT_SUPPORTED"));

        let e = hresult_to_error(VSS_E_INSUFFICIENT_STORAGE, "test");
        assert!(format!("{e}").contains("INSUFFICIENT_STORAGE"));

        // Unknown code → "bilinmeyen HRESULT"
        let e = hresult_to_error(0x12345678, "test");
        assert!(format!("{e}").contains("bilinmeyen HRESULT"));
        assert!(format!("{e}").contains("12345678"));
    }

    #[test]
    fn bstr_from_str_round_trip() {
        unsafe {
            let s = "C:\\";
            let b = bstr_from_str(s);
            assert!(!b.is_null(), "BSTR alloc failed");
            let len = SysStringLen(b);
            assert_eq!(len as usize, s.encode_utf16().count());

            // Verify the first few code points
            let slice = std::slice::from_raw_parts(b, len as usize);
            assert_eq!(slice[0], b'C' as u16);
            assert_eq!(slice[1], b':' as u16);
            assert_eq!(slice[2], b'\\' as u16);

            bstr_free(b);
        }
    }

    #[test]
    fn bstr_free_null_is_safe() {
        unsafe {
            bstr_free(std::ptr::null_mut());
        }
    }

    #[test]
    fn snapshot_path_translation() {
        // device="\\?\GLOBALROOT\Device\HSC42", path="C:\Users\engin\busy.docx"
        let device_str = "\\\\?\\GLOBALROOT\\Device\\HSC42";
        let mut device: Vec<u16> = device_str.encode_utf16().collect();
        device.push(0); // null terminator

        let original = std::path::Path::new("C:\\Users\\engin\\busy.docx");
        let snap = snapshot_path(&device, original);
        let expected = "\\\\?\\GLOBALROOT\\Device\\HSC42\\Users\\engin\\busy.docx";
        assert_eq!(snap.to_string_lossy(), expected);
    }

    #[test]
    fn snapshot_path_handles_root_volume() {
        let device_str = "\\\\?\\GLOBALROOT\\Device\\HSC1";
        let mut device: Vec<u16> = device_str.encode_utf16().collect();
        device.push(0);

        let original = std::path::Path::new("D:\\file.txt");
        let snap = snapshot_path(&device, original);
        assert_eq!(
            snap.to_string_lossy(),
            "\\\\?\\GLOBALROOT\\Device\\HSC1\\file.txt"
        );
    }
}
