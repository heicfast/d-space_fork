// SPDX-License-Identifier: GPL-3.0-or-later
//
// Staging operations — Chapter 12.
// v0.1 Phase 1: same-volume `fs::rename` (atomic). Cross-volume two-phase
// commit (Chapter 12.3 Layer C) comes in the next sprint.

use crate::error::{Error, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Chapter 12.2 — the default undo window is 24 hours.
pub const STAGING_TTL_SECS: i64 = 24 * 3600;

#[derive(Debug, Clone, Serialize)]
pub struct StagedItem {
    pub id: i64,
    pub original_path: String,
    pub staged_path: String,
    pub size_bytes: u64,
    pub staged_at_unix: i64,
    pub expires_at_unix: i64,
    pub is_dir: bool,
    pub fallback_tier: String,
}

fn staging_base() -> Result<PathBuf> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| Error::Staging("data_local_dir could not be resolved".into()))?;
    Ok(base.join("DiskWizard").join("staging"))
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(windows)]
fn first_drive_letter(p: &Path) -> Option<char> {
    use std::path::{Component, Prefix};
    p.components().next().and_then(|c| match c {
        Component::Prefix(prefix) => match prefix.kind() {
            Prefix::Disk(b) | Prefix::VerbatimDisk(b) => Some(b.to_ascii_uppercase() as char),
            _ => None,
        },
        _ => None,
    })
}

#[cfg(not(windows))]
fn first_drive_letter(_p: &Path) -> Option<char> {
    None
}

/// Chapter 12.2 — atomically MOVEs the user's selection into the staging
/// folder. Cross-volume (and UNC-sourced) requests go down the two-phase
/// commit path (Chapter 12.3 Layer C).
pub fn stage(path: &str, conn: &Connection) -> Result<StagedItem> {
    let src = PathBuf::from(path);
    if !src.exists() {
        return Err(Error::Staging(format!("Not found: {}", path)));
    }

    let metadata =
        fs::metadata(&src).map_err(|e| Error::Staging(format!("metadata '{}': {}", path, e)))?;
    let is_dir = metadata.is_dir();
    let size_bytes = if is_dir { 0 } else { metadata.len() };

    let staged_at = now_unix();
    let dest_dir = staging_base()?.join(staged_at.to_string());
    fs::create_dir_all(&dest_dir).map_err(|e| Error::Staging(format!("mkdir: {}", e)))?;

    let file_name = src
        .file_name()
        .ok_or_else(|| Error::Staging("missing filename".into()))?;
    let mut dest = dest_dir.join(file_name);

    // Collision avoidance — the same name can occur within the same second
    let mut suffix = 1u32;
    while dest.exists() {
        dest = dest_dir.join(format!("{:03}_{}", suffix, file_name.to_string_lossy()));
        suffix += 1;
        if suffix > 999 {
            return Err(Error::Staging("Conflict count limit exceeded".into()));
        }
    }

    // Cross-volume check (Chapter 12.3) — dispatch to the two-phase commit path.
    // Gemini review B11 — on a UNC source `first_drive_letter` returns None;
    // the old code assumed same-volume and produced a raw rename error. Now
    // None (UNC/rootless) also goes down the cross-volume copy path — the
    // streaming copy works from the network toward local staging.
    let src_drive = first_drive_letter(&src);
    let dest_drive = first_drive_letter(&dest);
    let is_cross = src_drive != dest_drive;

    if is_cross {
        debug!(
            src = %src.display(),
            dest = %dest.display(),
            "cross-volume detected → two-phase commit"
        );
        let target_vol = dest_drive
            .map(|c| format!("{}:", c))
            .unwrap_or_else(|| "?".into());
        return crate::staging::cross_volume::cross_volume_stage_file(
            &src,
            &dest,
            &target_vol,
            size_bytes,
            is_dir,
            conn,
        );
    }

    debug!(src = %src.display(), dest = %dest.display(), "same-volume fs::rename");
    fs::rename(&src, &dest).map_err(|e| Error::Staging(format!("rename: {}", e)))?;

    let expires_at = staged_at + STAGING_TTL_SECS;
    // Gemini review B3 — if the DB insert fails, roll the rename back; the
    // crash window narrows (a hard crash between rename→insert is still
    // possible; it will be closed in v0.3 by the startup recovery's
    // untracked bucket sweep).
    if let Err(e) = conn.execute(
        "INSERT INTO staging_items
            (original_path, staged_path, size_bytes, staged_at, expires_at,
             is_dir, reason, fallback_tier)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 'normal')",
        params![
            path,
            dest.to_string_lossy(),
            size_bytes as i64,
            staged_at,
            expires_at,
            is_dir as i64,
        ],
    ) {
        let _ = fs::rename(&dest, &src);
        return Err(Error::Staging(format!("DB insert: {}", e)));
    }

    let id = conn.last_insert_rowid();
    info!(
        id,
        size = size_bytes,
        is_dir,
        original = %path,
        staged = %dest.display(),
        "stage complete"
    );

    Ok(StagedItem {
        id,
        original_path: path.to_string(),
        staged_path: dest.to_string_lossy().to_string(),
        size_bytes,
        staged_at_unix: staged_at,
        expires_at_unix: expires_at,
        is_dir,
        fallback_tier: "normal".into(),
    })
}

