// SPDX-License-Identifier: GPL-3.0-or-later
//
// Scan orchestration — DiskWizard recovery engine, phase 4.
//
// Stage machine over the unified pipeline:
//   volume-analysis (preflight + NTFS probe, 4%)
//   → quick-index (raw MFT walk incl. deleted records, 71%)  [NTFS + admin]
//     or deep-walk (FindFirstFile fallback, 71%)              [other cases]
//   → tree-build (hierarchy + scoring + confidence, 25%)
//
// Emits `dw-scan-progress` events; supports pause / resume / cancel.
// Cancelled MFT scans keep their partial results (browsable), mirroring
// the product contract. Completed scans auto-persist a session.

use crate::error::{Error, Result};
use crate::recovery::collect::{ClusterBitmap, CollectedScan};
use crate::recovery::deleted::grade_deleted;
use crate::recovery::{
    emit_scan_progress, RecoveryScan, ScanMeta, ScanRegistry, ScanStageWire, ScanStatusWire,
    ScanTarget,
};
use crate::scan::tree::build_tree_with_user_rules;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;
use tracing::{info, warn};

/// Stage weights (sum to 1.0).
const W_VOLUME_ANALYSIS: f64 = 0.04;
const W_WALK: f64 = 0.71;
const W_TREE_BUILD: f64 = 0.25;

/// Resolves a UI target id to scan metadata.
/// Accepted forms: `C` / `C:` / `vol:C` / `dev:C` (letter volume),
/// `\\server\share[...]` (network share), `lp:<drive>:<base>` (lost
/// partition, resolved via the partitions module).
pub fn resolve_target(target_id: &str) -> Result<ScanMeta> {
    let tid = target_id.trim();
    if tid.starts_with(r"\\") {
        return Ok(ScanMeta {
            target: ScanTarget::Unc {
                root: tid.trim_end_matches('\\').to_string(),
            },
            target_id: tid.to_string(),
            path_scope: None,
            target_label: tid.to_string(),
            drive_letter: None,
            fs: "network".into(),
            volume_id: tid.to_string(),
        });
    }
    if let Some(rest) = tid.strip_prefix("lp:") {
        return crate::recovery::partitions::resolve_lost_target(rest);
    }
    // Letter volume (strip vol:/dev: prefixes).
    let letter_part = tid
        .strip_prefix("vol:")
        .or_else(|| tid.strip_prefix("dev:"))
        .unwrap_or(tid);
    let letter = crate::volume::parse_drive_letter(letter_part)
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| {
            Error::Volume(format!(
                "invalid scan target '{tid}' (expected a drive letter, a UNC path or a lost-partition id)"
            ))
        })?;
    let volume_path = format!(r"\\.\{letter}:");
    let volume_id = volume_path.clone();
    let info = crate::volume::pre_flight_check(&letter.to_string()).ok();
    let fs = info
        .as_ref()
        .map(|i| i.file_system.clone())
        .unwrap_or_default();
    let label = info
        .as_ref()
        .map(|i| {
            if i.volume_label.is_empty() {
                format!("Local Disk ({letter}:)")
            } else {
                format!("{} ({letter}:)", i.volume_label)
            }
        })
        .unwrap_or_else(|| format!("Local Disk ({letter}:)"));
    Ok(ScanMeta {
        target: ScanTarget::Volume {
            letter,
            volume_path,
        },
        target_id: tid.to_string(),
        path_scope: None,
        target_label: label,
        drive_letter: Some(letter),
        fs,
        volume_id,
    })
}

/// Sets the path scope on the meta (form validation only; existence is
/// checked during the scan against the built tree).
pub fn apply_path_scope(meta: &mut ScanMeta, scope: Option<&str>) -> Result<()> {
    let Some(scope) = scope else { return Ok(()) };
    let s = scope.trim();
    if s.is_empty() {
        return Ok(());
    }
    match &meta.target {
        ScanTarget::Volume { letter, .. } => {
            // Absolute on-volume ("C:\x" or "C:/x") or volume-relative ("x").
            let normalized =
                if s.len() >= 2 && s.as_bytes()[0].is_ascii_alphabetic() && s.as_bytes()[1] == b':'
                {
                    let drive = s.chars().next().unwrap().to_ascii_uppercase();
                    if drive != *letter {
                        return Err(Error::Scan(format!(
                            "scope '{s}' is not on volume {letter}:"
                        )));
                    }
                    s[2..].replace('/', "\\")
                } else {
                    s.replace('/', "\\")
                };
            let cleaned = normalized.trim_matches('\\').to_string();
            if !cleaned.is_empty() {
                meta.path_scope = Some(cleaned);
            }
            Ok(())
        }
        ScanTarget::Unc { root } => {
            let cleaned = if let Some(stripped) = s.strip_prefix(root.as_str()) {
                stripped.replace('/', "\\")
            } else {
                s.replace('/', "\\")
            };
            let cleaned = cleaned.trim_matches('\\').to_string();
            if !cleaned.is_empty() {
                meta.path_scope = Some(cleaned);
            }
            Ok(())
        }
        ScanTarget::LostPartition { .. } => Err(Error::Scan(
            "path scoping is not supported for lost-partition scans".into(),
        )),
    }
}

