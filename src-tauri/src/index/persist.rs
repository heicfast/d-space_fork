// SPDX-License-Identifier: GPL-3.0-or-later
//
// USN Index persist layer — Sprint 3.8.
//
// Writes/reads against the `usn_index` and `usn_watermark` tables. All DB
// operations go through a single `Connection`; the caller (a Tauri command
// or the watcher thread) holds the mutex.

use super::{IndexEntry, IndexSearchResult};
use crate::error::{Error, Result};
use rusqlite::{params, Connection};

/// A `usn_watermark` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Watermark {
    pub next_usn: i64,
    pub journal_id: i64,
}

/// Read the watermark of the given volume.
/// Gemini review B5 — the old `.ok()` chain mistook ALL DB errors (locked
/// DB, corruption, IO) for "no watermark" and triggered needless full
/// re-enumeration. Now only row-missing yields None; other errors propagate.
pub fn load_watermark(conn: &Connection, volume_id: &str) -> Result<Option<Watermark>> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT next_usn, journal_id FROM usn_watermark WHERE volume_id = ?1",
        params![volume_id],
        |r| {
            Ok(Watermark {
                next_usn: r.get(0)?,
                journal_id: r.get(1)?,
            })
        },
    )
    .optional()
    .map_err(|e| Error::Index(format!("load_watermark: {}", e)))
}

/// Watermark upsert.
pub fn save_watermark(conn: &Connection, volume_id: &str, wm: Watermark) -> Result<()> {
    conn.execute(
        "INSERT INTO usn_watermark(volume_id, next_usn, journal_id)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(volume_id) DO UPDATE SET
           next_usn = excluded.next_usn,
           journal_id = excluded.journal_id",
        params![volume_id, wm.next_usn, wm.journal_id],
    )
    .map_err(|e| Error::Index(format!("save_watermark: {}", e)))?;
    Ok(())
}

/// Bulk upsert of `IndexEntry` rows. Single transaction.
pub fn save_entries(conn: &mut Connection, entries: &[IndexEntry]) -> Result<usize> {
    if entries.is_empty() {
        return Ok(0);
    }
    let tx = conn
        .transaction()
        .map_err(|e| Error::Index(format!("transaction: {}", e)))?;
    let mut written = 0usize;
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO usn_index(volume_id, file_ref, parent_ref, name,
                                       usn_id, last_seen_unix, attrs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(volume_id, file_ref) DO UPDATE SET
                   parent_ref = excluded.parent_ref,
                   name = excluded.name,
                   usn_id = excluded.usn_id,
                   last_seen_unix = excluded.last_seen_unix,
                   attrs = excluded.attrs",
            )
            .map_err(|e| Error::Index(format!("prepare upsert: {}", e)))?;
        for e in entries {
            stmt.execute(params![
                e.volume_id,
                e.file_ref,
                e.parent_ref,
                e.name,
                e.usn_id,
                e.last_seen_unix,
                e.attrs,
            ])
            .map_err(|err| Error::Index(format!("upsert: {}", err)))?;
            written += 1;
        }
    }
    tx.commit()
        .map_err(|e| Error::Index(format!("commit: {}", e)))?;
    Ok(written)
}

/// Delete a single entry.
pub fn delete_entry(conn: &Connection, volume_id: &str, file_ref: i64) -> Result<bool> {
    let n = conn
        .execute(
            "DELETE FROM usn_index WHERE volume_id = ?1 AND file_ref = ?2",
            params![volume_id, file_ref],
        )
        .map_err(|e| Error::Index(format!("delete_entry: {}", e)))?;
    Ok(n > 0)
}

/// Delete all entries of a volume (before a full re-enumerate).
pub fn purge_volume(conn: &Connection, volume_id: &str) -> Result<usize> {
    let n1 = conn
        .execute(
            "DELETE FROM usn_index WHERE volume_id = ?1",
            params![volume_id],
        )
        .map_err(|e| Error::Index(format!("purge usn_index: {}", e)))?;
    conn.execute(
        "DELETE FROM usn_watermark WHERE volume_id = ?1",
        params![volume_id],
    )
    .map_err(|e| Error::Index(format!("purge usn_watermark: {}", e)))?;
    Ok(n1)
}