/// Returns all pending (not yet deleted) staging items in the DB.
/// In staged_at descending order, max 200 (in line with the Chapter 9.6 limit).
pub fn list_pending(conn: &Connection) -> Result<Vec<StagedItem>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, original_path, staged_path, size_bytes, staged_at,
                    expires_at, is_dir, fallback_tier
             FROM staging_items
             ORDER BY staged_at DESC
             LIMIT 200",
        )
        .map_err(|e| Error::Staging(format!("prepare: {}", e)))?;

    let rows = stmt
        .query_map([], |r| {
            Ok(StagedItem {
                id: r.get(0)?,
                original_path: r.get(1)?,
                staged_path: r.get(2)?,
                size_bytes: r.get::<_, i64>(3)? as u64,
                staged_at_unix: r.get(4)?,
                expires_at_unix: r.get(5)?,
                is_dir: r.get::<_, i64>(6)? != 0,
                fallback_tier: r.get(7)?,
            })
        })
        .map_err(|e| Error::Staging(format!("query: {}", e)))?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|e| Error::Staging(format!("row: {}", e)))?);
    }
    Ok(items)
}

/// Chapter 12.2.4.1 — file fingerprint for conflict detection.
#[derive(Debug, Clone, Serialize)]
pub struct FileSnapshot {
    pub path: String,
    pub size_bytes: u64,
    pub modified_unix: i64,
    /// Blake3 hash of the first 4 KB, hex. None for folders.
    pub blake3_first4kb_hex: Option<String>,
    pub is_dir: bool,
}

/// Chapter 12.2.4 — one of the four outcomes of the undo operation.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UndoOutcome {
    /// The target path was empty — the staged file was moved, the row deleted.
    Restored { original_path: String },
    /// The target already has the same content — deleting it is enough (idempotent).
    Idempotent { original_path: String },
    /// Conflict — a user decision is required (UI dialog).
    Conflict {
        original_path: String,
        staged: FileSnapshot,
        target: FileSnapshot,
    },
}

/// Chapter 12.2.4.2 — the user selection returned by the conflict dialog.
#[derive(Debug, Clone, Copy, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Delete the target file, move the staged one.
    Overwrite,
    /// Place the staged file next to the target with a new name like "name (1).ext".
    Rename,
    /// Same semantics as Rename (both are preserved). For UI distinction.
    KeepBoth,
    /// Undo cancelled — the staging row is preserved.
    Cancel,
}

fn first_4kb_hash(path: &Path) -> Option<[u8; 32]> {
    use blake3::Hasher;
    use std::io::{BufReader, Read};
    if path.is_dir() {
        return None;
    }
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::with_capacity(4096, file);
    let mut buf = vec![0u8; 4096];
    let mut total = 0;
    while total < 4096 {
        match reader.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => return None,
        }
    }
    let mut hasher = Hasher::new();
    hasher.update(&buf[..total]);
    Some(*hasher.finalize().as_bytes())
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn snapshot_of(path: &Path) -> FileSnapshot {
    let meta = fs::metadata(path).ok();
    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
    let size_bytes = if is_dir {
        0
    } else {
        meta.as_ref().map(|m| m.len()).unwrap_or(0)
    };
    let modified_unix = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let blake3_first4kb_hex = first_4kb_hash(path).map(|h| hex32(&h));
    FileSnapshot {
        path: path.to_string_lossy().to_string(),
        size_bytes,
        modified_unix,
        blake3_first4kb_hex,
        is_dir,
    }
}