/// Runs the whole scan pipeline on a dedicated thread, keeping the
/// `RecoveryScan` in the registry updated. The thread is *detached*:
/// `dw_scan_start` returns as soon as the scan id is minted and the UI
/// follows progress via `dw_scan_state` / `dw-scan-progress` events (the
/// previous design joined the thread inside the async command, freezing
/// the invoke for the entire scan). Status of completed/cancelled scans
/// lives in the runtime state; panics are caught (unwind) and mapped to
/// `status = failed` with the panic message.
pub fn run_scan<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    registry: Arc<ScanRegistry>,
    db: crate::db::DbState,
    scan: Arc<RecoveryScan>,
) {
    let app = window.app_handle().clone();
    let scan_for_task = scan.clone();
    let registry_for_task = registry.clone();
    let db_for_task = db;
    let window_for_task = window.clone();
    let spawn_result = std::thread::Builder::new()
        .name(format!("dw-scan-{}", scan.scan_id))
        .spawn(move || {
            let scan = scan_for_task;
            let window = window_for_task;
            let inner_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_scan_inner(window.clone(), scan.clone())
            }))
            .unwrap_or_else(|panic_payload| {
                let msg = panic_payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "scan thread panicked".into());
                Err(Error::Scan(format!("scan thread panicked: {msg}")))
            });

            if let Err(e) = inner_result {
                {
                    let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
                    rt.status = ScanStatusWire::Failed;
                    rt.failure = Some(e.to_string());
                    rt.completed_at_unix = Some(crate::recovery::now_unix());
                }
                warn!(scan = %scan.scan_id, error = %e, "recovery scan failed");
                emit_scan_progress(&window, &scan);
            }

            // Auto-persist a session on completion (cancelled scans keep
            // their partial session too — it is honest, browsable data).
            let status = scan.runtime_snapshot().status;
            if matches!(
                status,
                ScanStatusWire::Completed | ScanStatusWire::Cancelled
            ) {
                if let Err(e) = crate::recovery::sessions::auto_save_session(&db_for_task, &scan) {
                    warn!(scan = %scan.scan_id, error = %e, "session auto-save failed");
                }
                // Keep the USN quick-search index warm for scanned letter
                // volumes.
                if let ScanTarget::Volume { volume_path, .. } = &scan.meta.target {
                    crate::ensure_usn_watcher(&app, volume_path);
                }
            }
            registry_for_task.insert(scan.clone());
            info!(scan = %scan.scan_id, ?status, "recovery scan finished");
        });
    if let Err(e) = spawn_result {
        let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
        rt.status = ScanStatusWire::Failed;
        rt.failure = Some(format!("cannot start scan thread: {e}"));
        rt.completed_at_unix = Some(crate::recovery::now_unix());
        warn!(scan = %scan.scan_id, error = %e, "cannot spawn scan thread");
        emit_scan_progress(&window, &scan);
    }
}

fn set_stage<R: tauri::Runtime>(
    scan: &Arc<RecoveryScan>,
    window: &tauri::WebviewWindow<R>,
    stage: ScanStageWire,
    progress: f64,
) {
    {
        let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
        let base = match stage {
            ScanStageWire::VolumeAnalysis => 0.0,
            ScanStageWire::QuickIndex | ScanStageWire::DeepWalk => W_VOLUME_ANALYSIS,
            ScanStageWire::TreeBuild => W_VOLUME_ANALYSIS + W_WALK,
            ScanStageWire::Done => 1.0,
        };
        rt.stage = stage;
        rt.stage_progress = progress;
        rt.overall_progress = (base + progress * stage_weight(stage)).clamp(0.0, 0.999);
    }
    emit_scan_progress(window, scan);
}

