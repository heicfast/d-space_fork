// SPDX-License-Identifier: GPL-3.0-or-later
//
// SQLite veri modeli — Master mimari Bölüm 14.
//
// Konum: %LOCALAPPDATA%\DiskWizard\db\diskwizard.sqlite
// Mode: WAL, busy_timeout=5000ms, synchronous=NORMAL, foreign_keys=ON.
//
// Migration: rusqlite_migration crate, forward-only. Migration dosyaları
// `src-tauri/src/db/migrations/NNNN_<ad>.sql` — derleme sırasında
// `include_str!` ile binary'ye gömülür (Bölüm 14.3 fixture korunur).
//
// Tablolar (Bölüm 8.5, 12.5, 6.5.2): schema_meta, settings, snapshots,
// snapshot_entries, staging_items, staging_wal, permanent_deletes_forensic,
// ml_scores.

use crate::error::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

pub const SCHEMA_VERSION: u32 = 4;
pub const DB_FILENAME: &str = "diskwizard.sqlite";

/// `%LOCALAPPDATA%\DiskWizard\db\diskwizard.sqlite` — Windows.
/// Diğer platformlarda `dirs::data_local_dir()` döndüğü değer.
pub fn db_path() -> Result<PathBuf> {
    let base =
        dirs::data_local_dir().ok_or_else(|| Error::Db("data_local_dir bulunamadı".into()))?;
    Ok(base.join("DiskWizard").join("db").join(DB_FILENAME))
}

pub(crate) fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("migrations/0001_initial.sql")),
        M::up(include_str!("migrations/0002_user_rules.sql")),
        // Sprint 3.8 — Discovery Log #005 (Bölüm 5.6). USN Journal index
        // tabloları (usn_index + usn_watermark). Mevcut DB üzerinde
        // forward-only uygulanır.
        M::up(include_str!("migrations/0003_usn_index.sql")),
        // DiskWizard v0.4.0 — persisted recovery scan sessions.
        M::up(include_str!("migrations/0004_sessions.sql")),
    ])
}

/// DB dosyasını açar, PRAGMA'ları ayarlar, migrations'ı çalıştırır.
/// Klasör yoksa oluşturur.
pub fn open_db() -> Result<Connection> {
    let path = db_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    debug!(path = %path.display(), "DB açılıyor");

    let mut conn = Connection::open(&path)
        .map_err(|e| Error::Db(format!("aç '{}': {}", path.display(), e)))?;

    apply_pragmas(&conn)?;

    let mig = migrations();
    mig.to_latest(&mut conn)
        .map_err(|e| Error::Db(format!("migration: {}", e)))?;

    let user_version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| Error::Db(format!("user_version: {}", e)))?;
    info!(user_version, expected = SCHEMA_VERSION, "DB hazır");

    Ok(conn)
}

pub(crate) fn apply_pragmas(conn: &Connection) -> Result<()> {
    let pragmas: [(&str, &str); 4] = [
        ("journal_mode", "WAL"),
        ("synchronous", "NORMAL"),
        ("busy_timeout", "5000"),
        ("foreign_keys", "ON"),
    ];
    for (k, v) in pragmas {
        conn.pragma_update(None, k, v)
            .map_err(|e| Error::Db(format!("PRAGMA {}: {}", k, e)))?;
    }
    Ok(())
}

#[derive(Debug, Serialize, Clone)]
pub struct DbInfo {
    pub path: String,
    pub schema_version: u32,
    pub journal_mode: String,
    pub page_size: u32,
    pub table_count: u32,
    pub spec_version: String,
}

/// DB ile ilgili özet bilgi — UI'da göstermek için.
pub fn db_info(conn: &Connection) -> Result<DbInfo> {
    let path = db_path()?.display().to_string();
    let schema_version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| Error::Db(format!("user_version: {}", e)))?;
    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .map_err(|e| Error::Db(format!("journal_mode: {}", e)))?;
    let page_size: u32 = conn
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .map_err(|e| Error::Db(format!("page_size: {}", e)))?;
    let table_count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(format!("tablo sayımı: {}", e)))?;
    let spec_version: String = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'spec_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| Error::Db(format!("schema_meta: {}", e)))?
        .unwrap_or_default();

    Ok(DbInfo {
        path,
        schema_version,
        journal_mode,
        page_size,
        table_count,
        spec_version,
    })
}

/// Bölüm 14 — `settings` tablosu üzerinden key/value okuma. Yoksa `None`.
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .map_err(|e| Error::Db(format!("settings get '{}': {}", key, e)))
}

/// Bölüm 14 — upsert: aynı anahtar varsa value günceller.
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, now],
    )
    .map_err(|e| Error::Db(format!("settings set '{}': {}", key, e)))?;
    Ok(())
}

/// Tauri yönetilen state: tek `Connection`'ı `Arc<Mutex>` ile sarar.
/// Tek-kullanıcılı desktop için connection pool gerekmez. `Arc` sarması
/// (Gemini review: async komutlar) `spawn_blocking` içine clonlanabilir —
/// uzun DB işleri artık main thread'i bloklamaz.
#[derive(Clone)]
pub struct DbState {
    pub conn: std::sync::Arc<Mutex<Connection>>,
}

impl DbState {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: std::sync::Arc::new(Mutex::new(conn)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_compile_and_apply_in_memory() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_pragmas(&conn).unwrap();
        let mig = migrations();
        mig.to_latest(&mut conn).unwrap();

        // Tablo sayısı: schema_meta + settings + snapshots + snapshot_entries
        // + staging_items + staging_wal + permanent_deletes_forensic + ml_scores
        // + user_rules (0002) + usn_index + usn_watermark (0003)
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='table' AND name NOT LIKE 'sqlite_%' \
                 AND name NOT LIKE 'sqlite_sequence'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 11, "Migration sonrası 11 tablo bekleniyor");

        let spec: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'spec_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(spec, "v1.4");
    }

    #[test]
    fn migrations_validates() {
        // rusqlite_migration kendi içinde her migration'ın geçerli SQL
        // olduğunu in-memory deneyerek doğrular.
        let mig = migrations();
        mig.validate().expect("migrations valid olmalı");
    }
}