fn lookup_staging(conn: &Connection, id: i64) -> Result<(String, String)> {
    conn.query_row(
        "SELECT original_path, staged_path FROM staging_items WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(|e| Error::Staging(format!("undo lookup id={}: {}", id, e)))
}

fn ensure_parent(dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::Staging(format!("parent mkdir: {}", e)))?;
    }
    Ok(())
}

fn delete_staging_row(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM staging_items WHERE id = ?1", params![id])
        .map_err(|e| Error::Staging(format!("DB delete: {}", e)))?;
    Ok(())
}

/// Gemini review B32 — when the staged file leaves the bucket, silently
/// clean up the `staging\<timestamp>` directory left empty (no buildup).
fn cleanup_bucket_if_empty(staged_path: &Path) {
    if let Some(bucket) = staged_path.parent() {
        // remove_dir only succeeds on an EMPTY directory — if non-empty, no-op.
        if let Err(e) = fs::remove_dir(bucket) {
            debug!(bucket = %bucket.display(), error = %e, "bucket cleanup skipped");
        }
    }
}

/// Gemini review B1 — cross-volume undo: on Windows `fs::rename` gives a
/// cross-device error without `MOVEFILE_COPY_ALLOWED`. Moving back from the
/// staging area to the original volume is the same two-phase protocol as
/// stage: tmp copy → Blake3 verify → atomic rename → delete the source.
fn undo_move_across_volumes(src: &Path, dest: &Path) -> Result<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| Error::Staging("undo target parent missing".into()))?;
    ensure_parent(dest)?;
    let file_name = dest
        .file_name()
        .ok_or_else(|| Error::Staging("undo target filename missing".into()))?
        .to_string_lossy()
        .to_string();
    let tmp_path = parent.join(format!("{}.dspace_undo_tmp", file_name));

    // Phase 1a: copy
    crate::staging::cross_volume::copy_file_streaming(src, &tmp_path)?;

    // Phase 1b: hash verify
    let src_hash = crate::staging::cross_volume::blake3_file(src)?;
    let dst_hash = crate::staging::cross_volume::blake3_file(&tmp_path)?;
    if src_hash != dst_hash {
        let _ = fs::remove_file(&tmp_path);
        return Err(Error::Staging(
            "Undo hash mismatch (Blake3) — copy aborted, staged file intact".into(),
        ));
    }

    // Phase 2: atomic rename + delete the source
    fs::rename(&tmp_path, dest).map_err(|e| Error::Staging(format!("undo final rename: {}", e)))?;
    fs::remove_file(src).map_err(|e| {
        // The target was written + the source is still there — the user has
        // two copies; no data loss, but the row must be kept.
        Error::Staging(format!("undo source could not be deleted: {}", e))
    })?;
    cleanup_bucket_if_empty(src);
    Ok(())
}

/// Move the file to the target — same-volume rename, cross-volume copy+verify.
/// Shared helper of undo / undo_with_resolution (Gemini review B1).
fn undo_move(src: &Path, dest: &Path) -> Result<()> {
    let src_drive = first_drive_letter(src);
    let dest_drive = first_drive_letter(dest);
    if src_drive != dest_drive {
        undo_move_across_volumes(src, dest)
    } else {
        ensure_parent(dest)?;
        match fs::rename(src, dest) {
            Ok(()) => {
                cleanup_bucket_if_empty(src);
                Ok(())
            }
            Err(e) => Err(Error::Staging(format!("undo rename: {}", e))),
        }
    }
}

