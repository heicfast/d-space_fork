// SPDX-License-Identifier: GPL-3.0-or-later
//
// VSS (Volume Shadow Copy Service) düşük seviye COM köprüsü.
// Master mimari Bölüm 34.2 + 34.5.4 + Discovery Log #001, #002, #004.
//
// İlkeler:
//   * Tüm `IVssBackupComponents` çağrıları worker thread'de yapılır
//     (MTA apartment). UI thread VSS pointer'a dokunmaz.
//   * `RawSnapshot` Drop'ta panik yapar — manuel `destroy_snapshot`
//     çağrılması zorunlu. BackupComplete olmadan Release veya yanlış
//     sırayla cleanup → service-side leak + crash.
//   * Discovery Log #001 — `SetBackupState(false, false, VSS_BT_COPY, false)`.
//   * Discovery Log #004 — `SetContext(VSS_CTX_FILE_SHARE_BACKUP)` seçildi
//     (writer involvement yok, auto-release, non-persistent — sadece read).
//
// `windows 0.61` IVssBackupComponents requester interface'ini sunmuyor
// (Discovery Log #002). Plan A: `winapi 0.3.9` crate'i `vsbackup` feature
// ile `IVssBackupComponentsVtbl` + `CreateVssBackupComponents` factory'sini
// sağlıyor — manuel vtable kurulumu gereksiz.

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

// Bazı winapi sürümlerinde `VSS_S_ASYNC_FINISHED` doğrudan re-export
// edilmiyor; constant değerleri sabitleyerek kullanıyoruz.
const VSS_S_ASYNC_FINISHED: i32 = 0x0004_230A;
const VSS_E_VOLUME_NOT_SUPPORTED: i32 = 0x8004_230C_u32 as i32;
const VSS_E_BAD_STATE: i32 = 0x8004_2301_u32 as i32;
const VSS_E_INSUFFICIENT_STORAGE: i32 = 0x8004_231F_u32 as i32;

// COM authentication seviyeleri — winapi sürümleri arasında farklı yerlerde
// olabildiği için sabit literal'le tanımladık.
const RPC_C_AUTHN_LEVEL_PKT_PRIVACY: u32 = 6;
const RPC_C_IMP_LEVEL_IDENTIFY: u32 = 2;
const EOAC_NONE: u32 = 0;

const GUID_NULL: GUID = GUID {
    Data1: 0,
    Data2: 0,
    Data3: 0,
    Data4: [0; 8],
};

/// HRESULT → mesajlı `Error::Snapshot`.
/// Yaygın VSS hata kodları için açıklayıcı metin döner; aksi halde hex kod.
pub(crate) fn hresult_to_error(hr: i32, ctx: &str) -> Error {
    let detail = match hr {
        E_ACCESSDENIED => "E_ACCESSDENIED (admin gerekli)",
        VSS_E_BAD_STATE => "VSS_E_BAD_STATE (yanlış sıra veya zaten meşgul)",
        VSS_E_VOLUME_NOT_SUPPORTED => {
            "VSS_E_VOLUME_NOT_SUPPORTED (NTFS değil veya VSS desteklemiyor)"
        }
        VSS_E_INSUFFICIENT_STORAGE => "VSS_E_INSUFFICIENT_STORAGE (shadow storage yetersiz)",
        _ => "bilinmeyen HRESULT",
    };
    Error::Snapshot(format!("VSS {ctx}: {detail} (HRESULT=0x{:08X})", hr as u32))
}

/// UTF-8 → BSTR (SysAllocStringLen). Çağıran sahipliği alır, `bstr_free` ile
/// serbest bırakmalı.
pub(crate) unsafe fn bstr_from_str(s: &str) -> BSTR {
    let utf16: Vec<u16> = s.encode_utf16().collect();
    // SysAllocStringLen null terminator'u kendi ekler.
    SysAllocStringLen(utf16.as_ptr(), utf16.len() as u32)
}

/// BSTR'yi serbest bırakır. Null safe (SysFreeString null'a tolere).
pub(crate) unsafe fn bstr_free(b: BSTR) {
    if !b.is_null() {
        SysFreeString(b);
    }
}

/// COM MTA apartment init. Worker thread başlangıcında çağrılır.
/// `RPC_E_CHANGED_MODE` → aynı thread STA ile init edilmiş; bizim için OK
/// değil, error döner. `S_FALSE` → zaten init edilmiş (OK).
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
    // RPC_E_TOO_LATE → process'te zaten çağrılmış (Tauri ana thread'i yapmış olabilir).
    if sec_hr != S_OK && sec_hr != RPC_E_TOO_LATE {
        return Err(hresult_to_error(sec_hr, "CoInitializeSecurity"));
    }
    Ok(())
}

/// CoUninitialize — worker thread sonunda. Idempotent.
pub(crate) unsafe fn uninit_com() {
    CoUninitialize();
}

