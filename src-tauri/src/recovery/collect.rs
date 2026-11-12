// SPDX-License-Identifier: GPL-3.0-or-later
//
// Unified MFT collection — DiskWizard recovery engine, phase 1.
//
// One pass over the MFT collects BOTH live entries (the space-analysis tree
// feed, identical semantics to `scan::walk`) AND deleted records (the
// recovery feed). Deleted records keep their data-run map so that
// recoverability can be graded against the $Bitmap cluster allocation map
// and their bytes can be read back later (preview / recovery).
//
// Everything here is cross-platform parse logic over a `Read + Seek`
// volume handle; unit tests cover the pure parts (run math, bitmap
// grading) without requiring a real NTFS volume.

use crate::error::{Error, Result};
use crate::scan::walk::{extract_full, normalize_volume_path, nt_to_unix};
use ntfs::structured_values::NtfsFileNamespace;
use ntfs::{KnownNtfsFileRecordNumber, Ntfs, NtfsAttributeFlags, NtfsFile, NtfsFileFlags};
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tracing::{debug, info};

/// Hard ceiling for the record walk (defensive; matches `scan::walk`).
const MAX_RECORD_HARD_LIMIT: u64 = 50_000_000;
/// Progress/pause checkpoints every N records.
const PROGRESS_INTERVAL: u64 = 5000;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// One data run of a deleted file's `$DATA` attribute.
/// `lcn = None` marks a sparse run (reads as zeros).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DwRun {
    pub lcn: Option<u64>,
    pub clusters: u64,
}

/// A deleted MFT record kept for recovery.
#[derive(Debug, Clone, Serialize)]
pub struct DeletedEntry {
    pub record_no: u64,
    pub parent_record_no: u64,
    pub name: String,
    pub data_size: u64,
    pub is_dir: bool,
    pub modified_unix: i64,
    /// Resident data lives inside the MFT record itself (usually fully
    /// recoverable even after unlink).
    pub resident: bool,
    /// Extents of the non-resident `$DATA` (empty for resident/dirs).
    pub runs: Vec<DwRun>,
    /// NTFS-compressed attribute — raw cluster reads are NOT the logical
    /// file content; recovery is not possible without a decompressor.
    pub compressed: bool,
    /// EFS-encrypted attribute — recovery not possible without the key.
    pub encrypted: bool,
}

/// Result of the unified walk.
#[derive(Debug, Serialize)]
pub struct CollectedScan {
    /// Volume path actually opened (`\\.\C:` style).
    pub volume_path: String,
    pub cluster_size: u32,
    pub record_size: u32,
    pub total_records: u64,
    /// Live entries (existing files/dirs) — feed for `build_tree`.
    pub existing: Vec<crate::scan::walk::RawMftEntry>,
    /// Deleted records — feed for the recovery layer.
    pub deleted: Vec<DeletedEntry>,
    pub skipped_errors: u64,
    pub elapsed_ms: u64,
    /// True when the walk ended early because the user cancelled.
    pub cancelled: bool,
}

/// Run-control flags shared between the scan orchestration and the walker.
/// `cancel` aborts (partial data is returned), `pause` suspends the walk.
#[derive(Debug, Default)]
pub struct RunFlags {
    pub cancel: AtomicBool,
    pub pause: AtomicBool,
}

