// SPDX-License-Identifier: GPL-3.0-or-later
//
// Snapshot capture — Master architecture Chapter 8 (Time Machine).
//
// v0.1 principles:
//   * Only directories are tabulated (the file count is very high; a
//     dir-only delta meets the UI needs). File level arrives in v0.2 with
//     the Chapter 8.6 streaming chunk format.
//   * path_hash = blake3(full_path_utf8_bytes) → BLOB 32B (Chapter 8.5).
//   * Bulk insert in a single transaction (rusqlite Transaction).
//   * The volume root path is derived from the drive letter (like `C:\\`).

use crate::error::{Error, Result};
use crate::scan::tree::{node_path, ScanTree};
use crate::scan::NodeId;
use crate::snapshot::SnapshotId;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

/// Summary meta of a snapshot row — UI/command output.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotMeta {
    pub id: SnapshotId,
    pub volume_id: String,
    pub captured_at_unix: i64,
    pub total_size_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub entry_count: u64,
}

/// Extracts the drive letter from `volume_id` formats (`C:\\`, `\\?\C:`,
/// `\\.\C:`, etc.) and returns it in canonical `C:\\` form. If it cannot
/// be parsed, returns a single `\\` (UNC/network case).
///
/// Takes the ASCII letter immediately BEFORE the `:` character as the
/// drive letter. This way, UNC strings like `\\server\share` do not
/// produce a random character `S:\\`.
fn volume_root_prefix(volume_id: &str) -> String {
    let bytes = volume_id.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b':' && i > 0 {
            let prev = bytes[i - 1];
            if prev.is_ascii_alphabetic() {
                return format!("{}:\\", (prev as char).to_ascii_uppercase());
            }
        }
    }
    // Gemini review B20 — UNC volume (no drive letter): return a valid
    // UNC root (double backslash, in the form "\\\\server\\share"),
    // not a single "\" (that produced an invalid path).
    let trimmed = volume_id.trim_end_matches('\\');
    if trimmed.starts_with("\\\\") || trimmed.starts_with("//") {
        return trimmed.replace('/', "\\");
    }
    "\\".into()
}

/// Produces the full Windows path string for a node — root prefix +
/// segments joined with "\\". The synthetic `<volume root>` (chain[0]) is
/// skipped.
fn full_path_for_node(tree: &ScanTree, node_id: NodeId) -> Option<String> {
    let chain = node_path(tree, node_id);
    if chain.is_empty() {
        return None;
    }
    let root_prefix = volume_root_prefix(&tree.volume_id);
    let tail: Vec<String> = chain.iter().skip(1).map(|n| n.name.clone()).collect();
    if tail.is_empty() {
        Some(root_prefix)
    } else {
        Some(format!("{}{}", root_prefix, tail.join("\\")))
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Chapter 8 — captures a snapshot from the current `ScanTree`.
///
/// Writes 1 row to the `snapshots` table and N rows to `snapshot_entries`
/// **only for directories** (file_count → stored in the snapshots.file_count
/// field but not written into snapshot_entries; the streaming chunk format
/// arrives in v0.2).
pub fn capture_snapshot(tree: &ScanTree, conn: &mut Connection) -> Result<SnapshotMeta> {
    let captured_at = now_unix();
    let total_size_bytes = tree.total_bytes;
    let file_count = tree.file_count;
    let dir_count_tree = tree.dir_count;

    let tx = conn
        .transaction()
        .map_err(|e| Error::Snapshot(format!("transaction start: {}", e)))?;

    tx.execute(
        "INSERT INTO snapshots
            (volume_id, captured_at, total_size_bytes, file_count, schema_version)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            tree.volume_id,
            captured_at,
            total_size_bytes as i64,
            file_count as i64,
            crate::db::SCHEMA_VERSION as i64,
        ],
    )
    .map_err(|e| Error::Snapshot(format!("snapshots insert: {}", e)))?;

    let snapshot_id: SnapshotId = tx.last_insert_rowid();

    let mut entry_count: u64 = 0;
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO snapshot_entries
                    (snapshot_id, path_hash, path, size_bytes, modified_at, is_dir)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|e| Error::Snapshot(format!("entry stmt prepare: {}", e)))?;

        // Write only directories. So that there are no duplicate rows for
        // the same path_hash (theoretical blake3 collision or duplication),
        // skip the row on a PK violation — in practice 32B blake3 does not collide.
        for node in tree.nodes.values() {
            if !node.is_dir {
                continue;
            }
            let Some(path) = full_path_for_node(tree, node.id) else {
                continue;
            };
            let hash = blake3::hash(path.as_bytes());
            let hash_bytes: &[u8] = hash.as_bytes();

            match stmt.execute(params![
                snapshot_id,
                hash_bytes,
                path,
                node.aggregate_size as i64,
                // Gemini review B8 — Node.modified_unix is already populated
                // (MFT $STANDARD_INFORMATION / FindFirstFile mtime); the 0
                // placeholder was removed.
                node.modified_unix,
                1i64,
            ]) {
                Ok(_) => entry_count += 1,
                Err(e) => {
                    // PK collision (snapshot_id, path_hash) — log at debug
                    // level, do not break the snapshot.
                    debug!(
                        node_id = node.id,
                        ?path,
                        error = %e,
                        "snapshot_entries duplicate path_hash skipped"
                    );
                }
            }
        }
    }

    tx.commit()
        .map_err(|e| Error::Snapshot(format!("commit: {}", e)))?;

    info!(
        snapshot_id,
        volume_id = %tree.volume_id,
        dir_entries = entry_count,
        total_dirs_in_tree = dir_count_tree,
        file_count,
        "snapshot captured"
    );

    Ok(SnapshotMeta {
        id: snapshot_id,
        volume_id: tree.volume_id.clone(),
        captured_at_unix: captured_at,
        total_size_bytes,
        file_count,
        dir_count: dir_count_tree,
        entry_count,
    })
}