/// Read the existing entry.
pub fn load_entry(conn: &Connection, volume_id: &str, file_ref: i64) -> Result<Option<IndexEntry>> {
    let row = conn
        .query_row(
            "SELECT volume_id, file_ref, parent_ref, name, usn_id,
                    last_seen_unix, attrs
             FROM usn_index WHERE volume_id = ?1 AND file_ref = ?2",
            params![volume_id, file_ref],
            |r| {
                Ok(IndexEntry {
                    volume_id: r.get(0)?,
                    file_ref: r.get(1)?,
                    parent_ref: r.get(2)?,
                    name: r.get(3)?,
                    usn_id: r.get(4)?,
                    last_seen_unix: r.get(5)?,
                    attrs: r.get(6)?,
                })
            },
        )
        .ok();
    Ok(row)
}

/// LIKE '%query%' substring search, top-N.
pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<IndexSearchResult>> {
    // Turn the SQL escape characters (%/_) into a literal search via the ESCAPE clause.
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{}%", escaped);
    let mut stmt = conn
        .prepare(
            "SELECT volume_id, file_ref, parent_ref, name, attrs
             FROM usn_index
             WHERE name LIKE ?1 ESCAPE '\\'
             ORDER BY length(name) ASC, name ASC
             LIMIT ?2",
        )
        .map_err(|e| Error::Index(format!("prepare search: {}", e)))?;
    let rows = stmt
        .query_map(params![pattern, limit as i64], |r| {
            Ok(IndexSearchResult {
                volume_id: r.get(0)?,
                file_ref: r.get(1)?,
                parent_ref: r.get(2)?,
                name: r.get(3)?,
                full_path: None,
                attrs: r.get(4)?,
            })
        })
        .map_err(|e| Error::Index(format!("search query: {}", e)))?;
    let mut out = Vec::with_capacity(limit.min(64));
    for r in rows {
        out.push(r.map_err(|e| Error::Index(format!("search row: {}", e)))?);
    }
    Ok(out)
}

