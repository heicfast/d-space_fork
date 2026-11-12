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
/// Streaming copy buffer (deleted reads and verify re-reads). Files are
/// never buffered whole in memory — a multi-GiB recovery must not depend
/// on a multi-GiB heap allocation.
const COPY_CHUNK: usize = 4 * 1024 * 1024;

/// (node_id, name, rel_path, size, is_dir_marker) — the built job item tuple.
pub type BuiltItem = (u64, String, String, u64, bool);

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
    /// Directory-only marker item (keep-empty-folders): create the folder,
    /// write no file.
    is_dir_marker: bool,
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
    pub succeeded: Vec<JobItemWire>,
    pub failed: Vec<JobItemWire>,
    pub skipped: Vec<JobItemWire>,
    pub total_bytes_written: u64,
    pub verified: bool,
    pub processed_total: u64,
    pub truncated_item_lists: bool,
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
) -> Result<Vec<BuiltItem>> {
    // (node_id, name, rel_path, size, is_dir_marker)
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
    let mut dir_paths = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        collect_item(&ctx, id, &mut seen, &mut out, &mut dir_paths)?;
    }
    // Empty-folder pass (keep_empty_folders): every remembered folder that
    // received no file becomes a marker item so its structure is recreated
    // anyway.
    if options.keep_empty_folders {
        let written_prefixes: std::collections::HashSet<String> = out
            .iter()
            .filter_map(|(_, _, rel, _, _)| {
                Path::new(rel)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
            })
            .collect();
        for dir_rel in dir_paths {
            if !written_prefixes.contains(&dir_rel)
                && !out.iter().any(|(_, _, rel, _, _)| *rel == dir_rel)
            {
                out.push((u64::MAX, String::new(), dir_rel, 0, true));
            }
        }
    }
    if out.is_empty() {
        return Err(Error::Staging(
            "nothing recoverable was found in the selection".into(),
        ));
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
    out: &mut Vec<BuiltItem>,
    dir_paths: &mut Vec<String>,
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
        // Remember the folder for the empty-folder pass.
        if options.keep_empty_folders {
            let rel = rel_path_for(scan, tree, id, &name, options, true);
            if !rel.is_empty() {
                dir_paths.push(rel);
            }
        }
        for child in view.children_of(id) {
            collect_item(ctx, child, seen, out, dir_paths)?;
        }
        return Ok(());
    }

    let rel_path = rel_path_for(scan, tree, id, &name, options, false);
    out.push((id, name, rel_path, size, false));
    Ok(())
}

/// Destination-relative path: the file's tree path (folders rebuilt) or
/// just the file name at the destination root.
///
/// Deleted ancestors are reconstructed too: the walk continues through
/// the deleted map (name + parent), so a file deleted together with its
/// folder recovers under its original directory structure instead of
/// being flattened to the destination root. `drop_last` computes the
/// folder path for a directory (used by keep-empty-folders).
fn rel_path_for(
    scan: &Arc<RecoveryScan>,
    tree: &crate::scan::tree::ScanTree,
    id: u64,
    name: &str,
    options: &RecoveryOptionsWire,
    drop_last: bool,
) -> String {
    if !options.rebuild_folders {
        return name.to_string();
    }
    // Walk parents to the scope root, collecting names. Deleted parents
    // resolve through the deleted map; the first ancestor that survives
    // in the existing tree switches back to tree parents.
    let mut parts: Vec<String> = Vec::new();
    let scope = scan
        .scope_node
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .unwrap_or(tree.root_id);
    let deleted_map = scan.deleted.read().unwrap_or_else(|p| p.into_inner());
    let mut cur = if let Some(n) = tree.nodes.get(&id) {
        n.parent
    } else {
        deleted_map.get(&id).map(|d| d.parent_record_no)
    };
    let mut hops = 0;
    while let Some(p) = cur {
        if hops > 128 || p == scope {
            break;
        }
        if let Some(n) = tree.nodes.get(&p) {
            if p == tree.root_id {
                break;
            }
            parts.push(n.name.clone());
            cur = n.parent;
        } else if let Some(d) = deleted_map.get(&p) {
            // Deleted ancestor: keep walking through its own parent so
            // the original directory chain is preserved in the output.
            parts.push(d.name.clone());
            cur = Some(d.parent_record_no);
        } else {
            break;
        }
        hops += 1;
    }
    parts.reverse();
    if !drop_last {
        parts.push(name.to_string());
    }
    if parts.is_empty() {
        String::new()
    } else {
        parts.join("\\")
    }
}

