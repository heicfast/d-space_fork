// SPDX-License-Identifier: GPL-3.0-or-later
//
// DiskWizard recovery domain — module root.
//
// This layer implements the Recovery Connector contract (see
// docs/CONNECTOR-CONTRACT.md) on top of the proven d-space engine:
// unified MFT scan (existing + deleted), server-side result queries,
// bounded previews, verified recovery jobs, persisted sessions, disk
// health and volume repair. All state lives here; `commands.rs` exposes
// thin Tauri commands over it.

pub mod collect;
pub mod commands;
pub mod deleted;
pub mod health;
pub mod jobs;
pub mod partitions;
pub mod preview;
pub mod repair;
pub mod results;
pub mod scan;
pub mod sessions;

use crate::error::Result;
use crate::recovery::collect::DeletedEntry;
use crate::recovery::deleted::Confidence;
use crate::scan::tree::ScanTree;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tracing::warn;

/// Synthetic node id for the "Lost Path" branch (deleted entries whose
/// parent chain no longer resolves). Uses a range MFT records can never
/// reach in practice (records are capped at 50M).
pub const LOST_PATH_ROOT: u64 = u64::MAX - 1;

// ---------------------------------------------------------------------------
// Wire enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatusWire {
    Starting,
    Running,
    Paused,
    Pausing,
    Resuming,
    Stopping,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScanStageWire {
    VolumeAnalysis,
    QuickIndex,
    DeepWalk,
    TreeBuild,
    Done,
}

// ---------------------------------------------------------------------------
// Devices (wire model — one device per mounted volume)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DwVolume {
    pub id: String,
    pub device_id: String,
    pub label: String,
    pub letter: Option<char>,
    pub fs: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    /// ok | permission-denied | disconnected | locked
    pub status: String,
    pub quick_scan_available: bool,
    pub can_recover_deleted: bool,
    pub bitlocker: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DwDevice {
    pub id: String,
    /// internal | usb | sd | optical | network | virtual
    pub kind: String,
    pub model: String,
    pub label: String,
    pub serial: Option<String>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub status: String,
    pub volumes: Vec<DwVolume>,
    pub can_scan_whole_disk: bool,
    pub is_source_of_active_scan: bool,
}

