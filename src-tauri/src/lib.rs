// SPDX-License-Identifier: GPL-3.0-or-later
// D-Space — Windows için akıllı disk analiz ve geri kazanım platformu
// Copyright (C) 2026 D-Space contributors
//
// Master mimari: D-Space-Mimari-v1.4.docx (DONDURULDU)
// Modül-bölüm eşlemesi her modülün başındaki yorumda gösterilir.

pub mod db;
pub mod duplicate;
pub mod error;
pub mod index;
pub mod locked_file;
pub mod safe_delete;
pub mod scan;
pub mod snapshot;
pub mod staging;
pub mod v2;
pub mod volume;

use crate::db::{db_info, get_setting, open_db, set_setting, DbInfo, DbState};
use crate::duplicate::{find_duplicates, DuplicateOptions, DuplicateScanResult};
use crate::error::{Error, Result};
use crate::index::{
    index_search as index_search_db, index_status as index_status_db, list_indexed_volumes,
    sync_volume, IndexSearchResult, IndexStatus, SyncOutcome, DEFAULT_BASELINE_BUFFER,
};
use crate::locked_file::{probe_file, LockedFileProbe};
use crate::safe_delete::{
    add_rule as add_user_rule, delete_rule as delete_user_rule, list_active_snapshots,
    list_rules as list_user_rules, toggle_rule as toggle_user_rule, UserPatternType, UserRule,
};
use crate::scan::{
    is_elevated, node_path, pick_strategy, probe_cloud_state, probe_ntfs,
    scan_to_tree_with_progress, top_consumers, walk_mft, window_query, CloudProbe, MftProbe,
    MftWalkStats, Node, ScanProgress, ScanStrategy, ScanSummary, ScanTreeState, SortKey,
    WindowResult,
};
use crate::snapshot::{DeltaResult, SnapshotMeta};
use crate::staging::{
    cleanup_expired, list_expired, list_pending, permanent_delete, recover_wal, stage, undo,
    undo_with_resolution, CleanupReport, ConflictResolution, ExpiredItem, PermanentDeleteResult,
    StagedItem, UndoOutcome, WalRecoveryReport,
};
use crate::volume::{
    list_drives, parse_drive_letter, pre_flight_check, probe_drive_hardware, DriveHardware,
    VolumeInfo,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

/// Tarama iptal kaydı — her `scan_full` / `scan_unc_path` koşusu KENDİ
/// `Arc<AtomicBool>` bayrağını alır (Gemini review: eski tek-global-flag
/// tasarımında iptal→hızlı yeniden tarama, eski taramanın bayrağını
/// sıfırlayıp İKİ walker'ın paralel çalışmasına yol açabiliyordu).
/// `scan_cancel` aktif koşunun bayrağını set eder; walker periyodik kontrol
/// edip `Error::Scan("scan-cancelled")` ile çıkar.
#[derive(Default)]
pub struct ScanCancel(std::sync::Mutex<Arc<AtomicBool>>);

impl ScanCancel {
    /// Bu koşu için taze bir bayrak üret, aktif kayda yaz ve döndür.
    pub fn begin_run(&self) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut slot = match self.0.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *slot = flag.clone();
        flag
    }

    /// Aktif koşunun bayrağını set et (iptal). Koşu yoksa no-op.
    pub fn cancel_active(&self) {
        let slot = match self.0.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        slot.store(true, Ordering::Release);
    }
}

#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub spec_version: &'static str,
    pub spec_status: &'static str,
    pub license: &'static str,
    pub platform: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PrivilegeStatus {
    pub elevated: bool,
    pub strategy: ScanStrategy,
}

impl AppInfo {
    pub const fn current() -> Self {
        Self {
            name: "D-Space",
            version: env!("CARGO_PKG_VERSION"),
            spec_version: "v1.4",
            spec_status: "DONDURULDU",
            license: "GPL-3.0-or-later",
            platform: std::env::consts::OS,
        }
    }
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo::current()
}

#[tauri::command]
fn check_privilege() -> PrivilegeStatus {
    let elevated = is_elevated();
    let strategy = pick_strategy();
    info!(elevated, ?strategy, "yetki durumu sorgulandı");
    PrivilegeStatus { elevated, strategy }
}

/// Bölüm 33.2 Katman 0 — tarama denemesinden ÖNCE volume statüsü kontrolü.
/// Kullanıcı-uzayı Win32 API'leri, admin gerekmez.
#[tauri::command]
async fn pre_flight_volume(drive: String) -> Result<VolumeInfo> {
    tokio::task::spawn_blocking(move || pre_flight_check(&drive))
        .await
        .map_err(|e| crate::error::Error::Volume(format!("join hatası: {}", e)))?
}

/// Bölüm 33.2 + 15.1 v0.2 — sistemdeki tüm mount edilmiş sürücüler.
/// `GetLogicalDrives` bitmask → her sürücü için `pre_flight_check`. Sol
/// VolumeSidebar UI bunu çağırıp drive listesini render eder.
#[tauri::command]
async fn list_drives_cmd() -> Result<Vec<VolumeInfo>> {
    tokio::task::spawn_blocking(list_drives)
        .await
        .map_err(|e| crate::error::Error::Volume(format!("join hatası: {}", e)))?
}

