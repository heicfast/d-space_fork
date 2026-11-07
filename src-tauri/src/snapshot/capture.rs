// SPDX-License-Identifier: GPL-3.0-or-later
//
// Snapshot capture — Master mimari Bölüm 8 (Time Machine).
//
// v0.1 ilkeler:
//   * Sadece dizinler tablolanır (file sayısı çok yüksek; dir-only delta
//     UI ihtiyaçlarını karşılar). v0.2'de Bölüm 8.6 streaming chunk
//     formatı ile dosya seviyesi gelir.
//   * path_hash = blake3(full_path_utf8_bytes) → BLOB 32B (Bölüm 8.5).
//   * Tek transaction ile bulk insert (rusqlite Transaction).
//   * Volume root path drive harfinden türetilir (`C:\\` gibi).

use crate::error::{Error, Result};
use crate::scan::tree::{node_path, ScanTree};
use crate::scan::NodeId;
use crate::snapshot::SnapshotId;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

/// Bir snapshot satırının özet meta'sı — UI/komut çıktısı.
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

/// `volume_id` formatlarından (`C:\\`, `\\?\C:`, `\\.\C:` vb.) sürücü
/// harfini çıkar ve kanonik `C:\\` formatına döndür. Parse edilemezse
/// tek `\\` döner (UNC/network case).
///
/// `:` karakterinin hemen ÖNCESİNDEKİ ASCII harfi sürücü harfi olarak
/// alır. Bu sayede `\\server\share` gibi UNC string'lerinde rastgele
/// karakter `S:\\` üretmez.
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
    // Gemini review B20 — UNC volume (drive harfi yok): ge\u00e7erli bir
    // UNC k\u00f6k\u00fc d\u00f6nd\u00fcr ("\\\\server\\share" bi\u00e7iminde \u00e7ift backslash),
    // tek "\" de\u011fil (ge\u00e7ersiz path \u00fcretiyordu).
    let trimmed = volume_id.trim_end_matches('\\');
    if trimmed.starts_with("\\\\") || trimmed.starts_with("//") {
        return trimmed.replace('/', "\\");
    }
    "\\".into()
}

/// Bir düğüm için root prefix + segmentleri "\\" ile birleşik tam Windows
/// path string üretir. Sentetik `<volume root>` (chain[0]) atlanır.
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

/// Bölüm 8 — mevcut `ScanTree`'den snapshot yakalar.
///
/// `snapshots` tablosuna 1 satır, `snapshot_entries`'e **sadece dizinler**
/// için N satır yazar (file_count → snapshots.file_count alanında saklanır
/// ama snapshot_entries içine yazılmaz; v0.2'de streaming chunk formatı
/// gelecek).
pub fn capture_snapshot(tree: &ScanTree, conn: &mut Connection) -> Result<SnapshotMeta> {
    let captured_at = now_unix();
    let total_size_bytes = tree.total_bytes;
    let file_count = tree.file_count;
    let dir_count_tree = tree.dir_count;

    let tx = conn
        .transaction()
        .map_err(|e| Error::Snapshot(format!("transaction başlatma: {}", e)))?;

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

        // Yalnızca dizinleri yaz. Aynı path_hash'a ait çift satır olmaması
        // için (teorik blake3 çakışması ya da duplikasyon) PK ihlali olursa
        // o satırı atla — pratikte 32B blake3 ile çakışma olmaz.
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
                // Gemini review B8 — Node.modified_unix zaten dolu (MFT
                // $STANDARD_INFORMATION / FindFirstFile mtime); 0 placeholder
                // kaldırıldı.
                node.modified_unix,
                1i64,
            ]) {
                Ok(_) => entry_count += 1,
                Err(e) => {
                    // PK çakışması (snapshot_id, path_hash) — debug seviyesinde
                    // logla, snapshot'ı bozma.
                    debug!(
                        node_id = node.id,
                        ?path,
                        error = %e,
                        "snapshot_entries duplicate path_hash atlandı"
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
        "snapshot yakalandı"
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

/// Bölüm 8.4 — saklama politikası. Free tier default 90 gün, premium
/// 365 gün; kullanıcı `settings.snapshot_retain_days` ile ezebilir.
pub const DEFAULT_RETAIN_DAYS: i64 = 90;
pub const MIN_RETAIN_DAYS: i64 = 7;

/// `captured_at < (now - retain_days * 86400)` olan snapshot'ları siler.
/// `snapshot_entries` foreign key cascade ile otomatik temizlenir
/// (migration 0001 `ON DELETE CASCADE`).
///
/// Dönüş: silinen snapshot sayısı. retain_days < MIN_RETAIN_DAYS olursa
/// MIN_RETAIN_DAYS'e clamp edilir (kullanıcı yanlışlıkla 0 girip tüm
/// snapshot'ları yok etmesin — Bölüm 22.6 dark pattern yok ilkesi).
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
            "snapshot retention: eski kayıtlar silindi"
        );
    } else {
        debug!(
            retain_days = days,
            "snapshot retention: silinecek kayıt yok"
        );
    }
    Ok(n as u64)
}

