// SPDX-License-Identifier: GPL-3.0-or-later
//
// USN Journal Index — Sprint 3.8 / Discovery Log #005 (Chapter 5.6).
//
// Persistent file index on top of the NTFS USN Journal (Update Sequence
// Number). "Everything"-style instant substring search; < 200 ms load at
// startup + incremental delta sync with a 5 s background poll. The whole
// index lives in the `usn_index` table inside the existing `dspace.sqlite`.
//
// Data flow:
//   1) `IndexBuilder::baseline_enumerate` → walks all MFT entries via
//      FSCTL_ENUM_USN_DATA, writes them to the DB with `apply_baseline` +
//      records the first watermark (next_usn + journal_id) in `usn_watermark`.
//   2) `IndexBuilder::poll_delta` → reads the USN records after the last
//      watermark via FSCTL_READ_USN_JOURNAL, `delta::apply_delta`
//      applies upsert/delete, updates the watermark.
//   3) Wraparound: if journal_id changed, the caller gets `Error::Index`,
//      `baseline_enumerate` is triggered again (the old USN window is invalid).
//
// Fast search: `index_search(query, limit)` does top-N over SQLite
// `LIKE '%query%'`. v0.1 matches names only, full_path is optional —
// the parent chain is resolved with `WITH RECURSIVE` (enough for top-50).

pub mod baseline;
pub mod delta;
pub mod persist;
pub mod usn;
pub mod watcher;

use crate::error::{Error, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

pub use baseline::{
    apply_baseline_batch, enumerate_volume_baseline, BaselineSummary, DEFAULT_BASELINE_BUFFER,
};
pub use delta::{apply_delta, detect_wraparound, DeltaSummary};
pub use persist::{
    load_watermark, save_entries, save_watermark, search as persist_search, Watermark,
};
pub use usn::{
    parse_record_v2, read_journal_blocking, JournalReadResult, UsnRecord, USN_REASON_MASK,
};
pub use watcher::{spawn_watcher, WatcherHandle};

/// A single USN index entry. `volume_id` e.g. `\\.\C:`, `file_ref`/`parent_ref`
/// is the MFT segment number (low 48 bits) — without the sequence number
/// (typical NTFS USN usage: only the segment part is persistent).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexEntry {
    pub volume_id: String,
    pub file_ref: i64,
    pub parent_ref: i64,
    pub name: String,
    pub usn_id: i64,
    pub last_seen_unix: i64,
    /// NTFS file attribute flags (FILE_ATTRIBUTE_DIRECTORY etc.).
    pub attrs: i64,
}

/// `index_search` result. `full_path` is optional — in v0.1 the recursive
/// path chain (SQLite `WITH RECURSIVE`) is resolved only for the first top-50.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexSearchResult {
    pub volume_id: String,
    pub file_ref: i64,
    pub parent_ref: i64,
    pub name: String,
    pub full_path: Option<String>,
    pub attrs: i64,
}

/// `index_status` result. For the UI status badge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexStatus {
    pub volume_id: Option<String>,
    pub total_entries: i64,
    pub last_sync_unix: i64,
    /// "ready" | "building" | "idle" | "needs_admin" | "error"
    pub mode: String,
}

/// High-level index builder — encapsulates the baseline + delta
/// operations. The watcher thread uses this structure.
pub struct IndexBuilder {
    pub volume_id: String,
}

impl IndexBuilder {
    pub fn new(volume_id: impl Into<String>) -> Self {
        Self {
            volume_id: volume_id.into(),
        }
    }

    /// Reads the current watermark, if any.
    pub fn watermark(&self, conn: &Connection) -> Result<Option<Watermark>> {
        load_watermark(conn, &self.volume_id)
    }
}

/// Gemini review B9 / v0.3.1 prod wire — the list of IDs of the volumes
/// registered in the `usn_watermark` table (in `\\.\C:` form). Used to
/// spawn the watchers at startup.
pub fn list_indexed_volumes(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT volume_id FROM usn_watermark ORDER BY volume_id")
        .map_err(|e| Error::Index(format!("list_indexed_volumes: {}", e)))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| Error::Index(format!("list_indexed_volumes query: {}", e)))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| Error::Index(format!("list_indexed_volumes row: {}", e)))?);
    }
    Ok(out)
}

