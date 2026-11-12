// SPDX-License-Identifier: GPL-3.0-or-later
//
// Snapshot delta — Master architecture Chapter 8.6 (Time Machine comparison).
//
// Splits the paths changed between two snapshot IDs into 4 categories:
//   * added    — present in to, absent in from
//   * removed  — present in from, absent in to
//   * grew     — to_size > from_size
//   * shrunk   — to_size < from_size
//
// Returns the top-10 entries per category; however `total_changed_paths`
// gives the total path count across the 4 categories. The UI streaming
// delta loader (Chapter 9.6.5) uses this summary for its open → expand pattern.

use crate::error::{Error, Result};
use crate::snapshot::SnapshotId;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use tracing::{debug, info};

const TOP_N: usize = 10;

/// Min-heap wrapper keeping the top-N entries of one category. When a new
/// entry makes `len > N`, the smallest (= least interesting) item is dropped.
/// RAM stays O(N); used together with the streaming SQL JOIN instead of a
/// full HashMap (Gemini review 3.4 — prevents the 600 MB RAM blow-up on
/// 1M-entry snapshots).
struct TopN<T: Ord> {
    heap: BinaryHeap<Reverse<T>>,
    cap: usize,
}

impl<T: Ord> TopN<T> {
    fn new(cap: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(cap + 1),
            cap,
        }
    }
    fn push(&mut self, item: T) {
        self.heap.push(Reverse(item));
        if self.heap.len() > self.cap {
            self.heap.pop();
        }
    }
    fn into_sorted_desc(self) -> Vec<T> {
        let mut v: Vec<T> = self.heap.into_iter().map(|r| r.0).collect();
        v.sort_by(|a, b| b.cmp(a));
        v
    }
}

/// `PathEntry` heap ordering: first size_bytes (largest on top), if equal
/// then the path, lexically.
impl PartialOrd for PathEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PathEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.size_bytes
            .cmp(&other.size_bytes)
            .then_with(|| self.path.cmp(&other.path))
    }
}
impl PartialEq for PathEntry {
    fn eq(&self, other: &Self) -> bool {
        self.size_bytes == other.size_bytes && self.path == other.path
    }
}
impl Eq for PathEntry {}

/// `DeltaEntry` heap ordering: first the absolute |delta_bytes| (largest on top).
/// Positive for grew, negative for shrunk — the caller tells the kinds apart.
/// Ordering here is purely by absolute magnitude; later the
/// `into_sorted_desc` output is converted to the direction the caller needs.
impl PartialOrd for DeltaEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for DeltaEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.delta_bytes
            .abs()
            .cmp(&other.delta_bytes.abs())
            .then_with(|| self.path.cmp(&other.path))
    }
}
impl PartialEq for DeltaEntry {
    fn eq(&self, other: &Self) -> bool {
        self.delta_bytes == other.delta_bytes && self.path == other.path
    }
}
impl Eq for DeltaEntry {}