impl RunFlags {
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
    pub fn is_paused(&self) -> bool {
        self.pause.load(Ordering::Acquire)
    }
    /// Sleeps in small slices while paused; returns `true` if cancelled
    /// while waiting.
    pub fn wait_while_paused(&self) -> bool {
        while self.is_paused() {
            if self.is_cancelled() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Extraction helpers
// ---------------------------------------------------------------------------

/// Extracts the data-run map (or resident flag) of a record's unnamed
/// `$DATA` attribute. Returns `None` for records without `$DATA`.
fn extract_data_runs<T: Read + Seek>(
    file: &NtfsFile,
    handle: &mut T,
) -> Option<(bool, Vec<DwRun>, bool, bool)> {
    let item = file.data(handle, "")?.ok()?;
    let attribute = item.to_attribute().ok()?;
    let flags = attribute.flags();
    let compressed = flags.contains(NtfsAttributeFlags::COMPRESSED);
    let encrypted = flags.contains(NtfsAttributeFlags::ENCRYPTED);
    match attribute.value(handle) {
        Ok(ntfs::attribute_value::NtfsAttributeValue::Resident(_)) => {
            Some((true, Vec::new(), compressed, encrypted))
        }
        Ok(ntfs::attribute_value::NtfsAttributeValue::NonResident(nr)) => {
            let cluster_size = file.ntfs().cluster_size() as u64;
            if cluster_size == 0 {
                return Some((false, Vec::new(), compressed, encrypted));
            }
            let mut runs = Vec::new();
            for run in nr.data_runs() {
                let run = match run {
                    Ok(r) => r,
                    Err(_) => return Some((false, Vec::new(), compressed, encrypted)),
                };
                let lcn = run
                    .data_position()
                    .value()
                    .map(|pos| pos.get() / cluster_size);
                let clusters = run.allocated_size() / cluster_size;
                runs.push(DwRun { lcn, clusters });
                if runs.len() > 4096 {
                    // Defensive: absurdly fragmented record — keep what we have.
                    break;
                }
            }
            Some((false, runs, compressed, encrypted))
        }
        // Attribute-list spanning values: rare for user files; treat as
        // non-resident with unknown runs (visible, graded unrecoverable).
        Ok(ntfs::attribute_value::NtfsAttributeValue::AttributeListNonResident(_)) => {
            Some((false, Vec::new(), compressed, encrypted))
        }
        Err(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Bitmap (cluster allocation map)
// ---------------------------------------------------------------------------

/// In-memory view of the NTFS `$Bitmap` ($MFT record 6). Bit `i` set means
/// cluster `i` is allocated. Used to grade deleted-file recoverability:
/// clusters still free likely hold the original bytes; allocated clusters
/// have been reused.
pub struct ClusterBitmap {
    pub data: Vec<u8>,
    pub total_clusters: u64,
}

impl ClusterBitmap {
    /// Reads `$Bitmap` through the ntfs crate's attached reader.
    pub fn read<T: Read + Seek>(ntfs: &Ntfs, handle: &mut T) -> Result<Self> {
        let bitmap_file = ntfs
            .file(handle, KnownNtfsFileRecordNumber::Bitmap as u64)
            .map_err(|e| Error::Scan(format!("cannot read $Bitmap record: {:?}", e)))?;
        let item = bitmap_file
            .data(handle, "")
            .ok_or_else(|| Error::Scan("$Bitmap has no $DATA attribute".into()))?
            .map_err(|e| Error::Scan(format!("$Bitmap $DATA error: {:?}", e)))?;
        let attribute = item
            .to_attribute()
            .map_err(|e| Error::Scan(format!("$Bitmap attribute: {:?}", e)))?;
        let value = attribute
            .value(handle)
            .map_err(|e| Error::Scan(format!("$Bitmap value: {:?}", e)))?;
        let mut data = Vec::new();
        match value {
            ntfs::attribute_value::NtfsAttributeValue::Resident(r) => {
                data.extend_from_slice(r.data());
            }
            ntfs::attribute_value::NtfsAttributeValue::NonResident(nr) => {
                let mut reader = nr.attach(handle);
                reader
                    .read_to_end(&mut data)
                    .map_err(|e| Error::Scan(format!("$Bitmap read: {}", e)))?;
            }
            ntfs::attribute_value::NtfsAttributeValue::AttributeListNonResident(_) => {
                return Err(Error::Scan(
                    "$Bitmap spans an attribute list (unsupported)".into(),
                ));
            }
        }
        let total_clusters = data.len() as u64 * 8;
        Ok(Self {
            data,
            total_clusters,
        })
    }

    /// `true` when the cluster is allocated (bit set). Out-of-range
    /// clusters are treated as allocated (conservative).
    pub fn is_allocated(&self, cluster: u64) -> bool {
        let byte = (cluster / 8) as usize;
        match self.data.get(byte) {
            Some(b) => (b >> (cluster % 8)) & 1 == 1,
            None => true,
        }
    }

    /// Fraction (0.0..=1.0) of the entry's clusters that are still FREE.
    /// Sparse runs count as free (holes read back as zeros).
    pub fn free_fraction(&self, entry: &DeletedEntry) -> f64 {
        let mut total = 0u64;
        let mut free = 0u64;
        for run in &entry.runs {
            total += run.clusters;
            let free_in_run = match run.lcn {
                Some(lcn) => (lcn..lcn.saturating_add(run.clusters))
                    .take_while(|c| *c < self.total_clusters)
                    .filter(|c| !self.is_allocated(*c))
                    .count(),
                None => run.clusters as usize,
            };
            free += free_in_run as u64;
        }
        if total == 0 {
            return 1.0;
        }
        free as f64 / total as f64
    }
}

// ---------------------------------------------------------------------------
// The unified walk
// ---------------------------------------------------------------------------

/// Progress callback payload for the recovery scan collection phase.
pub struct CollectProgress {
    pub visited: u64,
    pub total: u64,
    pub existing: u64,
    pub deleted: u64,
    pub last_name: String,
}

/// Walks the full MFT collecting live and deleted entries in one pass.
///
/// * `volume_path` — raw volume path (`\\.\C:`); must already be validated.
/// * On cancel: returns the partial collection with `cancelled = true`
///   (never an error — partial results stay browsable, mirroring the
///   product's partial-scan UX contract).
pub fn collect_mft_all(
    volume_path: &str,
    flags: &RunFlags,
    progress: Option<&dyn Fn(&CollectProgress)>,
) -> Result<CollectedScan> {
    let start = Instant::now();
    let mut handle = File::open(volume_path).map_err(|e| {
        Error::Scan(format!(
            "cannot open volume '{}': {} (administrator privileges are required for raw volume access)",
            volume_path, e
        ))
    })?;

    let ntfs =
        Ntfs::new(&mut handle).map_err(|e| Error::Scan(format!("NTFS parse error: {:?}", e)))?;

    let estimated = crate::scan::walk::estimate_record_count(&ntfs, &mut handle)?;
    let cap = estimated.min(MAX_RECORD_HARD_LIMIT);
    debug!(volume = %volume_path, estimated, cap, "recovery MFT walk starts");

    let mut existing: Vec<crate::scan::walk::RawMftEntry> =
        Vec::with_capacity(((cap as usize) / 2).clamp(1024, 1_000_000));
    let mut deleted: Vec<DeletedEntry> = Vec::new();
    let mut skipped = 0u64;
    let mut last_name = String::new();
    let mut cancelled = false;

    'walk: for record_no in 16..cap {
        if (record_no - 16) % PROGRESS_INTERVAL == 0 {
            if flags.wait_while_paused() {
                cancelled = true;
                break 'walk;
            }
            if flags.is_cancelled() {
                cancelled = true;
                break 'walk;
            }
            if let Some(cb) = progress {
                cb(&CollectProgress {
                    visited: record_no - 16,
                    total: cap - 16,
                    existing: existing.len() as u64,
                    deleted: deleted.len() as u64,
                    last_name: last_name.clone(),
                });
            }
        }

        let file = match ntfs.file(&mut handle, record_no) {
            Ok(f) => f,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let in_use = file.flags().contains(NtfsFileFlags::IN_USE);
        match extract_full(&file, &mut handle) {
            Some((name, parent, size, is_dir, mtime)) => {
                last_name = name.clone();
                if in_use {
                    existing.push(crate::scan::walk::RawMftEntry {
                        record_no,
                        parent_record_no: parent,
                        name,
                        data_size: if is_dir { 0 } else { size },
                        is_dir,
                        modified_unix: mtime,
                    });
                } else {
                    // Deleted record: keep extent info for grading + reads.
                    let (resident, runs, compressed, encrypted) = extract_data_runs(
                        &file,
                        &mut handle,
                    )
                    .unwrap_or((false, Vec::new(), false, false));
                    deleted.push(DeletedEntry {
                        record_no,
                        parent_record_no: parent,
                        name,
                        data_size: if is_dir { 0 } else { size },
                        is_dir,
                        modified_unix: mtime,
                        resident,
                        runs,
                        compressed,
                        encrypted,
                    });
                }
            }
            None => {
                // Unparseable name (record reused or severely damaged).
                skipped += 1;
            }
        }
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;
    info!(
        volume = %volume_path,
        existing = existing.len(),
        deleted = deleted.len(),
        skipped,
        cancelled,
        elapsed_ms,
        "recovery MFT walk finished"
    );

    Ok(CollectedScan {
        volume_path: volume_path.to_string(),
        cluster_size: ntfs.cluster_size(),
        record_size: ntfs.file_record_size(),
        total_records: estimated,
        existing,
        deleted,
        skipped_errors: skipped,
        elapsed_ms,
        cancelled,
    })
}

/// Normalize a drive input to a raw volume path (delegates to the strict
/// parser; UNC is rejected here — network scans use the fallback walker).
pub fn volume_path_for_drive(drive: &str) -> Result<String> {
    normalize_volume_path(drive)
}

/// Converts an NT timestamp via the shared helper (kept for symmetry).
pub fn nt_time_to_unix(nt: u64) -> i64 {
    nt_to_unix(nt)
}

/// Win32-namespace name extraction for a single record (used by the
/// re-parse-at-recovery path in `deleted.rs`).
pub fn record_win32_name<T: Read + Seek>(file: &NtfsFile, handle: &mut T) -> Option<String> {
    if let Some(Ok(name)) = file.name(handle, Some(NtfsFileNamespace::Win32), None) {
        return Some(name.name().to_string_lossy());
    }
    if let Some(Ok(name)) = file.name(handle, None, None) {
        if !matches!(name.namespace(), NtfsFileNamespace::Dos) {
            return Some(name.name().to_string_lossy());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests — pure logic only (no NTFS volume required)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn deleted_with_runs(runs: Vec<(Option<u64>, u64)>) -> DeletedEntry {
        DeletedEntry {
            record_no: 100,
            parent_record_no: 5,
            name: "gone.dat".into(),
            data_size: 4096,
            is_dir: false,
            modified_unix: 0,
            resident: false,
            runs: runs
                .into_iter()
                .map(|(lcn, clusters)| DwRun { lcn, clusters })
                .collect(),
            compressed: false,
            encrypted: false,
        }
    }

    #[test]
    fn bitmap_detects_allocated_clusters() {
        // byte 0 = 0b10101010 → clusters 1,3,5,7 allocated; 0,2,4,6 free
        let bm = ClusterBitmap {
            data: vec![0b1010_1010],
            total_clusters: 8,
        };
        assert!(!bm.is_allocated(0));
        assert!(bm.is_allocated(1));
        assert!(!bm.is_allocated(2));
        assert!(bm.is_allocated(7));
        // Out of range → conservative allocated
        assert!(bm.is_allocated(9999));
    }

    #[test]
    fn free_fraction_all_free() {
        let bm = ClusterBitmap {
            data: vec![0x00, 0x00],
            total_clusters: 16,
        };
        let e = deleted_with_runs(vec![(Some(0), 4), (Some(100), 4)]);
        assert!((bm.free_fraction(&e) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn free_fraction_partially_overwritten() {
        // clusters 0..8 → byte 0 = 0xFF (all allocated)
        let bm = ClusterBitmap {
            data: vec![0xFF, 0x00],
            total_clusters: 16,
        };
        let e = deleted_with_runs(vec![(Some(0), 8), (Some(8), 8)]);
        assert!((bm.free_fraction(&e) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn free_fraction_sparse_runs_count_free() {
        let bm = ClusterBitmap {
            data: vec![0xFF],
            total_clusters: 8,
        };
        // Fully allocated disk, but the entry is one sparse run of 4
        let e = deleted_with_runs(vec![(None, 4)]);
        assert!((bm.free_fraction(&e) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn free_fraction_no_runs_is_fully_free() {
        let bm = ClusterBitmap {
            data: vec![0xFF],
            total_clusters: 8,
        };
        let e = deleted_with_runs(vec![]);
        assert!((bm.free_fraction(&e) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn run_flags_pause_wait_returns_on_cancel() {
        let flags = RunFlags::default();
        flags.pause.store(true, Ordering::Release);
        flags.cancel.store(true, Ordering::Release);
        assert!(flags.wait_while_paused());
    }

    #[test]
    fn run_flags_unpaused_returns_false() {
        let flags = RunFlags::default();
        assert!(!flags.wait_while_paused());
    }

    #[test]
    fn volume_path_normalizes_and_rejects_unc() {
        assert_eq!(volume_path_for_drive("C").unwrap(), r"\\.\C:");
        assert!(volume_path_for_drive(r"\\server\share").is_err());
        assert!(volume_path_for_drive("").is_err());
    }
}

/// Object-safe Read+Seek bound shared by volume/lost-partition readers.
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// Reader-based variant of [`collect_mft_all`] for arbitrary sources
/// (lost partitions wrap a physical drive with a base offset; the
/// `volume_label` is used for logging/session metadata).
pub fn collect_mft_all_reader(
    reader: &mut Box<dyn ReadSeek>,
    volume_label: &str,
    flags: &RunFlags,
    progress: Option<&dyn Fn(&CollectProgress)>,
) -> Result<CollectedScan> {
    let start = Instant::now();
    let ntfs = Ntfs::new(reader).map_err(|e| Error::Scan(format!("NTFS parse error: {:?}", e)))?;
    collect_mft_on_ntfs(&ntfs, reader, volume_label, flags, progress, start)
}

/// Opens a raw volume file and runs the unified walk.
fn collect_mft_on_ntfs<T: Read + Seek>(
    ntfs: &Ntfs,
    handle: &mut T,
    volume_label: &str,
    flags: &RunFlags,
    progress: Option<&dyn Fn(&CollectProgress)>,
    start: Instant,
) -> Result<CollectedScan> {
    let estimated = crate::scan::walk::estimate_record_count(ntfs, handle)?;
    let cap = estimated.min(MAX_RECORD_HARD_LIMIT);
    debug!(volume = %volume_label, estimated, cap, "recovery MFT walk starts");

    let mut existing: Vec<crate::scan::walk::RawMftEntry> =
        Vec::with_capacity(((cap as usize) / 2).clamp(1024, 1_000_000));
    let mut deleted: Vec<DeletedEntry> = Vec::new();
    let mut skipped = 0u64;
    let mut last_name = String::new();
    let mut cancelled = false;

    'walk: for record_no in 16..cap {
        if (record_no - 16) % PROGRESS_INTERVAL == 0 {
            if flags.wait_while_paused() {
                cancelled = true;
                break 'walk;
            }
            if flags.is_cancelled() {
                cancelled = true;
                break 'walk;
            }
            if let Some(cb) = progress {
                cb(&CollectProgress {
                    visited: record_no - 16,
                    total: cap - 16,
                    existing: existing.len() as u64,
                    deleted: deleted.len() as u64,
                    last_name: last_name.clone(),
                });
            }
        }

        let file = match ntfs.file(handle, record_no) {
            Ok(f) => f,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let in_use = file.flags().contains(NtfsFileFlags::IN_USE);
        match extract_full(&file, handle) {
            Some((name, parent, size, is_dir, mtime)) => {
                last_name = name.clone();
                if in_use {
                    existing.push(crate::scan::walk::RawMftEntry {
                        record_no,
                        parent_record_no: parent,
                        name,
                        data_size: if is_dir { 0 } else { size },
                        is_dir,
                        modified_unix: mtime,
                    });
                } else {
                    let (resident, runs, compressed, encrypted) = extract_data_runs(&file, handle)
                        .unwrap_or((false, Vec::new(), false, false));
                    deleted.push(DeletedEntry {
                        record_no,
                        parent_record_no: parent,
                        name,
                        data_size: if is_dir { 0 } else { size },
                        is_dir,
                        modified_unix: mtime,
                        resident,
                        runs,
                        compressed,
                        encrypted,
                    });
                }
            }
            None => {
                skipped += 1;
            }
        }
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;
    info!(
        volume = %volume_label,
        existing = existing.len(),
        deleted = deleted.len(),
        skipped,
        cancelled,
        elapsed_ms,
        "recovery MFT walk finished"
    );

    Ok(CollectedScan {
        volume_path: volume_label.to_string(),
        cluster_size: ntfs.cluster_size(),
        record_size: ntfs.file_record_size(),
        total_records: estimated,
        existing,
        deleted,
        skipped_errors: skipped,
        elapsed_ms,
        cancelled,
    })
}