fn stage_weight(stage: ScanStageWire) -> f64 {
    match stage {
        ScanStageWire::VolumeAnalysis => W_VOLUME_ANALYSIS,
        ScanStageWire::QuickIndex | ScanStageWire::DeepWalk => W_WALK,
        ScanStageWire::TreeBuild => W_TREE_BUILD,
        ScanStageWire::Done => 1.0,
    }
}

fn run_scan_inner<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    scan: Arc<RecoveryScan>,
) -> Result<()> {
    // ---- Stage: volume-analysis ------------------------------------
    set_stage(&scan, &window, ScanStageWire::VolumeAnalysis, 0.0);
    {
        let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
        rt.status = ScanStatusWire::Running;
    }
    emit_scan_progress(&window, &scan);

    let user_rules = load_user_rules();

    match &scan.meta.target {
        ScanTarget::Volume { letter, .. } => {
            let letter_s = letter.to_string();
            let info = crate::volume::pre_flight_check(&letter_s)
                .map_err(|e| Error::Volume(format!("volume {letter}: is not accessible: {e}")))?;
            if info.total_bytes == 0 {
                return Err(Error::Volume(format!("volume {letter}: is not mounted")));
            }
            let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
            rt.bytes_total = info.total_bytes.saturating_sub(info.free_bytes);
        }
        ScanTarget::Unc { root } => {
            let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
            if let Some((free, total)) = crate::recovery::partitions::unc_space(root) {
                rt.bytes_total = total.saturating_sub(free);
            }
        }
        ScanTarget::LostPartition { size_bytes, .. } => {
            let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
            rt.bytes_total = *size_bytes;
        }
    }
    set_stage(&scan, &window, ScanStageWire::VolumeAnalysis, 1.0);

    // ---- Stage: quick-index / deep-walk ----------------------------
    // Engine routing by filesystem: NTFS -> MFT engine; FAT12/16/32 and
    // exFAT -> FAT engine (both collect existing + deleted in one pass);
    // anything else (ReFS, network, unknown FS without admin) -> the
    // FindFirstFile fallback (existing files only).
    let fs_name = scan.meta.fs.to_ascii_uppercase();
    let raw_target = matches!(
        scan.meta.target,
        ScanTarget::Volume { .. } | ScanTarget::LostPartition { .. }
    );
    let fat_capable = raw_target
        && matches!(
            fs_name.as_str(),
            "FAT12" | "FAT16" | "FAT32" | "EXFAT" | "FAT"
        );
    let ext_capable = raw_target && matches!(fs_name.as_str(), "EXT2" | "EXT3" | "EXT4" | "EXT");
    let ntfs_capable = match &scan.meta.target {
        ScanTarget::Volume { letter, .. } => {
            crate::scan::tree::is_mft_compatible(&letter.to_string())
        }
        ScanTarget::LostPartition { .. } => scan.meta.fs == "ntfs",
        ScanTarget::Unc { .. } => false,
    };
    let elevated = crate::scan::is_elevated();
    let use_mft = ntfs_capable && elevated;
    let use_fat = fat_capable && elevated;
    let use_ext = ext_capable && elevated;

    let mut strategy: &'static str = "fallback";
    let finalize_input: FinalizeInput = if use_mft {
        let stage = ScanStageWire::QuickIndex;
        set_stage(&scan, &window, stage, 0.0);
        let mut reader = crate::recovery::partitions::open_target_reader(&scan.meta.target)?;
        let cb = make_progress_cb(&scan, &window);
        let cb_ref: &dyn Fn(&crate::recovery::collect::CollectProgress) = &cb;
        let collected = crate::recovery::collect::collect_mft_all_reader(
            &mut reader,
            &scan.meta.volume_id,
            &scan.flags,
            Some(cb_ref),
        )?;
        strategy = "MFT";
        FinalizeInput::Mft(collected)
    } else if use_fat {
        let stage = ScanStageWire::QuickIndex;
        set_stage(&scan, &window, stage, 0.0);
        let mut reader = crate::recovery::partitions::open_target_reader(&scan.meta.target)?;
        let cb = make_progress_cb(&scan, &window);
        let cb_ref: &dyn Fn(&crate::recovery::collect::CollectProgress) = &cb;
        let collected = crate::fsfat::collect_fat(
            &mut reader,
            &scan.meta.volume_id,
            &scan.flags,
            Some(cb_ref),
        )?;
        strategy = match collected.kind {
            crate::fsfat::FatKind::ExFat => "exFAT",
            _ => "FAT",
        };
        FinalizeInput::Fat(collected)
    } else if use_ext {
        let stage = ScanStageWire::QuickIndex;
        set_stage(&scan, &window, stage, 0.0);
        let mut reader = crate::recovery::partitions::open_target_reader(&scan.meta.target)?;
        let cb = make_progress_cb(&scan, &window);
        let cb_ref: &dyn Fn(&crate::recovery::collect::CollectProgress) = &cb;
        let collected = crate::fsext::collect_ext(
            &mut reader,
            &scan.meta.volume_id,
            &scan.flags,
            Some(cb_ref),
        )?;
        strategy = "ext";
        FinalizeInput::Ext(collected)
    } else {
        let stage = ScanStageWire::DeepWalk;
        set_stage(&scan, &window, stage, 0.0);
        FinalizeInput::Fallback(fallback_walk(&scan, &window)?)
    };

    // ---- Stage: tree-build -----------------------------------------
    let stage = ScanStageWire::TreeBuild;
    set_stage(&scan, &window, stage, 0.0);
    finalize_scan(&scan, &window, finalize_input, &user_rules, stage, strategy)?;

    // Resolve the path scope node (if any) against the built tree.
    resolve_scope_node(&scan)?;

    // Final counters + status.
    {
        let tree_guard = scan.tree.read().unwrap_or_else(|p| p.into_inner());
        let Some(t) = tree_guard.as_ref() else {
            // Defensive: finalize always publishes a tree before Ok —
            // reaching here means a bug, not a crash.
            return Err(Error::Scan("finalize did not publish a tree".into()));
        };
        let deleted_count = scan.deleted.read().unwrap_or_else(|p| p.into_inner()).len() as u64;
        let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
        rt.files_found = t.file_count;
        rt.data_found_bytes = t.total_bytes;
        rt.deleted_found = deleted_count;
        rt.overall_progress = 1.0;
        rt.stage_progress = 1.0;
        rt.stage = ScanStageWire::Done;
        rt.status = if scan.flags.is_cancelled() {
            ScanStatusWire::Cancelled
        } else {
            ScanStatusWire::Completed
        };
        rt.completed_at_unix = Some(crate::recovery::now_unix());
        rt.eta_seconds = Some(0);
        rt.current_path = None;
    }
    emit_scan_progress(&window, &scan);
    Ok(())
}