/// Bölüm 13.2 Tray Live Monitor — `tray-monitor-alert` event payload.
#[derive(Debug, Clone, Serialize)]
pub struct TrayMonitorAlert {
    pub drive_letter: String,
    pub volume_label: String,
    pub usage_percent: u32,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Tray monitor polling interval (varsayılan 30 dk). Settings key:
/// `tray_monitor_interval_minutes`.
const TRAY_MONITOR_DEFAULT_INTERVAL_MIN: u64 = 30;
/// Disk doluluğu eşiği — uyarı emit edilecek minimum yüzde. Default 90%.
/// Settings key: `tray_monitor_full_threshold`.
const TRAY_MONITOR_DEFAULT_THRESHOLD: u32 = 90;
/// Minimum polling interval — kullanıcı yanlışlığını clamp et.
const TRAY_MONITOR_MIN_INTERVAL_MIN: u64 = 5;

/// Win32 `GetSystemPowerStatus` ile sistemin AC mi pillen mi olduğunu
/// döner. Gemini review 2.4 ek — Bölüm 13.3 battery-aware throttle:
/// pillen modda tray monitor interval'i × 2 ile çarpılır (12 wake/saat
/// yerine 6).
///
/// Dönüş: `true` = battery / unknown (konservatif), `false` = AC plugged.
#[cfg(windows)]
fn is_on_battery() -> bool {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetSystemPowerStatus(&mut status) };
    if ok.is_err() {
        // Sorgu başarısız → konservatif, batarya varsay (interval × 2).
        return true;
    }
    // ACLineStatus: 0=offline (battery), 1=online (AC), 255=unknown.
    // Gemini review: 255 (bilinmiyor) da konservatif olarak batarya sayılır.
    status.ACLineStatus == 0 || status.ACLineStatus == 255
}

#[cfg(not(windows))]
fn is_on_battery() -> bool {
    false
}

/// Tray monitor enabled mi?  `settings.tray_monitor_enabled = "1"` ise true.
fn read_tray_monitor_settings(conn: &rusqlite::Connection) -> (bool, u64, u32) {
    let enabled = get_setting(conn, "tray_monitor_enabled")
        .ok()
        .flatten()
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let interval_min = get_setting(conn, "tray_monitor_interval_minutes")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(TRAY_MONITOR_DEFAULT_INTERVAL_MIN)
        .max(TRAY_MONITOR_MIN_INTERVAL_MIN);
    let threshold = get_setting(conn, "tray_monitor_full_threshold")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(TRAY_MONITOR_DEFAULT_THRESHOLD)
        .clamp(50, 99);
    (enabled, interval_min, threshold)
}

/// Sürmekte olan taramayı iptal et — aktif koşunun bayrağını set eder,
/// walker bir sonraki progress checkpoint'inde fark eder ve
/// `Error::Scan("scan-cancelled")` ile bail eder. Bir tarama yoksa no-op.
#[tauri::command]
fn scan_cancel(state: tauri::State<'_, ScanCancel>) -> Result<()> {
    state.cancel_active();
    info!("scan_cancel istendi — aktif koşu bayrağı set");
    Ok(())
}

/// Sprint 4.4 + Bölüm 26.2 — UNC path (`\\server\share[\sub]`) network
/// taraması. find_first walker UNC kabul eder; bu wrapper sayım +
/// bandwidth tahmini döner. `bandwidth_cap_bps` v0.3.2'de gerçek
/// throttling için tahsis edildi.
///
/// Gemini review — artık İPTAL EDİLEBİLİR ve canlı `scan-progress` event'i
/// emit eder (SMB walk saatler sürebilir; sessiz + durdurulamaz UX kabul
/// edilemez).
#[tauri::command]
async fn scan_unc_path<R: tauri::Runtime>(
    unc_path: String,
    bandwidth_cap_bps: Option<u64>,
    window: tauri::WebviewWindow<R>,
    cancel_state: tauri::State<'_, ScanCancel>,
) -> Result<crate::v2::NetworkScanResult> {
    use crate::v2::WindowsNetworkScanner;
    use tauri::Emitter;

    // Bu koşuya özel iptal bayrağı (scan_full ile aynı semantik).
    let cancel_handle = cancel_state.begin_run();

    // Progress channel — scan_full ile aynı mpsc decouple pattern'i.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ScanProgress>();
    let window_for_emit = window.clone();
    let emit_task = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = window_for_emit.emit("scan-progress", &progress);
        }
    });

    let result = tokio::task::spawn_blocking(move || {
        let scanner = WindowsNetworkScanner::new();
        let cb: Box<dyn Fn(&ScanProgress) + Send + Sync> = Box::new(move |p| {
            let _ = tx.send(p.clone());
        });
        scanner.scan_unc_with_progress(
            &unc_path,
            bandwidth_cap_bps.unwrap_or(0),
            Some(cb.as_ref()),
            Some(cancel_handle.as_ref()),
        )
    })
    .await
    .map_err(|e| Error::Volume(format!("join hatası: {}", e)))?;

    emit_task.abort();
    result
}

/// Bir sürücünün donanım profilini döner: bus tipi (NVMe/SATA/USB),
/// medya tipi (SSD/HDD), üretici, model, seri, tipik okuma hızı.
/// Win32 `IOCTL_STORAGE_QUERY_PROPERTY` kullanılır; admin gerekmez.
#[tauri::command]
async fn probe_drive_hardware_cmd(drive: String) -> Result<DriveHardware> {
    // Gemini review B12 — katı doğrulama: UNC girdi sessizce harfe dönüşmesin.
    let letter = parse_drive_letter(&drive)
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| Error::Volume(format!("geçersiz drive: '{}'", drive)))?;
    tokio::task::spawn_blocking(move || probe_drive_hardware(letter))
        .await
        .map_err(|e| crate::error::Error::Volume(format!("join hatası: {}", e)))?
}

/// Bölüm 5.1–5.3 — raw volume aç, NTFS boot sector parse et, metadata dön.
/// Yönetici yetkisi yoksa ACCESS_DENIED → Error::Scan.
#[tauri::command]
async fn probe_volume(drive: String) -> Result<MftProbe> {
    tokio::task::spawn_blocking(move || probe_ntfs(&drive))
        .await
        .map_err(|e| Error::Scan(format!("join hatası: {}", e)))?
}