/// `IVssAsync` operasyonunu tamamlanana kadar bekler. Async pointer her
/// durumda (hata olsa bile) Release edilir.
///
/// Gemini review B9 — eskiden `Wait(u32::MAX)` (INFINITE) kullanılıyordu;
/// VSS service'i kilitlenirse pool worker thread'i sonsuza kadar bloklanıp
/// duplicate scan'i donduruyordu. Artık sınırlı timeout ile döngü:
/// toplam 60 sn sonra `Error::Snapshot("timeout")` döner.
pub(crate) unsafe fn wait_async(p_async: *mut IVssAsync, ctx: &str) -> Result<()> {
    if p_async.is_null() {
        return Err(Error::Snapshot(format!("VSS {ctx}: async pointer null")));
    }

    // 60 × 1 sn bekleyiş — toplam 60 sn tavan. DoSnapshotSet normalde 1-3 sn.
    const WAIT_SLICE_MS: u32 = 1_000;
    const MAX_SLICES: u32 = 60;

    let mut wait_hr = S_OK;
    for _ in 0..MAX_SLICES {
        wait_hr = (*p_async).Wait(WAIT_SLICE_MS);
        if wait_hr != S_OK {
            // WAIT_TIMEOUT = HRESULT_FROM_WIN32(ERROR_TIMEOUT) 0x800705B4 —
            // dilim tükendi, tekrar dene. Diğer hatalar kalıcı.
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

    // Hâlă tamamlanmadıysa (futur durumlar) kalan süreyi harca — toplam
    // 60 sn dilim bütçesi içinde status VSS_S_ASYNC_FINISHED olmalı.
    for _ in 0..MAX_SLICES {
        if qs_hr != S_OK {
            break;
        }
        if status == VSS_S_ASYNC_FINISHED || status == S_OK {
            break;
        }
        if status == 0x0004_2306 {
            // VSS_S_ASYNC_CANCELLED — iptal, bekleme anlamsız.
            break;
        }
        if status == 0x0004_2305 {
            // VSS_S_ASYNC_PENDING — bir dilim daha bekle.
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
        return Err(Error::Snapshot(format!("VSS {ctx}: işlem iptal edildi")));
    }
    if status == 0x0004_2305 {
        return Err(Error::Snapshot(format!(
            "VSS {ctx}: 60 sn içinde tamamlanmadı (timeout)"
        )));
    }
    if status != S_OK && status != VSS_S_ASYNC_FINISHED {
        return Err(hresult_to_error(status, &format!("{ctx} (status)")));
    }
    Ok(())
}

/// Çekirdek raw snapshot — sadece `vss_pool::Worker` thread'inde yaşar.
/// Drop manuel `destroy_snapshot` ile zorunlu (otomatik drop panic'ler).
pub(crate) struct RawSnapshot {
    pub backup: *mut IVssBackupComponents,
    #[allow(dead_code)]
    pub snapshot_id: GUID,
    /// `\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopyN` (null-terminated wide).
    pub device_object: Vec<u16>,
}

// Worker thread sahibi → Send. Pointer yalnızca o thread'de dereference edilir.
unsafe impl Send for RawSnapshot {}

impl Drop for RawSnapshot {
    fn drop(&mut self) {
        // Üretimde panik istemiyoruz (worker thread düşmesin) ama
        // unwind sırasında düşerse tracing ile uyaralım. `destroy_snapshot`
        // çağrılmadan drop'a düşmek = service-side leak + olası crash.
        if !self.backup.is_null() {
            if std::thread::panicking() {
                // Panic unwind içindeyiz; abort yerine sessiz Release.
                unsafe {
                    (*self.backup).Release();
                }
            } else {
                // Bilinçli leak değil — log + tek bir Release deneyebiliriz
                // ama BackupComplete olmadan service-side state kirlenir.
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

/// `\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopyN` device path +
/// orijinal yolun volume root sonrası kısmını birleştirir.
///
/// Örnek: device="\\?\GLOBALROOT\Device\HSC42", original="C:\Users\engin\busy.docx"
///   → "\\?\GLOBALROOT\Device\HSC42\Users\engin\busy.docx"
pub(crate) fn snapshot_path(device_object: &[u16], original: &Path) -> PathBuf {
    // device_object null-terminated wide string olabilir; null'a kadar al.
    let dev_len = device_object
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(device_object.len());
    let device = String::from_utf16_lossy(&device_object[..dev_len]);

    let orig = original.to_string_lossy();
    // Volume root'unu (C:\, D:\, vs.) at, kalan path-after-root kısmını al.
    let after = if orig.len() >= 3
        && orig.as_bytes()[1] == b':'
        && (orig.as_bytes()[2] == b'\\' || orig.as_bytes()[2] == b'/')
    {
        &orig[2..] // "\Users\engin\busy.docx" (volume harfi atıldı)
    } else {
        // UNC veya beklenmedik format — olduğu gibi kullan.
        orig.as_ref()
    };

    // Trailing slash kontrol — device path zaten "...HSC42" şeklinde, slash YOK.
    let mut combined = device;
    if !after.starts_with('\\') {
        combined.push('\\');
    }
    combined.push_str(after);
    PathBuf::from(combined)
}

/// Volume snapshot'a uygun mu (NTFS + VSS destekli) hızlı sorgu.
/// Heavy işlem yapmaz — sadece `IsVolumeSupported` çağırır.
/// Sonraki sprint'te `vss_available_for` deep probe için kullanılacak;
/// şu an yalnızca debug yardımcısı.
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
            return Err(Error::Snapshot("volume BSTR alloc başarısız".into()));
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

/// Volume için snapshot oluşturur. Tam zincir:
///   CreateVssBackupComponents → InitializeForBackup → SetBackupState
///   → SetContext(FILE_SHARE_BACKUP) → GatherWriterMetadata
///   → IsVolumeSupported → StartSnapshotSet → AddToSnapshotSet
///   → PrepareForBackup → DoSnapshotSet → GetSnapshotProperties
///
/// Hata yolunda `cleanup_failed_backup` ile geri sarım.
pub(crate) unsafe fn create_snapshot(volume_root: &str) -> Result<RawSnapshot> {
    let mut p_backup: *mut IVssBackupComponents = ptr::null_mut();
    let hr = CreateVssBackupComponents(&mut p_backup);
    if hr != S_OK || p_backup.is_null() {
        return Err(hresult_to_error(hr, "CreateVssBackupComponents"));
    }

    // Hata path'inde p_backup'u toplayan helper.
    let cleanup = |past_do_snapshot: bool| {
        if past_do_snapshot {
            // DoSnapshotSet sonrası hata → BackupComplete + Release.
            let mut p_async: *mut IVssAsync = ptr::null_mut();
            let bc_hr = (*p_backup).BackupComplete(&mut p_async);
            if bc_hr == S_OK && !p_async.is_null() {
                let _ = (*p_async).Wait(u32::MAX);
                (*p_async).Release();
            }
        } else {
            // DoSnapshotSet öncesi → AbortBackup.
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
        return Err(Error::Snapshot("volume BSTR alloc başarısız".into()));
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
        cleanup(true); // DoSnapshotSet sonrası — BackupComplete gerekli
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

    // device_object kopyala (null-terminated wide).
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
        return Err(Error::Snapshot("snapshot device object boş".into()));
    }

    Ok(RawSnapshot {
        backup: p_backup,
        snapshot_id: snap_id,
        device_object,
    })
}

/// RawSnapshot'u sağlıklı şekilde temizler — `BackupComplete` + `Release`.
/// Sahiplik move alınır, drop panic tetiklenmez.
pub(crate) unsafe fn destroy_snapshot(mut snap: RawSnapshot) -> Result<()> {
    if snap.backup.is_null() {
        return Ok(());
    }

    let mut p_async: *mut IVssAsync = ptr::null_mut();
    let hr = (*snap.backup).BackupComplete(&mut p_async);
    let async_res = if hr == S_OK {
        wait_async(p_async, "BackupComplete")
    } else {
        Ok(()) // BackupComplete fail olsa bile Release et + raporla
    };

    (*snap.backup).Release();
    snap.backup = ptr::null_mut(); // Drop'u no-op et

    if hr != S_OK {
        return Err(hresult_to_error(hr, "BackupComplete"));
    }
    async_res
}

// ---------------------------------------------------------------------------
// Test ve mock destek
// ---------------------------------------------------------------------------

/// `SnapshotProvider` trait — test enjeksiyonu için. Production = `WinapiVssProvider`,
/// test = `MockProvider`. Pool generic değil; `Arc<dyn SnapshotProvider>` tutar.
pub(crate) trait SnapshotProvider: Send + Sync + 'static {
    /// Verilen volume root için snapshot oluşturur.
    /// # Safety
    /// COM apartment init edilmiş bir thread'de çağrılmalı.
    unsafe fn create(&self, volume: &str) -> Result<RawSnapshot>;

    /// Snapshot'u temizler.
    /// # Safety
    /// `snap.backup` aynı thread'de oluşturulmuş olmalı.
    unsafe fn destroy(&self, snap: RawSnapshot) -> Result<()>;
}

/// Production provider — winapi VSS çağrılarına delegasyon.
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

        // Bilinmeyen kod → "bilinmeyen HRESULT"
        let e = hresult_to_error(0x12345678, "test");
        assert!(format!("{e}").contains("bilinmeyen HRESULT"));
        assert!(format!("{e}").contains("12345678"));
    }

    #[test]
    fn bstr_from_str_round_trip() {
        unsafe {
            let s = "C:\\";
            let b = bstr_from_str(s);
            assert!(!b.is_null(), "BSTR alloc başarısız");
            let len = SysStringLen(b);
            assert_eq!(len as usize, s.encode_utf16().count());

            // İlk birkaç code point doğrula
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