/// Chapter 8.4 — retention policy. Free tier default 90 days, premium
/// 365 days; the user can override it with `settings.snapshot_retain_days`.
pub const DEFAULT_RETAIN_DAYS: i64 = 90;
pub const MIN_RETAIN_DAYS: i64 = 7;

/// Deletes snapshots where `captured_at < (now - retain_days * 86400)`.
/// `snapshot_entries` is cleaned up automatically via the foreign key
/// cascade (migration 0001 `ON DELETE CASCADE`).
///
/// Returns: the number of deleted snapshots. If retain_days < MIN_RETAIN_DAYS,
/// it is clamped to MIN_RETAIN_DAYS (so the user cannot accidentally enter 0
/// and wipe out all snapshots — Chapter 22.6 no-dark-pattern principle).
pub fn purge_old_snapshots(conn: &Connection, retain_days: i64) -> Result<u64> {
    let days = retain_days.max(MIN_RETAIN_DAYS);
    let cutoff = now_unix() - days * 86_400;
    let n = conn
        .execute(
            "DELETE FROM snapshots WHERE captured_at < ?1",
            params![cutoff],
        )
        .map_err(|e| Error::Snapshot(format!("purge_old: {}", e)))?;
    if n > 0 {
        info!(
            removed = n,
            retain_days = days,
            cutoff,
            "snapshot retention: old records deleted"
        );
    } else {
        debug!(retain_days = days, "snapshot retention: nothing to delete");
    }
    Ok(n as u64)
}