/// Bölüm 5.1 + 4.3 Adım 2 — tüm MFT record'larını sırayla gez, sayım + örnek
/// adlar dön. v0.1 tek-thread, hiyerarşi yok (sonraki sprint Bölüm 4.3 Adım 3).
#[tauri::command]
async fn walk_volume(drive: String) -> Result<MftWalkStats> {
    tokio::task::spawn_blocking(move || walk_mft(&drive))
        .await
        .map_err(|e| Error::Scan(format!("join hatası: {}", e)))?
}

/// Bölüm 4.3 Adım 3 + 4.4 + 6.4 + 9.6.5 — tam MFT entry koleksiyonu + hiyerarşi +
/// agregat. Bölüm 6.4 user rules önce yüklenir; Bölüm 9.6.5 progress event'leri
/// her N entry'de `scan-progress` olarak emit edilir.
#[tauri::command]
async fn scan_full<R: tauri::Runtime>(
    drive: String,
    window: tauri::WebviewWindow<R>,
    scan_state: tauri::State<'_, ScanTreeState>,
    db_state: tauri::State<'_, DbState>,
    cancel_state: tauri::State<'_, ScanCancel>,
) -> Result<ScanSummary> {
    use tauri::Emitter;
    let drv = drive.clone();
    let user_rules = {
        let conn = db_state
            .conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
        list_active_snapshots(&conn)?
    };

    // Gemini review (cancel race) — her koşu taze bayrak alır; önceki
    // iptal edilmiş bir koşunun bayrağı yeni koşuyu etkilemez.
    let cancel_handle = cancel_state.begin_run();

    // Progress channel: scan thread → emit task. spawn_blocking içinden
    // doğrudan window.emit çağrısı thread-safety açısından güvenli ama
    // mpsc decouple gathering+emit, smooth rate-limiting yapar.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ScanProgress>();
    let window_for_emit = window.clone();
    let emit_task = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = window_for_emit.emit("scan-progress", &progress);
        }
    });

    let (summary, tree) = tokio::task::spawn_blocking(move || {
        let cb: Box<dyn Fn(&ScanProgress) + Send + Sync> = Box::new(move |p| {
            let _ = tx.send(p.clone());
        });
        scan_to_tree_with_progress(
            &drv,
            &user_rules,
            Some(cb.as_ref()),
            Some(cancel_handle.as_ref()),
        )
    })
    .await
    .map_err(|e| Error::Scan(format!("join hatası: {}", e)))??;

    emit_task.abort();
    // Son bir emit — tamamlandı. Sprint 3.7: final partial_tree de göndeririz
    // ki canlı sunburst tarama sonunda tam haritayı (root + 2 seviye + top-200)
    // alıp final state'e geçiş yapsın. Gemini review: sihirli sayılar yerine
    // stream sabitleri (walk.rs) kullanılır.
    let final_partial = crate::scan::tree::build_partial_view(
        &tree,
        crate::scan::walk::PARTIAL_TREE_MAX_DEPTH,
        crate::scan::walk::PARTIAL_TREE_TOP_N,
    );
    let _ = window.emit(
        "scan-progress",
        &ScanProgress {
            phase: "done",
            visited: summary.node_count,
            total_estimate: summary.node_count,
            in_use: summary.node_count,
            last_name: String::new(),
            elapsed_ms: summary.collect_elapsed_ms + summary.build_elapsed_ms,
            partial_tree: Some(final_partial),
        },
    );

    let arc_tree = Arc::new(tree);
    {
        let mut slot = scan_state
            .current
            .write()
            .map_err(|e| Error::Scan(format!("rwlock poisoned: {}", e)))?;
        *slot = Some(arc_tree);
    }
    info!(
        drive,
        node_count = summary.node_count,
        gb = summary.total_bytes / 1_073_741_824,
        "scan_full → state güncellendi"
    );
    Ok(summary)
}

/// Bölüm 9.6 ön-örneği — bir düğümün çocuklarını agregat boyuta göre top-N.
#[tauri::command]
fn tree_top_consumers(
    state: tauri::State<'_, ScanTreeState>,
    parent: u64,
    limit: usize,
) -> Result<Vec<Node>> {
    let guard = state
        .current
        .read()
        .map_err(|e| Error::Scan(format!("rwlock poisoned: {}", e)))?;
    let tree = guard
        .as_ref()
        .ok_or_else(|| Error::Scan("scan_full henüz çağrılmadı".into()))?;
    Ok(top_consumers(tree, parent, limit))
}

/// Bölüm 9.6 — viewport-aware pencere sorgusu (filtreli + sıralı + sayfalı).
#[tauri::command]
fn tree_window(
    state: tauri::State<'_, ScanTreeState>,
    parent: u64,
    sort: Option<SortKey>,
    limit: Option<usize>,
    offset: Option<usize>,
    min_size_bytes: Option<u64>,
) -> Result<WindowResult> {
    let guard = state
        .current
        .read()
        .map_err(|e| Error::Scan(format!("rwlock poisoned: {}", e)))?;
    let tree = guard
        .as_ref()
        .ok_or_else(|| Error::Scan("scan_full henüz çağrılmadı".into()))?;
    Ok(window_query(
        tree,
        parent,
        sort.unwrap_or_default(),
        limit.unwrap_or(200),
        offset.unwrap_or(0),
        min_size_bytes,
    ))
}

/// Bir düğümün root'a kadar olan zincirini döner — breadcrumb için.
#[tauri::command]
fn tree_path(state: tauri::State<'_, ScanTreeState>, id: u64) -> Result<Vec<Node>> {
    let guard = state
        .current
        .read()
        .map_err(|e| Error::Scan(format!("rwlock poisoned: {}", e)))?;
    let tree = guard
        .as_ref()
        .ok_or_else(|| Error::Scan("scan_full henüz çağrılmadı".into()))?;
    Ok(node_path(tree, id))
}