fn load_user_rules() -> Vec<crate::safe_delete::UserRuleSnapshot> {
    // Dedicated connection: the scan thread must never hold the global
    // DB mutex (WAL mode makes concurrent readers safe).
    match crate::db::open_db() {
        Ok(conn) => crate::safe_delete::list_active_snapshots(&conn).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

enum FinalizeInput {
    Mft(crate::recovery::collect::CollectedScan),
    Fat(crate::fsfat::FatCollected),
    Ext(crate::fsext::ExtCollected),
    Fallback(crate::scan::walk::MftEntries),
}

fn finalize_scan<R: tauri::Runtime>(
    scan: &Arc<RecoveryScan>,
    window: &tauri::WebviewWindow<R>,
    input: FinalizeInput,
    user_rules: &[crate::safe_delete::UserRuleSnapshot],
    stage: ScanStageWire,
    strategy: &'static str,
) -> Result<()> {
    match input {
        FinalizeInput::Mft(c) => {
            set_stage(scan, window, stage, 0.3);
            // Consume the collected scan: the existing entries feed the
            // tree builder directly (no full deep clone — the collector's
            // Vec is moved), and the deleted map is built exactly once.
            let volume_id = c.volume_path.clone();
            let record_size = c.record_size;
            let total_records = c.total_records;
            let skipped_errors = c.skipped_errors;
            let CollectedScan {
                existing,
                deleted,
                cluster_size,
                ..
            } = c;
            let tree = build_tree_with_user_rules(volume_id, existing, user_rules);

            set_stage(scan, window, stage, 0.6);
            let deleted_map: HashMap<u64, crate::recovery::collect::DeletedEntry> =
                deleted.into_iter().map(|d| (d.record_no, d)).collect();
            let mut deleted_children: HashMap<u64, Vec<u64>> = HashMap::new();
            let mut lost: Vec<u64> = Vec::new();
            for d in deleted_map.values() {
                let parent = d.parent_record_no;
                if tree.nodes.contains_key(&parent) || deleted_map.contains_key(&parent) {
                    deleted_children
                        .entry(parent)
                        .or_default()
                        .push(d.record_no);
                } else {
                    lost.push(d.record_no);
                }
            }

            // Recursive dir sizes for deleted directories.
            let mut dir_sizes: HashMap<u64, u64> = HashMap::new();
            let dir_ids: Vec<u64> = deleted_map
                .values()
                .filter(|d| d.is_dir)
                .map(|d| d.record_no)
                .collect();
            for id in dir_ids {
                deleted_dir_size(id, &deleted_children, &deleted_map, &mut dir_sizes);
            }

            // Confidence grading against the $Bitmap.
            set_stage(scan, window, stage, 0.8);
            let bitmap = read_bitmap_for(&scan.meta.target);
            let mut confidence = HashMap::new();
            match &bitmap {
                Some(bm) => {
                    for d in deleted_map.values() {
                        confidence.insert(d.record_no, grade_deleted(d, bm));
                    }
                }
                None => {
                    for d in deleted_map.values() {
                        confidence
                            .insert(d.record_no, crate::recovery::deleted::Confidence::Unknown);
                    }
                }
            }

            {
                let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
                rt.errors = skipped_errors;
                rt.bytes_scanned = total_records.saturating_mul(record_size as u64);
                rt.files_found = tree.file_count;
                rt.data_found_bytes = tree.total_bytes;
            }

            *scan.tree.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(tree));
            *scan.deleted.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(deleted_map);
            *scan
                .deleted_children
                .write()
                .unwrap_or_else(|p| p.into_inner()) = deleted_children;
            *scan.lost_path.write().unwrap_or_else(|p| p.into_inner()) = lost;
            *scan
                .deleted_dir_sizes
                .write()
                .unwrap_or_else(|p| p.into_inner()) = dir_sizes;
            *scan.confidence.write().unwrap_or_else(|p| p.into_inner()) = confidence;
            *scan.cluster_size.write().unwrap_or_else(|p| p.into_inner()) = cluster_size;
            *scan.strategy.write().unwrap_or_else(|p| p.into_inner()) = Some(strategy);
            set_stage(scan, window, stage, 1.0);
            Ok(())
        }
        FinalizeInput::Fat(c) => {
            set_stage(scan, window, stage, 0.3);
            let volume_id = scan.meta.volume_id.clone();
            let cluster_size = c.cluster_size;
            let skipped_errors = c.skipped_errors;
            let kind = c.kind;
            let crate::fsfat::FatCollected {
                existing,
                deleted,
                alloc,
                geometry,
                ..
            } = c;
            let tree = build_tree_with_user_rules(volume_id, existing, user_rules);

            set_stage(scan, window, stage, 0.6);
            let deleted_map: HashMap<u64, crate::recovery::collect::DeletedEntry> =
                deleted.into_iter().map(|d| (d.record_no, d)).collect();
            let mut deleted_children: HashMap<u64, Vec<u64>> = HashMap::new();
            let mut lost: Vec<u64> = Vec::new();
            for d in deleted_map.values() {
                let parent = d.parent_record_no;
                if tree.nodes.contains_key(&parent) || deleted_map.contains_key(&parent) {
                    deleted_children
                        .entry(parent)
                        .or_default()
                        .push(d.record_no);
                } else {
                    lost.push(d.record_no);
                }
            }

            let mut dir_sizes: HashMap<u64, u64> = HashMap::new();
            let dir_ids: Vec<u64> = deleted_map
                .values()
                .filter(|d| d.is_dir)
                .map(|d| d.record_no)
                .collect();
            for id in dir_ids {
                deleted_dir_size(id, &deleted_children, &deleted_map, &mut dir_sizes);
            }

            // Confidence grading against the FAT/exFAT allocation view.
            set_stage(scan, window, stage, 0.8);
            let mut confidence = HashMap::new();
            for d in deleted_map.values() {
                confidence.insert(d.record_no, grade_deleted(d, &alloc));
            }

            {
                let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
                rt.errors = skipped_errors;
                rt.bytes_scanned = rt.bytes_total;
                rt.files_found = tree.file_count;
                rt.data_found_bytes = tree.total_bytes;
            }

            *scan.tree.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(tree));
            *scan.deleted.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(deleted_map);
            *scan
                .deleted_children
                .write()
                .unwrap_or_else(|p| p.into_inner()) = deleted_children;
            *scan.lost_path.write().unwrap_or_else(|p| p.into_inner()) = lost;
            *scan
                .deleted_dir_sizes
                .write()
                .unwrap_or_else(|p| p.into_inner()) = dir_sizes;
            *scan.confidence.write().unwrap_or_else(|p| p.into_inner()) = confidence;
            *scan.cluster_size.write().unwrap_or_else(|p| p.into_inner()) = cluster_size;
            *scan.strategy.write().unwrap_or_else(|p| p.into_inner()) = Some(strategy);
            *scan.backend.write().unwrap_or_else(|p| p.into_inner()) =
                crate::recovery::BackendGeometry::Fat(geometry);
            let _ = kind;
            set_stage(scan, window, stage, 1.0);
            Ok(())
        }
        FinalizeInput::Ext(c) => {
            set_stage(scan, window, stage, 0.3);
            let volume_id = scan.meta.volume_id.clone();
            let skipped_errors = c.skipped_errors;
            let crate::fsext::ExtCollected {
                existing,
                deleted,
                alloc,
                geometry,
                ..
            } = c;
            let tree = build_tree_with_user_rules(volume_id, existing, user_rules);

            set_stage(scan, window, stage, 0.6);
            let deleted_map: HashMap<u64, crate::recovery::collect::DeletedEntry> =
                deleted.into_iter().map(|d| (d.record_no, d)).collect();
            let mut deleted_children: HashMap<u64, Vec<u64>> = HashMap::new();
            let mut lost: Vec<u64> = Vec::new();
            for d in deleted_map.values() {
                let parent = d.parent_record_no;
                if tree.nodes.contains_key(&parent) || deleted_map.contains_key(&parent) {
                    deleted_children
                        .entry(parent)
                        .or_default()
                        .push(d.record_no);
                } else {
                    lost.push(d.record_no);
                }
            }

            let mut dir_sizes: HashMap<u64, u64> = HashMap::new();
            let dir_ids: Vec<u64> = deleted_map
                .values()
                .filter(|d| d.is_dir)
                .map(|d| d.record_no)
                .collect();
            for id in dir_ids {
                deleted_dir_size(id, &deleted_children, &deleted_map, &mut dir_sizes);
            }

            // Confidence grading against the group block bitmaps.
            set_stage(scan, window, stage, 0.8);
            let mut confidence = HashMap::new();
            for d in deleted_map.values() {
                confidence.insert(d.record_no, grade_deleted(d, &alloc));
            }

            {
                let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
                rt.errors = skipped_errors;
                rt.bytes_scanned = rt.bytes_total;
                rt.files_found = tree.file_count;
                rt.data_found_bytes = tree.total_bytes;
            }

            *scan.tree.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(tree));
            *scan.deleted.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(deleted_map);
            *scan
                .deleted_children
                .write()
                .unwrap_or_else(|p| p.into_inner()) = deleted_children;
            *scan.lost_path.write().unwrap_or_else(|p| p.into_inner()) = lost;
            *scan
                .deleted_dir_sizes
                .write()
                .unwrap_or_else(|p| p.into_inner()) = dir_sizes;
            *scan.confidence.write().unwrap_or_else(|p| p.into_inner()) = confidence;
            *scan.cluster_size.write().unwrap_or_else(|p| p.into_inner()) =
                geometry.block_size as u32;
            *scan.strategy.write().unwrap_or_else(|p| p.into_inner()) = Some(strategy);
            *scan.backend.write().unwrap_or_else(|p| p.into_inner()) =
                crate::recovery::BackendGeometry::Ext(geometry);
            set_stage(scan, window, stage, 1.0);
            Ok(())
        }
        FinalizeInput::Fallback(entries) => {
            set_stage(scan, window, stage, 0.5);
            let volume_id = if entries.volume_path.is_empty() {
                scan.meta.volume_id.clone()
            } else {
                entries.volume_path.clone()
            };
            let crate::scan::walk::MftEntries {
                entries,
                skipped_errors,
                ..
            } = entries;
            let tree = build_tree_with_user_rules(volume_id, entries, user_rules);
            {
                let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
                rt.errors = skipped_errors;
                rt.files_found = tree.file_count;
                rt.data_found_bytes = tree.total_bytes;
            }
            *scan.tree.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(tree));
            *scan.strategy.write().unwrap_or_else(|p| p.into_inner()) = Some(strategy);
            set_stage(scan, window, stage, 1.0);
            Ok(())
        }
    }
}