/// Chapter 12.2.4 — undo. Three possible returns:
///   * Restored — the target was empty, the move was performed.
///   * Idempotent — the target already has the same content, staged deleted.
///   * Conflict — user decision required; returns snapshots for the UI dialog.
///
/// Gemini review B2 — the idempotent decision is now based on file size:
///   * ≤ 64 MB: full-file Blake3 comparison (definitive result),
///   * \> 64 MB: a 4 KB + size match only indicates "probably the same";
///     it falls to Conflict so there is no RISK of silent deletion — the
///     user consciously chooses the same outcome via Overwrite.
pub fn undo(id: i64, conn: &Connection) -> Result<UndoOutcome> {
    let (original, staged) = lookup_staging(conn, id)?;
    let src = Path::new(&staged);
    let dest = Path::new(&original);

    if !src.exists() {
        return Err(Error::Staging(format!("Staged file not found: {}", staged)));
    }

    // Target empty — restore directly (cross-volume included, undo_move handles it).
    if !dest.exists() {
        undo_move(src, dest)?;
        delete_staging_row(conn, id)?;
        info!(id, original = %original, "undo complete (Restored)");
        return Ok(UndoOutcome::Restored {
            original_path: original,
        });
    }

    // Target occupied — content comparison (only for files).
    let staged_snap = snapshot_of(src);
    let target_snap = snapshot_of(dest);

    let both_files = !staged_snap.is_dir && !target_snap.is_dir;
    const FULL_HASH_THRESHOLD: u64 = 64 * 1024 * 1024;
    let same_hash = both_files
        && staged_snap.size_bytes == target_snap.size_bytes
        && staged_snap.size_bytes > 0
        && if staged_snap.size_bytes <= FULL_HASH_THRESHOLD {
            // Full-file Blake3 — definitive result.
            let sh = crate::staging::cross_volume::blake3_file(src);
            let th = crate::staging::cross_volume::blake3_file(dest);
            matches!((sh, th), (Ok(a), Ok(b)) if a == b)
        } else {
            // Over 64 MB: a 4 KB match means "probably the same" — not certain.
            // Gemini review B2: no silent deletion → falls to Conflict.
            false
        };

    if same_hash {
        // Idempotent: the target is already the same file. Delete the staged
        // file, remove the DB row.
        // Gemini review B12 — the row is removed only after a successful
        // delete; if deletion fails (lock etc.) warn + keep the row.
        match fs::remove_file(src) {
            Ok(()) => {
                cleanup_bucket_if_empty(src);
                delete_staging_row(conn, id)?;
                info!(id, original = %original, "undo idempotent — target identical to the staged file");
                Ok(UndoOutcome::Idempotent {
                    original_path: original,
                })
            }
            Err(e) => {
                warn!(staged = %src.display(), error = %e, "idempotent delete failed — row preserved");
                Err(Error::Staging(format!(
                    "staged file could not be deleted: {}",
                    e
                )))
            }
        }
    } else {
        warn!(
            dest = %dest.display(),
            "undo conflict — user decision required"
        );
        Ok(UndoOutcome::Conflict {
            original_path: original,
            staged: staged_snap,
            target: target_snap,
        })
    }
}