/// Maps the engine's drive list into the wire device model (one device per
/// mounted volume; hardware profiled per letter).
pub fn list_devices_wire() -> Result<Vec<DwDevice>> {
    let drives = crate::volume::list_drives()?;
    let elevated = crate::scan::is_elevated();
    let mut out = Vec::with_capacity(drives.len());
    for info in drives {
        let letter = info
            .drive_letter
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase());
        let letter = match letter {
            Some(l) if l.is_ascii_alphabetic() => l,
            _ => continue,
        };

        let hardware = crate::volume::probe_drive_hardware(letter).ok();
        let fs = info.file_system.trim().to_string();
        let is_ntfs = fs.eq_ignore_ascii_case("NTFS");

        // BitLocker / accessibility mapping from the pre-flight status.
        let (status, bitlocker) = match &info.status {
            crate::volume::VolumeStatus::Ready => ("ok".to_string(), None),
            crate::volume::VolumeStatus::BitLockerLocked => {
                ("locked".to_string(), Some("locked".to_string()))
            }
            crate::volume::VolumeStatus::BitLockerSuspended => {
                ("ok".to_string(), Some("unlocked".to_string()))
            }
            crate::volume::VolumeStatus::Encrypted { .. } => {
                ("ok".to_string(), Some("unlocked".to_string()))
            }
            crate::volume::VolumeStatus::NotMounted
            | crate::volume::VolumeStatus::UnsupportedDriveType { .. } => {
                ("disconnected".to_string(), None)
            }
            crate::volume::VolumeStatus::AccessDenied
            | crate::volume::VolumeStatus::NotFormatted => ("permission-denied".to_string(), None),
        };

        let kind = match info.drive_kind {
            crate::volume::DriveKind::Fixed => "internal",
            crate::volume::DriveKind::Removable => {
                // USB-bus removables are thumb drives; anything else
                // removable is treated as a memory card (SD).
                match hardware.as_ref().map(|h| h.bus_label.as_str()) {
                    Some("USB") => "usb",
                    _ => "sd",
                }
            }
            crate::volume::DriveKind::CdRom => "optical",
            crate::volume::DriveKind::Remote => "network",
            _ => "virtual",
        }
        .to_string();

        let model = hardware
            .as_ref()
            .map(|h| {
                let product = h.product.clone().unwrap_or_default();
                if product.is_empty() {
                    h.summary.clone()
                } else {
                    product
                }
            })
            .unwrap_or_else(|| format!("{} Drive", letter));

        let used = info.total_bytes.saturating_sub(info.free_bytes);
        let volume = DwVolume {
            id: format!("vol:{letter}"),
            device_id: format!("dev:{letter}"),
            label: if info.volume_label.is_empty() {
                format!("Local Disk ({letter}:)")
            } else {
                format!("{} ({letter}:)", info.volume_label)
            },
            letter: Some(letter),
            fs: if fs.is_empty() { "unknown".into() } else { fs },
            total_bytes: info.total_bytes,
            used_bytes: used,
            free_bytes: info.free_bytes,
            status: status.clone(),
            // Quick scan = raw MFT read: NTFS + admin only.
            quick_scan_available: is_ntfs && elevated,
            can_recover_deleted: is_ntfs && elevated,
            bitlocker: bitlocker.clone(),
        };

        out.push(DwDevice {
            id: format!("dev:{letter}"),
            kind,
            model,
            label: volume.label.clone(),
            serial: hardware.as_ref().and_then(|h| h.serial.clone()),
            total_bytes: info.total_bytes,
            used_bytes: used,
            free_bytes: info.free_bytes,
            status,
            volumes: vec![volume],
            can_scan_whole_disk: true,
            is_source_of_active_scan: false,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Scan runtime + registry
// ---------------------------------------------------------------------------

/// Where a scan reads from.
#[derive(Debug, Clone)]
pub enum ScanTarget {
    /// Local mounted volume by drive letter.
    Volume { letter: char, volume_path: String },
    /// Network share root (`\\server\share[\sub]`) — fallback walker only.
    Unc { root: String },
    /// Lost/unallocated partition on a physical drive (base byte offset).
    LostPartition {
        physical_drive: u32,
        base_byte: u64,
        size_bytes: u64,
        label: String,
    },
}

/// Immutable scan metadata set at start.
#[derive(Debug, Clone)]
pub struct ScanMeta {
    pub target: ScanTarget,
    /// Original target id as passed from the UI (for overlap exclusion).
    pub target_id: String,
    /// Optional path scope (absolute on volume, or volume-relative).
    pub path_scope: Option<String>,
    /// Target label for sessions ("Windows (C:)" style).
    pub target_label: String,
    pub drive_letter: Option<char>,
    pub fs: String,
    pub volume_id: String,
}

/// Mutable runtime state of a scan.
#[derive(Debug)]
pub struct ScanRuntime {
    pub status: ScanStatusWire,
    pub stage: ScanStageWire,
    pub stage_progress: f64,
    pub overall_progress: f64,
    pub bytes_scanned: u64,
    pub bytes_total: u64,
    pub files_found: u64,
    pub data_found_bytes: u64,
    pub deleted_found: u64,
    pub errors: u64,
    pub eta_seconds: Option<u64>,
    pub throughput_bytes_per_second: u64,
    pub current_path: Option<String>,
    pub device_connected: bool,
    pub started_at_unix: i64,
    pub completed_at_unix: Option<i64>,
    pub failure: Option<String>,
}

impl Default for ScanRuntime {
    fn default() -> Self {
        Self {
            status: ScanStatusWire::Starting,
            stage: ScanStageWire::VolumeAnalysis,
            stage_progress: 0.0,
            overall_progress: 0.0,
            bytes_scanned: 0,
            bytes_total: 0,
            files_found: 0,
            data_found_bytes: 0,
            deleted_found: 0,
            errors: 0,
            eta_seconds: None,
            throughput_bytes_per_second: 0,
            current_path: None,
            device_connected: true,
            started_at_unix: now_unix(),
            completed_at_unix: None,
            failure: None,
        }
    }
}

/// A completed or in-flight recovery scan, owned by the registry.
pub struct RecoveryScan {
    pub scan_id: String,
    pub meta: ScanMeta,
    pub flags: Arc<collect::RunFlags>,
    pub runtime: RwLock<ScanRuntime>,
    /// Existing-file tree (set once the build stage finishes).
    pub tree: RwLock<Option<Arc<ScanTree>>>,
    /// Deleted entries by record number.
    pub deleted: RwLock<HashMap<u64, DeletedEntry>>,
    /// Deleted confidence grades by record number.
    pub confidence: RwLock<HashMap<u64, Confidence>>,
    /// Parent → deleted child ids (covers both existing and deleted parents).
    pub deleted_children: RwLock<HashMap<u64, Vec<u64>>>,
    /// Deleted ids whose parent chain is dead (the "Lost Path" branch).
    pub lost_path: RwLock<Vec<u64>>,
    /// Aggregate sizes of deleted directories (recursive).
    pub deleted_dir_sizes: RwLock<HashMap<u64, u64>>,
    /// Node the path scope resolved to (None = whole volume).
    pub scope_node: RwLock<Option<u64>>,
    /// Scan strategy actually used ("MFT" | "fallback").
    pub strategy: RwLock<Option<&'static str>>,
    /// Cluster size (0 when unknown / fallback scans).
    pub cluster_size: RwLock<u32>,
}

impl RecoveryScan {
    pub fn new(scan_id: String, meta: ScanMeta, flags: Arc<collect::RunFlags>) -> Self {
        Self {
            scan_id,
            meta,
            flags,
            runtime: RwLock::new(ScanRuntime::default()),
            tree: RwLock::new(None),
            deleted: RwLock::new(HashMap::new()),
            confidence: RwLock::new(HashMap::new()),
            deleted_children: RwLock::new(HashMap::new()),
            lost_path: RwLock::new(Vec::new()),
            deleted_dir_sizes: RwLock::new(HashMap::new()),
            scope_node: RwLock::new(None),
            strategy: RwLock::new(None),
            cluster_size: RwLock::new(0),
        }
    }

    pub fn runtime_snapshot(&self) -> ScanRuntime {
        // ScanRuntime is plain data; a cheap manual copy avoids Clone on
        // f64 fields (no derive needed).
        let g = self.runtime.read().unwrap_or_else(|p| p.into_inner());
        ScanRuntime {
            status: g.status,
            stage: g.stage,
            stage_progress: g.stage_progress,
            overall_progress: g.overall_progress,
            bytes_scanned: g.bytes_scanned,
            bytes_total: g.bytes_total,
            files_found: g.files_found,
            data_found_bytes: g.data_found_bytes,
            deleted_found: g.deleted_found,
            errors: g.errors,
            eta_seconds: g.eta_seconds,
            throughput_bytes_per_second: g.throughput_bytes_per_second,
            current_path: g.current_path.clone(),
            device_connected: g.device_connected,
            started_at_unix: g.started_at_unix,
            completed_at_unix: g.completed_at_unix,
            failure: g.failure.clone(),
        }
    }

    /// Full display path of a node ("C:\Users\x" or "D2:lost\<path>").
    pub fn node_display_path(&self, id: u64) -> Option<String> {
        let tree_guard = self.tree.read().unwrap_or_else(|p| p.into_inner());
        let tree = tree_guard.as_ref()?;
        let chain = crate::scan::tree::node_path(tree, id);
        if chain.is_empty() {
            return None;
        }
        let parts: Vec<&str> = chain
            .iter()
            .filter(|n| n.id != tree.root_id)
            .map(|n| n.name.as_str())
            .collect();
        match &self.meta.target {
            ScanTarget::Volume { letter, .. } => {
                if parts.is_empty() {
                    Some(format!("{letter}:\\"))
                } else {
                    Some(format!("{letter}:\\{}", parts.join("\\")))
                }
            }
            ScanTarget::Unc { root } => {
                let root = root.trim_end_matches('\\');
                if parts.is_empty() {
                    Some(root.to_string())
                } else {
                    Some(format!("{}\\{}", root, parts.join("\\")))
                }
            }
            ScanTarget::LostPartition { label, .. } => {
                if parts.is_empty() {
                    Some(label.clone())
                } else {
                    Some(format!("{}\\{}", label, parts.join("\\")))
                }
            }
        }
    }
}

/// Registry of all scans this app run (in-flight, completed, restored).
#[derive(Default)]
pub struct ScanRegistry {
    scans: RwLock<HashMap<String, Arc<RecoveryScan>>>,
    counter: AtomicU64,
}

impl ScanRegistry {
    pub fn mint_id(&self) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("s{ts}-{n}")
    }

    pub fn insert(&self, scan: Arc<RecoveryScan>) {
        let mut map = self.scans.write().unwrap_or_else(|p| p.into_inner());
        // Bound the registry: keep the 8 most recent scans.
        if map.len() >= 8 {
            if let Some(oldest) = map.iter().min_by_key(|(k, _)| *k).map(|(k, _)| k.clone()) {
                map.remove(&oldest);
            }
        }
        map.insert(scan.scan_id.clone(), scan);
    }

    pub fn get(&self, scan_id: &str) -> Option<Arc<RecoveryScan>> {
        self.scans
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(scan_id)
            .cloned()
    }

    pub fn active_scan_for_target(&self, target_id: &str) -> Option<String> {
        let map = self.scans.read().unwrap_or_else(|p| p.into_inner());
        map.values()
            .find(|s| {
                s.meta.target_id == target_id
                    && matches!(
                        s.runtime_snapshot().status,
                        ScanStatusWire::Starting
                            | ScanStatusWire::Running
                            | ScanStatusWire::Paused
                            | ScanStatusWire::Pausing
                            | ScanStatusWire::Resuming
                    )
            })
            .map(|s| s.scan_id.clone())
    }
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Emits a scan progress event to the frontend (single event name; the
/// payload carries the scan id so the connector routes it).
pub fn emit_scan_progress<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
    scan: &RecoveryScan,
) {
    use tauri::Emitter;
    let rt = scan.runtime_snapshot();
    let payload = ScanProgressEventWire::from_runtime(&scan.scan_id, &rt);
    if let Err(e) = window.emit("dw-scan-progress", &payload) {
        warn!(?e, "failed to emit dw-scan-progress");
    }
}

/// Wire shape of a scan progress event (camelCase for the connector).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgressEventWire {
    pub scan_id: String,
    pub status: ScanStatusWire,
    pub stage: ScanStageWire,
    pub stage_progress: f64,
    pub overall_progress: f64,
    pub bytes_scanned: u64,
    pub bytes_total: u64,
    pub files_found: u64,
    pub data_found_bytes: u64,
    pub deleted_found: u64,
    pub errors: u64,
    pub bad_sector_map: Vec<BadSectorRangeWire>,
    pub eta_seconds: Option<u64>,
    pub throughput_bytes_per_second: u64,
    pub recoverable_during_scan: bool,
    pub device_connected: bool,
    pub current_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BadSectorRangeWire {
    pub start_byte: u64,
    pub size_bytes: u64,
}

impl ScanProgressEventWire {
    pub fn from_runtime(scan_id: &str, rt: &ScanRuntime) -> Self {
        Self {
            scan_id: scan_id.to_string(),
            status: rt.status,
            stage: rt.stage,
            stage_progress: rt.stage_progress,
            overall_progress: rt.overall_progress,
            bytes_scanned: rt.bytes_scanned,
            bytes_total: rt.bytes_total,
            files_found: rt.files_found,
            data_found_bytes: rt.data_found_bytes,
            deleted_found: rt.deleted_found,
            errors: rt.errors,
            // Local disks have no bad-sector map (that is a deep-carve
            // concept); kept for contract compatibility.
            bad_sector_map: Vec::new(),
            eta_seconds: rt.eta_seconds,
            throughput_bytes_per_second: rt.throughput_bytes_per_second,
            // Results are queryable as soon as the tree exists.
            recoverable_during_scan: rt.overall_progress >= 1.0,
            device_connected: rt.device_connected,
            current_path: rt.current_path.clone(),
        }
    }
}

/// Registry of recovery jobs.
#[derive(Default)]
pub struct JobRegistry {
    pub jobs: RwLock<HashMap<String, Arc<jobs::RecoveryJob>>>,
    counter: AtomicU64,
}

impl JobRegistry {
    pub fn mint_id(&self) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("j{ts}-{n}")
    }

    pub fn insert(&self, job: Arc<jobs::RecoveryJob>) {
        let mut map = self.jobs.write().unwrap_or_else(|p| p.into_inner());
        if map.len() >= 8 {
            if let Some(oldest) = map.iter().min_by_key(|(k, _)| *k).map(|(k, _)| k.clone()) {
                map.remove(&oldest);
            }
        }
        map.insert(job.job_id.clone(), job);
    }

    pub fn get(&self, job_id: &str) -> Option<Arc<jobs::RecoveryJob>> {
        self.jobs
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(job_id)
            .cloned()
    }
}
