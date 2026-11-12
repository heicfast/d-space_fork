// SPDX-License-Identifier: GPL-3.0-or-later
//
// USN delta application — Sprint 3.8.
//
// Discovery Log #005 — Chapter 5.6: apply the `UsnRecord` list that comes
// from the `FSCTL_READ_USN_JOURNAL` output to the index. By reason mask:
//   * USN_REASON_FILE_DELETE → DELETE
//   * USN_REASON_FILE_CREATE / RENAME_NEW_NAME / DATA_OVERWRITE → UPSERT
//   * USN_REASON_RENAME_OLD_NAME → skip (the NEW_NAME-paired record completes it)
//   * USN_REASON_CLOSE → upsert (final state)
//
// Wraparound: if `detect_wraparound(stored_jid, fresh_jid)` differs,
// `Err(Error::Index("wraparound"))` → the caller does a full re-enumerate.

use super::persist::{delete_entry, save_entries};
use super::usn::{
    UsnRecord, USN_REASON_FILE_CREATE, USN_REASON_FILE_DELETE, USN_REASON_RENAME_OLD_NAME,
};
use super::{now_unix, IndexEntry};
use crate::error::{Error, Result};
use rusqlite::Connection;
use serde::Serialize;
use tracing::trace;

/// Result of `apply_delta`.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct DeltaSummary {
    pub upserts: usize,
    pub deletes: usize,
    pub skipped: usize,
}

/// If the watermark journal_id changed, it is a wraparound — the old USN
/// window is gone; a full re-enumerate is required.
pub fn detect_wraparound(stored: i64, fresh: i64) -> Result<()> {
    if stored == 0 {
        // No watermark yet — we are in the baseline phase.
        return Ok(());
    }
    if stored != fresh {
        return Err(Error::Index(format!(
            "wraparound: journal_id stored={}, fresh={}",
            stored, fresh
        )));
    }
    Ok(())
}

/// Interpret the USN reason as delete intent. If FILE_DELETE is set, delete;
/// it comes together with USN_REASON_CLOSE (finalized after close).
#[inline]
fn is_delete(reason: u32) -> bool {
    reason & USN_REASON_FILE_DELETE != 0
}

/// RENAME_OLD_NAME is companion info only — when the matching NEW_NAME
/// arrives we upsert, so we skip OLD (otherwise we would upsert under the
/// old name and later overwrite it with NEW_NAME; that would also persist
/// inconsistent intermediate states).
#[inline]
fn is_skippable(reason: u32) -> bool {
    // Skip if OLD_NAME arrives alone; also skip when it arrives with NEW_NAME
    // (the NEW_NAME upsert covers it).
    reason & USN_REASON_RENAME_OLD_NAME != 0
        && reason & USN_REASON_FILE_CREATE == 0
        && !is_delete(reason)
}

