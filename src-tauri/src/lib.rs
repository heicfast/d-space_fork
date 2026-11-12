// SPDX-License-Identifier: GPL-3.0-or-later
// DiskWizard — Windows data recovery wizard and safe disk cleanup.
// Copyright (C) 2026 DiskWizard contributors
//
// Engine lineage: the D-Space engine (scan/staging/snapshot/duplicate/
// index modules, see docs/ and the frozen spec D-Space-Mimari-v1.4.docx)
// extended with the DiskWizard recovery domain (`recovery/`), which
// implements the connector contract in docs/CONNECTOR-CONTRACT.md.

pub mod db;
pub mod duplicate;
pub mod error;
pub mod fsfat;
pub mod index;
pub mod locked_file;
pub mod recovery;
pub mod safe_delete;
pub mod scan;
pub mod snapshot;
pub mod staging;
pub mod v2;
pub mod volume;

use crate::db::{get_setting, open_db, DbState};
use crate::error::Error;
use crate::recovery::repair::RepairUndoLog;
use crate::recovery::{JobRegistry, ScanRegistry};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Tray live monitor (retained D-Space feature)
// ---------------------------------------------------------------------------

/// `tray-monitor-alert` event payload.
#[derive(Debug, Clone, Serialize)]
pub struct TrayMonitorAlert {
    pub drive_letter: String,
    pub volume_label: String,
    pub usage_percent: u32,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

const TRAY_MONITOR_DEFAULT_INTERVAL_MIN: u64 = 30;
const TRAY_MONITOR_DEFAULT_THRESHOLD: u32 = 90;
const TRAY_MONITOR_MIN_INTERVAL_MIN: u64 = 5;

#[cfg(windows)]
fn is_on_battery() -> bool {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetSystemPowerStatus(&mut status) };
    if ok.is_err() {
        return true;
    }
    status.ACLineStatus == 0 || status.ACLineStatus == 255
}

#[cfg(not(windows))]
fn is_on_battery() -> bool {
    false
}

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

/// System tray: brand icon + open / scan C: / quit, plus the background
/// full-disk monitor loop.
fn build_tray<R: tauri::Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuEvent};
    use tauri::tray::TrayIconBuilder;
    use tauri::{Emitter, Manager};

    let menu = MenuBuilder::new(app)
        .text("open", "Open DiskWizard")
        .text("scan_c", "Scan C:")
        .separator()
        .text("quit", "Quit")
        .build()?;

    let Some(icon) = app.default_window_icon().cloned() else {
        warn!("no default window icon — tray skipped");
        return Ok(());
    };

    TrayIconBuilder::with_id("diskwizard-tray")
        .icon(icon)
        .menu(&menu)
        .tooltip("DiskWizard — recovery, cleanup, confidence")
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
                        // The UI listens and routes to the scan page.
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

    info!("system tray ready");
    Ok(())
}

fn spawn_tray_monitor<R: tauri::Runtime>(app: &tauri::App<R>) {
    use tauri::{Emitter, Manager};

    fn read_settings<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) -> Option<(bool, u64, u32)> {
        let db_state = handle.state::<DbState>();
        let conn = db_state.conn.lock().ok()?;
        Some(read_tray_monitor_settings(&conn))
    }

    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        // First tick delayed so the launch payload stays quiet.
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        loop {
            let (enabled, interval_min, threshold) = read_settings(&handle).unwrap_or((
                false,
                TRAY_MONITOR_DEFAULT_INTERVAL_MIN,
                TRAY_MONITOR_DEFAULT_THRESHOLD,
            ));

            if enabled {
                if let Err(e) = tray_monitor_tick(&handle, threshold).await {
                    warn!(?e, "tray monitor tick failed");
                }
            }
            // Battery-aware throttle: double the interval on battery.
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

    async fn tray_monitor_tick<R: tauri::Runtime>(
        handle: &tauri::AppHandle<R>,
        threshold: u32,
    ) -> std::result::Result<(), String> {
        let drives = tokio::task::spawn_blocking(volume::list_drives)
            .await
            .map_err(|e| format!("spawn_blocking join: {e}"))?
            .map_err(|e| format!("list_drives: {e:?}"))?;
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
// USN watcher registry (live index sync — retained D-Space feature)
// ---------------------------------------------------------------------------

pub struct UsnWatchers(pub std::sync::Mutex<HashMap<String, crate::index::watcher::WatcherHandle>>);

pub(crate) fn ensure_usn_watcher<R: tauri::Runtime>(app: &tauri::AppHandle<R>, volume_id: &str) {
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
            return;
        }
    }

    let vid = volume_id.to_string();
    let spawn_result = crate::index::watcher::spawn_watcher(vid.clone(), true, move |v: &str| {
        use std::sync::Mutex;
        thread_local! {
            static WATCHER_CONN: Mutex<Option<rusqlite::Connection>> = const { Mutex::new(None) };
        }
        WATCHER_CONN.with(|slot| -> crate::error::Result<()> {
            let mut guard = match slot.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            if guard.is_none() {
                *guard =
                    Some(open_db().map_err(|e| Error::Index(format!("watcher DB open: {e}")))?);
            }
            let conn = guard
                .as_mut()
                .ok_or_else(|| Error::Index("watcher connection missing".into()))?;
            crate::index::sync_volume(v, conn, crate::index::DEFAULT_BASELINE_BUFFER).map(|_| ())
        })
    });

    match spawn_result {
        Ok(Some(handle)) => {
            let mut map = match registry.0.lock() {
                Ok(m) => m,
                Err(p) => p.into_inner(),
            };
            map.insert(vid.clone(), handle);
            info!(volume = %vid, "USN watcher started");
        }
        Ok(None) => {
            info!(volume = %vid, "USN watcher not started (not elevated)");
        }
        Err(e) => {
            warn!(volume = %vid, ?e, "USN watcher spawn failed");
        }
    }
}

