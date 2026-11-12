// SPDX-License-Identifier: GPL-3.0-or-later
//
// Tauri command surface — DiskWizard.
//
// Thin, snake_case commands over the recovery domain. Argument and
// payload field casing: Tauri camelCases top-level args; wire structs
// carry `rename_all = "camelCase"` so the JS connector maps 1:1.
// Every command is either spawn_blocking-backed (IO) or a cheap
// registry read.

use crate::error::{Error, Result};
use crate::recovery::jobs::{
    items_wire, new_job, report_wire, run_job, RecoveryOptionsWire, RecoverySelectionWire,
};
use crate::recovery::preview::{request_preview, request_thumbnail, PreviewError};
use crate::recovery::results::{
    count_results, node_detail, query_results, result_tree, ResultsFilterWire, ResultsPageWire,
    SortDirectionWire, SortFieldWire, TreePageWire,
};
use crate::recovery::scan::{apply_path_scope, resolve_target, run_scan, scan_state_wire};
use crate::recovery::{
    list_devices_wire, DwDevice, JobRegistry, RecoveryScan, ScanProgressEventWire, ScanRegistry,
    ScanStatusWire,
};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

// ---------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn dw_devices_list() -> Result<Vec<DwDevice>> {
    tokio::task::spawn_blocking(list_devices_wire)
        .await
        .map_err(|e| Error::Volume(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_devices_refresh() -> Result<Vec<DwDevice>> {
    tokio::task::spawn_blocking(list_devices_wire)
        .await
        .map_err(|e| Error::Volume(format!("join error: {e}")))?
}

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStateWire {
    pub scan_id: String,
    pub target_id: String,
    pub target_label: String,
    pub status: ScanStatusWire,
    pub started_at_unix: i64,
    pub completed_at_unix: Option<i64>,
    pub progress: ScanProgressEventWire,
    pub failure: Option<String>,
}

#[tauri::command]
pub async fn dw_scan_start<R: tauri::Runtime>(
    target_id: String,
    path_scope: Option<String>,
    window: tauri::WebviewWindow<R>,
    scans: State<'_, Arc<ScanRegistry>>,
    db: State<'_, crate::db::DbState>,
) -> Result<ScanStartReply> {
    let mut meta = resolve_target(&target_id)?;
    apply_path_scope(&mut meta, path_scope.as_deref())?;

    // One active scan per target — the second start returns the running id.
    if let Some(active) = scans.active_scan_for_target(&target_id) {
        return Ok(ScanStartReply { scan_id: active });
    }

    let scan_id = scans.mint_id();
    let scan = Arc::new(RecoveryScan::new(
        scan_id.clone(),
        meta,
        Arc::new(crate::recovery::collect::RunFlags::default()),
    ));
    scans.insert(scan.clone());
    run_scan(window, scans.inner().clone(), db.inner().clone(), scan);
    Ok(ScanStartReply { scan_id })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStartReply {
    pub scan_id: String,
}

fn scan_or_404(scans: &State<'_, Arc<ScanRegistry>>, scan_id: &str) -> Result<Arc<RecoveryScan>> {
    scans.get(scan_id).ok_or_else(|| {
        Error::Scan(format!(
            "scan '{scan_id}' was not found (it may have been restarted)"
        ))
    })
}

#[tauri::command]
pub fn dw_scan_state(
    scan_id: String,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<ScanStateWire> {
    Ok(scan_state_wire(scan_or_404(&scans, &scan_id)?.as_ref()))
}

#[tauri::command]
pub fn dw_scan_pause(scan_id: String, scans: State<'_, Arc<ScanRegistry>>) -> Result<()> {
    let scan = scan_or_404(&scans, &scan_id)?;
    scan.flags
        .pause
        .store(true, std::sync::atomic::Ordering::Release);
    let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
    if rt.status == ScanStatusWire::Running {
        rt.status = ScanStatusWire::Pausing;
    }
    Ok(())
}

#[tauri::command]
pub fn dw_scan_resume(scan_id: String, scans: State<'_, Arc<ScanRegistry>>) -> Result<()> {
    let scan = scan_or_404(&scans, &scan_id)?;
    scan.flags
        .pause
        .store(false, std::sync::atomic::Ordering::Release);
    let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
    if matches!(rt.status, ScanStatusWire::Paused | ScanStatusWire::Pausing) {
        rt.status = ScanStatusWire::Resuming;
    }
    Ok(())
}

#[tauri::command]
pub fn dw_scan_cancel(scan_id: String, scans: State<'_, Arc<ScanRegistry>>) -> Result<()> {
    let scan = scan_or_404(&scans, &scan_id)?;
    scan.flags
        .pause
        .store(false, std::sync::atomic::Ordering::Release);
    scan.flags
        .cancel
        .store(true, std::sync::atomic::Ordering::Release);
    let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
    if !matches!(
        rt.status,
        ScanStatusWire::Completed | ScanStatusWire::Cancelled | ScanStatusWire::Failed
    ) {
        rt.status = ScanStatusWire::Stopping;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn dw_results_tree(
    scan_id: String,
    parent_id: Option<String>,
    mode: Option<String>,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<TreePageWire> {
    let scan = scan_or_404(&scans, &scan_id)?;
    let mode = mode.unwrap_or_else(|| "path".into());
    tokio::task::spawn_blocking(move || result_tree(&scan, parent_id.as_deref(), &mode))
        .await
        .map_err(|e| Error::Scan(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_results_query(
    scan_id: String,
    filter: Option<ResultsFilterWire>,
    sort_field: Option<SortFieldWire>,
    sort_direction: Option<SortDirectionWire>,
    offset: Option<usize>,
    limit: Option<usize>,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<ResultsPageWire> {
    let scan = scan_or_404(&scans, &scan_id)?;
    let filter = filter.unwrap_or_default();
    let sort_field = sort_field.unwrap_or(SortFieldWire::Modified);
    let sort_direction = sort_direction.unwrap_or(SortDirectionWire::Desc);
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    tokio::task::spawn_blocking(move || {
        query_results(&scan, &filter, sort_field, sort_direction, offset, limit)
    })
    .await
    .map_err(|e| Error::Scan(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_results_count(
    scan_id: String,
    filter: Option<ResultsFilterWire>,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<u64> {
    let scan = scan_or_404(&scans, &scan_id)?;
    let filter = filter.unwrap_or_default();
    tokio::task::spawn_blocking(move || count_results(&scan, &filter))
        .await
        .map_err(|e| Error::Scan(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_node_detail(
    scan_id: String,
    node_id: String,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<crate::recovery::results::NodeDetailWire> {
    let scan = scan_or_404(&scans, &scan_id)?;
    tokio::task::spawn_blocking(move || node_detail(&scan, &node_id))
        .await
        .map_err(|e| Error::Scan(format!("join error: {e}")))?
}

// ---------------------------------------------------------------------------
// Preview
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn dw_preview(
    scan_id: String,
    node_id: String,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<crate::recovery::preview::PreviewWire> {
    let scan = scan_or_404(&scans, &scan_id)?;
    tokio::task::spawn_blocking(move || {
        request_preview(&scan, &node_id).map_err(|e: PreviewError| Error::Scan(e.to_string()))
    })
    .await
    .map_err(|e| Error::Scan(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_thumbnail(
    scan_id: String,
    node_id: String,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<Option<crate::recovery::preview::ThumbWire>> {
    let scan = scan_or_404(&scans, &scan_id)?;
    tokio::task::spawn_blocking(move || request_thumbnail(&scan, &node_id))
        .await
        .map_err(|e| Error::Scan(format!("join error: {e}")))?
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn dw_session_save(
    scan_id: String,
    db: State<'_, crate::db::DbState>,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<crate::recovery::sessions::SessionMetaWire> {
    let scan = scan_or_404(&scans, &scan_id)?;
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || crate::recovery::sessions::save_session(&db, &scan))
        .await
        .map_err(|e| Error::Db(format!("join error: {e}")))?
}

#[tauri::command]
pub fn dw_sessions_list(
    db: State<'_, crate::db::DbState>,
) -> Result<Vec<crate::recovery::sessions::SessionMetaWire>> {
    crate::recovery::sessions::list_sessions(db.inner())
}

#[tauri::command]
pub fn dw_session_load(
    file_name: String,
    db: State<'_, crate::db::DbState>,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<ScanStartReply> {
    let scan_id = crate::recovery::sessions::load_session(db.inner(), scans.inner(), &file_name)?;
    Ok(ScanStartReply { scan_id })
}

// ---------------------------------------------------------------------------
// Recovery jobs
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn dw_recover_start<R: tauri::Runtime>(
    scan_id: String,
    selection: RecoverySelectionWire,
    destination: String,
    options: RecoveryOptionsWire,
    window: tauri::WebviewWindow<R>,
    scans: State<'_, Arc<ScanRegistry>>,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<JobStartReply> {
    let scan = scan_or_404(&scans, &scan_id)?;
    let job_id = jobs.mint_id();
    let options_for_build = options.clone();
    let destination_for_build = destination.clone();
    let built = tokio::task::spawn_blocking(move || {
        crate::recovery::jobs::build_job_items(
            &scan,
            &selection,
            &destination_for_build,
            &options_for_build,
        )
    })
    .await
    .map_err(|e| Error::Staging(format!("join error: {e}")))??;

    let job = Arc::new(new_job(
        job_id.clone(),
        scan_id.clone(),
        destination,
        options,
        built,
    ));
    {
        let mut p = job.progress.write().unwrap_or_else(|p| p.into_inner());
        p.scan_id = scan_id.clone();
    }
    jobs.insert(job.clone());
    let scan_for_job = scan_or_404(&scans, &scan_id)?;
    run_job(window, job, scan_for_job);
    Ok(JobStartReply { job_id })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStartReply {
    pub job_id: String,
}

#[tauri::command]
pub fn dw_recover_cancel(job_id: String, jobs: State<'_, Arc<JobRegistry>>) -> Result<()> {
    let job = jobs
        .get(&job_id)
        .ok_or_else(|| Error::Staging(format!("job '{job_id}' was not found")))?;
    job.cancel.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}

#[tauri::command]
pub fn dw_recover_state(
    job_id: String,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<crate::recovery::jobs::RecoveryProgressWire> {
    let job = jobs
        .get(&job_id)
        .ok_or_else(|| Error::Staging(format!("job '{job_id}' was not found")))?;
    Ok(job.snapshot_progress())
}

#[tauri::command]
pub fn dw_recover_items(
    job_id: String,
    offset: Option<usize>,
    limit: Option<usize>,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<Vec<crate::recovery::jobs::JobItemWire>> {
    let job = jobs
        .get(&job_id)
        .ok_or_else(|| Error::Staging(format!("job '{job_id}' was not found")))?;
    Ok(items_wire(&job, offset.unwrap_or(0), limit.unwrap_or(500)))
}

#[tauri::command]
pub fn dw_recover_report(
    job_id: String,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<crate::recovery::jobs::RecoveryReportWire> {
    let job = jobs
        .get(&job_id)
        .ok_or_else(|| Error::Staging(format!("job '{job_id}' was not found")))?;
    Ok(report_wire(&job))
}

// ---------------------------------------------------------------------------
// Health / Repair / Partitions
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn dw_disk_health(device_id: String) -> Result<crate::recovery::health::DiskHealthWire> {
    tokio::task::spawn_blocking(move || crate::recovery::health::disk_health(&device_id))
        .await
        .map_err(|e| Error::Volume(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_repair_dry_run(
    device_id: String,
) -> Result<crate::recovery::repair::RepairDryRunWire> {
    tokio::task::spawn_blocking(move || crate::recovery::repair::dry_run(&device_id))
        .await
        .map_err(|e| Error::Volume(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_repair_apply(
    device_id: String,
    options: crate::recovery::repair::ApplyOptionsWire,
    undo_log: State<'_, Arc<crate::recovery::repair::RepairUndoLog>>,
) -> Result<u32> {
    let log = undo_log.inner().clone();
    tokio::task::spawn_blocking(move || {
        crate::recovery::repair::apply_repair(&device_id, &options, &log)
    })
    .await
    .map_err(|e| Error::Volume(format!("join error: {e}")))?
}

#[tauri::command]
pub fn dw_repair_undo_log(
    device_id: String,
    undo_log: State<'_, Arc<crate::recovery::repair::RepairUndoLog>>,
) -> Result<Vec<crate::recovery::repair::RepairUndoEntryWire>> {
    Ok(crate::recovery::repair::undo_log_wire(
        &device_id,
        undo_log.inner(),
    ))
}

#[tauri::command]
pub async fn dw_repair_undo(
    device_id: String,
    undo_log: State<'_, Arc<crate::recovery::repair::RepairUndoLog>>,
) -> Result<u32> {
    let log = undo_log.inner().clone();
    tokio::task::spawn_blocking(move || crate::recovery::repair::undo_repairs(&device_id, &log))
        .await
        .map_err(|e| Error::Volume(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_disk_layouts() -> Result<Vec<crate::recovery::partitions::DiskLayoutWire>> {
    tokio::task::spawn_blocking(crate::recovery::partitions::list_disk_layouts)
        .await
        .map_err(|e| Error::Volume(format!("join error: {e}")))?
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfoWire {
    pub name: String,
    pub version: String,
    pub connector_mode: String,
    pub engine_version: String,
    pub platform: String,
}

#[tauri::command]
pub fn dw_app_info() -> AppInfoWire {
    AppInfoWire {
        name: "DiskWizard".into(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        connector_mode: "tauri".into(),
        engine_version: format!(
            "dw-engine {} (MFT+USN+VSS, {})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::ARCH
        ),
        platform: std::env::consts::OS.to_string(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountPointWire {
    pub id: String,
    pub label: String,
    pub path: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

#[tauri::command]
pub async fn dw_mount_points() -> Result<Vec<MountPointWire>> {
    tokio::task::spawn_blocking(|| -> Result<Vec<MountPointWire>> {
        let drives = crate::volume::list_drives()?;
        Ok(drives
            .into_iter()
            .filter(|d| {
                matches!(
                    d.status,
                    crate::volume::VolumeStatus::Ready
                        | crate::volume::VolumeStatus::Encrypted { .. }
                ) && d.total_bytes > 0
            })
            .map(|d| MountPointWire {
                id: format!("vol:{}", d.drive_letter),
                label: if d.volume_label.is_empty() {
                    d.drive_letter.clone()
                } else {
                    format!("{} ({})", d.volume_label, d.drive_letter)
                },
                path: d.root_path.clone(),
                free_bytes: d.free_bytes,
                total_bytes: d.total_bytes,
            })
            .collect::<Vec<_>>())
    })
    .await
    .map_err(|e| Error::Volume(format!("join error: {e}")))?
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationValidationWire {
    pub allowed: bool,
    pub reason: Option<String>,
    pub free_bytes: Option<u64>,
    pub needed_bytes: Option<u64>,
}

#[tauri::command]
pub async fn dw_validate_destination(
    path: String,
    source_device_id: String,
    needed_bytes: Option<u64>,
) -> Result<DestinationValidationWire> {
    tokio::task::spawn_blocking(move || {
        validate_destination(&path, &source_device_id, needed_bytes)
    })
    .await
    .map_err(|e| Error::Volume(format!("join error: {e}")))?
}

fn validate_destination(
    path: &str,
    source_device_id: &str,
    needed_bytes: Option<u64>,
) -> Result<DestinationValidationWire> {
    let p = std::path::Path::new(path);
    if !p.is_absolute() {
        return Ok(DestinationValidationWire {
            allowed: false,
            reason: Some("invalid".into()),
            free_bytes: None,
            needed_bytes: None,
        });
    }
    // Existence / writability.
    if !p.exists() {
        return Ok(DestinationValidationWire {
            allowed: false,
            reason: Some("invalid".into()),
            free_bytes: None,
            needed_bytes: None,
        });
    }
    // Source overlap: destination letter must differ from source volume.
    let src_letter = source_device_id
        .trim_start_matches("dev:")
        .trim_start_matches("vol:")
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic());
    let dest_letter = path.chars().next().filter(|c| c.is_ascii_alphabetic());
    if let (Some(s), Some(d)) = (src_letter, dest_letter) {
        if s.eq_ignore_ascii_case(&d) {
            return Ok(DestinationValidationWire {
                allowed: false,
                reason: Some("overlap".into()),
                free_bytes: None,
                needed_bytes: None,
            });
        }
    }
    // Free space.
    if let Some((free, _total)) = crate::recovery::partitions::unc_space(
        &path
            .chars()
            .take_while(|&c| c != '\\' || path.starts_with(r"\\"))
            .collect::<String>()
            .to_string(),
    ) {
        let _ = free;
    }
    // Per-root free space via the drive list (robust for local paths).
    let letter = dest_letter.map(|c| c.to_ascii_uppercase());
    if let Some(letter) = letter {
        let drives = crate::volume::list_drives()?;
        if let Some(d) = drives.iter().find(|d| d.drive_letter.starts_with(letter)) {
            let free = d.free_bytes;
            if let Some(needed) = needed_bytes {
                if needed > free {
                    return Ok(DestinationValidationWire {
                        allowed: false,
                        reason: Some("out-of-space".into()),
                        free_bytes: Some(free),
                        needed_bytes: Some(needed),
                    });
                }
            }
            return Ok(DestinationValidationWire {
                allowed: true,
                reason: None,
                free_bytes: Some(free),
                needed_bytes,
            });
        }
    }
    Ok(DestinationValidationWire {
        allowed: false,
        reason: Some("invalid".into()),
        free_bytes: None,
        needed_bytes: None,
    })
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecialFoldersWire {
    pub recycle_bin: Option<String>,
    pub desktop: Option<String>,
    pub documents: Option<String>,
    pub downloads: Option<String>,
}

#[tauri::command]
pub async fn dw_special_folders() -> SpecialFoldersWire {
    tokio::task::spawn_blocking(special_folders)
        .await
        .unwrap_or_default()
}

fn special_folders() -> SpecialFoldersWire {
    let desktop = dirs::desktop_dir().map(|p| p.to_string_lossy().to_string());
    let documents = dirs::document_dir().map(|p| p.to_string_lossy().to_string());
    let downloads = dirs::download_dir().map(|p| p.to_string_lossy().to_string());
    let recycle_bin = std::env::var("SystemDrive")
        .ok()
        .map(|sd| format!("{sd}\\$Recycle.Bin"));
    SpecialFoldersWire {
        recycle_bin,
        desktop,
        documents,
        downloads,
    }
}

#[tauri::command]
pub async fn dw_pick_directory(
    hint_volume_id: Option<String>,
    exclude_device_id: Option<String>,
    app: tauri::AppHandle<tauri::Wry>,
) -> Result<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    // The native folder picker is invoked from the main thread context via
    // the blocking API on a worker thread.
    let handle = app.clone();
    let picked = tokio::task::spawn_blocking(move || {
        handle
            .dialog()
            .file()
            .blocking_pick_folder()
            .map(|f| f.to_string())
    })
    .await
    .map_err(|e| Error::Volume(format!("join error: {e}")))?;
    let _ = (hint_volume_id, exclude_device_id);
    Ok(picked.map(|s| s.trim_start_matches(r"\\?\").to_string()))
}

#[tauri::command]
pub async fn dw_open_path(path: String) -> Result<()> {
    tokio::task::spawn_blocking(move || reveal_in_explorer(&path))
        .await
        .map_err(|e| Error::Volume(format!("join error: {e}")))?
}

fn reveal_in_explorer(path: &str) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let p = std::path::Path::new(path);
        let (target, select) = if p.is_dir() {
            (p.to_string_lossy().to_string(), false)
        } else if let Some(_parent) = p.parent() {
            (format!("/select,{path}"), true)
        } else {
            (path.to_string(), false)
        };
        let _ = select;
        let mut cmd = std::process::Command::new("explorer");
        if !select {
            cmd.arg(&target);
        } else {
            cmd.arg(format!("/select,{path}"));
        }
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        cmd.spawn()
            .map_err(|e| Error::Volume(format!("explorer launch failed: {e}")))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(Error::NotImplemented("explorer reveal is Windows-only"))
    }
}

// ---------------------------------------------------------------------------
// Cleanup domain (D-Space features retained in DiskWizard)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn dw_stage_path(
    path: String,
    db: State<'_, crate::db::DbState>,
) -> Result<crate::staging::StagedItem> {
    let conn = db.inner().conn.clone();
    tokio::task::spawn_blocking(move || {
        let conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
        crate::staging::stage(&path, &conn)
    })
    .await
    .map_err(|e| Error::Staging(format!("join error: {e}")))?
}

#[tauri::command]
pub fn dw_list_staging(
    db: State<'_, crate::db::DbState>,
) -> Result<Vec<crate::staging::StagedItem>> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    crate::staging::list_pending(&conn)
}

#[tauri::command]
pub async fn dw_undo_staging(
    id: i64,
    db: State<'_, crate::db::DbState>,
) -> Result<crate::staging::UndoOutcome> {
    let conn = db.inner().conn.clone();
    tokio::task::spawn_blocking(move || {
        let conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
        crate::staging::undo(id, &conn)
    })
    .await
    .map_err(|e| Error::Staging(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_undo_resolve(
    id: i64,
    resolution: crate::staging::ConflictResolution,
    db: State<'_, crate::db::DbState>,
) -> Result<crate::staging::UndoOutcome> {
    let conn = db.inner().conn.clone();
    tokio::task::spawn_blocking(move || {
        let conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
        crate::staging::undo_with_resolution(id, resolution, &conn)
    })
    .await
    .map_err(|e| Error::Staging(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_permanent_delete(
    id: i64,
    confirm_phrase: String,
    secure_wipe: Option<bool>,
    db: State<'_, crate::db::DbState>,
) -> Result<crate::staging::PermanentDeleteResult> {
    let conn = db.inner().conn.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
        crate::staging::permanent_delete(
            id,
            &confirm_phrase,
            secure_wipe.unwrap_or(false),
            &mut conn,
        )
    })
    .await
    .map_err(|e| Error::Staging(format!("join error: {e}")))?
}

#[tauri::command]
pub fn dw_list_expired(
    db: State<'_, crate::db::DbState>,
) -> Result<Vec<crate::staging::ExpiredItem>> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    crate::staging::list_expired(&conn)
}

#[tauri::command]
pub async fn dw_cleanup_expired(
    rate_per_sec: Option<u32>,
    force: Option<bool>,
    db: State<'_, crate::db::DbState>,
) -> Result<crate::staging::CleanupReport> {
    let conn = db.inner().conn.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
        crate::staging::cleanup_expired(
            &mut conn,
            rate_per_sec.unwrap_or(crate::staging::expiry::DEFAULT_CLEANUP_RATE_PER_SEC),
            force.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| Error::Staging(format!("join error: {e}")))?
}

#[tauri::command]
pub fn dw_rules_list(
    db: State<'_, crate::db::DbState>,
) -> Result<Vec<crate::safe_delete::UserRule>> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    crate::safe_delete::list_rules(&conn)
}

#[tauri::command]
pub fn dw_rule_add(
    pattern: String,
    pattern_type: crate::safe_delete::UserPatternType,
    score: u8,
    explanation: String,
    db: State<'_, crate::db::DbState>,
) -> Result<crate::safe_delete::UserRule> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    crate::safe_delete::add_rule(&conn, &pattern, pattern_type, score, &explanation)
}

#[tauri::command]
pub fn dw_rule_delete(id: i64, db: State<'_, crate::db::DbState>) -> Result<()> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    crate::safe_delete::delete_rule(&conn, id)
}

#[tauri::command]
pub fn dw_rule_toggle(id: i64, enabled: bool, db: State<'_, crate::db::DbState>) -> Result<()> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    crate::safe_delete::toggle_rule(&conn, id, enabled)
}

#[tauri::command]
pub async fn dw_duplicates_find(
    scan_id: String,
    min_size_bytes: Option<u64>,
    size_only: Option<bool>,
    max_groups: Option<usize>,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<crate::duplicate::DuplicateScanResult> {
    let scan = scan_or_404(&scans, &scan_id)?;
    let letter = scan.meta.drive_letter.ok_or_else(|| {
        Error::Duplicate("duplicates are only available for letter volumes".into())
    })?;
    let tree = {
        let guard = scan.tree.read().unwrap_or_else(|p| p.into_inner());
        guard
            .clone()
            .ok_or_else(|| Error::Duplicate("scan results are not ready".into()))?
    };
    let opts = crate::duplicate::DuplicateOptions {
        min_size_bytes: min_size_bytes.unwrap_or(crate::duplicate::DEFAULT_MIN_DUP_SIZE),
        size_only: size_only.unwrap_or(false),
        skip_head_prefilter: false,
        max_groups: max_groups.unwrap_or(500),
    };
    tokio::task::spawn_blocking(move || crate::duplicate::find_duplicates(&tree, letter, opts))
        .await
        .map_err(|e| Error::Duplicate(format!("join error: {e}")))?
}

#[tauri::command]
pub async fn dw_snapshot_capture(
    scan_id: String,
    db: State<'_, crate::db::DbState>,
    scans: State<'_, Arc<ScanRegistry>>,
) -> Result<crate::snapshot::SnapshotMeta> {
    let scan = scan_or_404(&scans, &scan_id)?;
    let tree = {
        let guard = scan.tree.read().unwrap_or_else(|p| p.into_inner());
        guard
            .clone()
            .ok_or_else(|| Error::Snapshot("scan results are not ready".into()))?
    };
    let conn = db.inner().conn.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
        crate::snapshot::capture_snapshot(&tree, &mut conn)
    })
    .await
    .map_err(|e| Error::Snapshot(format!("join error: {e}")))?
}

#[tauri::command]
pub fn dw_snapshots_list(
    db: State<'_, crate::db::DbState>,
) -> Result<Vec<crate::snapshot::SnapshotMeta>> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    crate::snapshot::list_snapshots(&conn)
}

#[tauri::command]
pub async fn dw_snapshot_delta(
    from: i64,
    to: i64,
    db: State<'_, crate::db::DbState>,
) -> Result<crate::snapshot::DeltaResult> {
    let conn = db.inner().conn.clone();
    tokio::task::spawn_blocking(move || {
        let conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
        crate::snapshot::compute_delta(from, to, &conn)
    })
    .await
    .map_err(|e| Error::Snapshot(format!("join error: {e}")))?
}

#[tauri::command]
pub fn dw_snapshots_cleanup(
    retain_days: Option<i64>,
    db: State<'_, crate::db::DbState>,
) -> Result<u64> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    let effective = retain_days.unwrap_or(crate::snapshot::DEFAULT_RETAIN_DAYS);
    crate::snapshot::purge_old_snapshots(&conn, effective)
}

#[tauri::command]
pub async fn dw_index_search(
    query: String,
    limit: Option<usize>,
    db: State<'_, crate::db::DbState>,
) -> Result<Vec<crate::index::IndexSearchResult>> {
    let conn = db.inner().conn.clone();
    tokio::task::spawn_blocking(move || {
        let conn = conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
        crate::index::index_search(&conn, &query, limit.unwrap_or(50))
    })
    .await
    .map_err(|e| Error::Index(format!("join error: {e}")))?
}

#[tauri::command]
pub fn dw_get_setting(key: String, db: State<'_, crate::db::DbState>) -> Result<Option<String>> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    crate::db::get_setting(&conn, &key)
}

#[tauri::command]
pub fn dw_set_setting(key: String, value: String, db: State<'_, crate::db::DbState>) -> Result<()> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    crate::db::set_setting(&conn, &key, &value)
}