/// `sync_volume` result — whether an incremental delta or a full baseline was run.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SyncOutcome {
    /// There was no watermark / wraparound — full `FSCTL_ENUM_USN_DATA` rebuild.
    Baseline(Box<BaselineSummary>),
    /// The watermark was valid — only a `FSCTL_READ_USN_JOURNAL` delta was applied.
    Delta {
        volume_id: String,
        #[serde(flatten)]
        summary: DeltaSummary,
        next_usn: i64,
    },
}

/// Update the volume's USN index — incremental delta if a watermark exists,
/// otherwise (or on wraparound) a full baseline. The shared core of the
/// `index_build` command and the watcher tick (Gemini review: watermark
/// short-circuit instead of a full rebuild on every startup).
pub fn sync_volume(
    volume_id: &str,
    conn: &mut Connection,
    buffer_size: usize,
) -> Result<SyncOutcome> {
    let fresh = baseline::query_journal(volume_id)?;
    let stored = load_watermark(conn, volume_id)?;

    match stored {
        Some(wm) => {
            // Wraparound check — if journal_id changed, full rebuild.
            if let Err(e) = detect_wraparound(wm.journal_id, fresh.journal_id) {
                info!(volume = volume_id, ?e, "wraparound — full rebuild");
                let summary = enumerate_volume_baseline(volume_id, conn, buffer_size)?;
                return Ok(SyncOutcome::Baseline(Box::new(summary)));
            }
            // Incremental delta: read without waiting, starting at the watermark.
            let read = usn::read_journal_blocking(
                volume_id,
                wm.next_usn,
                fresh.journal_id,
                0, // no-wait: collect accumulated records, no waiting
                buffer_size,
            )?;
            let summary = apply_delta(conn, volume_id, &read.records)?;
            let next_usn = if read.next_usn > wm.next_usn {
                read.next_usn
            } else {
                wm.next_usn
            };
            save_watermark(
                conn,
                volume_id,
                Watermark {
                    next_usn,
                    journal_id: fresh.journal_id,
                },
            )?;
            Ok(SyncOutcome::Delta {
                volume_id: volume_id.to_string(),
                summary,
                next_usn,
            })
        }
        None => {
            // No baseline yet — full enumerate.
            let summary = enumerate_volume_baseline(volume_id, conn, buffer_size)?;
            Ok(SyncOutcome::Baseline(Box::new(summary)))
        }
    }
}

/// Total row count of `usn_index`. For the UI badge.
pub fn count_entries(conn: &Connection, volume_id: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM usn_index WHERE volume_id = ?1",
        params![volume_id],
        |r| r.get(0),
    )
    .map_err(|e| Error::Index(format!("count_entries: {}", e)))
}

/// Last sync time. If `volume_id` is given, only that volume; otherwise
/// the global MAX. Returns 0 for an empty index. (Gemini review B24 — a
/// per-volume query must not report another volume's freshness.)
pub fn last_sync_unix(conn: &Connection, volume_id: Option<&str>) -> Result<i64> {
    let ts: Option<i64> = match volume_id {
        Some(v) => conn
            .query_row(
                "SELECT MAX(last_seen_unix) FROM usn_index WHERE volume_id = ?1",
                params![v],
                |r| r.get(0),
            )
            .map_err(|e| Error::Index(format!("last_sync_unix: {}", e)))?,
        None => conn
            .query_row("SELECT MAX(last_seen_unix) FROM usn_index", [], |r| {
                r.get(0)
            })
            .map_err(|e| Error::Index(format!("last_sync_unix: {}", e)))?,
    };
    Ok(ts.unwrap_or(0))
}

/// Status query for Tauri commands. The volume parameter is optional:
/// if not given, the "global" count is returned (volume_id = None).
pub fn index_status(conn: &Connection, volume_id: Option<&str>) -> Result<IndexStatus> {
    let total = match volume_id {
        Some(v) => count_entries(conn, v)?,
        None => conn
            .query_row("SELECT COUNT(*) FROM usn_index", [], |r| r.get::<_, i64>(0))
            .map_err(|e| Error::Index(format!("total count: {}", e)))?,
    };
    let last_sync = last_sync_unix(conn, volume_id)?;
    let mode = if total == 0 { "idle" } else { "ready" };
    Ok(IndexStatus {
        volume_id: volume_id.map(|s| s.to_string()),
        total_entries: total,
        last_sync_unix: last_sync,
        mode: mode.to_string(),
    })
}