fn make_progress_cb<R: tauri::Runtime>(
    scan: &Arc<RecoveryScan>,
    window: &tauri::WebviewWindow<R>,
) -> impl Fn(&crate::recovery::collect::CollectProgress) {
    let scan = scan.clone();
    let window = window.clone();
    move |p: &crate::recovery::collect::CollectProgress| {
        {
            let mut rt = scan.runtime.write().unwrap_or_else(|e| e.into_inner());
            let ratio = if p.total > 0 {
                (p.visited as f64 / p.total as f64).clamp(0.0, 0.999)
            } else {
                0.0
            };
            rt.stage_progress = ratio;
            let base = W_VOLUME_ANALYSIS;
            rt.overall_progress = (base + ratio * W_WALK).min(0.99);
            rt.files_found = p.existing;
            rt.deleted_found = p.deleted;
            if !p.last_name.is_empty() {
                rt.current_path = Some(p.last_name.clone());
            }
            let elapsed = rt.started_at_unix.max(0) as u64;
            let now = crate::recovery::now_unix().max(0) as u64;
            let elapsed_now = now.saturating_sub(elapsed);
            if ratio > 0.02 && elapsed_now > 0 {
                let remaining = (elapsed_now as f64 * (1.0 - ratio) / ratio) as u64;
                rt.eta_seconds = Some(remaining);
            }
        }
        emit_scan_progress(&window, &scan);
    }
}

