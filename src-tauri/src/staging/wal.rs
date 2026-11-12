// SPDX-License-Identifier: GPL-3.0-or-later
//
// Write-Ahead Log (WAL) — Chapter 12.3 cross-volume two-phase commit.
//
// `staging_wal` table states:
//   BEGIN     — copy started, not yet committed
//   COMMITTED — final rename + source delete completed
//   ABORTED   — hash mismatch / crash / user cancellation
//
// Recovery (runs at application startup):
//   * BEGIN, .dspace_tmp exists   → delete tmp, source intact, mark ABORTED
//   * BEGIN, file missing         → copy interrupted, source intact, ABORTED
//   * BEGIN, hash mismatch        → delete tmp, ABORTED, notify the user
//   * COMMITTED, source missing   → inconsistent, forensic log (v0.2)

use crate::error::{Error, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// WAL: BEGIN — copy operation started. Returns the ID (for the next steps).
pub fn wal_begin(
    conn: &Connection,
    source_path: &str,
    tmp_path: &str,
    target_volume: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO staging_wal
            (source_path, tmp_path, target_volume, state, started_at)
         VALUES (?1, ?2, ?3, 'BEGIN', ?4)",
        params![source_path, tmp_path, target_volume, now_unix()],
    )
    .map_err(|e| Error::Staging(format!("WAL BEGIN: {}", e)))?;
    Ok(conn.last_insert_rowid())
}

/// WAL: COMMITTED — final rename + source delete completed.
pub fn wal_commit(conn: &Connection, wal_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE staging_wal SET state = 'COMMITTED', completed_at = ?1 \
         WHERE id = ?2",
        params![now_unix(), wal_id],
    )
    .map_err(|e| Error::Staging(format!("WAL COMMIT: {}", e)))?;
    Ok(())
}

/// WAL: ABORTED — hash mismatch / error / cancellation.
pub fn wal_abort(conn: &Connection, wal_id: i64, error_message: &str) -> Result<()> {
    conn.execute(
        "UPDATE staging_wal SET state = 'ABORTED', completed_at = ?1, \
            error_message = ?2 WHERE id = ?3",
        params![now_unix(), error_message, wal_id],
    )
    .map_err(|e| Error::Staging(format!("WAL ABORT: {}", e)))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct WalRecoveryReport {
    pub scanned: u32,
    pub aborted_begin: u32,
    pub tmp_cleaned: u32,
    pub inconsistent_committed: u32,
}

/// Called at application startup. Moves entries in the BEGIN state to
/// ABORTED, deletes leftover `.dspace_tmp` files. Implementation of the
/// Chapter 12.3 "Recovery Path — After a Crash" block.
pub fn recover_wal(conn: &Connection) -> Result<WalRecoveryReport> {
    let mut stmt = conn
        .prepare(
            "SELECT id, source_path, tmp_path FROM staging_wal \
             WHERE state = 'BEGIN'",
        )
        .map_err(|e| Error::Staging(format!("recovery prepare: {}", e)))?;

    let mut report = WalRecoveryReport {
        scanned: 0,
        aborted_begin: 0,
        tmp_cleaned: 0,
        inconsistent_committed: 0,
    };

    let rows: Vec<(i64, String, Option<String>)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| Error::Staging(format!("recovery query: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    drop(stmt);

    for (id, source, tmp_opt) in rows {
        report.scanned += 1;
        // Delete the leftover tmp file if present
        if let Some(tmp) = &tmp_opt {
            let tmp_path = Path::new(tmp);
            if tmp_path.exists() {
                match std::fs::remove_file(tmp_path) {
                    Ok(_) => {
                        report.tmp_cleaned += 1;
                        info!(tmp = %tmp, "post-crash .dspace_tmp cleaned");
                    }
                    Err(e) => {
                        warn!(tmp = %tmp, error = %e, "tmp silinemedi");
                    }
                }
            }
        }
        // Is the source intact? In two-phase commit the source is deleted last,
        // so in BEGIN the source should still be there. If it is missing, data
        // loss is possible, but we do not recover — just move the WAL to abort.
        let _ = source;
        wal_abort(conn, id, "crash recovery: BEGIN without commit")?;
        report.aborted_begin += 1;
    }

    // COMMITTED records whose file is missing — forensic log (v0.2)
    // For now, just count them.
    let mut committed_stmt = conn
        .prepare("SELECT id, tmp_path FROM staging_wal WHERE state = 'COMMITTED'")
        .map_err(|e| Error::Staging(format!("committed prepare: {}", e)))?;
    let committed_rows: Vec<(i64, Option<String>)> = committed_stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| Error::Staging(format!("committed query: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();
    drop(committed_stmt);

    for (_id, _tmp) in committed_rows {
        // v0.2: check whether COMMITTED entries actually have their staged_paths;
        // raise a forensic alarm if not. No-op for now.
    }

    if report.scanned > 0 {
        info!(
            scanned = report.scanned,
            aborted = report.aborted_begin,
            cleaned = report.tmp_cleaned,
            "WAL recovery complete"
        );
    }
    Ok(report)
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

    #[test]
    fn begin_commit_transition() {
        let conn = fresh_db();
        let id = wal_begin(&conn, "/src/a", "/tmp/a.dspace_tmp", "D:").unwrap();
        wal_commit(&conn, id).unwrap();
        let state: String = conn
            .query_row(
                "SELECT state FROM staging_wal WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "COMMITTED");
    }

    #[test]
    fn begin_abort_transition() {
        let conn = fresh_db();
        let id = wal_begin(&conn, "/src/b", "/tmp/b.dspace_tmp", "D:").unwrap();
        wal_abort(&conn, id, "hash mismatch").unwrap();
        let (state, err): (String, Option<String>) = conn
            .query_row(
                "SELECT state, error_message FROM staging_wal WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, "ABORTED");
        assert_eq!(err.as_deref(), Some("hash mismatch"));
    }

    #[test]
    fn recovery_aborts_dangling_begin() {
        let conn = fresh_db();
        // 3 entries: BEGIN, COMMITTED, ABORTED
        wal_begin(&conn, "/src/x", "/tmp/x", "E:").unwrap();
        let id2 = wal_begin(&conn, "/src/y", "/tmp/y", "E:").unwrap();
        wal_commit(&conn, id2).unwrap();
        let id3 = wal_begin(&conn, "/src/z", "/tmp/z", "E:").unwrap();
        wal_abort(&conn, id3, "test").unwrap();

        let report = recover_wal(&conn).unwrap();
        assert_eq!(report.scanned, 1);
        assert_eq!(report.aborted_begin, 1);
        // no tmp files, cleaned == 0
        assert_eq!(report.tmp_cleaned, 0);

        // Now no BEGIN remains
        let begin_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM staging_wal WHERE state = 'BEGIN'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(begin_count, 0);
    }
}