/// Build the parent chain up to the root for a file_ref. SQLite
/// `WITH RECURSIVE` is used. Walk only inside the relevant volume,
/// limited to 64 levels (against infinite loops).
///
/// Gemini review B4 — the NTFS root (file_ref 5, parent 5) used to self-join
/// and produce `C:\C:\C:\...` garbage. The `u.file_ref != c.file_ref` condition
/// on the recursive term cuts the self-join; the chain stops at the root.
pub fn resolve_full_path(conn: &Connection, volume_id: &str, file_ref: i64) -> Result<String> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE chain(file_ref, parent_ref, name, depth) AS (
                SELECT file_ref, parent_ref, name, 0
                FROM usn_index
                WHERE volume_id = ?1 AND file_ref = ?2
                UNION ALL
                SELECT u.file_ref, u.parent_ref, u.name, c.depth + 1
                FROM usn_index u
                JOIN chain c ON u.file_ref = c.parent_ref
                WHERE u.volume_id = ?1 AND c.depth < 64 AND u.file_ref != c.file_ref
            )
            SELECT name FROM chain",
        )
        .map_err(|e| Error::Index(format!("prepare full_path: {}", e)))?;
    let rows = stmt
        .query_map(params![volume_id, file_ref], |r| r.get::<_, String>(0))
        .map_err(|e| Error::Index(format!("full_path query: {}", e)))?;
    let mut parts: Vec<String> = Vec::new();
    for r in rows {
        parts.push(r.map_err(|e| Error::Index(format!("full_path row: {}", e)))?);
    }
    if parts.is_empty() {
        return Err(Error::Index(format!(
            "file_ref not found: {} (volume={})",
            file_ref, volume_id
        )));
    }
    parts.reverse();
    Ok(parts.join("\\"))
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn sample(volume: &str, fr: i64, pr: i64, name: &str, usn: i64) -> IndexEntry {
        IndexEntry {
            volume_id: volume.into(),
            file_ref: fr,
            parent_ref: pr,
            name: name.into(),
            usn_id: usn,
            last_seen_unix: 1_700_000_000 + usn,
            attrs: 0,
        }
    }

    #[test]
    fn save_and_load_roundtrip_preserves_order() {
        let mut conn = open_test_db();
        let entries: Vec<IndexEntry> = (1..=10)
            .map(|i| sample(r"\\.\C:", i, 5, &format!("file_{:02}.txt", i), i))
            .collect();
        let n = save_entries(&mut conn, &entries).unwrap();
        assert_eq!(n, 10);

        for e in &entries {
            let loaded = load_entry(&conn, &e.volume_id, e.file_ref)
                .unwrap()
                .unwrap();
            assert_eq!(&loaded, e);
        }

        // Substring via LIKE; all 10 files start with "file_"
        let r = search(&conn, "file_", 100).unwrap();
        assert_eq!(r.len(), 10);
    }

    #[test]
    fn watermark_upsert() {
        let conn = open_test_db();
        assert!(load_watermark(&conn, r"\\.\C:").unwrap().is_none());
        save_watermark(
            &conn,
            r"\\.\C:",
            Watermark {
                next_usn: 100,
                journal_id: 9001,
            },
        )
        .unwrap();
        let w = load_watermark(&conn, r"\\.\C:").unwrap().unwrap();
        assert_eq!(w.next_usn, 100);
        assert_eq!(w.journal_id, 9001);

        save_watermark(
            &conn,
            r"\\.\C:",
            Watermark {
                next_usn: 200,
                journal_id: 9001,
            },
        )
        .unwrap();
        let w2 = load_watermark(&conn, r"\\.\C:").unwrap().unwrap();
        assert_eq!(w2.next_usn, 200);
    }

    #[test]
    fn delete_and_purge() {
        let mut conn = open_test_db();
        let entries = vec![
            sample(r"\\.\C:", 1, 5, "a.txt", 1),
            sample(r"\\.\C:", 2, 5, "b.txt", 2),
            sample(r"\\.\D:", 3, 5, "c.txt", 3),
        ];
        save_entries(&mut conn, &entries).unwrap();
        assert!(delete_entry(&conn, r"\\.\C:", 1).unwrap());
        assert!(!delete_entry(&conn, r"\\.\C:", 999).unwrap());
        // C: dropped row → 1 must remain (b.txt); D: should stay
        assert_eq!(crate::index::count_entries(&conn, r"\\.\C:").unwrap(), 1);
        assert_eq!(crate::index::count_entries(&conn, r"\\.\D:").unwrap(), 1);
        let purged = purge_volume(&conn, r"\\.\C:").unwrap();
        assert_eq!(purged, 1);
        assert_eq!(crate::index::count_entries(&conn, r"\\.\C:").unwrap(), 0);
        // D: untouched
        assert_eq!(crate::index::count_entries(&conn, r"\\.\D:").unwrap(), 1);
    }

    #[test]
    fn full_path_recursive_chain() {
        let mut conn = open_test_db();
        let v = r"\\.\C:";
        // Hierarchy: 5 root → 10 Users → 20 engin → 30 report.docx
        let entries = vec![
            sample(v, 5, 5, "C:", 1), // root self-ref
            sample(v, 10, 5, "Users", 2),
            sample(v, 20, 10, "engin", 3),
            sample(v, 30, 20, "report.docx", 4),
        ];
        save_entries(&mut conn, &entries).unwrap();
        // The 30 → Users → C: chain; the root self-ref MUST be cut (Gemini review B4 —
        // it used to repeat ~61 times).
        let path = resolve_full_path(&conn, v, 30).unwrap();
        assert_eq!(path, "C:\\Users\\engin\\report.docx");
    }
}
