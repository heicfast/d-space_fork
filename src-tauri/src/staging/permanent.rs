// SPDX-License-Identifier: GPL-3.0-or-later
//
// Permanent Delete — Master architecture Chapter 12.4.
//
// Spec: "Permanent delete is a separate button now, double confirmation. For
// sensitive files, DoD 5220.22-M compliant overwrite (Phase 2)."
//
// v0.1 scope:
//   * Confirm phrase verification — the user must type the staging item's
//     **exact file name**. Not a single click, a keyboard confirmation.
//   * Forensic ledger (Chapter 12.5): into the `permanent_deletes_forensic`
//     table go original_path + size + deleted_at + blake3_first4kb +
//     double_confirm flag. For a future forensic audit (who deleted what).
//   * Standard `fs::remove_file` / `fs::remove_dir_all`. DoD wipe in v0.2.
//
// Principle (Chapter 22.6): the user always sees and confirms — no dark
// patterns. Typing the confirm phrase makes the user's choice concrete.

use crate::error::{Error, Result};
use crate::staging::ops::now_unix;
use blake3::Hasher;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

const FIRST_KB: usize = 4096;

#[derive(Debug, Clone, Serialize)]
pub struct PermanentDeleteResult {
    pub id: i64,
    pub original_path: String,
    pub staged_path: String,
    pub size_bytes: u64,
    pub deleted_at_unix: i64,
    pub blake3_first4kb_hex: Option<String>,
    pub is_dir: bool,
}