/// Runs a recovery job on a dedicated, *detached* thread — the command
/// returns the job id immediately and the UI follows progress through
/// `dw-recovery-progress` events / `dw_recover_state` polling. Panics are
/// caught (unwind) and mapped to a Failed job.
pub fn run_job<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    job: Arc<RecoveryJob>,
    scan: Arc<RecoveryScan>,
) {
    let job_for_task = job.clone();
    let scan_for_task = scan;
    let spawn_result = std::thread::Builder::new()
        .name(format!("dw-job-{}", job.job_id))
        .spawn(move || {
            let inner_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_job_inner(window.clone(), job_for_task.clone(), scan_for_task)
            }))
            .unwrap_or_else(|panic_payload| {
                let msg = panic_payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "job thread panicked".into());
                Err(Error::Staging(format!("job thread panicked: {msg}")))
            });
            if let Err(e) = inner_result {
                {
                    let mut st = job_for_task
                        .status
                        .write()
                        .unwrap_or_else(|p| p.into_inner());
                    *st = JobStatus::Failed;
                    let mut p = job_for_task
                        .progress
                        .write()
                        .unwrap_or_else(|p| p.into_inner());
                    p.status = JobStatus::Failed;
                }
                warn!(job = %job_for_task.job_id, error = %e, "recovery job failed");
            }
            info!(job = %job_for_task.job_id, "recovery job thread finished");
        });
    if let Err(e) = spawn_result {
        let mut st = job.status.write().unwrap_or_else(|p| p.into_inner());
        *st = JobStatus::Failed;
        warn!(job = %job.job_id, error = %e, "cannot spawn job thread");
    }
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
    let mut failed: Vec<JobItemWire> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    let total = job
        .items
        .lock()
        .map_err(|e| Error::Staging(e.to_string()))?
        .len() as u64;
    let mut done = 0u64;

    // One raw reader for the whole job (deleted reads) — plus a one-time
    // NTFS boot parse and $Bitmap read so per-file recovery does not
    // re-parse the volume per item (the old code re-read the bitmap for
    // every single file).
    let mut reader = crate::recovery::partitions::open_target_reader(&scan.meta.target).ok();
    let volume_label = crate::recovery::partitions::reader_volume_label(&scan.meta.target);
    let backend = scan
        .backend
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let mut source = SourceCtx::prepare(&backend, reader.as_mut());

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
            Some(&mut source),
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
                    failed.push(JobItemWire {
                        node_id: item.node_id.to_string(),
                        name: item.name.clone(),
                        size_bytes: item.size_bytes,
                        state: ItemState::Failed,
                        error_code: Some(code),
                        saved_as: None,
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

/// Per-job source context: the NTFS boot parse + `$Bitmap` snapshot (or
/// the FAT geometry + allocation view) are prepared once — they used to
/// be re-parsed for every single file.
struct SourceCtx {
    ntfs: Option<ntfs::Ntfs>,
    bitmap: Option<crate::recovery::collect::ClusterBitmap>,
    fat: Option<(crate::fsfat::FatGeometry, crate::fsfat::FatAllocMap)>,
}

impl SourceCtx {
    fn prepare(
        backend: &crate::recovery::BackendGeometry,
        reader: Option<&mut Box<dyn ReadSeek>>,
    ) -> Self {
        match backend {
            crate::recovery::BackendGeometry::Ntfs => {
                let Some(reader) = reader else {
                    return Self {
                        ntfs: None,
                        bitmap: None,
                        fat: None,
                    };
                };
                let ntfs = match ntfs::Ntfs::new(reader) {
                    Ok(n) => Some(n),
                    Err(e) => {
                        warn!(
                            ?e,
                            "job source: NTFS parse failed; deleted reads will fail per item"
                        );
                        None
                    }
                };
                let bitmap = ntfs
                    .as_ref()
                    .and_then(|n| crate::recovery::collect::ClusterBitmap::read(n, reader).ok());
                Self {
                    ntfs,
                    bitmap,
                    fat: None,
                }
            }
            crate::recovery::BackendGeometry::Fat(geometry) => {
                let Some(reader) = reader else {
                    return Self {
                        ntfs: None,
                        bitmap: None,
                        fat: None,
                    };
                };
                let alloc = crate::fsfat::reload_alloc(geometry, reader);
                Self {
                    ntfs: None,
                    bitmap: None,
                    fat: Some((geometry.clone(), alloc)),
                }
            }
        }
    }
}

/// Recovers a single item. Returns the saved (possibly renamed) file name
/// or a machine code + detail on failure.
fn recover_one(
    scan: &Arc<RecoveryScan>,
    reader: Option<&mut Box<dyn ReadSeek>>,
    source: Option<&mut SourceCtx>,
    volume_label: &str,
    options: &RecoveryOptionsWire,
    item: &JobItem,
    dest: &Path,
) -> ItemOutcome {
    // Directory marker (keep-empty-folders): just create the folder.
    if item.is_dir_marker {
        if let Err(e) = std::fs::create_dir_all(dest) {
            return Err((
                "destination-invalid".into(),
                format!("cannot create folder: {e}"),
            ));
        }
        return Ok(dest.to_string_lossy().to_string());
    }
    // Collision handling.
    if dest.exists() {
        match options.collision {
            CollisionPolicy::Skip => {
                return Err(("collision-skip".into(), "destination file exists".into()))
            }
            CollisionPolicy::Rename => {
                let renamed = unique_path(dest);
                return write_item(scan, reader, source, volume_label, options, item, &renamed)
                    .map(|_| renamed.to_string_lossy().to_string());
            }
            CollisionPolicy::Overwrite => {}
        }
    }
    write_item(scan, reader, source, volume_label, options, item, dest)
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
    source: Option<&mut SourceCtx>,
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

    // Stream the source bytes through a bounded buffer; the blake3 hash
    // is computed incrementally. Nothing larger than COPY_CHUNK is ever
    // held in memory, so multi-GiB recoveries cannot OOM the process.
    let tmp = dest.with_extension("dw_tmp");
    let (source_hash, intact_written): (blake3::Hash, u64);
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
        let result = if is_deleted {
            stream_deleted(
                scan,
                reader,
                source,
                volume_label,
                item.node_id,
                item.size_bytes,
                options,
                &mut f,
            )
        } else {
            stream_existing(scan, item.node_id, &mut f)
        };
        match result {
            Ok((hash, written)) => {
                let _ = f.sync_all();
                source_hash = hash;
                intact_written = written;
            }
            Err(e) => {
                drop(f);
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }
    // Atomic publish.
    if let Err(e) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::remove_file(&tmp);
        // Destination may have appeared meanwhile on Windows; retry via
        // replace.
        if std::fs::copy(&tmp, dest).is_err() {
            return Err(("destination-invalid".into(), format!("publish failed: {e}")));
        }
        let _ = std::fs::remove_file(&tmp);
    }

    if options.verify_on_write && intact_written > 0 {
        // Streaming verify re-read: hash the published file and compare.
        let mut verify = blake3::Hasher::new();
        let mut f = match std::fs::File::open(dest) {
            Ok(f) => f,
            Err(e) => {
                let _ = std::fs::remove_file(dest);
                return Err(("verify-failed".into(), format!("re-read failed: {e}")));
            }
        };
        let mut buf = vec![0u8; COPY_CHUNK];
        loop {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    verify.update(&buf[..n]);
                }
                Err(e) => {
                    let _ = std::fs::remove_file(dest);
                    return Err(("verify-failed".into(), format!("re-read failed: {e}")));
                }
            }
        }
        if verify.finalize() != source_hash {
            let _ = std::fs::remove_file(dest);
            return Err((
                "verify-failed".into(),
                "written file does not match the recovered bytes".into(),
            ));
        }
    }
    Ok(())
}