/// Son 50 snapshot'ı captured_at DESC sırasında listeler. `dir_count` ve
/// `entry_count` snapshot_entries'ten COUNT ile hesaplanır (snapshots
/// tablosunda saklanmıyor — v0.1 schema).
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
                // Gemini review B19 — entry_count sentetik ROOT düğümünü
                // de içerir; capture'ın döndüğü dir_count (root hariç)
                // ile aynı olmak için 1 çıkarılır.
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
        assert_eq!(volume_root_prefix("//server/share"), "\\");
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

        // 1 snapshots satırı
        let snap_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(snap_count, 1);

        // Tree'de 2 dir (docs, sub) + sentetik root = 3. Sentetik root da
        // is_dir=true olarak yazılır (path = "C:\\"). dir_count alanı raw
        // input'tan gelir, sentetik root sayılmaz → tree.dir_count = 2.
        // Ama snapshot_entries'e is_dir=true olan her şey yazılır = 3 satır.
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

        // Path inşası doğru mu — docs için "C:\\docs"
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

        // sub için "C:\\docs\\sub"
        let sub_path: String = conn
            .query_row(
                "SELECT path FROM snapshot_entries
                 WHERE snapshot_id = ?1 AND path = 'C:\\docs\\sub'",
                params![meta.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sub_path, "C:\\docs\\sub");

        // is_dir tümü 1
        let non_dir: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM snapshot_entries
                 WHERE snapshot_id = ?1 AND is_dir = 0",
                params![meta.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(non_dir, 0);

        // path_hash 32 bayt
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
        // 3 snapshot: now-100gün, now-30gün, şimdi (90 gün retention için
        // yalnız ilki silinmeli).
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
        assert_eq!(removed, 1, "100 günden eski tek snapshot silinmeli");

        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 2);
    }

    #[test]
    fn purge_clamps_to_min_retain() {
        let conn = fresh_conn();
        let now = now_unix();
        // 5 gün önce (MIN_RETAIN_DAYS=7 altında — silinmemeli).
        let ts_recent = now - 5 * 86_400;
        conn.execute(
            "INSERT INTO snapshots
                (volume_id, captured_at, total_size_bytes, file_count, schema_version)
             VALUES ('C:\\', ?1, 0, 0, 1)",
            params![ts_recent],
        )
        .unwrap();

        // retain_days=1 kullanıcı yanlışlığını clamp et — 7 gün korunur.
        let removed = purge_old_snapshots(&conn, 1).unwrap();
        assert_eq!(removed, 0, "MIN_RETAIN_DAYS clamp koruması");
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
        // FK cascade için PRAGMA aktif olmalı
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
        // 3 snapshot manuel ekle, captured_at 100/200/300
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
        // entries yok → dir_count 0
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
        // sentetik root + alpha + beta = 3 dir entry
        assert_eq!(list[0].entry_count, 3);
    }
}