fn rename_target(dest: &Path) -> PathBuf {
    let parent = dest.parent().unwrap_or_else(|| Path::new(""));
    let stem = dest
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "untitled".into());
    let ext = dest
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 1..=999 {
        let candidate = parent.join(format!("{} ({}){}", stem, n, ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{} (rename-overflow){}", stem, ext))
}

/// Chapter 12.2.4.2 — the user has made a choice in the conflict dialog.
/// Place the staged file according to that choice.
pub fn undo_with_resolution(
    id: i64,
    resolution: ConflictResolution,
    conn: &Connection,
) -> Result<UndoOutcome> {
    let (original, staged) = lookup_staging(conn, id)?;
    let src = Path::new(&staged);
    let dest = Path::new(&original);

    if !src.exists() {
        return Err(Error::Staging(format!("Staged file not found: {}", staged)));
    }

    match resolution {
        ConflictResolution::Cancel => {
            info!(id, "undo conflict cancelled — staging row preserved");
            Err(Error::Staging("Undo cancelled.".into()))
        }
        ConflictResolution::Overwrite => {
            if !dest.exists() {
                // Race: the target has meanwhile been emptied. Do a normal restore.
                undo_move(src, dest)?;
            } else {
                let target_is_dir = dest.is_dir();
                if target_is_dir {
                    fs::remove_dir_all(dest)
                        .map_err(|e| Error::Staging(format!("overwrite remove_dir_all: {}", e)))?;
                } else {
                    fs::remove_file(dest)
                        .map_err(|e| Error::Staging(format!("overwrite remove_file: {}", e)))?;
                }
                undo_move(src, dest)?;
            }
            delete_staging_row(conn, id)?;
            info!(id, original = %original, "undo conflict → overwrite");
            Ok(UndoOutcome::Restored {
                original_path: original,
            })
        }
        ConflictResolution::Rename | ConflictResolution::KeepBoth => {
            ensure_parent(dest)?;
            let new_dest = rename_target(dest);
            undo_move(src, &new_dest)?;
            delete_staging_row(conn, id)?;
            info!(
                id,
                original = %original,
                new = %new_dest.display(),
                "undo conflict → rename"
            );
            Ok(UndoOutcome::Restored {
                original_path: new_dest.to_string_lossy().to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;

    fn unique_tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dspace-stage-{}-{}-{}",
            std::process::id(),
            now_unix(),
            name
        ))
    }

    #[test]
    fn stage_and_undo_roundtrip() {
        // This test uses a real DB and file system (in-memory connection +
        // temp dir). It performs a same-volume rename.
        let conn = Connection::open_in_memory().unwrap();
        let migrations = rusqlite_migration::Migrations::new(vec![rusqlite_migration::M::up(
            include_str!("../db/migrations/0001_initial.sql"),
        )]);
        let mut conn = conn;
        migrations.to_latest(&mut conn).unwrap();

        // Workspace: real FS, temporary directories. The source and the
        // staging area must be on the same volume (TEMP).
        let work_root = unique_tmp("work");
        fs::create_dir_all(&work_root).unwrap();
        let src = work_root.join("doomed.txt");
        fs::write(&src, b"deletable content").unwrap();
        assert!(src.exists());

        // The staging base is already LOCALAPPDATA — unless this test
        // overrides it, the real staging folder is used; we clean up afterwards.
        let staged = stage(src.to_str().unwrap(), &conn).expect("stage complete");
        assert!(!src.exists(), "source should have been moved");
        assert!(Path::new(&staged.staged_path).exists());

        // Undo — target empty, Restored is expected
        let outcome = undo(staged.id, &conn).expect("undo should succeed");
        match outcome {
            UndoOutcome::Restored { original_path } => {
                assert_eq!(original_path, src.to_string_lossy());
            }
            _ => panic!("expected Restored"),
        }
        assert!(src.exists(), "source should be restored");
        assert!(!Path::new(&staged.staged_path).exists());

        // The staging_item row must have been deleted from the DB
        let pending = list_pending(&conn).unwrap();
        assert!(pending.iter().all(|p| p.id != staged.id));

        // Cleanup
        let _ = fs::remove_dir_all(&work_root);
    }

    #[test]
    fn undo_detects_conflict_when_target_differs() {
        let mut conn = Connection::open_in_memory().unwrap();
        let migrations = rusqlite_migration::Migrations::new(vec![rusqlite_migration::M::up(
            include_str!("../db/migrations/0001_initial.sql"),
        )]);
        migrations.to_latest(&mut conn).unwrap();

        let work = unique_tmp("conflict");
        fs::create_dir_all(&work).unwrap();
        let target = work.join("Rapor.pdf");
        let staged_dir = work.join("staging-bucket");
        fs::create_dir_all(&staged_dir).unwrap();
        let staged_file = staged_dir.join("Rapor.pdf");
        fs::write(&target, b"yeni icerik 13 mayis").unwrap();
        fs::write(&staged_file, b"eski icerik 12 mayis").unwrap();

        // Insert the staging_items row manually
        let now = now_unix();
        conn.execute(
            "INSERT INTO staging_items
                (original_path, staged_path, size_bytes, staged_at, expires_at,
                 is_dir, reason, fallback_tier)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, 'normal')",
            params![
                target.to_string_lossy(),
                staged_file.to_string_lossy(),
                staged_file.metadata().unwrap().len() as i64,
                now,
                now + 86400,
            ],
        )
        .unwrap();
        let id = conn.last_insert_rowid();

        let outcome = undo(id, &conn).expect("undo should report conflict");
        match outcome {
            UndoOutcome::Conflict {
                original_path,
                staged,
                target: t,
            } => {
                assert_eq!(original_path, target.to_string_lossy());
                assert_ne!(staged.blake3_first4kb_hex, t.blake3_first4kb_hex);
                assert_eq!(staged.size_bytes, 20);
                assert_eq!(t.size_bytes, 20);
            }
            other => panic!("expected Conflict, got: {:?}", other),
        }
        // When Conflict is returned, the staged file is still there
        assert!(staged_file.exists());

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn undo_idempotent_when_target_same_hash() {
        let mut conn = Connection::open_in_memory().unwrap();
        let migrations = rusqlite_migration::Migrations::new(vec![rusqlite_migration::M::up(
            include_str!("../db/migrations/0001_initial.sql"),
        )]);
        migrations.to_latest(&mut conn).unwrap();

        let work = unique_tmp("idempotent");
        fs::create_dir_all(&work).unwrap();
        let target = work.join("Photo.jpg");
        let staged_dir = work.join("staging-bucket");
        fs::create_dir_all(&staged_dir).unwrap();
        let staged_file = staged_dir.join("Photo.jpg");
        let bytes = b"identical bytes 1234567890";
        fs::write(&target, bytes).unwrap();
        fs::write(&staged_file, bytes).unwrap();

        let now = now_unix();
        conn.execute(
            "INSERT INTO staging_items
                (original_path, staged_path, size_bytes, staged_at, expires_at,
                 is_dir, reason, fallback_tier)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, 'normal')",
            params![
                target.to_string_lossy(),
                staged_file.to_string_lossy(),
                bytes.len() as i64,
                now,
                now + 86400,
            ],
        )
        .unwrap();
        let id = conn.last_insert_rowid();

        let outcome = undo(id, &conn).expect("idempotent ok");
        assert!(matches!(outcome, UndoOutcome::Idempotent { .. }));
        assert!(target.exists(), "target must be preserved");
        assert!(!staged_file.exists(), "staged silinmeli");

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn undo_rename_resolution_keeps_both() {
        let mut conn = Connection::open_in_memory().unwrap();
        let migrations = rusqlite_migration::Migrations::new(vec![rusqlite_migration::M::up(
            include_str!("../db/migrations/0001_initial.sql"),
        )]);
        migrations.to_latest(&mut conn).unwrap();

        let work = unique_tmp("rename");
        fs::create_dir_all(&work).unwrap();
        let target = work.join("Notes.txt");
        let staged_dir = work.join("staging-bucket");
        fs::create_dir_all(&staged_dir).unwrap();
        let staged_file = staged_dir.join("Notes.txt");
        fs::write(&target, b"new").unwrap();
        fs::write(&staged_file, b"old").unwrap();

        let now = now_unix();
        conn.execute(
            "INSERT INTO staging_items
                (original_path, staged_path, size_bytes, staged_at, expires_at,
                 is_dir, reason, fallback_tier)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, 'normal')",
            params![
                target.to_string_lossy(),
                staged_file.to_string_lossy(),
                3i64,
                now,
                now + 86400,
            ],
        )
        .unwrap();
        let id = conn.last_insert_rowid();

        // First undo → Conflict
        let _ = undo(id, &conn).unwrap();
        // Resolve: Rename
        let outcome = undo_with_resolution(id, ConflictResolution::Rename, &conn).unwrap();
        match outcome {
            UndoOutcome::Restored { original_path } => {
                let new_path = std::path::PathBuf::from(&original_path);
                assert!(new_path.exists(), "renamed file must exist");
                assert!(new_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("(1)"));
            }
            _ => panic!("expected Restored (rename)"),
        }
        // The target is still in its original state
        assert!(target.exists());

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn list_pending_orders_by_staged_at_desc() {
        let mut conn = Connection::open_in_memory().unwrap();
        let migrations = rusqlite_migration::Migrations::new(vec![rusqlite_migration::M::up(
            include_str!("../db/migrations/0001_initial.sql"),
        )]);
        migrations.to_latest(&mut conn).unwrap();

        // Manual insert (without touching the real FS)
        let now = now_unix();
        conn.execute(
            "INSERT INTO staging_items
             (original_path, staged_path, size_bytes, staged_at, expires_at, is_dir, fallback_tier)
             VALUES ('/a', '/s/a', 10, ?1, ?2, 0, 'normal'),
                    ('/b', '/s/b', 20, ?3, ?4, 0, 'normal'),
                    ('/c', '/s/c', 30, ?5, ?6, 0, 'normal')",
            params![
                now - 100,
                now - 100 + 86400,
                now - 50,
                now - 50 + 86400,
                now,
                now + 86400,
            ],
        )
        .unwrap();

        let items = list_pending(&conn).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].original_path, "/c");
        assert_eq!(items[1].original_path, "/b");
        assert_eq!(items[2].original_path, "/a");
    }

    #[allow(dead_code)]
    fn _silence_unused_open_db_import() {
        let _ = open_db;
    }
}