/// Bölüm 12.2 — staging: dosyayı `%LOCALAPPDATA%\DSpace\staging\<ts>` altına
/// atomik MOVE eder ve `staging_items` tablosuna kayıt düşer. 24h undo penceresi.
/// Gemini review — file I/O (rename/cross-volume copy) spawn_blocking'e taşındı.
#[tauri::command]
async fn stage_path(path: String, state: tauri::State<'_, DbState>) -> Result<StagedItem> {
    let conn = state.conn.clone();
    tokio::task::spawn_blocking(move || {
        let conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
        stage(&path, &conn)
    })
    .await
    .map_err(|e| Error::Staging(format!("join hatası: {}", e)))?
}

/// Bölüm 12.2 — bekleyen tüm staging item'larını listeler (staged_at DESC).
#[tauri::command]
fn list_staging(state: tauri::State<'_, DbState>) -> Result<Vec<StagedItem>> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    list_pending(&conn)
}

/// Bölüm 12.3 — WAL recovery raporu (açılışta otomatik çağrılır, bu komut
/// manuel re-run veya UI diagnostic için).
#[tauri::command]
fn run_wal_recovery(state: tauri::State<'_, DbState>) -> Result<WalRecoveryReport> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    recover_wal(&conn)
}

/// Bölüm 12.2.4 — staging item'ı orijinal yoluna geri taşır. Üç sonuç:
/// Restored (hedef boştu), Idempotent (hedef = staged, hash match),
/// Conflict (UI dialog → undo_resolve_staging çağrısı).
/// Gemini review — cross-volume undo + hash I/O spawn_blocking'e taşındı.
#[tauri::command]
async fn undo_staging(id: i64, state: tauri::State<'_, DbState>) -> Result<UndoOutcome> {
    let conn = state.conn.clone();
    tokio::task::spawn_blocking(move || {
        let conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
        undo(id, &conn)
    })
    .await
    .map_err(|e| Error::Staging(format!("join hatası: {}", e)))?
}

/// Bölüm 12.2.4.2 — conflict dialog'undan dönen kullanıcı seçimini
/// uygula: Overwrite / Rename / KeepBoth / Cancel.
#[tauri::command]
async fn undo_resolve_staging(
    id: i64,
    resolution: ConflictResolution,
    state: tauri::State<'_, DbState>,
) -> Result<UndoOutcome> {
    let conn = state.conn.clone();
    tokio::task::spawn_blocking(move || {
        let conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
        undo_with_resolution(id, resolution, &conn)
    })
    .await
    .map_err(|e| Error::Staging(format!("join hatası: {}", e)))?
}

/// Bölüm 12.2.1 — süresi geçmiş staging item'larını listele (expires_at < now).
#[tauri::command]
fn list_expired_staging_cmd(state: tauri::State<'_, DbState>) -> Result<Vec<ExpiredItem>> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    list_expired(&conn)
}

/// Bölüm 12.2.2-2.3 — onaylı toplu cleanup. AUTO_THRESHOLD aşımında dönen
/// rapor `aborted_threshold = true` olur; UI kullanıcıdan açık onay ister.
/// `force=true` "Hepsini sil" onayını işaretler.
/// Gemini review — rate-limited sleep loop main thread'i donduruyordu;
/// spawn_blocking'e taşındı.
#[tauri::command]
async fn cleanup_expired_staging_cmd(
    rate_per_sec: Option<u32>,
    force: Option<bool>,
    state: tauri::State<'_, DbState>,
) -> Result<CleanupReport> {
    let conn = state.conn.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
        let rate = rate_per_sec.unwrap_or(crate::staging::expiry::DEFAULT_CLEANUP_RATE_PER_SEC);
        cleanup_expired(&mut conn, rate, force.unwrap_or(false))
    })
    .await
    .map_err(|e| Error::Staging(format!("join hatası: {}", e)))?
}

/// Bölüm 12.4 — staging item'ı disk'ten kalıcı sil + forensic ledger.
/// `confirm_phrase` orijinal dosya adına (case-insensitive) eşit olmalı.
/// Bu çift onayın **ikinci** adımı; ilk adım UI'da "Sil" butonuna basmak.
/// Gemini review — DoD wipe dakikalar sürebilir; spawn_blocking'e taşındı.
#[tauri::command]
async fn permanent_delete_cmd(
    id: i64,
    confirm_phrase: String,
    secure_wipe: Option<bool>,
    state: tauri::State<'_, DbState>,
) -> Result<PermanentDeleteResult> {
    let conn = state.conn.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
        permanent_delete(id, &confirm_phrase, secure_wipe.unwrap_or(false), &mut conn)
    })
    .await
    .map_err(|e| Error::Staging(format!("join hatası: {}", e)))?
}

/// Bölüm 6.4 — kullanıcı tanımlı kuralları listele.
#[tauri::command]
fn list_user_rules_cmd(state: tauri::State<'_, DbState>) -> Result<Vec<UserRule>> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    list_user_rules(&conn)
}

/// Bölüm 6.4 — yeni kural ekle. Pattern boş olamaz, skor 0-100.
#[tauri::command]
fn add_user_rule_cmd(
    pattern: String,
    pattern_type: UserPatternType,
    score: u8,
    explanation: String,
    state: tauri::State<'_, DbState>,
) -> Result<UserRule> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    add_user_rule(&conn, &pattern, pattern_type, score, &explanation)
}

/// Bölüm 6.4 — kuralı sil.
#[tauri::command]
fn delete_user_rule_cmd(id: i64, state: tauri::State<'_, DbState>) -> Result<()> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    delete_user_rule(&conn, id)
}