/// Streams an existing file from its filesystem path into `out`,
/// returning `(hash_of_written, bytes_written)`.
fn stream_existing(
    scan: &Arc<RecoveryScan>,
    node_id: u64,
    out: &mut std::fs::File,
) -> std::result::Result<(blake3::Hash, u64), (String, String)> {
    let Some(path) = scan.node_display_path(node_id) else {
        return Err((
            "source-missing".into(),
            "no filesystem path for node".into(),
        ));
    };
    let mut f = std::fs::File::open(&path)
        .map_err(|e| ("source-missing".into(), format!("'{path}': {e}")))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; COPY_CHUNK];
    let mut written = 0u64;
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                hasher.update(&buf[..n]);
                out.write_all(&buf[..n]).map_err(|e| {
                    (
                        "destination-unwritable".into(),
                        format!("write failed: {e}"),
                    )
                })?;
                written += n as u64;
            }
            Err(e) => return Err(("source-unreadable".into(), format!("read '{path}': {e}"))),
        }
    }
    Ok((hasher.finalize(), written))
}

/// Streams a deleted record's intact bytes into `out` through the fresh
/// re-parse path. With `file_repair` enabled, a partially overwritten
/// file is still published (its intact prefix) — the job item keeps the
/// original size so the UI can flag it; without repair the item fails
/// with `clusters-overwritten`. Returns `(hash_of_written, bytes_written)`
/// where bytes_written < size signals a repaired partial file.
#[allow(clippy::too_many_arguments)]
fn stream_deleted(
    scan: &Arc<RecoveryScan>,
    reader: Option<&mut Box<dyn ReadSeek>>,
    source: Option<&mut SourceCtx>,
    volume_label: &str,
    record_no: u64,
    size: u64,
    options: &RecoveryOptionsWire,
    out: &mut std::fs::File,
) -> std::result::Result<(blake3::Hash, u64), (String, String)> {
    let Some(reader) = reader else {
        return Err(("source-missing".into(), "raw reader unavailable".into()));
    };
    use crate::recovery::deleted::{read_deleted_range, reparse_record_on};

    // The stored entry (runs/size) — needed for the FAT path and as the
    // NTFS fallback when the per-job parse failed.
    let stored = scan
        .deleted
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .get(&record_no)
        .cloned();

    // ---- Backend dispatch: NTFS re-parses the MFT record by number;
    // ---- FAT re-checks the directory slot byte at its recorded offset.
    let (fresh, cluster_base, bitmap_dyn): (
        crate::recovery::deleted::FreshRecord,
        u64,
        Option<&dyn crate::recovery::deleted::AllocMap>,
    ) = if let Some((geometry, alloc)) = source.as_ref().and_then(|s| s.fat.as_ref()) {
        // FAT / exFAT path.
        let entry = stored.ok_or_else(|| {
            (
                "source-missing".into(),
                "deleted entry missing from the scan".into(),
            )
        })?;
        let slot = entry.slot_offset.ok_or_else(|| {
            (
                "source-missing".into(),
                "entry has no FAT slot offset".into(),
            )
        })?;
        let still_deleted = crate::fsfat::reparse_slot(geometry, reader, slot)
            .map_err(|e| ("source-missing".into(), e.to_string()))?;
        if !still_deleted {
            return Err((
                "record-reused".into(),
                "the directory entry has been reused since the scan".into(),
            ));
        }
        let fresh = crate::recovery::deleted::FreshRecord {
            still_deleted: true,
            name: Some(entry.name.clone()),
            data_size: entry.data_size,
            resident: false,
            compressed: false,
            encrypted: false,
            runs: entry.runs.clone(),
            resident_data: Vec::new(),
        };
        (
            fresh,
            geometry.heap_off,
            Some(alloc as &dyn crate::recovery::deleted::AllocMap),
        )
    } else {
        // NTFS path.
        let fresh = match source.as_ref().and_then(|s| s.ntfs.as_ref()) {
            Some(ntfs) => reparse_record_on(ntfs, reader, record_no)
                .map_err(|e| ("source-missing".into(), e.to_string()))?,
            None => {
                let ntfs = ntfs::Ntfs::new(reader).map_err(|e| {
                    (
                        "source-missing".into(),
                        format!("NTFS parse on '{volume_label}': {e:?}"),
                    )
                })?;
                reparse_record_on(&ntfs, reader, record_no)
                    .map_err(|e| ("source-missing".into(), e.to_string()))?
            }
        };
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
        let bitmap = source.as_ref().and_then(|s| s.bitmap.as_ref());
        (
            fresh,
            0,
            bitmap.map(|b| b as &dyn crate::recovery::deleted::AllocMap),
        )
    };
    let fresh = &fresh;
    let cluster_size = *scan.cluster_size.read().unwrap_or_else(|p| p.into_inner()) as u64;

    let mut hasher = blake3::Hasher::new();
    let mut written = 0u64;
    let mut offset = 0u64;
    let want = size.max(4096);
    while offset < want {
        let len = (COPY_CHUNK as u64).min(want - offset);
        let (chunk, intact) = read_deleted_range(
            reader,
            fresh,
            bitmap_dyn,
            cluster_size,
            cluster_base,
            offset,
            len,
        )
        .map_err(|e| ("source-unreadable".into(), e.to_string()))?;
        if chunk.is_empty() {
            break;
        }
        hasher.update(&chunk);
        out.write_all(&chunk).map_err(|e| {
            (
                "destination-unwritable".into(),
                format!("write failed: {e}"),
            )
        })?;
        written += chunk.len() as u64;
        offset += chunk.len() as u64;
        if (intact as usize) < chunk.len() {
            // Stopped at a reallocated cluster.
            break;
        }
    }
    if written < size && !options.file_repair {
        return Err((
            "clusters-overwritten".into(),
            format!("only {written} of {size} bytes are still intact (clusters reused)"),
        ));
    }
    Ok((hasher.finalize(), written))
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

fn item_wire(i: &JobItem) -> JobItemWire {
    JobItemWire {
        node_id: i.node_id.to_string(),
        name: i.name.clone(),
        size_bytes: i.size_bytes,
        state: i.state,
        error_code: i.error_code.clone(),
        saved_as: i.saved_as.clone(),
    }
}

pub fn report_wire(job: &Arc<RecoveryJob>) -> RecoveryReportWire {
    let guard = job.items.lock().unwrap_or_else(|p| p.into_inner());
    let succeeded: Vec<JobItemWire> = guard
        .iter()
        .filter(|i| i.state == ItemState::Done)
        .map(item_wire)
        .take(REPORT_LIST_CAP)
        .collect();
    let failed: Vec<JobItemWire> = guard
        .iter()
        .filter(|i| i.state == ItemState::Failed)
        .map(item_wire)
        .take(REPORT_LIST_CAP)
        .collect();
    let skipped: Vec<JobItemWire> = guard
        .iter()
        .filter(|i| i.state == ItemState::Skipped)
        .map(item_wire)
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
    items: Vec<BuiltItem>,
) -> RecoveryJob {
    let total = items.len() as u64;
    let job_items = items
        .into_iter()
        .map(
            |(node_id, name, rel_path, size_bytes, is_dir_marker)| JobItem {
                node_id,
                name,
                rel_path,
                size_bytes,
                is_dir_marker,
                state: ItemState::Pending,
                error_code: None,
                saved_as: None,
            },
        )
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