/// Blake3 hash of the first 4 KB — sufficient as forensic evidence (hashing
/// the whole file is an unacceptable cost on 100 GB files). Returns None for folders.
fn hash_first_4kb(path: &Path) -> Result<Option<[u8; 32]>> {
    if path.is_dir() {
        return Ok(None);
    }
    let file = fs::File::open(path)
        .map_err(|e| Error::Staging(format!("permanent hash open '{}': {}", path.display(), e)))?;
    let mut reader = BufReader::with_capacity(FIRST_KB, file);
    let mut buf = vec![0u8; FIRST_KB];
    let mut total = 0usize;
    while total < FIRST_KB {
        let n = reader
            .read(&mut buf[total..])
            .map_err(|e| Error::Staging(format!("permanent hash read: {}", e)))?;
        if n == 0 {
            break;
        }
        total += n;
    }
    let mut hasher = Hasher::new();
    hasher.update(&buf[..total]);
    Ok(Some(*hasher.finalize().as_bytes()))
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Chapter 12.4 — permanently deletes the staging item from disk + writes
/// a record into the forensic ledger. `confirm_phrase` must equal the staging
/// item's original file name (case-insensitive) — the second step of the double
/// confirmation (first step: the "Sil" (delete) button, second: typing the file name).
///
/// For folders, `remove_dir_all` (recursive). Hash only on single files
/// (first 4 KB).
///
/// If `secure_wipe` is true, single files (not folders) go through
/// `staging::wipe::dod_wipe_file`: 3-pass overwrite (0x00, 0xFF,
/// random) + sync + delete. Forensic recovery tools cannot bring the
/// data back. SSDs have the wear-leveling limitation (UI warning).
pub fn permanent_delete(
    id: i64,
    confirm_phrase: &str,
    secure_wipe: bool,
    conn: &mut Connection,
) -> Result<PermanentDeleteResult> {
    let (original_path, staged_path, size_bytes, is_dir): (String, String, i64, i64) = conn
        .query_row(
            "SELECT original_path, staged_path, size_bytes, is_dir
             FROM staging_items WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| Error::Staging(format!("permanent lookup id={}: {}", id, e)))?;
    let is_dir = is_dir != 0;

    let original_pb = PathBuf::from(&original_path);
    let expected = original_pb
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if expected.is_empty() {
        return Err(Error::Staging(format!(
            "cannot derive the file name from the original path: {}",
            original_path
        )));
    }
    if !confirm_phrase.eq_ignore_ascii_case(&expected) {
        warn!(
            expected = %expected,
            received_len = confirm_phrase.len(),
            "permanent delete confirmation failed (phrase mismatch)"
        );
        return Err(Error::Staging(format!(
            "Confirmation failed. You must type the full file name: '{}'",
            expected
        )));
    }

    let staged_pb = PathBuf::from(&staged_path);
    if !staged_pb.exists() {
        return Err(Error::Staging(format!(
            "Staged file not found: {}",
            staged_path
        )));
    }

    // Gemini review B16 — a hash error must not silently become None; the
    // forensic row is written with a NULL hash but it shows up in the log.
    let hash = match hash_first_4kb(&staged_pb) {
        Ok(h) => h,
        Err(e) => {
            warn!(staged = %staged_pb.display(), error = %e, "forensic hash unavailable");
            None
        }
    };
    let hash_hex = hash.as_ref().map(hex32);

    // Forensic record goes to the DB first — even if deletion fails, the original intent remains.
    let deleted_at = now_unix();
    let tx = conn
        .transaction()
        .map_err(|e| Error::Staging(format!("tx begin: {}", e)))?;

    tx.execute(
        "INSERT INTO permanent_deletes_forensic
            (original_path, size_bytes, deleted_at, blake3_first4kb, user_confirmed_twice)
         VALUES (?1, ?2, ?3, ?4, 1)",
        params![
            original_path,
            size_bytes,
            deleted_at,
            hash.as_ref().map(|h| h.as_slice()),
        ],
    )
    .map_err(|e| Error::Staging(format!("forensic insert: {}", e)))?;

    tx.execute("DELETE FROM staging_items WHERE id = ?1", params![id])
        .map_err(|e| Error::Staging(format!("staging delete: {}", e)))?;

    tx.commit()
        .map_err(|e| Error::Staging(format!("tx commit: {}", e)))?;

    // Then delete the file. Even though the DB transaction is committed, an FS
    // error does not corrupt the ledger — a trace remains for manual cleanup.
    // secure_wipe is applied only to single files; folders go via recursive remove
    // (DoD wipe is pointless for folders — wiping every file one by one is too
    // expensive, and the user already confirmed in the UI).
    let fs_result = if is_dir {
        fs::remove_dir_all(&staged_pb)
            .map_err(|e| std::io::Error::other(format!("folder recursive remove: {}", e)))
    } else if secure_wipe {
        crate::staging::wipe::dod_wipe_file(&staged_pb)
            .map_err(|e| std::io::Error::other(format!("DoD wipe: {}", e)))
    } else {
        fs::remove_file(&staged_pb)
    };
    if let Err(e) = fs_result {
        warn!(
            staged = %staged_pb.display(),
            error = %e,
            "permanent delete filesystem error — forensic ledger already written"
        );
        return Err(Error::Staging(format!(
            "File deletion failed ('{}'): {} — \
             the forensic ledger entry was written; you can clean up manually",
            staged_pb.display(),
            e
        )));
    }

    // Gemini review B32 — clean up the staging bucket directory left empty.
    if let Some(bucket) = staged_pb.parent() {
        if let Err(e) = fs::remove_dir(bucket) {
            debug!(bucket = %bucket.display(), error = %e, "bucket cleanup skipped");
        }
    }

    info!(
        id,
        original = %original_path,
        is_dir,
        size = size_bytes,
        hash_hex = hash_hex.as_deref().unwrap_or("-"),
        secure_wipe,
        "permanent delete complete"
    );

    Ok(PermanentDeleteResult {
        id,
        original_path,
        staged_path,
        size_bytes: size_bytes as u64,
        deleted_at_unix: deleted_at,
        blake3_first4kb_hex: hash_hex,
        is_dir,
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

    fn unique_tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dspace-permdel-{}-{}-{}",
            std::process::id(),
            now_unix(),
            name
        ))
    }

    fn seed_staging(conn: &Connection, original: &str, staged: &Path, size: u64) -> i64 {
        let now = now_unix();
        conn.execute(
            "INSERT INTO staging_items
                (original_path, staged_path, size_bytes, staged_at, expires_at,
                 is_dir, reason, fallback_tier)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, 'normal')",
            params![
                original,
                staged.to_string_lossy(),
                size as i64,
                now,
                now + 86400
            ],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn wrong_phrase_rejects_and_keeps_file() {
        let mut conn = fresh_db();
        let work = unique_tmp("rejects");
        fs::create_dir_all(&work).unwrap();
        let staged = work.join("staged-secret.bin");
        fs::write(&staged, b"sensitive").unwrap();
        let id = seed_staging(&conn, r"C:\Users\test\secret.bin", &staged, 9);

        let err = permanent_delete(id, "wrong-name.bin", false, &mut conn);
        assert!(err.is_err());
        // The file is still there
        assert!(staged.exists());
        // The staging_items row is preserved
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM staging_items WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn correct_phrase_deletes_and_writes_forensic() {
        let mut conn = fresh_db();
        let work = unique_tmp("deletes");
        fs::create_dir_all(&work).unwrap();
        let staged = work.join("staged-doomed.txt");
        fs::write(&staged, b"some content here").unwrap();
        let id = seed_staging(&conn, r"C:\Users\test\doomed.txt", &staged, 17);

        let result =
            permanent_delete(id, "doomed.txt", false, &mut conn).expect("phrase is correct");
        assert_eq!(result.size_bytes, 17);
        assert!(result.blake3_first4kb_hex.is_some());
        assert!(!result.is_dir);

        // No file on disk
        assert!(!staged.exists());
        // Deleted from staging_items
        let staging_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM staging_items WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(staging_count, 0);
        // The forensic record exists
        let forensic_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM permanent_deletes_forensic \
                 WHERE original_path = ?1",
                params![r"C:\Users\test\doomed.txt"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(forensic_count, 1);

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn phrase_matches_case_insensitive() {
        let mut conn = fresh_db();
        let work = unique_tmp("ci");
        fs::create_dir_all(&work).unwrap();
        let staged = work.join("staged-Report.PDF");
        fs::write(&staged, b"pdf bytes").unwrap();
        let id = seed_staging(&conn, r"C:\Users\test\Report.PDF", &staged, 9);

        // Lowercase phrase, uppercase file name — must still match
        let result = permanent_delete(id, "report.pdf", false, &mut conn);
        assert!(result.is_ok());
        assert!(!staged.exists());

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn secure_wipe_overwrites_then_deletes() {
        let mut conn = fresh_db();
        let work = unique_tmp("wipe");
        fs::create_dir_all(&work).unwrap();
        let staged = work.join("staged-secret.bin");
        let sensitive = b"super sensitive payload 0123456789".repeat(100);
        fs::write(&staged, &sensitive).unwrap();
        let id = seed_staging(
            &conn,
            r"C:\Users\test\secret.bin",
            &staged,
            sensitive.len() as u64,
        );

        let result = permanent_delete(id, "secret.bin", true, &mut conn)
            .expect("secure wipe should succeed");
        assert!(!staged.exists(), "file should be gone");
        // The forensic record was still written (hash + path)
        let forensic_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM permanent_deletes_forensic \
                 WHERE original_path = ?1",
                params![r"C:\Users\test\secret.bin"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(forensic_count, 1);
        assert_eq!(result.size_bytes, sensitive.len() as u64);

        let _ = fs::remove_dir_all(&work);
    }
}