/// Bölüm 6.4 — kuralı aç/kapat (silmeden devre dışı bırak).
#[tauri::command]
fn toggle_user_rule_cmd(id: i64, enabled: bool, state: tauri::State<'_, DbState>) -> Result<()> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    toggle_user_rule(&conn, id, enabled)
}

/// Bölüm 14 — settings KV okuma (onboarding flag, kullanıcı tercihleri vs.).
#[tauri::command]
fn get_setting_cmd(key: String, state: tauri::State<'_, DbState>) -> Result<Option<String>> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    get_setting(&conn, &key)
}

/// Bölüm 14 — settings KV upsert.
#[tauri::command]
fn set_setting_cmd(key: String, value: String, state: tauri::State<'_, DbState>) -> Result<()> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    set_setting(&conn, &key, &value)
}

/// Bölüm 14 — DB metadata sorgusu (path, schema_version, journal_mode, ...).
#[tauri::command]
fn get_db_info(state: tauri::State<'_, DbState>) -> Result<DbInfo> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    db_info(&conn)
}

/// Bölüm 8 — mevcut taranmış ScanTree'den bir snapshot yakalar. `scan_full`
/// önce çağrılmış olmalı. Yalnızca dizinler `snapshot_entries`'e yazılır
/// (v0.1 dir-only; Bölüm 8.6 dosya streaming v0.2'de gelir).
/// Gemini review — bulk insert (100k+ satır) spawn_blocking'e taşındı.
#[tauri::command]
async fn capture_snapshot(
    scan_state: tauri::State<'_, ScanTreeState>,
    db_state: tauri::State<'_, DbState>,
) -> Result<SnapshotMeta> {
    let tree_arc = {
        let guard = scan_state
            .current
            .read()
            .map_err(|e| Error::Scan(format!("rwlock poisoned: {}", e)))?;
        guard
            .as_ref()
            .ok_or_else(|| Error::Snapshot("scan_full henüz çağrılmadı".into()))?
            .clone()
    };
    let conn = db_state.conn.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
        crate::snapshot::capture_snapshot(&tree_arc, &mut conn)
    })
    .await
    .map_err(|e| Error::Snapshot(format!("join hatası: {}", e)))?
}

/// Bölüm 8 — son 50 snapshot'ı captured_at DESC sırasında döner.
#[tauri::command]
fn list_snapshots(db_state: tauri::State<'_, DbState>) -> Result<Vec<SnapshotMeta>> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    crate::snapshot::list_snapshots(&conn)
}

/// Sprint 4.2 (Bölüm 8.4) — eski snapshot'ları sil. `retain_days` None
/// ise settings.snapshot_retain_days okunur, o da yoksa
/// `DEFAULT_RETAIN_DAYS` (90) kullanılır. MIN_RETAIN_DAYS (7) altı
/// otomatik clamp edilir (Bölüm 22.6 dark pattern yok).
#[tauri::command]
fn cleanup_old_snapshots(
    retain_days: Option<i64>,
    db_state: tauri::State<'_, DbState>,
) -> Result<u64> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    let effective = retain_days.unwrap_or_else(|| {
        get_setting(&conn, "snapshot_retain_days")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(crate::snapshot::DEFAULT_RETAIN_DAYS)
    });
    crate::snapshot::purge_old_snapshots(&conn, effective)
}

/// Bölüm 11.1 — tek dosya için cloud placeholder probe. GetFileAttributesW
/// ile FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS / RECALL_ON_OPEN / REPARSE_POINT
/// bayraklarını okur. spawn_blocking — Win32 senkron çağrı.
#[tauri::command]
async fn probe_cloud_state_cmd(path: String) -> Result<CloudProbe> {
    let p = std::path::PathBuf::from(path);
    tokio::task::spawn_blocking(move || probe_cloud_state(&p))
        .await
        .map_err(|e| Error::Scan(format!("join hatası: {}", e)))?
}

/// Bölüm 34 — tek dosya için locked state + lock owner probe.
/// Yalnızca on-demand çağrılır (Bölüm 34.5.1 hot path izolasyonu). VSS yok,
/// snapshot read v0.2 sprint'inde gelir. spawn_blocking → CreateFileW +
/// Restart Manager senkron Win32 çağrıları.
#[tauri::command]
async fn probe_locked_file_cmd(path: String) -> Result<LockedFileProbe> {
    let p = std::path::PathBuf::from(path);
    tokio::task::spawn_blocking(move || probe_file(&p))
        .await
        .map_err(|e| Error::LockedFile(format!("join hatası: {}", e)))?
}

/// Bölüm 7 — taranmış ScanTree üzerinde duplicate aramayı çalıştırır. `scan_full`
/// önce çağrılmış olmalı. Blake3 streaming hash; tek-thread v0.1. Sonuç:
/// boyut-bucket → hash-bucket grupları, en çok kazandıran önce.
#[tauri::command]
async fn find_duplicates_cmd(
    drive: String,
    min_size_bytes: Option<u64>,
    size_only: Option<bool>,
    max_groups: Option<usize>,
    state: tauri::State<'_, ScanTreeState>,
) -> Result<DuplicateScanResult> {
    let tree_arc = {
        let guard = state
            .current
            .read()
            .map_err(|e| Error::Scan(format!("rwlock poisoned: {}", e)))?;
        guard
            .as_ref()
            .ok_or_else(|| Error::Duplicate("scan_full henüz çağrılmadı".into()))?
            .clone()
    };
    // Gemini review B12 — katı doğrulama: UNC girdi sessizce harfe dönüşmesin.
    let letter = parse_drive_letter(&drive)
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| Error::Duplicate(format!("Geçersiz sürücü: '{}'", drive)))?;
    let opts = DuplicateOptions {
        min_size_bytes: min_size_bytes.unwrap_or(crate::duplicate::DEFAULT_MIN_DUP_SIZE),
        size_only: size_only.unwrap_or(false),
        skip_head_prefilter: false,
        max_groups: max_groups.unwrap_or(500),
    };
    // Hash I/O blocking — spawn_blocking ile asenkron.
    tokio::task::spawn_blocking(move || find_duplicates(&tree_arc, letter, opts))
        .await
        .map_err(|e| Error::Duplicate(format!("join hatası: {}", e)))?
}

