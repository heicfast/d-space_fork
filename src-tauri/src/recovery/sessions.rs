// SPDX-License-Identifier: GPL-3.0-or-later
//
// Scan sessions — DiskWizard recovery engine.
//
// A session persists a finished scan (existing tree + deleted map +
// metadata) so it can be re-opened later without rescanning. Storage:
//   * a compact binary blob per session under
//     %LOCALAPPDATA%\DiskWizard\sessions\<id>.dws
//   * a SQLite row per session (id, target, counts, timestamps) —
//     migration 0004_sessions
// Completed scans auto-save; the newest 10 sessions are retained.

use crate::error::{Error, Result};
use crate::recovery::collect::DeletedEntry;
use crate::recovery::deleted::Confidence;
use crate::recovery::{RecoveryScan, ScanMeta, ScanStageWire, ScanStatusWire};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

/// Sessions kept on disk (auto-save pruning).
const KEEP_SESSIONS: usize = 10;

// ---------------------------------------------------------------------------
// Wire type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetaWire {
    pub id: String,
    pub file_name: String,
    pub saved_at_unix: i64,
    pub target_label: String,
    pub files_found: u64,
    pub data_found_bytes: u64,
    pub deleted_found: u64,
    pub completed: bool,
}

// ---------------------------------------------------------------------------
// Binary codec (little-endian, length-prefixed strings)
// ---------------------------------------------------------------------------

const MAGIC: &[u8; 8] = b"DWSESS02";