fn spawn_usn_watchers<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    if !crate::scan::is_elevated() {
        info!("USN watchers skipped: not elevated");
        return;
    }
    let db_state = app.state::<DbState>();
    let volumes: Vec<String> = {
        let conn = match db_state.conn.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        match crate::index::list_indexed_volumes(&conn) {
            Ok(v) => v,
            Err(e) => {
                warn!(?e, "indexed volume list unavailable — watchers skipped");
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

    let filter = EnvFilter::try_from_env("DISKWIZARD_LOG")
        .or_else(|_| EnvFilter::try_from_env("DSPACE_LOG"))
        .unwrap_or_else(|_| EnvFilter::new("info,diskwizard_lib=debug"));

    let _ = fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_level(true)
        .try_init();
}

/// Installs a panic hook that logs every panic (message + location) to
/// the tracing sink before the default handler runs. Worker threads wrap
/// their bodies in `catch_unwind`, so a panic surfaces as a Failed scan /
/// job with a log trail instead of a silent process death.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".into());
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".into());
        tracing::error!(
            %location,
            message = %message,
            thread = std::thread::current().name().unwrap_or("<unnamed>"),
            "panic caught"
        );
        default(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    install_panic_hook();
    info!(version = env!("CARGO_PKG_VERSION"), "DiskWizard starting");

    let db_state = match open_db() {
        Ok(conn) => DbState::new(conn),
        Err(e) => {
            warn!(?e, "DB open failed — continuing with an in-memory database");
            let mut conn =
                rusqlite::Connection::open_in_memory().expect("cannot open in-memory SQLite");
            if let Err(pe) = db::apply_pragmas(&conn) {
                warn!(?pe, "in-memory PRAGMA error");
            }
            if let Err(me) = db::migrations().to_latest(&mut conn) {
                warn!(?me, "in-memory migration error");
            }
            DbState::new(conn)
        }
    };

    // Startup WAL recovery + snapshot retention (crash safety).
    {
        let conn = db_state.conn.lock().expect("DB mutex poisoned");
        match crate::staging::recover_wal(&conn) {
            Ok(report) => {
                if report.scanned > 0 {
                    info!(?report, "startup WAL recovery");
                }
            }
            Err(e) => warn!(?e, "WAL recovery failed"),
        }
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
                "startup snapshot retention"
            ),
            Err(e) => warn!(?e, "startup retention cleanup failed"),
        }
    }

    tauri::Builder::default()
        .manage(db_state)
        .manage(Arc::new(ScanRegistry::default()))
        .manage(Arc::new(JobRegistry::default()))
        .manage(Arc::new(RepairUndoLog::default()))
        .manage(UsnWatchers(std::sync::Mutex::new(HashMap::new())))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus the existing window on a second launch attempt.
            use tauri::Manager;
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .setup(|app| {
            build_tray(app)?;
            spawn_tray_monitor(app);
            spawn_usn_watchers(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Devices
            recovery::commands::dw_devices_list,
            recovery::commands::dw_devices_refresh,
            // Scan
            recovery::commands::dw_scan_start,
            recovery::commands::dw_scan_state,
            recovery::commands::dw_scan_pause,
            recovery::commands::dw_scan_resume,
            recovery::commands::dw_scan_cancel,
            // Results
            recovery::commands::dw_results_tree,
            recovery::commands::dw_results_query,
            recovery::commands::dw_results_count,
            recovery::commands::dw_node_detail,
            // Preview
            recovery::commands::dw_preview,
            recovery::commands::dw_thumbnail,
            // Sessions
            recovery::commands::dw_session_save,
            recovery::commands::dw_sessions_list,
            recovery::commands::dw_session_load,
            // Recovery jobs
            recovery::commands::dw_recover_start,
            recovery::commands::dw_recover_cancel,
            recovery::commands::dw_recover_state,
            recovery::commands::dw_recover_items,
            recovery::commands::dw_recover_report,
            // Health / repair / partitions
            recovery::commands::dw_disk_health,
            recovery::commands::dw_repair_dry_run,
            recovery::commands::dw_repair_apply,
            recovery::commands::dw_repair_undo_log,
            recovery::commands::dw_repair_undo,
            recovery::commands::dw_disk_layouts,
            // System
            recovery::commands::dw_app_info,
            recovery::commands::dw_mount_points,
            recovery::commands::dw_validate_destination,
            recovery::commands::dw_special_folders,
            recovery::commands::dw_pick_directory,
            recovery::commands::dw_open_path,
            // Cleanup domain (D-Space features)
            recovery::commands::dw_stage_path,
            recovery::commands::dw_list_staging,
            recovery::commands::dw_undo_staging,
            recovery::commands::dw_undo_resolve,
            recovery::commands::dw_permanent_delete,
            recovery::commands::dw_list_expired,
            recovery::commands::dw_cleanup_expired,
            recovery::commands::dw_rules_list,
            recovery::commands::dw_rule_add,
            recovery::commands::dw_rule_delete,
            recovery::commands::dw_rule_toggle,
            recovery::commands::dw_duplicates_find,
            recovery::commands::dw_snapshot_capture,
            recovery::commands::dw_snapshots_list,
            recovery::commands::dw_snapshot_delta,
            recovery::commands::dw_snapshots_cleanup,
            recovery::commands::dw_index_search,
            recovery::commands::dw_get_setting,
            recovery::commands::dw_set_setting,
        ])
        .run(tauri::generate_context!())
        .expect("DiskWizard failed to start");
}