/// Sprint 3.8 — Discovery #005 / Bölüm 5.6. USN journal index'inden anlık
/// substring araması. Tüm ciltlerden eşleşen ilk `limit` kaydı döner;
/// `full_path` opsiyonel olarak parent zinciri ile çözülür (top-N için
/// yeterli; v0.1 perf hedefi < 50 ms).
/// Gemini review — milyonlarca satırda LIKE + COUNT taraması ana thread'i
/// kilitliyordu; spawn_blocking'e taşındı.
#[tauri::command]
async fn index_search(
    query: String,
    limit: Option<usize>,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<IndexSearchResult>> {
    let conn = state.conn.clone();
    tokio::task::spawn_blocking(move || {
        let conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
        index_search_db(&conn, &query, limit.unwrap_or(50))
    })
    .await
    .map_err(|e| Error::Index(format!("join hatası: {}", e)))?
}

/// Sprint 3.8 — Discovery #005. Index durum sorgusu. `drive` opsiyonel:
/// boş geçildiğinde global sayım dönülür. Admin yoksa `mode="needs_admin"`.
#[tauri::command]
async fn index_status(
    drive: Option<String>,
    state: tauri::State<'_, DbState>,
) -> Result<IndexStatus> {
    // Gemini review B12 — katı doğrulama (UNC reddi).
    let volume_id = drive
        .as_ref()
        .and_then(|d| parse_drive_letter(d))
        .map(|c| format!(r"\\.\{}:", c.to_ascii_uppercase()));
    let conn = state.conn.clone();
    let mut st = tokio::task::spawn_blocking(move || {
        let conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
        index_status_db(&conn, volume_id.as_deref())
    })
    .await
    .map_err(|e| Error::Index(format!("join hatası: {}", e)))??;
    if !is_elevated() && st.mode == "idle" {
        // Henüz indeks yok ve admin de yok → UI'a "needs_admin" göster.
        st.mode = "needs_admin".to_string();
    }
    Ok(st)
}

/// Sprint 3.8.1 — index güncelleme. Gemini review (açılış donması):
///   * Watermark VARSA → incremental delta (`FSCTL_READ_USN_JOURNAL`),
///     saniyeler içinde biter; full rebuild yalnız wraparound'ta.
///   * Yürüyüş DEDICATED connection + spawn_blocking ile koşar — global DB
///     mutex'i ve ana thread artık dakikalarca kilitlenmez (WAL mode çoklu
///     bağlantıyı güvenle destekler).
///   * Başarıda volume için USN watcher (yeniden) spawn edilir (Bölüm 5.6
///     canlı delta senkronizasyonu — v0.3.1 prod wire).
#[tauri::command]
async fn index_build<R: tauri::Runtime>(
    drive: String,
    _force: Option<bool>,
    app: tauri::AppHandle<R>,
    db_state: tauri::State<'_, DbState>,
) -> Result<IndexStatus> {
    let drive_letter = parse_drive_letter(&drive)
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| Error::Index(format!("geçersiz drive parametresi: {}", drive)))?;
    let volume_id = format!(r"\\.\{}:", drive_letter);

    if !is_elevated() {
        let conn = db_state
            .conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
        let mut st = index_status_db(&conn, Some(&volume_id))?;
        st.mode = "needs_admin".to_string();
        return Ok(st);
    }

    let vid_for_status = volume_id.clone();
    let vid_for_watcher = volume_id.clone();
    let summary = tokio::task::spawn_blocking(move || -> Result<SyncOutcome> {
        // Dedicated connection — uzun yürüyüş global mutex'i bloklamaz.
        let mut conn = open_db()?;
        sync_volume(&vid_for_watcher, &mut conn, DEFAULT_BASELINE_BUFFER)
    })
    .await
    .map_err(|e| Error::Index(format!("join hatası: {}", e)))??;

    match &summary {
        SyncOutcome::Baseline(b) => info!(
            volume = %volume_id,
            records = b.records_seen,
            entries = b.entries_written,
            batches = b.batches,
            "USN baseline tamamlandı"
        ),
        SyncOutcome::Delta {
            summary: d,
            next_usn,
            ..
        } => info!(
            volume = %volume_id,
            upserts = d.upserts,
            deletes = d.deletes,
            skipped = d.skipped,
            next_usn,
            "USN incremental delta uygulandı"
        ),
    }

    // v0.3.1 — volume için watcher'ı garanti et (idempotent).
    ensure_usn_watcher(&app, &volume_id);

    let conn = db_state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    index_status_db(&conn, Some(&vid_for_status))
}

/// Bölüm 8.6 — iki snapshot ID arasındaki delta (added/removed/grew/shrunk
/// top-10 + net byte change).
#[tauri::command]
fn snapshot_delta(from: i64, to: i64, db_state: tauri::State<'_, DbState>) -> Result<DeltaResult> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {}", e)))?;
    crate::snapshot::compute_delta(from, to, &conn)
}

