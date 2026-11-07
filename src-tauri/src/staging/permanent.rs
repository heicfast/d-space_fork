// SPDX-License-Identifier: GPL-3.0-or-later
//
// Permanent Delete — Master mimari Bölüm 12.4.
//
// Spec: "Şimdi kalıcı sil ayrı buton, çift onay. Hassas dosyalar için
// DoD 5220.22-M uyumlu overwrite (faz 2)."
//
// v0.1 kapsamı:
//   * Confirm phrase doğrulaması — kullanıcı staging item'ın **tam dosya
//     adını** yazmak zorunda. Tek tık değil, klavye onayı.
//   * Forensic ledger (Bölüm 12.5): `permanent_deletes_forensic` tablosuna
//     orijinal_path + size + deleted_at + blake3_first4kb + double_confirm
//     flag. İleride forensic audit (kim ne sildi) için.
//   * Standart `fs::remove_file` / `fs::remove_dir_all`. DoD wipe v0.2.
//
// İlke (Bölüm 22.6): kullanıcı her zaman görür ve onaylar — dark pattern
// yok. Confirm phrase typed kullanıcı seçimini somutlaştırır.

use crate::error::{Error, Result};
use crate::staging::ops::now_unix;
use blake3::Hasher;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

const FIRST_KB: usize = 4096;

#[derive(Debug, Clone, Serialize)]
pub struct PermanentDeleteResult {
    pub id: i64,
    pub original_path: String,
    pub staged_path: String,
    pub size_bytes: u64,
    pub deleted_at_unix: i64,
    pub blake3_first4kb_hex: Option<String>,
    pub is_dir: bool,
}

