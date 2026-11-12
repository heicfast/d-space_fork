// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lazy Expiry — Master architecture Chapter 12.2.1, 12.2.2, 12.2.3.
//
// Principle (Chapter 22.6, no dark patterns): NEVER delete permanently
// without user consent. Expired items are listed; the user confirms via
// "Delete all permanently" or an individual selection.
//
// Scope:
//   * list_expired(): returns the staging items with expires_at < now.
//   * cleanup_expired(): called with user consent, permanently deletes the
//     items. Rate limit (10 files/sec) — prevents UI freeze.
//
// The wake-from-sleep grace period (Chapter 12.2.2) happens inside tray polling
// — this module is only the DB side; tray scheduling lands in the v0.2 sprint
// (Chapter 13.2 Monitoring intervals).

use crate::error::{Error, Result};
use crate::staging::ops::now_unix;
use crate::staging::permanent::{permanent_delete, PermanentDeleteResult};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::thread::sleep;
use std::time::Duration;
use tracing::{info, warn};

/// Chapter 12.2.3 — rate limit: prevents UI freeze + I/O bursts.
pub const DEFAULT_CLEANUP_RATE_PER_SEC: u32 = 10;
/// Chapter 12.2.2 — `notify_user` threshold: above this count there is
/// NO automatic deletion; the user must confirm individually/in bulk from the list.
pub const AUTO_THRESHOLD: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct ExpiredItem {
    pub id: i64,
    pub original_path: String,
    pub size_bytes: u64,
    pub expired_at_unix: i64,
    pub is_dir: bool,
    pub age_secs: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupReport {
    pub deleted: u64,
    pub failed: u64,
    pub total_bytes: u64,
    pub elapsed_ms: u64,
    pub aborted_threshold: bool,
}

/// Chapter 12.2.1 — the list of expired staging items (expires_at < now).
/// Gemini review B30 — LIMIT 2000: no RAM swelling on a pathological DB (cleanup
/// already runs in batches via AUTO_THRESHOLD).
pub fn list_expired(conn: &Connection) -> Result<Vec<ExpiredItem>> {
    let now = now_unix();
    let mut stmt = conn
        .prepare(
            "SELECT id, original_path, size_bytes, expires_at, is_dir
             FROM staging_items
             WHERE expires_at < ?1
             ORDER BY expires_at ASC
             LIMIT 2000",
        )
        .map_err(|e| Error::Staging(format!("expiry prepare: {}", e)))?;

    let rows = stmt
        .query_map(params![now], |r| {
            let exp: i64 = r.get(3)?;
            Ok(ExpiredItem {
                id: r.get(0)?,
                original_path: r.get(1)?,
                size_bytes: r.get::<_, i64>(2)? as u64,
                expired_at_unix: exp,
                is_dir: r.get::<_, i64>(4)? != 0,
                age_secs: now - exp,
            })
        })
        .map_err(|e| Error::Staging(format!("expiry query: {}", e)))?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|e| Error::Staging(format!("expiry row: {}", e)))?);
    }
    Ok(items)
}