/// Bölüm 13.1 — system tray ikon + minimum menü. Auto-clean polling
/// (Bölüm 13.2/13.3) v0.2 sprint'inde; bu v0.1 sadece pencere
/// göster/gizle + çıkış.
///
/// İkon: Tauri'nin default app ikonunu kullanır (icon.ico zaten paketli).
/// İleride özel tray ikonu (16x16 mono) eklenebilir.
fn build_tray<R: tauri::Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuEvent};
    use tauri::tray::TrayIconBuilder;
    use tauri::{Emitter, Manager};

    let menu = MenuBuilder::new(app)
        .text("open", "D-Space'i aç")
        .text("scan_c", "Tara: C:")
        .separator()
        .text("quit", "Çıkış")
        .build()?;

    let Some(icon) = app.default_window_icon().cloned() else {
        warn!("default window icon yok — tray atlandı");
        return Ok(());
    };

    TrayIconBuilder::with_id("dspace-tray")
        .icon(icon)
        .menu(&menu)
        .tooltip("D-Space — Görmek, anlamak, geri kazanmak")
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event: MenuEvent| {
            match event.id().as_ref() {
                "open" => {
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
                "scan_c" => {
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.set_focus();
                        // Pencereye event yolla; UI yakalayıp runFullScan tetikleyecek
                        let _ = win.emit("tray-scan-request", "C");
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;

    info!("system tray hazır");
    Ok(())
}

/// Bölüm 13.2 — tray live monitor. Background tokio task arka planda
/// `tray_monitor_interval_minutes` periyoduyla `list_drives` çağırır;
/// herhangi bir sürücüde `tray_monitor_full_threshold` aşılırsa frontend'e
/// `tray-monitor-alert` event'i emit eder.
///
/// Setting'ler çalışma sırasında okunur (her tick): toggle restart
/// gerektirmez. `enabled=false` ise tick no-op, sadece sleep.
fn spawn_tray_monitor<R: tauri::Runtime>(app: &tauri::App<R>) {
    use tauri::{Emitter, Manager};

    /// MutexGuard'ı await sınırında geçirmemek için sync helper —
    /// `Send` future kısıtı (tray_monitor_tick async).
    fn read_settings<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) -> Option<(bool, u64, u32)> {
        let db_state = handle.state::<DbState>();
        let conn = db_state.conn.lock().ok()?;
        Some(read_tray_monitor_settings(&conn))
    }

    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        // İlk tick'i 30 sn'lik açılış payload'unu bozmamak için geciktiriyoruz.
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        loop {
            let (enabled, interval_min, threshold) = read_settings(&handle).unwrap_or((
                false,
                TRAY_MONITOR_DEFAULT_INTERVAL_MIN,
                TRAY_MONITOR_DEFAULT_THRESHOLD,
            ));

            if enabled {
                if let Err(e) = tray_monitor_tick(&handle, threshold).await {
                    warn!(?e, "tray monitor tick hatası");
                }
            }
            // Bölüm 13.3 — battery-aware throttle: pillen modda interval × 2.
            // Gemini review B8 — saturating_mul taşma koruması (elle
            // düzenlenmiş dev ayar busy-loop'a düşmesin).
            let effective_interval = if is_on_battery() {
                interval_min.saturating_mul(2)
            } else {
                interval_min
            };
            tokio::time::sleep(std::time::Duration::from_secs(
                effective_interval.saturating_mul(60),
            ))
            .await;
        }
    });

    /// Tek bir polling tick — list_drives + threshold check + emit.
    async fn tray_monitor_tick<R: tauri::Runtime>(
        handle: &tauri::AppHandle<R>,
        threshold: u32,
    ) -> std::result::Result<(), String> {
        let drives = tokio::task::spawn_blocking(list_drives)
            .await
            .map_err(|e| format!("spawn_blocking join: {}", e))?
            .map_err(|e| format!("list_drives: {:?}", e))?;
        for info in drives {
            if !matches!(info.drive_kind, crate::volume::DriveKind::Fixed) {
                continue;
            }
            if info.total_bytes == 0 {
                continue;
            }
            let used = info.total_bytes.saturating_sub(info.free_bytes);
            let pct = ((used as f64 / info.total_bytes as f64) * 100.0).round() as u32;
            if pct >= threshold {
                let alert = TrayMonitorAlert {
                    drive_letter: info.drive_letter.clone(),
                    volume_label: info.volume_label.clone(),
                    usage_percent: pct,
                    total_bytes: info.total_bytes,
                    free_bytes: info.free_bytes,
                };
                let _ = handle.emit("tray-monitor-alert", &alert);
                info!(
                    drive = %info.drive_letter,
                    usage_percent = pct,
                    threshold,
                    "tray monitor alert"
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// USN watcher registry — v0.3.1 prod wire (Bölüm 5.6 canlı delta sync).
// ---------------------------------------------------------------------------

/// Volume başına watcher handle kaydı. Aynı volume için çift spawn engellenir;
/// handle'lar uygulama ömrü boyunca yaşar (drop = kapanma sinyali).
pub struct UsnWatchers(pub std::sync::Mutex<HashMap<String, crate::index::watcher::WatcherHandle>>);

/// Bir volume için USN watcher spawn et (yoksa). Tick closure'ı kendi
/// dedicated SQLite connection'ını lazy açar ve `sync_volume` çağırır —
/// incremental delta + wraparound'ta otomatik full rebuild.
fn ensure_usn_watcher<R: tauri::Runtime>(app: &tauri::AppHandle<R>, volume_id: &str) {
    use tauri::Manager;
    ensure_usn_watcher_inner(app.state::<UsnWatchers>(), volume_id)
}

fn ensure_usn_watcher_inner(registry: tauri::State<'_, UsnWatchers>, volume_id: &str) {
    {
        let map = match registry.0.lock() {
            Ok(m) => m,
            Err(p) => p.into_inner(),
        };
        if map.contains_key(volume_id) {
            return; // zaten izleniyor
        }
    }

    let vid = volume_id.to_string();
    let spawn_result = crate::index::watcher::spawn_watcher(vid.clone(), true, move |v: &str| {
        // Lazy dedicated connection — watcher thread ömrü boyunca yaşar.
        // Açılışta başarısızsa her tick retry eder (60 sn sonra).
        use std::sync::Mutex;
        thread_local! {
            static WATCHER_CONN: Mutex<Option<rusqlite::Connection>> = const { Mutex::new(None) };
        }
        WATCHER_CONN.with(|slot| -> Result<()> {
            let mut guard = match slot.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            if guard.is_none() {
                *guard = Some(
                    open_db().map_err(|e| Error::Index(format!("watcher DB açılışı: {}", e)))?,
                );
            }
            let conn = guard
                .as_mut()
                .ok_or_else(|| Error::Index("watcher conn yok".into()))?;
            sync_volume(v, conn, DEFAULT_BASELINE_BUFFER).map(|_| ())
        })
    });

    match spawn_result {
        Ok(Some(handle)) => {
            let mut map = match registry.0.lock() {
                Ok(m) => m,
                Err(p) => p.into_inner(),
            };
            map.insert(vid.clone(), handle);
            info!(volume = %vid, "USN watcher başlatıldı");
        }
        Ok(None) => {
            // is_admin=false — çağıran zaten elevated kontrolü yapmış olmalı.
            info!(volume = %vid, "USN watcher başlatılamadı (admin değil)");
        }
        Err(e) => {
            warn!(volume = %vid, ?e, "USN watcher spawn hatası");
        }
    }
}

/// Açılışta: watermark'ı olan tüm volumeler için watcher başlat.
/// Admin yoksa sessizce atlanır (index_status needs_admin rozetini gösterir).
fn spawn_usn_watchers<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    if !is_elevated() {
        info!("USN watcher'lar atlandı: admin değil");
        return;
    }
    let db_state = app.state::<DbState>();
    let volumes: Vec<String> = {
        let conn = match db_state.conn.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        match list_indexed_volumes(&conn) {
            Ok(v) => v,
            Err(e) => {
                warn!(?e, "indexed volume listesi okunamadı — watcher atlandı");
                return;
            }
        }
    };
    let registry = app.state::<UsnWatchers>();
    for v in volumes {
        ensure_usn_watcher_inner(registry.clone(), &v);
    }
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_env("DSPACE_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,dspace_lib=debug"));

    let _ = fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_level(true)
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    info!(
        version = AppInfo::current().version,
        spec = AppInfo::current().spec_version,
        "D-Space başlatılıyor"
    );

    let db_state = match open_db() {
        Ok(conn) => DbState::new(conn),
        Err(e) => {
            warn!(?e, "DB açılamadı — geçici in-memory ile devam");
            let mut conn =
                rusqlite::Connection::open_in_memory().expect("in-memory SQLite açılamadı");
            // Gemini review — fallback connection'a da PRAGMA + migration
            // uygula; eskiden şemasız in-memory DB tüm komutları "no such
            // table" ile öldürüyordu.
            if let Err(pe) = db::apply_pragmas(&conn) {
                warn!(?pe, "in-memory PRAGMA hatası");
            }
            if let Err(me) = db::migrations().to_latest(&mut conn) {
                warn!(?me, "in-memory migration hatası");
            }
            DbState::new(conn)
        }
    };

    // Bölüm 12.3 — startup WAL recovery (crash sonrası BEGIN durumundaki
    // entry'leri ABORTED'a çevir, ortalıkta kalan .dspace_tmp temizle).
    {
        let conn = db_state.conn.lock().expect("DB mutex poisoned");
        match recover_wal(&conn) {
            Ok(report) => {
                if report.scanned > 0 {
                    info!(?report, "açılış WAL recovery");
                }
            }
            Err(e) => warn!(?e, "WAL recovery hatası"),
        }
        // Sprint 4.2 — açılışta retention cleanup. Settings'ten okunur,
        // yoksa 90 gün default. Sessizce çalışır; hata olursa loglanır
        // (uygulama açılışını bloklamaz).
        let retain = get_setting(&conn, "snapshot_retain_days")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(crate::snapshot::DEFAULT_RETAIN_DAYS);
        match crate::snapshot::purge_old_snapshots(&conn, retain) {
            Ok(0) => {}
            Ok(n) => info!(
                removed = n,
                retain_days = retain,
                "açılış snapshot retention"
            ),
            Err(e) => warn!(?e, "açılış retention cleanup hatası"),
        }
    }

    tauri::Builder::default()
        .manage(db_state)
        .manage(ScanTreeState::default())
        .manage(ScanCancel::default())
        .manage(UsnWatchers(std::sync::Mutex::new(HashMap::new())))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            build_tray(app)?;
            spawn_tray_monitor(app);
            // v0.3.1 — USN watcher'ları başlat (Bölüm 5.6 canlı index).
            spawn_usn_watchers(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            check_privilege,
            pre_flight_volume,
            list_drives_cmd,
            probe_drive_hardware_cmd,
            scan_unc_path,
            probe_volume,
            walk_volume,
            scan_full,
            scan_cancel,
            tree_top_consumers,
            tree_window,
            tree_path,
            stage_path,
            list_staging,
            undo_staging,
            run_wal_recovery,
            get_db_info,
            capture_snapshot,
            list_snapshots,
            cleanup_old_snapshots,
            snapshot_delta,
            find_duplicates_cmd,
            probe_locked_file_cmd,
            permanent_delete_cmd,
            undo_resolve_staging,
            get_setting_cmd,
            set_setting_cmd,
            list_expired_staging_cmd,
            cleanup_expired_staging_cmd,
            list_user_rules_cmd,
            add_user_rule_cmd,
            delete_user_rule_cmd,
            toggle_user_rule_cmd,
            probe_cloud_state_cmd,
            index_build,
            index_status,
            index_search,
        ])
        .run(tauri::generate_context!())
        .expect("D-Space başlatma hatası");
}