/// Aggregate size of a deleted directory (recursive over deleted
/// children). Cycle-safe: an in-progress sentinel (`u64::MAX`) is
/// inserted *before* recursing, so a corrupt MFT with cyclic deleted
/// parent chains (A→B→A) terminates instead of overflowing the stack.
fn deleted_dir_size(
    id: u64,
    children: &HashMap<u64, Vec<u64>>,
    deleted: &HashMap<u64, crate::recovery::collect::DeletedEntry>,
    memo: &mut HashMap<u64, u64>,
) -> u64 {
    if let Some(v) = memo.get(&id) {
        return *v;
    }
    // Sentinel: any re-entrant visit sees 0 and stops recursing.
    memo.insert(id, 0);
    let mut total = 0u64;
    if let Some(kids) = children.get(&id) {
        for k in kids {
            if let Some(d) = deleted.get(k) {
                if d.is_dir {
                    total += deleted_dir_size(*k, children, deleted, memo);
                } else {
                    total += d.data_size;
                }
            }
        }
    }
    memo.insert(id, total);
    total
}

fn read_bitmap_for(target: &ScanTarget) -> Option<ClusterBitmap> {
    let mut reader = crate::recovery::partitions::open_target_reader(target).ok()?;
    crate::recovery::partitions::bitmap_on_reader(
        &mut reader,
        &crate::recovery::partitions::reader_volume_label(target),
    )
    .ok()
}

