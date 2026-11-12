// SPDX-License-Identifier: GPL-3.0-or-later
//
// Cross-Volume Staging — Two-Phase Commit
// Master architecture Chapter 12.3 Layer C (v1.4 fix).
//
// Failure scenario: a cross-volume copy takes 2-5 min and is not atomic.
// If a power loss / app crash hits at the 40% mark, the target has a
// half-written corrupt file + the source is still there + tracking is broken.
// The AI review in v1.4 flagged this race condition.
//
// Solution: write-ahead pattern.
//   Phase 1 (write):  .dspace_tmp + WAL BEGIN + Blake3 hash verify
//   Phase 2 (commit): atomic rename → final name, DB insert, source delete,
//                     WAL COMMITTED

use crate::error::{Error, Result};
use crate::staging::ops::StagedItem;
use crate::staging::wal::{wal_abort, wal_begin, wal_commit};
use blake3::Hasher;
use rusqlite::{params, Connection};
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info, warn};

const HASH_BUFFER: usize = 64 * 1024;

/// The file's Blake3 hash. Streaming, constant RAM.
pub fn blake3_file(path: &Path) -> Result<[u8; 32]> {
    let file = fs::File::open(path)
        .map_err(|e| Error::Staging(format!("hash open '{}': {}", path.display(), e)))?;
    let mut reader = BufReader::with_capacity(HASH_BUFFER, file);
    let mut hasher = Hasher::new();
    let mut buf = vec![0u8; HASH_BUFFER];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| Error::Staging(format!("hash read: {}", e)))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Streaming copy. Overwrites the destination. No progress callback for
/// now (event stream to the UI in v0.2). Gemini review B1 — `pub(crate)`: the
/// undo cross-volume move-back also uses this same copy helper.
pub(crate) fn copy_file_streaming(src: &Path, dst: &Path) -> Result<u64> {
    let mut s = fs::File::open(src).map_err(|e| Error::Staging(format!("copy src open: {}", e)))?;
    let mut d =
        fs::File::create(dst).map_err(|e| Error::Staging(format!("copy dst create: {}", e)))?;
    std::io::copy(&mut s, &mut d).map_err(|e| Error::Staging(format!("copy: {}", e)))
}