/// İlk 4 KB'lik Blake3 hash — forensic kanıt için yeterli (tam dosya hash
/// 100 GB'lik dosyalarda kabul edilemez maliyet). Klasör için None döner.
fn hash_first_4kb(path: &Path) -> Result<Option<[u8; 32]>> {
    if path.is_dir() {
        return Ok(None);
    }
    let file = fs::File::open(path)
        .map_err(|e| Error::Staging(format!("permanent hash aç '{}': {}", path.display(), e)))?;
    let mut reader = BufReader::with_capacity(FIRST_KB, file);
    let mut buf = vec![0u8; FIRST_KB];
    let mut total = 0usize;
    while total < FIRST_KB {
        let n = reader
            .read(&mut buf[total..])
            .map_err(|e| Error::Staging(format!("permanent hash read: {}", e)))?;
        if n == 0 {
            break;
        }
        total += n;
    }
    let mut hasher = Hasher::new();
    hasher.update(&buf[..total]);
    Ok(Some(*hasher.finalize().as_bytes()))
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Bölüm 12.4 — staging item'ı disk'ten kalıcı silsin + forensic ledger'a
/// kayıt düşsün. `confirm_phrase` staging item'ın orijinal dosya adına
/// (case-insensitive) eşit olmalı — bu çift onayın ikinci adımı (ilk adım
/// UI'daki "Sil" butonu, ikinci adım dosya adı yazımı).
///
/// Klasör için `remove_dir_all` (recursive). Hash sadece tek dosyada
/// (ilk 4 KB).
///
/// `secure_wipe` true ise tek dosyalar (klasör değil) için
/// `staging::wipe::dod_wipe_file` çağrılır: 3-pass overwrite (0x00, 0xFF,
/// random) + sync + delete. Forensic recovery araçları veriyi geri
/// getiremez. SSD'lerde wear leveling sınırı vardır (UI uyarısı).
pub fn permanent_delete(
    id: i64,
    confirm_phrase: &str,
    secure_wipe: bool,
    conn: &mut Connection,
) -> Result<PermanentDeleteResult> {
    let (original_path, staged_path, size_bytes, is_dir): (String, String, i64, i64) = conn
        .query_row(
            "SELECT original_path, staged_path, size_bytes, is_dir
             FROM staging_items WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| Error::Staging(format!("permanent lookup id={}: {}", id, e)))?;
    let is_dir = is_dir != 0;

    let original_pb = PathBuf::from(&original_path);
    let expected = original_pb
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if expected.is_empty() {
        return Err(Error::Staging(format!(
            "Orijinal path'ten dosya adı çıkarılamadı: {}",
            original_path
        )));
    }
    if !confirm_phrase.eq_ignore_ascii_case(&expected) {
        warn!(
            expected = %expected,
            received_len = confirm_phrase.len(),
            "permanent delete onay başarısız (phrase uyuşmuyor)"
        );
        return Err(Error::Staging(format!(
            "Onay başarısız. Tam dosya adını yazmalısın: '{}'",
            expected
        )));
    }

    let staged_pb = PathBuf::from(&staged_path);
    if !staged_pb.exists() {
        return Err(Error::Staging(format!(
            "Staging dosyası bulunamadı: {}",
            staged_path
        )));
    }

    // Gemini review B16 — hash hatası sessiz None olmasın; forensic satırı
    // NULL hash ile yazılır ama log'da görünür.
    let hash = match hash_first_4kb(&staged_pb) {
        Ok(h) => h,
        Err(e) => {
            warn!(staged = %staged_pb.display(), error = %e, "forensic hash alınamadı");
            None
        }
    };
    let hash_hex = hash.as_ref().map(hex32);

    // Önce DB'ye forensic kayıt — silme başarısızsa bile orijinal niyet kalır.
    let deleted_at = now_unix();
    let tx = conn
        .transaction()
        .map_err(|e| Error::Staging(format!("tx begin: {}", e)))?;

    tx.execute(
        "INSERT INTO permanent_deletes_forensic
            (original_path, size_bytes, deleted_at, blake3_first4kb, user_confirmed_twice)
         VALUES (?1, ?2, ?3, ?4, 1)",
        params![
            original_path,
            size_bytes,
            deleted_at,
            hash.as_ref().map(|h| h.as_slice()),
        ],
    )
    .map_err(|e| Error::Staging(format!("forensic insert: {}", e)))?;

    tx.execute("DELETE FROM staging_items WHERE id = ?1", params![id])
        .map_err(|e| Error::Staging(format!("staging delete: {}", e)))?;

    tx.commit()
        .map_err(|e| Error::Staging(format!("tx commit: {}", e)))?;

    // Sonra dosyayı sil. DB transaction commit edilmiş olsa bile FS hatası
    // ledger'ı bozmaz — manuel temizlik için iz kalır.
    // secure_wipe yalnız tek dosyaya uygulanır; klasör recursive remove ile
    // (DoD wipe klasör için anlamsız — her dosyayı tek tek wipe etmek çok
    // pahalı, kullanıcı UI'da onayladı zaten).
    let fs_result = if is_dir {
        fs::remove_dir_all(&staged_pb)
            .map_err(|e| std::io::Error::other(format!("klasör recursive remove: {}", e)))
    } else if secure_wipe {
        crate::staging::wipe::dod_wipe_file(&staged_pb)
            .map_err(|e| std::io::Error::other(format!("DoD wipe: {}", e)))
    } else {
        fs::remove_file(&staged_pb)
    };
    if let Err(e) = fs_result {
        warn!(
            staged = %staged_pb.display(),
            error = %e,
            "permanent delete FS hatası — forensic ledger zaten yazıldı"
        );
        return Err(Error::Staging(format!(
            "Dosya silme başarısız ('{}'): {} — \
             forensic ledger kaydı yazıldı, manuel temizleyebilirsin",
            staged_pb.display(),
            e
        )));
    }

    // Gemini review B32 — boş kalan staging bucket dizinini temizle.
    if let Some(bucket) = staged_pb.parent() {
        if let Err(e) = fs::remove_dir(bucket) {
            debug!(bucket = %bucket.display(), error = %e, "bucket cleanup atlandı");
        }
    }

    info!(
        id,
        original = %original_path,
        is_dir,
        size = size_bytes,
        hash_hex = hash_hex.as_deref().unwrap_or("-"),
        secure_wipe,
        "permanent delete tamamlandı"
    );

    Ok(PermanentDeleteResult {
        id,
        original_path,
        staged_path,
        size_bytes: size_bytes as u64,
        deleted_at_unix: deleted_at,
        blake3_first4kb_hex: hash_hex,
        is_dir,
    })
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

    fn unique_tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dspace-permdel-{}-{}-{}",
            std::process::id(),
            now_unix(),
            name
        ))
    }

    fn seed_staging(conn: &Connection, original: &str, staged: &Path, size: u64) -> i64 {
        let now = now_unix();
        conn.execute(
            "INSERT INTO staging_items
                (original_path, staged_path, size_bytes, staged_at, expires_at,
                 is_dir, reason, fallback_tier)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, 'normal')",
            params![
                original,
                staged.to_string_lossy(),
                size as i64,
                now,
                now + 86400
            ],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn wrong_phrase_rejects_and_keeps_file() {
        let mut conn = fresh_db();
        let work = unique_tmp("rejects");
        fs::create_dir_all(&work).unwrap();
        let staged = work.join("staged-secret.bin");
        fs::write(&staged, b"sensitive").unwrap();
        let id = seed_staging(&conn, r"C:\Users\test\secret.bin", &staged, 9);

        let err = permanent_delete(id, "yanlış-isim.bin", false, &mut conn);
        assert!(err.is_err());
        // Dosya hâlâ orada
        assert!(staged.exists());
        // staging_items satırı korunmuş
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM staging_items WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn correct_phrase_deletes_and_writes_forensic() {
        let mut conn = fresh_db();
        let work = unique_tmp("deletes");
        fs::create_dir_all(&work).unwrap();
        let staged = work.join("staged-doomed.txt");
        fs::write(&staged, b"some content here").unwrap();
        let id = seed_staging(&conn, r"C:\Users\test\doomed.txt", &staged, 17);

        let result = permanent_delete(id, "doomed.txt", false, &mut conn).expect("phrase doğru");
        assert_eq!(result.size_bytes, 17);
        assert!(result.blake3_first4kb_hex.is_some());
        assert!(!result.is_dir);

        // Disk'te dosya yok
        assert!(!staged.exists());
        // staging_items'tan silindi
        let staging_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM staging_items WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(staging_count, 0);
        // Forensic kayıt var
        let forensic_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM permanent_deletes_forensic \
                 WHERE original_path = ?1",
                params![r"C:\Users\test\doomed.txt"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(forensic_count, 1);

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn phrase_matches_case_insensitive() {
        let mut conn = fresh_db();
        let work = unique_tmp("ci");
        fs::create_dir_all(&work).unwrap();
        let staged = work.join("staged-Report.PDF");
        fs::write(&staged, b"pdf bytes").unwrap();
        let id = seed_staging(&conn, r"C:\Users\test\Report.PDF", &staged, 9);

        // Küçük harfli phrase, dosya adı büyük harfli — eşleşmeli
        let result = permanent_delete(id, "report.pdf", false, &mut conn);
        assert!(result.is_ok());
        assert!(!staged.exists());

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn secure_wipe_overwrites_then_deletes() {
        let mut conn = fresh_db();
        let work = unique_tmp("wipe");
        fs::create_dir_all(&work).unwrap();
        let staged = work.join("staged-secret.bin");
        let sensitive = b"super sensitive payload 0123456789".repeat(100);
        fs::write(&staged, &sensitive).unwrap();
        let id = seed_staging(
            &conn,
            r"C:\Users\test\secret.bin",
            &staged,
            sensitive.len() as u64,
        );

        let result = permanent_delete(id, "secret.bin", true, &mut conn)
            .expect("secure wipe başarılı olmalı");
        assert!(!staged.exists(), "dosya silinmiş olmalı");
        // Forensic kayıt yine yazıldı (hash + path)
        let forensic_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM permanent_deletes_forensic \
                 WHERE original_path = ?1",
                params![r"C:\Users\test\secret.bin"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(forensic_count, 1);
        assert_eq!(result.size_bytes, sensitive.len() as u64);

        let _ = fs::remove_dir_all(&work);
    }
}