/// Substring search. If `query` is empty, `Ok(vec![])`. `limit` is the
/// upper bound. Discovery Log #005 — Chapter 5.6: v0.1 matches only via
/// name LIKE, full_path is resolved optionally (only for returned results).
pub fn index_search(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<IndexSearchResult>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }
    let cap = limit.clamp(1, 500);
    debug!(query = trimmed, limit = cap, "index_search called");
    let mut results = persist_search(conn, trimmed, cap)?;
    // Recursive path chain for the top-N. On error, stay silent —
    // full_path is only a convenience; its absence is not a problem.
    for r in results.iter_mut() {
        if let Ok(p) = persist::resolve_full_path(conn, &r.volume_id, r.file_ref) {
            r.full_path = Some(p);
        }
    }
    Ok(results)
}

/// The current unix epoch (seconds). A separate function for testability.
pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self};
    use rusqlite::Connection;

    fn open_test_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        rusqlite_migration::Migrations::new(vec![
            rusqlite_migration::M::up(include_str!("../db/migrations/0001_initial.sql")),
            rusqlite_migration::M::up(include_str!("../db/migrations/0002_user_rules.sql")),
            rusqlite_migration::M::up(include_str!("../db/migrations/0003_usn_index.sql")),
        ])
        .to_latest(&mut conn)
        .unwrap();
        conn
    }

    #[test]
    fn empty_query_returns_empty_vec() {
        let conn = open_test_db();
        let r = index_search(&conn, "", 10).unwrap();
        assert!(r.is_empty());
        let r2 = index_search(&conn, "   ", 10).unwrap();
        assert!(r2.is_empty());
    }

    #[test]
    fn search_substring_matches_and_respects_limit() {
        let mut conn = open_test_db();
        let now = 1_700_000_000_i64;
        let rows = vec![
            IndexEntry {
                volume_id: r"\\.\C:".into(),
                file_ref: 1,
                parent_ref: 5,
                name: "Cargo.toml".into(),
                usn_id: 1,
                last_seen_unix: now,
                attrs: 0,
            },
            IndexEntry {
                volume_id: r"\\.\C:".into(),
                file_ref: 2,
                parent_ref: 5,
                name: "Cargo.lock".into(),
                usn_id: 2,
                last_seen_unix: now,
                attrs: 0,
            },
            IndexEntry {
                volume_id: r"\\.\C:".into(),
                file_ref: 3,
                parent_ref: 5,
                name: "readme.md".into(),
                usn_id: 3,
                last_seen_unix: now,
                attrs: 0,
            },
        ];
        save_entries(&mut conn, &rows).unwrap();

        let r = index_search(&conn, "Cargo", 10).unwrap();
        assert_eq!(r.len(), 2);
        let names: Vec<_> = r.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Cargo.toml"));
        assert!(names.contains(&"Cargo.lock"));

        let r_limit = index_search(&conn, "C", 1).unwrap();
        assert_eq!(r_limit.len(), 1);
    }

    #[test]
    fn status_reports_count_and_mode() {
        let mut conn = open_test_db();
        let st0 = index_status(&conn, Some(r"\\.\C:")).unwrap();
        assert_eq!(st0.total_entries, 0);
        assert_eq!(st0.mode, "idle");
        save_entries(
            &mut conn,
            &[IndexEntry {
                volume_id: r"\\.\C:".into(),
                file_ref: 10,
                parent_ref: 5,
                name: "test.txt".into(),
                usn_id: 1,
                last_seen_unix: 42,
                attrs: 0,
            }],
        )
        .unwrap();
        let st1 = index_status(&conn, Some(r"\\.\C:")).unwrap();
        assert_eq!(st1.total_entries, 1);
        assert_eq!(st1.mode, "ready");
        assert_eq!(st1.last_sync_unix, 42);
    }

    // Sentinel for db module access — alias in the migration chain.
    #[allow(dead_code)]
    fn _db_alias() {
        let _ = db::SCHEMA_VERSION;
    }
}
