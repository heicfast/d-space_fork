// SPDX-License-Identifier: GPL-3.0-or-later
//
// Recovery jobs — DiskWizard recovery engine.
//
// Copies selected files (existing via the filesystem, deleted via fresh
// data-run reads with allocation re-checks) to a destination with a
// collision policy, folder rebuild and optional Blake3 verification.
// Per-file progress events (`dw-recovery-progress`) drive the queue UI.
//
// Honesty rules:
//   * Records reused since scan time fail loudly (record-reused).
//   * Deleted files whose clusters were reallocated fail (clusters-overwritten).
//   * Compressed/encrypted deleted files fail (unsupported-encoding).
//   * Existing files are re-read from disk — they may have changed or
//     vanished since the scan (source-missing).

use crate::error::{Error, Result};
use crate::recovery::collect::ReadSeek;
use crate::recovery::results::{
    query_results, ResultsFilterWire, SortDirectionWire, SortFieldWire,
};
use crate::recovery::{RecoveryScan, ScanTarget};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tracing::{info, warn};

/// Hard cap for one job (mirrors the duplicates engine's guard).
const MAX_JOB_ITEMS: usize = 500_000;
/// Items kept in the succeeded/failed/skipped report lists.
const REPORT_LIST_CAP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Validating,
    Running,
    Completed,
    CompletedPartial,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
    Verifying,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionPolicy {
    Rename,
    Overwrite,
    Skip,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryOptionsWire {
    pub collision: CollisionPolicy,
    pub rebuild_folders: bool,
    pub keep_empty_folders: bool,
    pub file_repair: bool,
    pub verify_on_write: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoverySelectionWire {
    Nodes { node_ids: Vec<String> },
    All { filter: ResultsFilterWire },
}

#[derive(Debug, Clone)]
pub(crate) struct JobItem {
    node_id: u64,
    /// Destination-relative path (folder rebuild applied).
    rel_path: String,
    name: String,
    size_bytes: u64,
    state: ItemState,
    error_code: Option<String>,
    saved_as: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobItemWire {
    node_id: String,
    name: String,
    size_bytes: u64,
    state: ItemState,
    error_code: Option<String>,
    saved_as: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryProgressWire {
    pub job_id: String,
    pub scan_id: String,
    pub status: JobStatus,
    pub overall_progress: f64,
    pub current_file: Option<String>,
    pub current_file_progress: f64,
    pub completed_count: u64,
    pub failed_count: u64,
    pub skipped_count: u64,
    pub total_count: u64,
    pub throughput_bytes_per_second: u64,
    pub verifying: bool,
    pub eta_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReportWire {
    pub job_id: String,
    pub status: JobStatus,
    pub started_at_unix: i64,
    pub finished_at_unix: i64,
    pub succeeded: Vec<String>,
    pub failed: Vec<JobFailureWire>,
    pub skipped: Vec<String>,
    pub total_bytes_written: u64,
    pub verified: bool,
    pub processed_total: u64,
    pub truncated_item_lists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFailureWire {
    pub node_id: String,
    pub name: String,
    pub code: String,
}

/// A running or finished recovery job.
pub struct RecoveryJob {
    pub job_id: String,
    pub scan_id: String,
    pub destination: String,
    pub options: RecoveryOptionsWire,
    pub(crate) items: Mutex<Vec<JobItem>>,
    pub status: RwLock<JobStatus>,
    pub progress: RwLock<RecoveryProgressWire>,
    pub cancel: AtomicBool,
    started_at_unix: i64,
    finished_at_unix: RwLock<Option<i64>>,
}

impl RecoveryJob {
    pub fn snapshot_progress(&self) -> RecoveryProgressWire {
        self.progress
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

/// Builds the job item list from the selection (validating step).
pub fn build_job_items(
    scan: &Arc<RecoveryScan>,
    selection: &RecoverySelectionWire,
    destination: &str,
    options: &RecoveryOptionsWire,
) -> Result<Vec<(u64, String, String, u64)>> {
    // (node_id, name, rel_path, size)
    let tree_guard = scan.tree.read().unwrap_or_else(|p| p.into_inner());
    let tree = tree_guard
        .as_ref()
        .ok_or_else(|| Error::Staging("scan results are not ready".into()))?
        .clone();
    drop(tree_guard);

    let ids: Vec<u64> = match selection {
        RecoverySelectionWire::Nodes { node_ids } => {
            node_ids.iter().filter_map(|s| s.parse().ok()).collect()
        }
        RecoverySelectionWire::All { filter } => {
            let page = query_results(
                scan,
                filter,
                SortFieldWire::Size,
                SortDirectionWire::Desc,
                0,
                MAX_JOB_ITEMS,
            )?;
            page.nodes
                .iter()
                .filter_map(|n| n.id.parse().ok())
                .collect()
        }
    };
    if ids.is_empty() {
        return Err(Error::Staging("nothing selected for recovery".into()));
    }
    if ids.len() > MAX_JOB_ITEMS {
        return Err(Error::Staging(format!(
            "selection too large ({} items); recover in smaller batches",
            ids.len()
        )));
    }

    // Reject destination overlap with the source volume (contract: hard).
    if let ScanTarget::Volume { letter, .. } = &scan.meta.target {
        let dest_letter = destination.chars().next().map(|c| c.to_ascii_uppercase());
        if dest_letter == Some(*letter) {
            return Err(Error::Staging(format!(
                "destination '{destination}' is on the source volume {letter}:"
            )));
        }
    }
    if let ScanTarget::LostPartition { physical_drive, .. } = &scan.meta.target {
        if destination.contains(&format!("PhysicalDrive{physical_drive}")) {
            return Err(Error::Staging(
                "destination overlaps the source disk".into(),
            ));
        }
    }

    // Expand directories to their file children; keep files as-is.
    let view = crate::recovery::results::ScanView::new(scan);
    let ctx = CollectCtx {
        scan,
        view: &view,
        tree: &tree,
        options,
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        collect_item(&ctx, id, &mut seen, &mut out)?;
    }
    Ok(out)
}

struct CollectCtx<'a> {
    scan: &'a Arc<RecoveryScan>,
    view: &'a crate::recovery::results::ScanView<'a>,
    tree: &'a crate::scan::tree::ScanTree,
    options: &'a RecoveryOptionsWire,
}

fn collect_item(
    ctx: &CollectCtx<'_>,
    id: u64,
    seen: &mut std::collections::HashSet<u64>,
    out: &mut Vec<(u64, String, String, u64)>,
) -> Result<()> {
    let (scan, view, tree, options) = (ctx.scan, ctx.view, ctx.tree, ctx.options);
    if !seen.insert(id) {
        return Ok(());
    }
    let existing = tree.nodes.get(&id).cloned();
    let deleted = scan
        .deleted
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .get(&id)
        .cloned();

    let (name, is_dir, size) = match (&existing, &deleted) {
        (Some(n), _) => (n.name.clone(), n.is_dir, n.data_size),
        (_, Some(d)) => (d.name.clone(), d.is_dir, d.data_size),
        _ => return Ok(()), // unknown id: skip
    };

    if is_dir {
        if !options.rebuild_folders {
            // Directories are recreated only for file paths; standalone
            // dir selection without rebuild is a no-op.
            return Ok(());
        }
        for child in view.children_of(id) {
            collect_item(ctx, child, seen, out)?;
        }
        return Ok(());
    }

    let rel_path = rel_path_for(scan, tree, id, &name, options);
    out.push((id, name, rel_path, size));
    Ok(())
}

/// Destination-relative path: the file's tree path (folders rebuilt) or
/// just the file name at the destination root.
fn rel_path_for(
    scan: &Arc<RecoveryScan>,
    tree: &crate::scan::tree::ScanTree,
    id: u64,
    name: &str,
    options: &RecoveryOptionsWire,
) -> String {
    if !options.rebuild_folders {
        return name.to_string();
    }
    // Walk parents to the scope root, collecting names.
    let mut parts: Vec<String> = Vec::new();
    let scope = scan
        .scope_node
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .unwrap_or(tree.root_id);
    let mut cur = if let Some(n) = tree.nodes.get(&id) {
        n.parent
    } else {
        scan.deleted
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .map(|d| d.parent_record_no)
    };
    let mut hops = 0;
    while let Some(p) = cur {
        if hops > 64 || p == scope {
            break;
        }
        if let Some(n) = tree.nodes.get(&p) {
            if p == tree.root_id {
                break;
            }
            parts.push(n.name.clone());
            cur = n.parent;
        } else {
            break;
        }
        hops += 1;
    }
    parts.reverse();
    parts.push(name.to_string());
    parts.join("\\")
}

/// Runs a recovery job on a dedicated thread.
pub fn run_job<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    job: Arc<RecoveryJob>,
    scan: Arc<RecoveryScan>,
) {
    let job_for_task = job.clone();
    let scan_for_task = scan.clone();
    let join = std::thread::Builder::new()
        .name(format!("dw-job-{}", job.job_id))
        .spawn(move || run_job_inner(window.clone(), job_for_task, scan_for_task));
    match join {
        Ok(handle) => {
            if let Err(e) = handle
                .join()
                .unwrap_or_else(|_| Err(Error::Staging("job thread panicked".into())))
            {
                {
                    let mut st = job.status.write().unwrap_or_else(|p| p.into_inner());
                    *st = JobStatus::Failed;
                    let mut p = job.progress.write().unwrap_or_else(|p| p.into_inner());
                    p.status = JobStatus::Failed;
                }
                warn!(job = %job.job_id, error = %e, "recovery job failed");
            }
        }
        Err(e) => {
            let mut st = job.status.write().unwrap_or_else(|p| p.into_inner());
            *st = JobStatus::Failed;
            let _ = e;
        }
    }
    info!(job = %job.job_id, "recovery job thread finished");
}

fn set_progress(job: &Arc<RecoveryJob>, f: impl FnOnce(&mut RecoveryProgressWire)) {
    {
        let mut p = job.progress.write().unwrap_or_else(|p| p.into_inner());
        f(&mut p);
    }
    let _ = &job.status;
}

fn emit<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>, job: &Arc<RecoveryJob>) {
    use tauri::Emitter;
    let snapshot = job.snapshot_progress();
    if let Err(e) = window.emit("dw-recovery-progress", &snapshot) {
        warn!(?e, "failed to emit dw-recovery-progress");
    }
}

fn run_job_inner<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    job: Arc<RecoveryJob>,
    scan: Arc<RecoveryScan>,
) -> Result<()> {
    {
        let mut st = job.status.write().unwrap_or_else(|p| p.into_inner());
        *st = JobStatus::Running;
    }
    set_progress(&job, |p| p.status = JobStatus::Running);
    emit(&window, &job);

    let start = std::time::Instant::now();
    let mut written_total: u64 = 0;
    let mut succeeded: Vec<String> = Vec::new();
    let mut failed: Vec<JobFailureWire> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    let total = job
        .items
        .lock()
        .map_err(|e| Error::Staging(e.to_string()))?
        .len() as u64;
    let mut done = 0u64;

    // One raw reader for the whole job (deleted reads).
    let mut reader = crate::recovery::partitions::open_target_reader(&scan.meta.target).ok();
    let volume_label = crate::recovery::partitions::reader_volume_label(&scan.meta.target);

    let mut items = job
        .items
        .lock()
        .map_err(|e| Error::Staging(e.to_string()))?
        .clone();

    for item in items.iter_mut() {
        if job.cancel.load(Ordering::Acquire) {
            item.state = ItemState::Skipped;
            continue;
        }
        item.state = ItemState::Running;
        set_progress(&job, |p| {
            p.current_file = Some(item.name.clone());
            p.current_file_progress = 0.0;
            p.verifying = false;
        });
        emit(&window, &job);

        let dest = PathBuf::from(&job.destination).join(&item.rel_path);
        let outcome = recover_one(
            &scan,
            reader.as_mut(),
            &volume_label,
            &job.options,
            item,
            &dest,
        );

        match outcome {
            Ok(saved_as) => {
                written_total += item.size_bytes;
                item.state = ItemState::Done;
                item.saved_as = Some(saved_as);
                succeeded.push(item.rel_path.clone());
            }
            Err((code, detail)) => match code.as_str() {
                "collision-skip" => {
                    item.state = ItemState::Skipped;
                    skipped.push(item.rel_path.clone());
                }
                _ => {
                    item.state = ItemState::Failed;
                    item.error_code = Some(code.clone());
                    failed.push(JobFailureWire {
                        node_id: item.node_id.to_string(),
                        name: item.name.clone(),
                        code,
                    });
                    warn!(node = item.node_id, detail, "recovery item failed");
                }
            },
        }

        if job.options.verify_on_write && item.state == ItemState::Done {
            item.state = ItemState::Verifying;
            set_progress(&job, |p| p.verifying = true);
            emit(&window, &job);
            // Verification happens inside recover_one (hash compare);
            // this state flip is a UI affordance.
            item.state = ItemState::Done;
        }

        done += 1;
        let elapsed = start.elapsed().as_secs_f64();
        set_progress(&job, |p| {
            p.completed_count = succeeded.len() as u64;
            p.failed_count = failed.len() as u64;
            p.skipped_count = skipped.len() as u64;
            p.overall_progress = done as f64 / total.max(1) as f64;
            p.current_file_progress = 1.0;
            if elapsed > 0.0 {
                p.throughput_bytes_per_second = (written_total as f64 / elapsed) as u64;
                let remaining = ((total - done) as f64 * elapsed / done.max(1) as f64) as u64;
                p.eta_seconds = Some(remaining);
            }
        });
        emit(&window, &job);
    }

    // Persist final item states back into the job.
    {
        let mut guard = job
            .items
            .lock()
            .map_err(|e| Error::Staging(e.to_string()))?;
        *guard = items;
    }

    let cancelled = job.cancel.load(Ordering::Acquire);
    let final_status = if cancelled {
        JobStatus::Cancelled
    } else if failed.is_empty() {
        JobStatus::Completed
    } else if !succeeded.is_empty() {
        JobStatus::CompletedPartial
    } else {
        JobStatus::Failed
    };
    {
        let mut st = job.status.write().unwrap_or_else(|p| p.into_inner());
        *st = final_status;
    }
    let _truncated = succeeded.len() > REPORT_LIST_CAP
        || failed.len() > REPORT_LIST_CAP
        || skipped.len() > REPORT_LIST_CAP;
    set_progress(&job, |p| {
        p.status = final_status;
        p.overall_progress = 1.0;
        p.current_file = None;
        p.verifying = false;
    });
    *job.finished_at_unix
        .write()
        .unwrap_or_else(|p| p.into_inner()) = Some(crate::recovery::now_unix());
    emit(&window, &job);
    Ok(())
}

type ItemOutcome = std::result::Result<String, (String, String)>;

/// Recovers a single item. Returns the saved (possibly renamed) file name
/// or a machine code + detail on failure.
fn recover_one(
    scan: &Arc<RecoveryScan>,
    reader: Option<&mut Box<dyn ReadSeek>>,
    volume_label: &str,
    options: &RecoveryOptionsWire,
    item: &JobItem,
    dest: &Path,
) -> ItemOutcome {
    // Collision handling.
    if dest.exists() {
        match options.collision {
            CollisionPolicy::Skip => {
                return Err(("collision-skip".into(), "destination file exists".into()))
            }
            CollisionPolicy::Rename => {
                let renamed = unique_path(dest);
                return write_item(scan, reader, volume_label, options, item, &renamed)
                    .map(|_| renamed.to_string_lossy().to_string());
            }
            CollisionPolicy::Overwrite => {}
        }
    }
    write_item(scan, reader, volume_label, options, item, dest)
        .map(|_| dest.to_string_lossy().to_string())
}

fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let parent = path.parent().unwrap_or(Path::new("."));
    for i in 1..10_000u32 {
        let candidate = parent.join(format!("{stem} ({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

fn write_item(
    scan: &Arc<RecoveryScan>,
    reader: Option<&mut Box<dyn ReadSeek>>,
    volume_label: &str,
    options: &RecoveryOptionsWire,
    item: &JobItem,
    dest: &Path,
) -> std::result::Result<(), (String, String)> {
    if let Some(parent) = dest.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err((
                "destination-invalid".into(),
                format!("cannot create folder: {e}"),
            ));
        }
    }

    let is_deleted = scan
        .deleted
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .contains_key(&item.node_id);

    // Read the source bytes.
    let bytes: Vec<u8> = if is_deleted {
        let Some(reader) = reader else {
            return Err(("source-missing".into(), "raw reader unavailable".into()));
        };
        read_deleted_full(scan, reader, volume_label, item.node_id, item.size_bytes)?
    } else {
        read_existing_full(scan, item.node_id, item.size_bytes)?
    };

    let source_hash = blake3::hash(&bytes);
    let tmp = dest.with_extension("dw_tmp");
    {
        let mut f = match std::fs::File::create(&tmp) {
            Ok(f) => f,
            Err(e) => {
                return Err((
                    "destination-unwritable".into(),
                    format!("cannot create file: {e}"),
                ))
            }
        };
        if let Err(e) = f.write_all(&bytes) {
            let _ = std::fs::remove_file(&tmp);
            return Err((
                "destination-unwritable".into(),
                format!("write failed: {e}"),
            ));
        }
        let _ = f.sync_all();
    }
    // Atomic publish.
    if let Err(e) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::remove_file(&tmp);
        // Destination may have appeared meanwhile on Windows; retry via replace.
        if std::fs::copy(&tmp, dest).is_err() {
            return Err(("destination-invalid".into(), format!("publish failed: {e}")));
        }
        let _ = std::fs::remove_file(&tmp);
    }

    if options.verify_on_write {
        let written = std::fs::read(dest)
            .map_err(|e| ("verify-failed".into(), format!("re-read failed: {e}")))?;
        if blake3::hash(&written) != source_hash {
            let _ = std::fs::remove_file(dest);
            return Err((
                "verify-failed".into(),
                "written file does not match the recovered bytes".into(),
            ));
        }
    }
    Ok(())
}

fn read_existing_full(
    scan: &Arc<RecoveryScan>,
    node_id: u64,
    size: u64,
) -> std::result::Result<Vec<u8>, (String, String)> {
    let Some(path) = scan.node_display_path(node_id) else {
        return Err((
            "source-missing".into(),
            "no filesystem path for node".into(),
        ));
    };
    match std::fs::File::open(&path) {
        Ok(mut f) => {
            let mut buf = Vec::with_capacity(size.min(64 * 1024 * 1024) as usize);
            if let Err(e) = f.read_to_end(&mut buf) {
                return Err(("source-unreadable".into(), format!("read '{path}': {e}")));
            }
            Ok(buf)
        }
        Err(e) => Err(("source-missing".into(), format!("'{path}': {e}"))),
    }
}

fn read_deleted_full(
    scan: &Arc<RecoveryScan>,
    reader: &mut Box<dyn ReadSeek>,
    volume_label: &str,
    record_no: u64,
    size: u64,
) -> std::result::Result<Vec<u8>, (String, String)> {
    use crate::recovery::deleted::{read_deleted_range, reparse_record_on};
    let ntfs = ntfs::Ntfs::new(reader).map_err(|e| {
        (
            "source-missing".into(),
            format!("NTFS parse on '{volume_label}': {e:?}"),
        )
    })?;
    let fresh = reparse_record_on(&ntfs, reader, record_no)
        .map_err(|e| ("source-missing".into(), e.to_string()))?;
    if !fresh.still_deleted {
        return Err((
            "record-reused".into(),
            "the MFT record has been reused since the scan".into(),
        ));
    }
    if fresh.compressed || fresh.encrypted {
        return Err((
            "unsupported-encoding".into(),
            "compressed/encrypted deleted files cannot be recovered raw".into(),
        ));
    }
    // Fresh bitmap: re-read from the same volume.
    let bitmap = crate::recovery::collect::ClusterBitmap::read(&ntfs, reader).ok();
    let cluster_size = *scan.cluster_size.read().unwrap_or_else(|p| p.into_inner()) as u64;
    let (bytes, intact) = read_deleted_range(
        reader,
        &fresh,
        bitmap.as_ref(),
        cluster_size,
        0,
        size.max(4096),
    )
    .map_err(|e| ("source-unreadable".into(), e.to_string()))?;
    if intact < size {
        return Err((
            "clusters-overwritten".into(),
            format!("only {intact} of {size} bytes are still intact (clusters reused)"),
        ));
    }
    Ok(bytes)
}

/// Wire accessors for the command layer.
pub fn items_wire(job: &Arc<RecoveryJob>, offset: usize, limit: usize) -> Vec<JobItemWire> {
    let guard = job.items.lock().unwrap_or_else(|p| p.into_inner());
    guard
        .iter()
        .skip(offset)
        .take(limit)
        .map(|i| JobItemWire {
            node_id: i.node_id.to_string(),
            name: i.name.clone(),
            size_bytes: i.size_bytes,
            state: i.state,
            error_code: i.error_code.clone(),
            saved_as: i.saved_as.clone(),
        })
        .collect()
}

pub fn report_wire(job: &Arc<RecoveryJob>) -> RecoveryReportWire {
    let guard = job.items.lock().unwrap_or_else(|p| p.into_inner());
    let succeeded: Vec<String> = guard
        .iter()
        .filter(|i| i.state == ItemState::Done)
        .map(|i| i.saved_as.clone().unwrap_or_else(|| i.rel_path.clone()))
        .take(REPORT_LIST_CAP)
        .collect();
    let failed: Vec<JobFailureWire> = guard
        .iter()
        .filter(|i| i.state == ItemState::Failed)
        .map(|i| JobFailureWire {
            node_id: i.node_id.to_string(),
            name: i.name.clone(),
            code: i.error_code.clone().unwrap_or_else(|| "unknown".into()),
        })
        .take(REPORT_LIST_CAP)
        .collect();
    let skipped: Vec<String> = guard
        .iter()
        .filter(|i| i.state == ItemState::Skipped)
        .map(|i| i.rel_path.clone())
        .take(REPORT_LIST_CAP)
        .collect();
    let truncated = guard.len() > REPORT_LIST_CAP;
    let status = *job.status.read().unwrap_or_else(|p| p.into_inner());
    RecoveryReportWire {
        job_id: job.job_id.clone(),
        status,
        started_at_unix: job.started_at_unix,
        finished_at_unix: job
            .finished_at_unix
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .unwrap_or(0),
        total_bytes_written: guard
            .iter()
            .filter(|i| i.state == ItemState::Done)
            .map(|i| i.size_bytes)
            .sum(),
        verified: true,
        processed_total: guard.len() as u64,
        truncated_item_lists: truncated,
        succeeded,
        failed,
        skipped,
    }
}

/// Factory used by the command layer.
pub fn new_job(
    job_id: String,
    scan_id: String,
    destination: String,
    options: RecoveryOptionsWire,
    items: Vec<(u64, String, String, u64)>,
) -> RecoveryJob {
    let total = items.len() as u64;
    let job_items = items
        .into_iter()
        .map(|(node_id, name, rel_path, size_bytes)| JobItem {
            node_id,
            name,
            rel_path,
            size_bytes,
            state: ItemState::Pending,
            error_code: None,
            saved_as: None,
        })
        .collect();
    let now = crate::recovery::now_unix();
    RecoveryJob {
        job_id: job_id.clone(),
        scan_id,
        destination,
        options,
        items: Mutex::new(job_items),
        status: RwLock::new(JobStatus::Validating),
        progress: RwLock::new(RecoveryProgressWire {
            job_id,
            scan_id: String::new(),
            status: JobStatus::Validating,
            overall_progress: 0.0,
            current_file: None,
            current_file_progress: 0.0,
            completed_count: 0,
            failed_count: 0,
            skipped_count: 0,
            total_count: total,
            throughput_bytes_per_second: 0,
            verifying: false,
            eta_seconds: None,
        }),
        cancel: AtomicBool::new(false),
        started_at_unix: now,
        finished_at_unix: RwLock::new(None),
    }
}