/// Apply the given records to the DB. `volume_id` comes from the caller
/// (the volume of the record is already known from the Win32 volume handle).
pub fn apply_delta(
    conn: &mut Connection,
    volume_id: &str,
    records: &[UsnRecord],
) -> Result<DeltaSummary> {
    let mut summary = DeltaSummary::default();
    if records.is_empty() {
        return Ok(summary);
    }
    let ts = now_unix();
    let mut upsert_batch: Vec<IndexEntry> = Vec::new();
    let mut to_delete: Vec<i64> = Vec::new();

    for rec in records {
        if is_delete(rec.reason) {
            to_delete.push(rec.file_ref);
            continue;
        }
        if is_skippable(rec.reason) {
            summary.skipped += 1;
            trace!(
                file_ref = rec.file_ref,
                reason = rec.reason,
                "RENAME_OLD_NAME skip"
            );
            continue;
        }
        upsert_batch.push(IndexEntry {
            volume_id: volume_id.to_string(),
            file_ref: rec.file_ref,
            parent_ref: rec.parent_ref,
            name: rec.name.clone(),
            usn_id: rec.usn_id,
            last_seen_unix: ts,
            attrs: rec.attributes as i64,
        });
    }

    if !upsert_batch.is_empty() {
        summary.upserts = save_entries(conn, &upsert_batch)?;
    }
    for fr in to_delete {
        if delete_entry(conn, volume_id, fr)? {
            summary.deletes += 1;
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::persist::{load_entry, save_entries, search};
    use crate::index::usn::{USN_REASON_CLOSE, USN_REASON_RENAME_NEW_NAME};
    use crate::index::IndexEntry;
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

    fn rec(file_ref: i64, parent: i64, usn: i64, reason: u32, name: &str) -> UsnRecord {
        UsnRecord {
            file_ref,
            parent_ref: parent,
            usn_id: usn,
            reason,
            attributes: 0,
            name: name.into(),
        }
    }

    #[test]
    fn detect_wraparound_no_watermark_passes() {
        detect_wraparound(0, 12345).unwrap();
    }

    #[test]
    fn detect_wraparound_same_id_passes() {
        detect_wraparound(12345, 12345).unwrap();
    }

    #[test]
    fn detect_wraparound_different_id_errors() {
        let err = detect_wraparound(12345, 67890).unwrap_err();
        match err {
            Error::Index(m) => assert!(m.contains("wraparound")),
            _ => panic!("expected Index variant"),
        }
    }

    #[test]
    fn apply_delta_creates_and_overwrites() {
        let mut conn = open_test_db();
        let v = r"\\.\C:";
        let records = vec![
            rec(
                10,
                5,
                100,
                USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
                "a.txt",
            ),
            rec(
                11,
                5,
                101,
                USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
                "b.txt",
            ),
        ];
        let s = apply_delta(&mut conn, v, &records).unwrap();
        assert_eq!(s.upserts, 2);
        assert_eq!(s.deletes, 0);
        let a = load_entry(&conn, v, 10).unwrap().unwrap();
        assert_eq!(a.name, "a.txt");
        assert_eq!(a.parent_ref, 5);

        // Overwrite for the same file_ref — rename simulation
        let upd = vec![rec(10, 5, 102, USN_REASON_RENAME_NEW_NAME, "a_renamed.txt")];
        apply_delta(&mut conn, v, &upd).unwrap();
        let a2 = load_entry(&conn, v, 10).unwrap().unwrap();
        assert_eq!(a2.name, "a_renamed.txt");
        assert_eq!(a2.usn_id, 102);
    }

    #[test]
    fn apply_delta_deletes_existing() {
        let mut conn = open_test_db();
        let v = r"\\.\C:";
        // Baseline first
        save_entries(
            &mut conn,
            &[IndexEntry {
                volume_id: v.into(),
                file_ref: 7,
                parent_ref: 5,
                name: "obsolete.tmp".into(),
                usn_id: 1,
                last_seen_unix: 1,
                attrs: 0,
            }],
        )
        .unwrap();
        let s = apply_delta(
            &mut conn,
            v,
            &[rec(
                7,
                5,
                99,
                USN_REASON_FILE_DELETE | USN_REASON_CLOSE,
                "obsolete.tmp",
            )],
        )
        .unwrap();
        assert_eq!(s.deletes, 1);
        assert_eq!(s.upserts, 0);
        assert!(load_entry(&conn, v, 7).unwrap().is_none());
    }

    #[test]
    fn apply_delta_skips_old_name_only() {
        let mut conn = open_test_db();
        let v = r"\\.\C:";
        // OLD_NAME only — skip until the new name arrives
        let records = vec![rec(20, 5, 5, USN_REASON_RENAME_OLD_NAME, "old.txt")];
        let s = apply_delta(&mut conn, v, &records).unwrap();
        assert_eq!(s.skipped, 1);
        assert_eq!(s.upserts, 0);
        assert!(load_entry(&conn, v, 20).unwrap().is_none());
    }

    #[test]
    fn apply_delta_search_visibility_after_create() {
        let mut conn = open_test_db();
        let v = r"\\.\C:";
        apply_delta(
            &mut conn,
            v,
            &[rec(
                42,
                5,
                10,
                USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
                "needle_in_haystack.txt",
            )],
        )
        .unwrap();
        let r = search(&conn, "needle", 10).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "needle_in_haystack.txt");
    }

    #[test]
    fn apply_delta_empty_records_no_op() {
        let mut conn = open_test_db();
        let s = apply_delta(&mut conn, r"\\.\C:", &[]).unwrap();
        assert_eq!(s.upserts, 0);
        assert_eq!(s.deletes, 0);
        assert_eq!(s.skipped, 0);
    }
}