#[derive(Debug, Clone, Serialize)]
pub struct PathEntry {
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeltaEntry {
    pub path: String,
    pub from_size: u64,
    pub to_size: u64,
    pub delta_bytes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeltaResult {
    pub from_id: SnapshotId,
    pub to_id: SnapshotId,
    pub from_captured_at: i64,
    pub to_captured_at: i64,
    pub net_change_bytes: i64,
    pub total_changed_paths: u64,
    pub added: Vec<PathEntry>,
    pub removed: Vec<PathEntry>,
    pub grew: Vec<DeltaEntry>,
    pub shrunk: Vec<DeltaEntry>,
}

fn snapshot_captured_at(conn: &Connection, snapshot_id: SnapshotId) -> Result<i64> {
    conn.query_row(
        "SELECT captured_at FROM snapshots WHERE id = ?1",
        params![snapshot_id],
        |r| r.get(0),
    )
    .map_err(|e| Error::Snapshot(format!("snapshot id={} not found: {}", snapshot_id, e)))
}

/// Chapter 8.6 — computes the set-difference + size-difference between two
/// snapshots. Returns the top-10 entries for each category.
///
/// Gemini review 3.4 — RAM optimization: the previous impl loaded both
/// snapshots fully into HashMaps (~600 MB / 1M entries). Now rows are
/// streamed one by one via a SQL FULL OUTER JOIN + a per-category BinaryHeap
/// top-N. Constant, ~MB-level RAM.
pub fn compute_delta(
    from_id: SnapshotId,
    to_id: SnapshotId,
    conn: &Connection,
) -> Result<DeltaResult> {
    debug!(from_id, to_id, "computing delta (streaming)");

    let from_captured_at = snapshot_captured_at(conn, from_id)?;
    let to_captured_at = snapshot_captured_at(conn, to_id)?;

    // Gemini review B21 — the two snapshots must come from the same volume; a
    // cross-volume delta would produce meaningless results.
    let from_volume: String = conn
        .query_row(
            "SELECT volume_id FROM snapshots WHERE id = ?1",
            params![from_id],
            |r| r.get(0),
        )
        .map_err(|e| {
            Error::Snapshot(format!("snapshot id={} volume read failed: {}", from_id, e))
        })?;
    let to_volume: String = conn
        .query_row(
            "SELECT volume_id FROM snapshots WHERE id = ?1",
            params![to_id],
            |r| r.get(0),
        )
        .map_err(|e| Error::Snapshot(format!("snapshot id={} volume read failed: {}", to_id, e)))?;
    if from_volume != to_volume {
        return Err(Error::Snapshot(format!(
            "Delta only within the same volume: '{}' vs '{}'",
            from_volume, to_volume
        )));
    }

    // FULL OUTER JOIN: supported since SQLite 3.39+ (rusqlite 0.32 bundles ≥3.46).
    // `a` is the from snapshot, `b` the to snapshot. NULL → absent on that side.
    let mut stmt = conn
        .prepare(
            "SELECT a.path, a.size_bytes, b.path, b.size_bytes
             FROM (SELECT path_hash, path, size_bytes
                   FROM snapshot_entries WHERE snapshot_id = ?1) a
             FULL OUTER JOIN
                  (SELECT path_hash, path, size_bytes
                   FROM snapshot_entries WHERE snapshot_id = ?2) b
             ON a.path_hash = b.path_hash",
        )
        .map_err(|e| Error::Snapshot(format!("delta prepare: {}", e)))?;

    let mut added_h: TopN<PathEntry> = TopN::new(TOP_N);
    let mut removed_h: TopN<PathEntry> = TopN::new(TOP_N);
    let mut grew_h: TopN<DeltaEntry> = TopN::new(TOP_N);
    let mut shrunk_h: TopN<DeltaEntry> = TopN::new(TOP_N);

    let mut net_change: i64 = 0;
    let mut total_changed: u64 = 0;

    let mut rows = stmt
        .query(params![from_id, to_id])
        .map_err(|e| Error::Snapshot(format!("delta query: {}", e)))?;

    while let Some(row) = rows
        .next()
        .map_err(|e| Error::Snapshot(format!("delta next: {}", e)))?
    {
        let a_path: Option<String> = row.get(0).ok();
        let a_size: Option<i64> = row.get(1).ok();
        let b_path: Option<String> = row.get(2).ok();
        let b_size: Option<i64> = row.get(3).ok();

        match (a_path, b_path) {
            (None, Some(p)) => {
                // added: present only in to
                let size = b_size.unwrap_or(0).max(0) as u64;
                total_changed += 1;
                net_change = net_change.saturating_add(size as i64);
                added_h.push(PathEntry {
                    path: p,
                    size_bytes: size,
                });
            }
            (Some(p), None) => {
                // removed: present only in from
                let size = a_size.unwrap_or(0).max(0) as u64;
                total_changed += 1;
                net_change = net_change.saturating_sub(size as i64);
                removed_h.push(PathEntry {
                    path: p,
                    size_bytes: size,
                });
            }
            (Some(_a), Some(p)) => {
                let from_s = a_size.unwrap_or(0).max(0) as u64;
                let to_s = b_size.unwrap_or(0).max(0) as u64;
                let delta = to_s as i64 - from_s as i64;
                if delta > 0 {
                    total_changed += 1;
                    net_change = net_change.saturating_add(delta);
                    grew_h.push(DeltaEntry {
                        path: p,
                        from_size: from_s,
                        to_size: to_s,
                        delta_bytes: delta,
                    });
                } else if delta < 0 {
                    total_changed += 1;
                    net_change = net_change.saturating_add(delta);
                    shrunk_h.push(DeltaEntry {
                        path: p,
                        from_size: from_s,
                        to_size: to_s,
                        delta_bytes: delta,
                    });
                }
                // delta == 0 → unchanged
            }
            (None, None) => {} // impossible
        }
    }

    let added = added_h.into_sorted_desc();
    let removed = removed_h.into_sorted_desc();
    let mut grew = grew_h.into_sorted_desc();
    grew.retain(|d| d.delta_bytes > 0);
    let mut shrunk = shrunk_h.into_sorted_desc();
    shrunk.retain(|d| d.delta_bytes < 0);
    // For shrunk the most negative must come first — the heap is ordered by
    // absolute magnitude, then reverse-sorted: the output is already DESC by
    // abs; for shrunk, ASC delta (i.e. most negative first) comes naturally.
    shrunk.sort_by_key(|a| a.delta_bytes);

    let total_changed_paths = total_changed;

    info!(
        from_id,
        to_id,
        net_change_bytes = net_change,
        total_changed_paths,
        "delta ready"
    );

    Ok(DeltaResult {
        from_id,
        to_id,
        from_captured_at,
        to_captured_at,
        net_change_bytes: net_change,
        total_changed_paths,
        added,
        removed,
        grew,
        shrunk,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn fresh_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        let mig = rusqlite_migration::Migrations::new(vec![rusqlite_migration::M::up(
            include_str!("../db/migrations/0001_initial.sql"),
        )]);
        mig.to_latest(&mut conn).unwrap();
        conn
    }

    fn insert_snapshot(conn: &Connection, captured_at: i64) -> i64 {
        conn.execute(
            "INSERT INTO snapshots
                (volume_id, captured_at, total_size_bytes, file_count, schema_version)
             VALUES ('C:\\', ?1, 0, 0, 1)",
            params![captured_at],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_entry(conn: &Connection, snapshot_id: i64, path: &str, size: u64) {
        let hash = blake3::hash(path.as_bytes());
        conn.execute(
            "INSERT INTO snapshot_entries
                (snapshot_id, path_hash, path, size_bytes, modified_at, is_dir)
             VALUES (?1, ?2, ?3, ?4, 0, 1)",
            params![snapshot_id, hash.as_bytes() as &[u8], path, size as i64],
        )
        .unwrap();
    }

    #[test]
    fn delta_four_categories() {
        let conn = fresh_conn();

        // FROM snapshot (t=100)
        let from = insert_snapshot(&conn, 100);
        insert_entry(&conn, from, r"C:\common-stable", 1_000);
        insert_entry(&conn, from, r"C:\going-away", 500);
        insert_entry(&conn, from, r"C:\will-grow", 200);
        insert_entry(&conn, from, r"C:\will-shrink", 800);

        // TO snapshot (t=200)
        let to = insert_snapshot(&conn, 200);
        insert_entry(&conn, to, r"C:\common-stable", 1_000); // unchanged
        insert_entry(&conn, to, r"C:\will-grow", 700); // +500
        insert_entry(&conn, to, r"C:\will-shrink", 300); // -500
        insert_entry(&conn, to, r"C:\brand-new", 900); // added

        let d = compute_delta(from, to, &conn).expect("delta OK");

        assert_eq!(d.from_id, from);
        assert_eq!(d.to_id, to);
        assert_eq!(d.from_captured_at, 100);
        assert_eq!(d.to_captured_at, 200);

        // added: brand-new
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.added[0].path, r"C:\brand-new");
        assert_eq!(d.added[0].size_bytes, 900);

        // removed: going-away
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.removed[0].path, r"C:\going-away");
        assert_eq!(d.removed[0].size_bytes, 500);

        // grew: will-grow +500
        assert_eq!(d.grew.len(), 1);
        assert_eq!(d.grew[0].path, r"C:\will-grow");
        assert_eq!(d.grew[0].from_size, 200);
        assert_eq!(d.grew[0].to_size, 700);
        assert_eq!(d.grew[0].delta_bytes, 500);

        // shrunk: will-shrink -500
        assert_eq!(d.shrunk.len(), 1);
        assert_eq!(d.shrunk[0].path, r"C:\will-shrink");
        assert_eq!(d.shrunk[0].from_size, 800);
        assert_eq!(d.shrunk[0].to_size, 300);
        assert_eq!(d.shrunk[0].delta_bytes, -500);

        // net: +900 (added) + 500 (grew) - 500 (removed) - 500 (shrunk) = 400
        assert_eq!(d.net_change_bytes, 400);

        // 1+1+1+1 = 4 total changed paths
        assert_eq!(d.total_changed_paths, 4);
    }

    #[test]
    fn delta_top_n_ordering_and_truncation() {
        let conn = fresh_conn();

        let from = insert_snapshot(&conn, 100);
        let to = insert_snapshot(&conn, 200);

        // 15 added — only top 10 by size should remain, descending.
        for i in 0..15 {
            let path = format!("C:\\added-{:02}", i);
            // size = 1000 + i*100, so largest is i=14 → 2400.
            insert_entry(&conn, to, &path, 1000 + (i * 100) as u64);
        }

        let d = compute_delta(from, to, &conn).unwrap();
        assert_eq!(d.added.len(), 10);
        assert_eq!(d.total_changed_paths, 15);
        // The first element is the largest
        assert_eq!(d.added[0].size_bytes, 1000 + 14 * 100);
        // The ordering is DESC
        for w in d.added.windows(2) {
            assert!(w[0].size_bytes >= w[1].size_bytes);
        }
    }

    #[test]
    fn delta_shrunk_ordering_most_negative_first() {
        let conn = fresh_conn();
        let from = insert_snapshot(&conn, 100);
        let to = insert_snapshot(&conn, 200);

        insert_entry(&conn, from, r"C:\a", 1_000);
        insert_entry(&conn, to, r"C:\a", 900); // -100

        insert_entry(&conn, from, r"C:\b", 10_000);
        insert_entry(&conn, to, r"C:\b", 1_000); // -9000

        insert_entry(&conn, from, r"C:\c", 500);
        insert_entry(&conn, to, r"C:\c", 450); // -50

        let d = compute_delta(from, to, &conn).unwrap();
        assert_eq!(d.shrunk.len(), 3);
        assert_eq!(d.shrunk[0].path, r"C:\b"); // -9000 most negative
        assert_eq!(d.shrunk[1].path, r"C:\a"); // -100
        assert_eq!(d.shrunk[2].path, r"C:\c"); // -50
        assert!(d.shrunk[0].delta_bytes < d.shrunk[1].delta_bytes);
    }

    #[test]
    fn delta_empty_snapshots() {
        let conn = fresh_conn();
        let from = insert_snapshot(&conn, 100);
        let to = insert_snapshot(&conn, 200);

        let d = compute_delta(from, to, &conn).unwrap();
        assert_eq!(d.added.len(), 0);
        assert_eq!(d.removed.len(), 0);
        assert_eq!(d.grew.len(), 0);
        assert_eq!(d.shrunk.len(), 0);
        assert_eq!(d.net_change_bytes, 0);
        assert_eq!(d.total_changed_paths, 0);
    }

    #[test]
    fn delta_unknown_snapshot_id_errors() {
        let conn = fresh_conn();
        let real = insert_snapshot(&conn, 100);
        let err = compute_delta(real, 9999, &conn).unwrap_err();
        match err {
            Error::Snapshot(msg) => assert!(msg.contains("9999")),
            other => panic!("expected a Snapshot error, got: {:?}", other),
        }
    }
}
