// SPDX-License-Identifier: GPL-3.0-or-later
//
// USN Journal Index — Sprint 3.8 / Discovery Log #005 (Bölüm 5.6).
//
// NTFS USN Journal (Update Sequence Number) üzerinden persistent dosya
// indeksi. "Everything" tipi anlık substring arama; açılışta < 200 ms
// load + arka planda 5 sn poll ile incremental delta sync. Tüm index
// mevcut `dspace.sqlite` içindeki `usn_index` tablosunda yaşar.
//
// Veri akışı:
//   1) `IndexBuilder::baseline_enumerate` → FSCTL_ENUM_USN_DATA ile tüm
//      MFT girdilerini gezer, `apply_baseline` ile DB'ye yazar +
//      ilk watermark (next_usn + journal_id) `usn_watermark`'a kaydeder.
//   2) `IndexBuilder::poll_delta` → FSCTL_READ_USN_JOURNAL ile son
//      watermark sonrası USN kayıtlarını okur, `delta::apply_delta`
//      upsert/delete uygular, watermark günceller.
//   3) Wraparound: journal_id değiştiyse caller `Error::Index` alır,
//      `baseline_enumerate` tekrar tetiklenir (eski USN penceresi geçersiz).
//
// Hızlı arama: `index_search(query, limit)` SQLite `LIKE '%query%'`
// üzerinden top-N. v0.1 sadece isim eşleşmesi, full_path opsiyonel —
// `WITH RECURSIVE` ile parent zinciri çözülür (top-50 için yeterli).

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

/// Tek bir USN index girişi. `volume_id` örn. `\\.\C:`, `file_ref`/`parent_ref`
/// MFT segment numarası (alt 48 bit) — sequence number'sız (NTFS USN tipik
/// kullanımı: yalnız segment kısmı persistent).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexEntry {
    pub volume_id: String,
    pub file_ref: i64,
    pub parent_ref: i64,
    pub name: String,
    pub usn_id: i64,
    pub last_seen_unix: i64,
    /// NTFS file attribute bayrakları (FILE_ATTRIBUTE_DIRECTORY vb.).
    pub attrs: i64,
}

/// `index_search` sonucu. `full_path` opsiyonel — v0.1 yalnız ilk top-50
/// için recursive path zinciri (SQLite `WITH RECURSIVE`) çözülebilir.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexSearchResult {
    pub volume_id: String,
    pub file_ref: i64,
    pub parent_ref: i64,
    pub name: String,
    pub full_path: Option<String>,
    pub attrs: i64,
}

/// `index_status` sonucu. UI status badge'i için.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexStatus {
    pub volume_id: Option<String>,
    pub total_entries: i64,
    pub last_sync_unix: i64,
    /// "ready" | "building" | "idle" | "needs_admin" | "error"
    pub mode: String,
}

/// Yüksek seviye index oluşturucu — baseline + delta operasyonlarını
/// kapsüller. Watcher thread bu yapıyı kullanır.
pub struct IndexBuilder {
    pub volume_id: String,
}

impl IndexBuilder {
    pub fn new(volume_id: impl Into<String>) -> Self {
        Self {
            volume_id: volume_id.into(),
        }
    }

    /// Mevcut watermark'ı (varsa) okur.
    pub fn watermark(&self, conn: &Connection) -> Result<Option<Watermark>> {
        load_watermark(conn, &self.volume_id)
    }
}

/// Gemini review B9 / v0.3.1 prod wire — `usn_watermark` tablosunda kayıtlı
/// volume'lerin ID listesi (`\\.\C:` biçiminde). Açılışta watcher spawn
/// etmek için kullanılır.
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

/// `sync_volume` sonucu — incremental delta mı full baseline mı koşuldu.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SyncOutcome {
    /// Watermark yoktu / wraparound — full `FSCTL_ENUM_USN_DATA` rebuild.
    Baseline(Box<BaselineSummary>),
    /// Watermark geçerliydi — yalnız `FSCTL_READ_USN_JOURNAL` delta uygulandı.
    Delta {
        volume_id: String,
        #[serde(flatten)]
        summary: DeltaSummary,
        next_usn: i64,
    },
}

/// Bir volume'ün USN index'ini güncelle — watermark varsa incremental delta,
/// yoksa (veya wraparound'ta) full baseline. `index_build` komutu ve watcher
/// tick'inin ortak çekirdeği (Gemini review: her açılışta full rebuild
/// yerine watermark kısa devresi).
pub fn sync_volume(
    volume_id: &str,
    conn: &mut Connection,
    buffer_size: usize,
) -> Result<SyncOutcome> {
    let fresh = baseline::query_journal(volume_id)?;
    let stored = load_watermark(conn, volume_id)?;

    match stored {
        Some(wm) => {
            // Wraparound kontrolü — journal_id değiştiyse full rebuild.
            if let Err(e) = detect_wraparound(wm.journal_id, fresh.journal_id) {
                info!(volume = volume_id, ?e, "wraparound — full rebuild");
                let summary = enumerate_volume_baseline(volume_id, conn, buffer_size)?;
                return Ok(SyncOutcome::Baseline(Box::new(summary)));
            }
            // Incremental delta: watermark'dan itibaren beklemesiz oku.
            let read = usn::read_journal_blocking(
                volume_id,
                wm.next_usn,
                fresh.journal_id,
                0, // no-wait: birikmiş kayıtları topla, bekleme yok
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
            // Henüz baseline yok — full enumerate.
            let summary = enumerate_volume_baseline(volume_id, conn, buffer_size)?;
            Ok(SyncOutcome::Baseline(Box::new(summary)))
        }
    }
}

/// `usn_index` toplam satır sayısı. UI badge için.
pub fn count_entries(conn: &Connection, volume_id: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM usn_index WHERE volume_id = ?1",
        params![volume_id],
        |r| r.get(0),
    )
    .map_err(|e| Error::Index(format!("count_entries: {}", e)))
}

/// Son sync zamanı. `volume_id` verilirse yalnız o cilt; yoksa global
/// MAX. Boş indeks için 0 döner. (Gemini review B24 — per-volume sorgu
/// başka bir cildin tazeliğini raporlamasın.)
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

/// Tauri command'leri için durum sorgusu. Volume parametresi opsiyonel:
/// verilmezse "global" sayım dönülür (volume_id = None).
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

/// Substring arama. `query` boş ise `Ok(vec![])`. `limit` üst sınır.
/// Discovery Log #005 — Bölüm 5.6: v0.1 yalnız name LIKE eşleşmesi,
/// full_path opsiyonel olarak çözülür (yalnız döndürülen sonuçlar için).
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
    // Top-N için recursive path zinciri. Hata olursa silent — full_path
    // yalnızca konfor alanı, eksikliği sorun değil.
    for r in results.iter_mut() {
        if let Ok(p) = persist::resolve_full_path(conn, &r.volume_id, r.file_ref) {
            r.full_path = Some(p);
        }
    }
    Ok(results)
}

/// Şu anki unix epoch (saniye). Test edilebilirlik için ayrı fonksiyon.
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

    // db modülüne erişim sentinel — migration zincirinde alias.
    #[allow(dead_code)]
    fn _db_alias() {
        let _ = db::SCHEMA_VERSION;
    }
}