fn put_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn put_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn put_i64(buf: &mut Vec<u8>, v: i64) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn put_str(buf: &mut Vec<u8>, s: &str) {
    put_u32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return Err(Error::Db("session blob truncated".into()));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes(b.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64> {
        let b = self.take(8)?;
        Ok(i64::from_le_bytes(b.try_into().unwrap()))
    }
    fn str(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        if len > 64 * 1024 * 1024 {
            return Err(Error::Db("session blob string too large".into()));
        }
        let b = self.take(len)?;
        String::from_utf8(b.to_vec()).map_err(|e| Error::Db(format!("session blob: {e}")))
    }
}

/// Serializes a finished scan.
pub fn encode_session(scan: &RecoveryScan) -> Result<Vec<u8>> {
    let tree_guard = scan.tree.read().unwrap_or_else(|p| p.into_inner());
    let Some(tree) = tree_guard.as_ref() else {
        return Err(Error::Db("scan has no tree to save".into()));
    };
    let deleted = scan.deleted.read().unwrap_or_else(|p| p.into_inner());
    let confidence = scan.confidence.read().unwrap_or_else(|p| p.into_inner());
    let _deleted_children = scan
        .deleted_children
        .read()
        .unwrap_or_else(|p| p.into_inner());
    let lost = scan.lost_path.read().unwrap_or_else(|p| p.into_inner());
    let dir_sizes = scan
        .deleted_dir_sizes
        .read()
        .unwrap_or_else(|p| p.into_inner());

    let mut buf = Vec::with_capacity(64 * 1024 * 1024);
    buf.extend_from_slice(MAGIC);

    // ---- meta ----
    put_str(&mut buf, &scan.scan_id);
    put_str(&mut buf, &scan.meta.target_id);
    put_str(&mut buf, &scan.meta.target_label);
    put_str(&mut buf, &scan.meta.volume_id);
    put_str(&mut buf, &scan.meta.fs);
    // target enum
    match &scan.meta.target {
        crate::recovery::ScanTarget::Volume {
            letter,
            volume_path,
        } => {
            put_u32(&mut buf, 0);
            put_str(&mut buf, &letter.to_string());
            put_str(&mut buf, volume_path);
        }
        crate::recovery::ScanTarget::Unc { root } => {
            put_u32(&mut buf, 1);
            put_str(&mut buf, root);
        }
        crate::recovery::ScanTarget::LostPartition {
            physical_drive,
            base_byte,
            size_bytes,
            label,
        } => {
            put_u32(&mut buf, 2);
            put_u64(&mut buf, *physical_drive as u64);
            put_u64(&mut buf, *base_byte);
            put_u64(&mut buf, *size_bytes);
            put_str(&mut buf, label);
        }
    }
    put_str(&mut buf, scan.meta.path_scope.as_deref().unwrap_or(""));
    put_u32(
        &mut buf,
        *scan.cluster_size.read().unwrap_or_else(|p| p.into_inner()),
    );

    // ---- backend geometry (read-back dispatch) ----
    match &*scan.backend.read().unwrap_or_else(|p| p.into_inner()) {
        crate::recovery::BackendGeometry::Ntfs => {
            put_u32(&mut buf, 0);
        }
        crate::recovery::BackendGeometry::Fat(g) => {
            put_u32(&mut buf, 1);
            put_u32(
                &mut buf,
                match g.kind {
                    crate::fsfat::FatKindWire::Fat12 => 0,
                    crate::fsfat::FatKindWire::Fat16 => 1,
                    crate::fsfat::FatKindWire::Fat32 => 2,
                    crate::fsfat::FatKindWire::ExFat => 3,
                },
            );
            put_u64(&mut buf, g.cluster_size);
            put_u64(&mut buf, g.heap_off);
            put_u32(&mut buf, g.fat_copies.len() as u32);
            for off in &g.fat_copies {
                put_u64(&mut buf, *off);
            }
            put_u64(&mut buf, g.fat_bytes);
            put_u32(&mut buf, g.cluster_end);
            match g.exfat_bitmap {
                Some((first, len)) => {
                    put_u32(&mut buf, 1);
                    put_u32(&mut buf, first);
                    put_u64(&mut buf, len);
                }
                None => {
                    put_u32(&mut buf, 0);
                }
            }
        }
    }

    // ---- existing tree nodes ----
    put_u64(&mut buf, tree.nodes.len() as u64);
    let mut ids: Vec<&u64> = tree.nodes.keys().collect();
    ids.sort_unstable();
    for id in ids {
        let n = &tree.nodes[id];
        put_u64(&mut buf, n.id);
        match n.parent {
            Some(p) => {
                put_u32(&mut buf, 1);
                put_u64(&mut buf, p);
            }
            None => put_u32(&mut buf, 0),
        }
        put_str(&mut buf, &n.name);
        put_u64(&mut buf, n.data_size);
        put_u64(&mut buf, n.aggregate_size);
        put_u32(&mut buf, u8::from(n.is_dir) as u32);
        put_i64(&mut buf, n.modified_unix);
        put_u32(&mut buf, n.score.map(u32::from).unwrap_or(u32::MAX));
        put_str(&mut buf, n.score_rule.as_deref().unwrap_or(""));
        put_str(&mut buf, n.score_reason.as_deref().unwrap_or(""));
    }
    // Children map is derivable from parents — not stored.

    // ---- deleted entries ----
    put_u64(&mut buf, deleted.len() as u64);
    for (id, d) in deleted.iter() {
        put_u64(&mut buf, *id);
        put_u64(&mut buf, d.parent_record_no);
        put_str(&mut buf, &d.name);
        put_u64(&mut buf, d.data_size);
        put_u32(&mut buf, u8::from(d.is_dir) as u32);
        put_i64(&mut buf, d.modified_unix);
        put_u32(&mut buf, u8::from(d.resident) as u32);
        put_u32(&mut buf, u8::from(d.compressed) as u32);
        put_u32(&mut buf, u8::from(d.encrypted) as u32);
        put_u32(&mut buf, d.runs.len() as u32);
        for r in &d.runs {
            put_u32(&mut buf, r.lcn.map(|_l| 1).unwrap_or(0));
            put_u64(&mut buf, r.lcn.unwrap_or(0));
            put_u64(&mut buf, r.clusters);
        }
        // Slot offset (FAT/exFAT read-back re-check; 0 = none).
        put_u32(&mut buf, d.slot_offset.map(|_o| 1).unwrap_or(0));
        put_u64(&mut buf, d.slot_offset.unwrap_or(0));
        // Confidence byte.
        let conf = confidence.get(id).copied().unwrap_or(Confidence::Unknown);
        put_u32(&mut buf, confidence_byte(conf));
    }

    // ---- dir sizes + lost list ----
    put_u64(&mut buf, dir_sizes.len() as u64);
    for (k, v) in dir_sizes.iter() {
        put_u64(&mut buf, *k);
        put_u64(&mut buf, *v);
    }
    put_u64(&mut buf, lost.len() as u64);
    for l in lost.iter() {
        put_u64(&mut buf, *l);
    }
    // deleted_children is derivable from parent ids — not stored.

    // ---- runtime counters ----
    let rt = scan.runtime_snapshot();
    put_u64(&mut buf, rt.files_found);
    put_u64(&mut buf, rt.data_found_bytes);
    put_u64(&mut buf, rt.deleted_found);
    put_u32(&mut buf, completed_byte(rt.status));

    Ok(buf)
}

fn confidence_byte(c: Confidence) -> u32 {
    match c {
        Confidence::Excellent => 1,
        Confidence::Good => 2,
        Confidence::Fair => 3,
        Confidence::Poor => 4,
        Confidence::Unknown => 5,
    }
}

fn confidence_from(b: u32) -> Confidence {
    match b {
        1 => Confidence::Excellent,
        2 => Confidence::Good,
        3 => Confidence::Fair,
        4 => Confidence::Poor,
        _ => Confidence::Unknown,
    }
}

fn completed_byte(s: ScanStatusWire) -> u32 {
    if matches!(s, ScanStatusWire::Completed) {
        1
    } else {
        0
    }
}

/// Decode result: everything needed to rebuild the scan.
pub struct DecodedSession {
    pub meta: ScanMeta,
    pub scan_id: String,
    pub tree: crate::scan::tree::ScanTree,
    pub deleted: HashMap<u64, DeletedEntry>,
    pub confidence: HashMap<u64, Confidence>,
    pub dir_sizes: HashMap<u64, u64>,
    pub lost: Vec<u64>,
    pub files_found: u64,
    pub data_found_bytes: u64,
    pub deleted_found: u64,
    pub completed: bool,
    pub cluster_size: u32,
    pub backend: crate::recovery::BackendGeometry,
}

pub fn decode_session(data: &[u8]) -> Result<DecodedSession> {
    let mut c = Cursor { data, pos: 0 };
    let magic = c.take(8)?;
    if magic != MAGIC {
        if magic.starts_with(b"DWSESS") {
            return Err(Error::Db(
                "session saved by an older DiskWizard version; please re-scan".into(),
            ));
        }
        return Err(Error::Db("not a DiskWizard session file".into()));
    }

    let scan_id = c.str()?;
    let target_id = c.str()?;
    let target_label = c.str()?;
    let volume_id = c.str()?;
    let fs = c.str()?;
    let target = match c.u32()? {
        0 => {
            let letter = c.str()?;
            let volume_path = c.str()?;
            crate::recovery::ScanTarget::Volume {
                letter: letter.chars().next().unwrap_or('C'),
                volume_path,
            }
        }
        1 => {
            let root = c.str()?;
            crate::recovery::ScanTarget::Unc { root }
        }
        2 => {
            let drive = c.u64()? as u32;
            let base = c.u64()?;
            let size = c.u64()?;
            let label = c.str()?;
            crate::recovery::ScanTarget::LostPartition {
                physical_drive: drive,
                base_byte: base,
                size_bytes: size,
                label,
            }
        }
        _ => return Err(Error::Db("session blob: unknown target kind".into())),
    };
    let scope_raw = c.str()?;
    let path_scope = if scope_raw.is_empty() {
        None
    } else {
        Some(scope_raw)
    };
    let cluster_size = c.u32()?;
    let backend = match c.u32()? {
        0 => crate::recovery::BackendGeometry::Ntfs,
        1 => {
            let kind = match c.u32()? {
                0 => crate::fsfat::FatKindWire::Fat12,
                1 => crate::fsfat::FatKindWire::Fat16,
                2 => crate::fsfat::FatKindWire::Fat32,
                _ => crate::fsfat::FatKindWire::ExFat,
            };
            let g_cluster_size = c.u64()?;
            let heap_off = c.u64()?;
            let copies_len = c.u32()? as usize;
            if copies_len > 4 {
                return Err(Error::Db("session blob: absurd FAT copy count".into()));
            }
            let mut fat_copies = Vec::with_capacity(copies_len);
            for _ in 0..copies_len {
                fat_copies.push(c.u64()?);
            }
            let fat_bytes = c.u64()?;
            let cluster_end = c.u32()?;
            let exfat_bitmap = if c.u32()? != 0 {
                Some((c.u32()?, c.u64()?))
            } else {
                None
            };
            crate::recovery::BackendGeometry::Fat(crate::fsfat::FatGeometry {
                kind,
                cluster_size: g_cluster_size,
                heap_off,
                fat_copies,
                fat_bytes,
                cluster_end,
                exfat_bitmap,
            })
        }
        _ => return Err(Error::Db("session blob: unknown backend tag".into())),
    };

    let meta = ScanMeta {
        target,
        target_id,
        target_label,
        path_scope,
        drive_letter: None,
        fs,
        volume_id,
    };

    // ---- nodes ----
    let node_count = c.u64()? as usize;
    if node_count > 60_000_000 {
        return Err(Error::Db("session blob: absurd node count".into()));
    }
    let mut nodes = HashMap::with_capacity(node_count);
    let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
    for _ in 0..node_count {
        let id = c.u64()?;
        let parent = if c.u32()? == 1 { Some(c.u64()?) } else { None };
        let name = c.str()?;
        let data_size = c.u64()?;
        let aggregate_size = c.u64()?;
        let is_dir = c.u32()? != 0;
        let modified_unix = c.i64()?;
        let score_raw = c.u32()?;
        let score = if score_raw == u32::MAX {
            None
        } else {
            Some(score_raw as u8)
        };
        let score_rule = c.str()?;
        let score_reason = c.str()?;
        if let Some(p) = parent {
            children.entry(p).or_default().push(id);
        }
        nodes.insert(
            id,
            crate::scan::tree::Node {
                id,
                parent,
                name,
                data_size,
                aggregate_size,
                is_dir,
                score,
                score_rule: if score_rule.is_empty() {
                    None
                } else {
                    Some(score_rule)
                },
                score_reason: if score_reason.is_empty() {
                    None
                } else {
                    Some(score_reason)
                },
                modified_unix,
            },
        );
    }
    let root_id = nodes
        .keys()
        .copied()
        .find(|id| nodes.get(id).map(|n| n.parent.is_none()).unwrap_or(false))
        .unwrap_or(crate::scan::tree::ROOT_RECORD);

    // ---- deleted ----
    let deleted_count = c.u64()? as usize;
    if deleted_count > 60_000_000 {
        return Err(Error::Db("session blob: absurd deleted count".into()));
    }
    let mut deleted = HashMap::with_capacity(deleted_count);
    let mut confidence = HashMap::with_capacity(deleted_count);
    let mut deleted_children: HashMap<u64, Vec<u64>> = HashMap::new();
    for _ in 0..deleted_count {
        let id = c.u64()?;
        let parent = c.u64()?;
        let name = c.str()?;
        let data_size = c.u64()?;
        let is_dir = c.u32()? != 0;
        let modified_unix = c.i64()?;
        let resident = c.u32()? != 0;
        let compressed = c.u32()? != 0;
        let encrypted = c.u32()? != 0;
        let run_count = c.u32()? as usize;
        if run_count > 4096 {
            return Err(Error::Db("session blob: absurd run count".into()));
        }
        let mut runs = Vec::with_capacity(run_count);
        for _ in 0..run_count {
            let has_lcn = c.u32()? != 0;
            let lcn_v = c.u64()?;
            let clusters = c.u64()?;
            runs.push(crate::recovery::collect::DwRun {
                lcn: if has_lcn { Some(lcn_v) } else { None },
                clusters,
            });
        }
        let has_slot = c.u32()? != 0;
        let slot_v = c.u64()?;
        let slot_offset = if has_slot { Some(slot_v) } else { None };
        let conf = confidence_from(c.u32()?);
        deleted_children.entry(parent).or_default().push(id);
        deleted.insert(
            id,
            DeletedEntry {
                record_no: id,
                parent_record_no: parent,
                name,
                data_size,
                is_dir,
                modified_unix,
                resident,
                runs,
                compressed,
                encrypted,
                slot_offset,
            },
        );
        confidence.insert(id, conf);
    }

    // ---- dir sizes + lost ----
    let ds_count = c.u64()? as usize;
    let mut dir_sizes = HashMap::with_capacity(ds_count);
    for _ in 0..ds_count {
        let k = c.u64()?;
        let v = c.u64()?;
        dir_sizes.insert(k, v);
    }
    let lost_count = c.u64()? as usize;
    let mut lost = Vec::with_capacity(lost_count);
    for _ in 0..lost_count {
        lost.push(c.u64()?);
    }

    let files_found = c.u64()?;
    let data_found_bytes = c.u64()?;
    let deleted_found = c.u64()?;
    let completed = c.u32()? == 1;

    let tree = crate::scan::tree::ScanTree {
        volume_id: meta.volume_id.clone(),
        root_id,
        nodes,
        children,
        total_bytes: data_found_bytes,
        file_count: files_found,
        dir_count: 0,
        built_at_unix: crate::recovery::now_unix(),
        build_elapsed_ms: 0,
    };

    Ok(DecodedSession {
        meta,
        scan_id,
        tree,
        deleted,
        confidence,
        dir_sizes,
        lost,
        files_found,
        data_found_bytes,
        deleted_found,
        completed,
        cluster_size,
        backend,
    })
}

// ---------------------------------------------------------------------------
// Storage (files + SQLite rows)
// ---------------------------------------------------------------------------

fn sessions_dir() -> Result<PathBuf> {
    let base =
        dirs::data_local_dir().ok_or_else(|| Error::Db("data_local_dir unavailable".into()))?;
    let dir = base.join("DiskWizard").join("sessions");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Saves a scan as a session; returns the session id (== scan id).
pub fn save_session(db: &crate::db::DbState, scan: &Arc<RecoveryScan>) -> Result<SessionMetaWire> {
    let blob = encode_session(scan)?;
    let dir = sessions_dir()?;
    let file_name = format!("{}.dws", scan.scan_id);
    let path = dir.join(&file_name);
    std::fs::write(&path, &blob)?;

    let rt = scan.runtime_snapshot();
    let completed = matches!(rt.status, ScanStatusWire::Completed);
    let deleted_found = scan.deleted.read().unwrap_or_else(|p| p.into_inner()).len() as u64;

    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
        conn.execute(
            "INSERT OR REPLACE INTO sessions (id, file_name, target_label, volume_id, saved_at_unix, files_found, data_found_bytes, deleted_found, completed) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                scan.scan_id,
                file_name,
                scan.meta.target_label,
                scan.meta.volume_id,
                crate::recovery::now_unix(),
                rt.files_found,
                rt.data_found_bytes,
                deleted_found,
                completed as i64,
            ],
        )
        .map_err(|e| Error::Db(format!("session insert: {e}")))?;
    }
    prune_sessions(db)?;
    info!(session = %scan.scan_id, bytes = blob.len(), "session saved");
    Ok(SessionMetaWire {
        id: scan.scan_id.clone(),
        file_name,
        saved_at_unix: crate::recovery::now_unix(),
        target_label: scan.meta.target_label.clone(),
        files_found: rt.files_found,
        data_found_bytes: rt.data_found_bytes,
        deleted_found,
        completed,
    })
}