/// Cross-volume two-phase commit for a single file. `final_path` must be
/// in the same directory (for the atomic rename guarantee).
///
/// `target_volume` is informational in the WAL record (e.g. "D:").
pub fn cross_volume_stage_file(
    src: &Path,
    final_path: &Path,
    target_volume: &str,
    size_bytes: u64,
    is_dir: bool,
    conn: &Connection,
) -> Result<StagedItem> {
    let started = Instant::now();
    if is_dir {
        // Chapter 12.3 is defined in terms of files. For folders: recursive
        // walk + per-file two-phase + remove the folder at the end (v0.2). For
        // now, cross-volume is rejected for folders — explicit message to the user.
        return Err(Error::Staging(
            "Cross-volume folder staging arrives in v0.2 (per-file recursive two-phase). \
             For now, send the folder to staging on the same volume."
                .into(),
        ));
    }

    let parent = final_path
        .parent()
        .ok_or_else(|| Error::Staging("final_path has no parent".into()))?;
    let file_name = final_path
        .file_name()
        .ok_or_else(|| Error::Staging("final_path has no filename".into()))?
        .to_string_lossy()
        .to_string();
    let tmp_path: PathBuf = parent.join(format!("{}.dspace_tmp", file_name));

    fs::create_dir_all(parent).map_err(|e| Error::Staging(format!("mkdir parent: {}", e)))?;

    let wal_id = wal_begin(
        conn,
        &src.to_string_lossy(),
        &tmp_path.to_string_lossy(),
        target_volume,
    )?;
    debug!(wal_id, "two-phase commit phase 1 starting");

    // Phase 1a: copy
    if let Err(e) = copy_file_streaming(src, &tmp_path) {
        let _ = fs::remove_file(&tmp_path);
        wal_abort(conn, wal_id, &format!("copy: {}", e))?;
        return Err(e);
    }

    // Phase 1b: hash verify
    let src_hash = match blake3_file(src) {
        Ok(h) => h,
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            wal_abort(conn, wal_id, &format!("src hash: {}", e))?;
            return Err(e);
        }
    };
    let dst_hash = match blake3_file(&tmp_path) {
        Ok(h) => h,
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            wal_abort(conn, wal_id, &format!("dst hash: {}", e))?;
            return Err(e);
        }
    };
    if src_hash != dst_hash {
        let _ = fs::remove_file(&tmp_path);
        wal_abort(conn, wal_id, "hash mismatch (Blake3)")?;
        warn!(
            src = %src.display(),
            "cross-volume copy hash mismatch — tmp silindi"
        );
        return Err(Error::Staging(
            "Hash uyuşmazlığı (Blake3) — hedef diskte bozulma olabilir. \
             Operasyon iptal edildi, kaynak intact."
                .into(),
        ));
    }

    debug!(wal_id, "phase 1 done, phase 2 starting");

    // Phase 2a: atomic rename .dspace_tmp → final
    if let Err(e) = fs::rename(&tmp_path, final_path) {
        let _ = fs::remove_file(&tmp_path);
        wal_abort(conn, wal_id, &format!("final rename: {}", e))?;
        return Err(Error::Staging(format!("final rename: {}", e)));
    }

    let staged_at = crate::staging::ops::now_unix();
    let expires_at = staged_at + crate::staging::ops::STAGING_TTL_SECS;

    // Phase 2b: staging_items record
    if let Err(e) = conn.execute(
        "INSERT INTO staging_items
            (original_path, staged_path, size_bytes, staged_at, expires_at,
             is_dir, reason, fallback_tier)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 'cross_volume')",
        params![
            src.to_string_lossy(),
            final_path.to_string_lossy(),
            size_bytes as i64,
            staged_at,
            expires_at,
            is_dir as i64,
        ],
    ) {
        // Gemini review B13 — the final file was committed but the DB insert failed:
        // instead of leaving the WAL row in BEGIN, ABORT it explicitly (so the
        // next startup recovery does not misread it); the final file stays in
        // place (no data loss), and the error is thrown.
        let msg = format!("DB insert (final var ama tracking yok): {}", e);
        warn!(?msg);
        let _ = wal_abort(conn, wal_id, &msg);
        return Err(Error::Staging(msg));
    }
    let staging_id = conn.last_insert_rowid();

    // Phase 2c: delete the source
    if let Err(e) = fs::remove_file(src) {
        // The source could not be deleted but the target was written + the DB row
        // exists. The user owns both copies. A conflict, but NO data loss.
        warn!(?e, "source could not be deleted — user keeps both copies");
        return Err(Error::Staging(format!(
            "source deletion failed: {} (target intact, undo possible)",
            e
        )));
    }

    // Phase 2d: WAL COMMITTED
    wal_commit(conn, wal_id)?;

    info!(
        wal_id,
        staging_id,
        src = %src.display(),
        dst = %final_path.display(),
        size = size_bytes,
        elapsed_ms = started.elapsed().as_millis(),
        "cross-volume two-phase commit complete"
    );

    Ok(StagedItem {
        id: staging_id,
        original_path: src.to_string_lossy().to_string(),
        staged_path: final_path.to_string_lossy().to_string(),
        size_bytes,
        staged_at_unix: staged_at,
        expires_at_unix: expires_at,
        is_dir,
        fallback_tier: "cross_volume".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        let migrations = rusqlite_migration::Migrations::new(vec![rusqlite_migration::M::up(
            include_str!("../db/migrations/0001_initial.sql"),
        )]);
        migrations.to_latest(&mut conn).unwrap();
        conn
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "dspace-2pc-{}-{}-{}",
            std::process::id(),
            crate::staging::ops::now_unix(),
            tag
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn blake3_file_deterministic() {
        let dir = unique_dir("hash");
        let p = dir.join("data.bin");
        fs::write(&p, b"D-Space is awesome").unwrap();
        let h1 = blake3_file(&p).unwrap();
        let h2 = blake3_file(&p).unwrap();
        assert_eq!(h1, h2);
        // If the first byte changes, the hash must change too
        fs::write(&p, b"E-Space is awesome").unwrap();
        let h3 = blake3_file(&p).unwrap();
        assert_ne!(h1, h3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_phase_commit_happy_path() {
        // We use same-volume, but the function still runs the two-phase
        // protocol — the semantics are the same.
        let src_dir = unique_dir("src");
        let dst_dir = unique_dir("dst");
        let src = src_dir.join("important.txt");
        fs::write(&src, b"valuable cargo").unwrap();
        let size = fs::metadata(&src).unwrap().len();

        let final_path = dst_dir.join("important.txt");
        let conn = fresh_db();

        let staged = cross_volume_stage_file(&src, &final_path, "TEST:", size, false, &conn)
            .expect("two-phase commit should succeed");

        assert!(!src.exists(), "source should be gone");
        assert!(final_path.exists(), "target should exist");
        assert_eq!(staged.size_bytes, size);
        assert_eq!(staged.fallback_tier, "cross_volume");

        // WAL state COMMITTED
        let state: String = conn
            .query_row(
                "SELECT state FROM staging_wal WHERE source_path = ?1",
                params![src.to_string_lossy()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "COMMITTED");

        // No .dspace_tmp left lying around
        let tmp_glob = dst_dir.join("important.txt.dspace_tmp");
        assert!(!tmp_glob.exists());

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }
}