fn fallback_walk<R: tauri::Runtime>(
    scan: &Arc<RecoveryScan>,
    window: &tauri::WebviewWindow<R>,
) -> Result<crate::scan::walk::MftEntries> {
    let root = match &scan.meta.target {
        ScanTarget::Volume { letter, .. } => match &scan.meta.path_scope {
            Some(scope) => format!("{letter}:\\{scope}"),
            None => letter.to_string(),
        },
        ScanTarget::Unc { root } => root.clone(),
        ScanTarget::LostPartition { .. } => {
            return Err(Error::Scan(
                "lost partitions require the MFT engine (administrator)".into(),
            ))
        }
    };

    let scan_cb = scan.clone();
    let window_cb = window.clone();
    let cancel_handle = &scan.flags.cancel;
    let pause_handle = &scan.flags.pause;
    let bytes_total = scan.runtime_snapshot().bytes_total;

    // Pause is honored inside the progress callback (every 32 dirs); a
    // cancel during pause trips the flag the walker already watches.
    let cb = move |p: &crate::scan::walk::ScanProgress| {
        if pause_handle.load(std::sync::atomic::Ordering::Acquire)
            && scan_cb.flags.wait_while_paused()
        {
            cancel_handle.store(true, std::sync::atomic::Ordering::Release);
        }
        {
            let mut rt = scan_cb.runtime.write().unwrap_or_else(|e| e.into_inner());
            rt.files_found = p.visited;
            if bytes_total > 0 {
                // Bytes walked ≈ sum of entry sizes (approximation of
                // `bytes_total` = volume used bytes).
                let ratio = (p.visited as f64 / 500_000.0).min(0.98);
                rt.stage_progress = ratio;
                rt.overall_progress = (W_VOLUME_ANALYSIS + ratio * W_WALK).min(0.99);
            }
            rt.current_path = Some(p.last_name.clone());
        }
        emit_scan_progress(&window_cb, &scan_cb);
    };
    let cb_ref: &(dyn Fn(&crate::scan::walk::ScanProgress) + Send + Sync) = &cb;

    match crate::scan::find_first::scan_find_first_with_progress(
        &root,
        Some(cb_ref),
        Some(cancel_handle),
    ) {
        Ok(entries) => Ok(entries),
        Err(e) => {
            if scan.flags.is_cancelled() {
                // Cancelled fallback walk: the walker discards its buffer;
                // report an empty (but valid) result set.
                Ok(crate::scan::walk::MftEntries {
                    drive: root,
                    volume_path: String::new(),
                    entries: Vec::new(),
                    skipped_errors: 0,
                    elapsed_ms: 0,
                })
            } else {
                Err(e)
            }
        }
    }
}