/// Auto-save wrapper used by the scan orchestration.
pub fn auto_save_session(db: &crate::db::DbState, scan: &Arc<RecoveryScan>) -> Result<()> {
    save_session(db, scan).map(|_| ())
}

/// Lists persisted sessions (newest first).
pub fn list_sessions(db: &crate::db::DbState) -> Result<Vec<SessionMetaWire>> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    let mut stmt = conn
        .prepare(
            "SELECT id, file_name, target_label, saved_at_unix, files_found, data_found_bytes, deleted_found, completed FROM sessions ORDER BY saved_at_unix DESC LIMIT 100",
        )
        .map_err(|e| Error::Db(format!("session select: {e}")))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SessionMetaWire {
                id: row.get(0)?,
                file_name: row.get(1)?,
                target_label: row.get(2)?,
                saved_at_unix: row.get(3)?,
                files_found: row.get(4)?,
                data_found_bytes: row.get(5)?,
                deleted_found: row.get::<_, i64>(6)? as u64,
                completed: row.get::<_, i64>(7)? != 0,
            })
        })
        .map_err(|e| Error::Db(format!("session query: {e}")))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| Error::Db(format!("session row: {e}")))?);
    }
    Ok(out)
}

/// Loads a session file and rebuilds the scan state (fresh scan id).
pub fn load_session(
    db: &crate::db::DbState,
    registry: &crate::recovery::ScanRegistry,
    file_name: &str,
) -> Result<String> {
    // Only load sessions we know (prevents arbitrary file reads).
    let known = list_sessions(db)?;
    let known = known
        .iter()
        .find(|s| s.file_name == file_name)
        .ok_or_else(|| Error::Db(format!("unknown session '{file_name}'")))?;
    let dir = sessions_dir()?;
    let path = dir.join(&known.file_name);
    let data = std::fs::read(&path).map_err(|e| Error::Db(format!("session read: {e}")))?;
    let decoded = decode_session(&data)?;

    let scan_id = registry.mint_id();
    let scan = Arc::new(RecoveryScan::new(
        scan_id.clone(),
        decoded.meta,
        std::sync::Arc::new(crate::recovery::collect::RunFlags::default()),
    ));
    *scan.tree.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(decoded.tree));
    *scan.deleted.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(decoded.deleted);
    *scan.confidence.write().unwrap_or_else(|p| p.into_inner()) = decoded.confidence;
    *scan
        .deleted_dir_sizes
        .write()
        .unwrap_or_else(|p| p.into_inner()) = decoded.dir_sizes;
    *scan.lost_path.write().unwrap_or_else(|p| p.into_inner()) = decoded.lost;
    // Cluster size drives deleted-file read-back math (runs are cluster
    // indexed) — a restored session without it silently produced empty
    // deleted previews and failed recoveries.
    *scan.cluster_size.write().unwrap_or_else(|p| p.into_inner()) = decoded.cluster_size;
    *scan.backend.write().unwrap_or_else(|p| p.into_inner()) = decoded.backend;
    *scan.strategy.write().unwrap_or_else(|p| p.into_inner()) = Some("session");
    {
        let mut rt = scan.runtime.write().unwrap_or_else(|p| p.into_inner());
        rt.status = if decoded.completed {
            ScanStatusWire::Completed
        } else {
            ScanStatusWire::Cancelled
        };
        rt.stage = ScanStageWire::Done;
        rt.overall_progress = 1.0;
        rt.files_found = decoded.files_found;
        rt.data_found_bytes = decoded.data_found_bytes;
        rt.deleted_found = decoded.deleted_found;
        rt.completed_at_unix = Some(crate::recovery::now_unix());
    }
    registry.insert(scan);
    Ok(scan_id)
}