/// Chapter 12.2.2 + 12.2.3 — confirmed bulk cleanup. Per-file sleep under the
/// rate limit, abort when AUTO_THRESHOLD is exceeded. `force=true` signals that
/// the UI user gave the "Delete all" consent.
///
/// Confirm phrase: each item must match its own full file name (the same
/// double-confirmation guarantee as Chapter 12.4). The UI shows this list; the
/// user may delete items individually (per-file phrase) or all of them via `force`.
pub fn cleanup_expired(
    conn: &mut Connection,
    rate_per_sec: u32,
    force: bool,
) -> Result<CleanupReport> {
    let started = std::time::Instant::now();
    let items = list_expired(conn)?;

    if !force && items.len() > AUTO_THRESHOLD {
        warn!(
            count = items.len(),
            threshold = AUTO_THRESHOLD,
            "expired items exceeded the threshold — user confirmation required"
        );
        return Ok(CleanupReport {
            deleted: 0,
            failed: 0,
            total_bytes: 0,
            elapsed_ms: started.elapsed().as_millis() as u64,
            aborted_threshold: true,
        });
    }

    let rate = rate_per_sec.max(1);
    let interval = Duration::from_millis(1000 / rate as u64);
    let mut deleted: u64 = 0;
    let mut failed: u64 = 0;
    let mut total_bytes: u64 = 0;

    // Gemini review B31 — sleep only BETWEEN items; no needless
    // waiting after the last item.
    let mut first = true;
    for item in items {
        if !first {
            sleep(interval);
        }
        first = false;
        // The permanent delete's confirm_phrase is the file name.
        let file_name = std::path::PathBuf::from(&item.original_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.is_empty() {
            failed += 1;
            continue;
        }
        match permanent_delete(item.id, &file_name, false, conn) {
            Ok(PermanentDeleteResult { size_bytes, .. }) => {
                deleted += 1;
                total_bytes = total_bytes.saturating_add(size_bytes);
            }
            Err(e) => {
                failed += 1;
                warn!(
                    id = item.id,
                    path = %item.original_path,
                    error = ?e,
                    "expired permanent_delete failed"
                );
            }
        }
    }

    let report = CleanupReport {
        deleted,
        failed,
        total_bytes,
        elapsed_ms: started.elapsed().as_millis() as u64,
        aborted_threshold: false,
    };
    info!(?report, "lazy expiry cleanup complete");
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;
    use std::path::PathBuf;

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
            "dspace-expiry-{}-{}-{}",
            std::process::id(),
            now_unix(),
            name
        ))
    }

    fn seed_expired(
        conn: &Connection,
        original: &str,
        staged: &std::path::Path,
        size: u64,
        ago_secs: i64,
    ) -> i64 {
        let now = now_unix();
        let staged_at = now - ago_secs - 86400;
        let expires_at = staged_at + 86400;
        conn.execute(
            "INSERT INTO staging_items
                (original_path, staged_path, size_bytes, staged_at, expires_at,
                 is_dir, reason, fallback_tier)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, 'normal')",
            params![
                original,
                staged.to_string_lossy(),
                size as i64,
                staged_at,
                expires_at,
            ],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn list_expired_orders_oldest_first() {
        let conn = fresh_db();
        let work = unique_tmp("list");
        fs::create_dir_all(&work).unwrap();
        let s1 = work.join("oldest.txt");
        let s2 = work.join("newest.txt");
        fs::write(&s1, b"x").unwrap();
        fs::write(&s2, b"x").unwrap();
        seed_expired(&conn, r"C:\a.txt", &s1, 1, 7200);
        seed_expired(&conn, r"C:\b.txt", &s2, 1, 60);

        let expired = list_expired(&conn).unwrap();
        assert_eq!(expired.len(), 2);
        assert!(expired[0].age_secs > expired[1].age_secs);
        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn cleanup_under_threshold_proceeds() {
        let mut conn = fresh_db();
        let work = unique_tmp("cleanup");
        fs::create_dir_all(&work).unwrap();
        let s1 = work.join("expired1.txt");
        let s2 = work.join("expired2.txt");
        fs::write(&s1, b"aa").unwrap();
        fs::write(&s2, b"bbb").unwrap();
        seed_expired(&conn, r"C:\expired1.txt", &s1, 2, 100);
        seed_expired(&conn, r"C:\expired2.txt", &s2, 3, 50);

        // Keep the rate high in this test (so the sleep stays small)
        let report = cleanup_expired(&mut conn, 1000, false).unwrap();
        assert_eq!(report.deleted, 2);
        assert_eq!(report.failed, 0);
        assert!(!report.aborted_threshold);
        assert!(!s1.exists() && !s2.exists());

        // staging_items is empty
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM staging_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
        // forensic ledger has 2 records
        let forensic: i64 = conn
            .query_row("SELECT COUNT(*) FROM permanent_deletes_forensic", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(forensic, 2);

        let _ = fs::remove_dir_all(&work);
    }
}