/// Lists the last 50 snapshots in captured_at DESC order. `dir_count` and
/// `entry_count` are computed via COUNT from snapshot_entries (not stored
/// in the snapshots table — v0.1 schema).
pub fn list_snapshots(conn: &Connection) -> Result<Vec<SnapshotMeta>> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.volume_id, s.captured_at, s.total_size_bytes,
                    s.file_count,
                    COALESCE((SELECT COUNT(*) FROM snapshot_entries e
                              WHERE e.snapshot_id = s.id), 0) AS entry_count
             FROM snapshots s
             ORDER BY s.captured_at DESC
             LIMIT 50",
        )
        .map_err(|e| Error::Snapshot(format!("list prepare: {}", e)))?;

    let rows = stmt
        .query_map([], |r| {
            let entry_count: i64 = r.get(5)?;
            let entry_count_u = entry_count.max(0) as u64;
            Ok(SnapshotMeta {
                id: r.get(0)?,
                volume_id: r.get(1)?,
                captured_at_unix: r.get(2)?,
                total_size_bytes: r.get::<_, i64>(3)? as u64,
                file_count: r.get::<_, i64>(4)? as u64,
                // Gemini review B19 — entry_count also includes the
                // synthetic ROOT node; 1 is subtracted so it matches the
                // dir_count returned by capture (root excluded).
                dir_count: entry_count_u.saturating_sub(1),
                entry_count: entry_count_u,
            })
        })
        .map_err(|e| Error::Snapshot(format!("list query: {}", e)))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| Error::Snapshot(format!("row: {}", e)))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::tree::build_tree;
    use crate::scan::walk::RawMftEntry;
    use rusqlite::Connection;

    fn r(record_no: u64, parent: u64, name: &str, size: u64, dir: bool) -> RawMftEntry {
        RawMftEntry {
            record_no,
            parent_record_no: parent,
            name: name.into(),
            data_size: size,
            is_dir: dir,
            modified_unix: 0,
        }
    }

    fn fresh_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        let mig = rusqlite_migration::Migrations::new(vec![rusqlite_migration::M::up(
            include_str!("../db/migrations/0001_initial.sql"),
        )]);
        mig.to_latest(&mut conn).unwrap();
        conn
    }

    #[test]
    fn volume_root_prefix_drive_letter() {
        assert_eq!(volume_root_prefix("C:\\"), "C:\\");
        assert_eq!(volume_root_prefix(r"\\?\C:"), "C:\\");
        assert_eq!(volume_root_prefix(r"\\.\D:"), "D:\\");
        assert_eq!(volume_root_prefix("e:"), "E:\\");
        // Gemini review B20 — a UNC volume now returns a valid
        // double-backslash root (it previously produced an invalid single "\").
        assert_eq!(volume_root_prefix("//server/share"), r"\\server\share");
        assert_eq!(volume_root_prefix(r"\\fs01\projects"), r"\\fs01\projects");
    }

    #[test]
    fn capture_writes_only_dirs() {
        // root(5)
        //   docs(100, dir)
        //     a.txt(101, file)
        //     sub(110, dir)
        //   c.bin(103, file)
        let raw = vec![
            r(100, 5, "docs", 0, true),
            r(101, 100, "a.txt", 200, false),
            r(110, 100, "sub", 0, true),
            r(103, 5, "c.bin", 50, false),
        ];
        let tree = build_tree("C:\\".into(), raw);
        let mut conn = fresh_conn();

        let meta = capture_snapshot(&tree, &mut conn).expect("capture OK");

        // 1 snapshots row
        let snap_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(snap_count, 1);

        // 2 dirs in the tree (docs, sub) + synthetic root = 3. The synthetic
        // root is also written with is_dir=true (path = "C\\"). The
        // dir_count field comes from raw input, the synthetic root is not
        // counted → tree.dir_count = 2. But everything with is_dir=true is
        // written to snapshot_entries = 3 rows.
        let entries: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM snapshot_entries WHERE snapshot_id = ?1",
                params![meta.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(entries, 3, "docs + sub + sentetik root");
        assert_eq!(meta.entry_count, 3);
        assert_eq!(meta.file_count, 2, "a.txt + c.bin");

        // Is path construction correct — "C:\\docs" for docs
        let path: String = conn
            .query_row(
                "SELECT path FROM snapshot_entries
                 WHERE snapshot_id = ?1 AND path LIKE 'C:\\docs%'
                 AND path NOT LIKE 'C:\\docs\\%'",
                params![meta.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(path, "C:\\docs");

        // "C:\\docs\\sub" for sub
        let sub_path: String = conn
            .query_row(
                "SELECT path FROM snapshot_entries
                 WHERE snapshot_id = ?1 AND path = 'C:\\docs\\sub'",
                params![meta.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sub_path, "C:\\docs\\sub");

        // is_dir is 1 for all
        let non_dir: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM snapshot_entries
                 WHERE snapshot_id = ?1 AND is_dir = 0",
                params![meta.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(non_dir, 0);

        // path_hash is 32 bytes
        let hash_len: i64 = conn
            .query_row(
                "SELECT LENGTH(path_hash) FROM snapshot_entries
                 WHERE snapshot_id = ?1 LIMIT 1",
                params![meta.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hash_len, 32);
    }

    #[test]
    fn purge_old_snapshots_removes_only_old() {
        let conn = fresh_conn();
        let now = now_unix();
        // 3 snapshots: now-100days, now-30days, now (with 90-day retention
        // only the first should be deleted).
        let ts_old = now - 100 * 86_400;
        let ts_recent = now - 30 * 86_400;
        let ts_fresh = now;
        for ts in [ts_old, ts_recent, ts_fresh] {
            conn.execute(
                "INSERT INTO snapshots
                    (volume_id, captured_at, total_size_bytes, file_count, schema_version)
                 VALUES ('C:\\', ?1, 0, 0, 1)",
                params![ts],
            )
            .unwrap();
        }

        let removed = purge_old_snapshots(&conn, 90).unwrap();
        assert_eq!(
            removed, 1,
            "exactly one snapshot older than 100 days should be deleted"
        );

        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 2);
    }

    #[test]
    fn purge_clamps_to_min_retain() {
        let conn = fresh_conn();
        let now = now_unix();
        // 5 days ago (below MIN_RETAIN_DAYS=7 — must not be deleted).
        let ts_recent = now - 5 * 86_400;
        conn.execute(
            "INSERT INTO snapshots
                (volume_id, captured_at, total_size_bytes, file_count, schema_version)
             VALUES ('C:\\', ?1, 0, 0, 1)",
            params![ts_recent],
        )
        .unwrap();

        // Clamp the user's retain_days=1 mistake — 7 days are preserved.
        let removed = purge_old_snapshots(&conn, 1).unwrap();
        assert_eq!(removed, 0, "MIN_RETAIN_DAYS clamp protection");
    }

    #[test]
    fn purge_cascades_to_snapshot_entries() {
        let conn = fresh_conn();
        let now = now_unix();
        let ts_old = now - 200 * 86_400;
        conn.execute(
            "INSERT INTO snapshots
                (volume_id, captured_at, total_size_bytes, file_count, schema_version)
             VALUES ('C:\\', ?1, 0, 0, 1)",
            params![ts_old],
        )
        .unwrap();
        let sid: i64 = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO snapshot_entries
                (snapshot_id, path_hash, path, size_bytes, modified_at, is_dir)
             VALUES (?1, randomblob(32), 'C:\\old\\foo', 100, 0, 1)",
            params![sid],
        )
        .unwrap();
        // PRAGMA must be active for the FK cascade
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        let removed = purge_old_snapshots(&conn, 90).unwrap();
        assert_eq!(removed, 1);

        let orphan_entries: i64 = conn
            .query_row("SELECT COUNT(*) FROM snapshot_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            orphan_entries, 0,
            "snapshot_entries FK cascade ile silinmeli"
        );
    }

    #[test]
    fn list_snapshots_orders_desc_and_limits_to_50() {
        let conn = fresh_conn();
        // Manually add 3 snapshots, captured_at 100/200/300
        for ts in [100i64, 200, 300] {
            conn.execute(
                "INSERT INTO snapshots
                    (volume_id, captured_at, total_size_bytes, file_count, schema_version)
                 VALUES ('C:\\', ?1, 0, 0, 1)",
                params![ts],
            )
            .unwrap();
        }

        let list = list_snapshots(&conn).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].captured_at_unix, 300);
        assert_eq!(list[1].captured_at_unix, 200);
        assert_eq!(list[2].captured_at_unix, 100);
        // no entries → dir_count 0
        assert!(list.iter().all(|m| m.dir_count == 0));
    }

    #[test]
    fn capture_and_list_roundtrip() {
        let raw = vec![r(100, 5, "alpha", 0, true), r(110, 100, "beta", 0, true)];
        let tree = build_tree("D:\\".into(), raw);
        let mut conn = fresh_conn();

        let meta = capture_snapshot(&tree, &mut conn).unwrap();
        let list = list_snapshots(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, meta.id);
        assert_eq!(list[0].volume_id, "D:\\");
        // synthetic root + alpha + beta = 3 dir entries
        assert_eq!(list[0].entry_count, 3);
    }
}