/// Resolves `meta.path_scope` to a tree node id (None = whole volume).
/// Unknown scope → hard error (contract: `scan-scope-invalid`).
fn resolve_scope_node(scan: &Arc<RecoveryScan>) -> Result<()> {
    let Some(scope) = scan.meta.path_scope.clone() else {
        return Ok(());
    };
    let tree_guard = scan.tree.read().unwrap_or_else(|p| p.into_inner());
    let Some(tree) = tree_guard.as_ref() else {
        return Ok(());
    };
    let mut current = tree.root_id;
    for seg in scope.split('\\').filter(|s| !s.is_empty()) {
        let lower = seg.to_ascii_lowercase();
        let next = tree
            .children
            .get(&current)
            .and_then(|kids| {
                kids.iter().find(|k| {
                    tree.nodes
                        .get(*k)
                        .map(|n| n.name.to_ascii_lowercase() == lower)
                        .unwrap_or(false)
                })
            })
            .copied();
        match next {
            Some(n) => current = n,
            None => {
                return Err(Error::Scan(format!(
                    "scan scope '{scope}' was not found on the volume"
                )))
            }
        }
    }
    drop(tree_guard);
    *scan.scope_node.write().unwrap_or_else(|p| p.into_inner()) = Some(current);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_target_letter_forms() {
        for form in ["C", "c:", "vol:C", "dev:c"] {
            let meta = resolve_target(form).unwrap();
            assert!(matches!(
                meta.target,
                ScanTarget::Volume { letter: 'C', .. }
            ));
            assert_eq!(meta.volume_id, r"\\.\C:");
        }
    }

    #[test]
    fn resolve_target_rejects_garbage() {
        assert!(resolve_target("abc").is_err());
        assert!(resolve_target("").is_err());
        assert!(resolve_target("12").is_err());
    }

    #[test]
    fn resolve_target_unc() {
        let meta = resolve_target(r"\\nas\share").unwrap();
        assert!(matches!(meta.target, ScanTarget::Unc { .. }));
        assert_eq!(meta.fs, "network");
    }

    #[test]
    fn path_scope_normalization() {
        let mut meta = resolve_target("C").unwrap();
        apply_path_scope(&mut meta, Some("Users\\bob")).unwrap();
        assert_eq!(meta.path_scope.as_deref(), Some("Users\\bob"));

        let mut meta = resolve_target("C").unwrap();
        apply_path_scope(&mut meta, Some(r"c:/Users/bob/")).unwrap();
        assert_eq!(meta.path_scope.as_deref(), Some("Users\\bob"));

        let mut meta = resolve_target("C").unwrap();
        apply_path_scope(&mut meta, Some(r"\\")).unwrap();
        assert!(meta.path_scope.is_none());

        // Wrong volume prefix rejected.
        let mut meta = resolve_target("C").unwrap();
        assert!(apply_path_scope(&mut meta, Some(r"d:\x")).is_err());
    }

    #[test]
    fn stage_weights_sum_to_one() {
        let sum = W_VOLUME_ANALYSIS + W_WALK + W_TREE_BUILD;
        assert!((sum - 1.0).abs() < 1e-9);
    }
}

/// Wire snapshot of a scan's state (for `getScanState`).
pub fn scan_state_wire(scan: &RecoveryScan) -> crate::recovery::commands::ScanStateWire {
    use crate::recovery::commands::ScanStateWire;
    let rt = scan.runtime_snapshot();
    let progress = crate::recovery::ScanProgressEventWire::from_runtime(&scan.scan_id, &rt);
    ScanStateWire {
        scan_id: scan.scan_id.clone(),
        target_id: scan.meta.target_id.clone(),
        target_label: scan.meta.target_label.clone(),
        status: rt.status,
        started_at_unix: rt.started_at_unix,
        completed_at_unix: rt.completed_at_unix,
        progress,
        failure: rt.failure,
    }
}