/// Prunes old sessions beyond the retention window.
fn prune_sessions(db: &crate::db::DbState) -> Result<()> {
    let ids: Vec<(String, String)> = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
        let mut stmt = conn
            .prepare("SELECT id, file_name FROM sessions ORDER BY saved_at_unix DESC")
            .map_err(|e| Error::Db(format!("prune select: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| Error::Db(format!("prune query: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| Error::Db(format!("prune row: {e}")))?);
        }
        out
    };
    if ids.len() <= KEEP_SESSIONS {
        return Ok(());
    }
    let dir = sessions_dir()?;
    let conn = db
        .conn
        .lock()
        .map_err(|e| Error::Db(format!("mutex poisoned: {e}")))?;
    for (id, file_name) in ids.iter().skip(KEEP_SESSIONS) {
        let _ = conn.execute("DELETE FROM sessions WHERE id = ?1", rusqlite::params![id]);
        let _ = std::fs::remove_file(dir.join(file_name));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — codec roundtrip with a synthetic scan
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery::collect::{DeletedEntry, DwRun};
    use crate::recovery::{RecoveryScan, ScanMeta, ScanTarget};

    fn test_scan() -> Arc<RecoveryScan> {
        let meta = ScanMeta {
            target: ScanTarget::Volume {
                letter: 'C',
                volume_path: r"\\.\C:".into(),
            },
            target_id: "vol:C".into(),
            path_scope: None,
            target_label: "Local Disk (C:)".into(),
            drive_letter: Some('C'),
            fs: "NTFS".into(),
            volume_id: r"\\.\C:".into(),
        };
        let scan = Arc::new(RecoveryScan::new(
            "s-test-1".into(),
            meta,
            Arc::new(crate::recovery::collect::RunFlags::default()),
        ));
        let tree = crate::scan::tree::build_tree(
            r"\\.\C:".into(),
            vec![
                crate::scan::walk::RawMftEntry {
                    record_no: 100,
                    parent_record_no: 5,
                    name: "docs".into(),
                    data_size: 0,
                    is_dir: true,
                    modified_unix: 1000,
                },
                crate::scan::walk::RawMftEntry {
                    record_no: 101,
                    parent_record_no: 100,
                    name: "a.txt".into(),
                    data_size: 200,
                    is_dir: false,
                    modified_unix: 2000,
                },
            ],
        );
        *scan.tree.write().unwrap() = Some(Arc::new(tree));
        let mut deleted = HashMap::new();
        deleted.insert(
            900u64,
            DeletedEntry {
                record_no: 900,
                parent_record_no: 100,
                name: "gone.dat".into(),
                data_size: 4096,
                is_dir: false,
                modified_unix: 3000,
                resident: false,
                runs: vec![DwRun {
                    lcn: Some(1234),
                    clusters: 1,
                }],
                compressed: false,
                encrypted: false,
                slot_offset: None,
            },
        );
        *scan.deleted.write().unwrap() = std::sync::Arc::new(deleted);
        let mut confidence = HashMap::new();
        confidence.insert(900, Confidence::Excellent);
        *scan.confidence.write().unwrap() = confidence;
        let mut rt = scan.runtime.write().unwrap();
        rt.status = ScanStatusWire::Completed;
        rt.files_found = 1;
        rt.data_found_bytes = 200;
        rt.deleted_found = 1;
        drop(rt);
        scan
    }

    #[test]
    fn session_codec_roundtrip() {
        let scan = test_scan();
        let blob = encode_session(&scan).unwrap();
        let decoded = decode_session(&blob).unwrap();

        assert_eq!(decoded.scan_id, "s-test-1");
        assert_eq!(decoded.files_found, 1);
        assert_eq!(decoded.data_found_bytes, 200);
        assert!(decoded.completed);
        let tree = decoded.tree;
        assert!(tree.nodes.contains_key(&100));
        assert!(tree.nodes.contains_key(&101));
        assert_eq!(tree.nodes.get(&101).unwrap().data_size, 200);

        let d = decoded.deleted.get(&900).unwrap();
        assert_eq!(d.name, "gone.dat");
        assert_eq!(d.runs.len(), 1);
        assert_eq!(d.runs[0].lcn, Some(1234));
        assert_eq!(decoded.confidence.get(&900), Some(&Confidence::Excellent));
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode_session(b"garbage-not-a-session").is_err());
        let mut bad = Vec::new();
        bad.extend_from_slice(MAGIC);
        assert!(decode_session(&bad).is_err());
    }

    #[test]
    fn encoding_is_deterministic() {
        let scan = test_scan();
        let a = encode_session(&scan).unwrap();
        let b = encode_session(&scan).unwrap();
        assert_eq!(a, b);
    }
}
