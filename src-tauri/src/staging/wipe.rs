// SPDX-License-Identifier: GPL-3.0-or-later
//
// DoD 5220.22-M secure wipe — Master mimari Bölüm 12.4 (hassas dosya
// modu, opt-in). Gemini review 3.1 — kalıcı silme öncesi disk üstündeki
// veriyi 3 geçişte üzerine yazar, ardından dosyayı siler. Forensic
// recovery araçları (Recuva, PhotoRec) artık veriyi geri getiremez.
//
// Standart:
//   * Pass 1: 0x00 (sıfır) — orijinal byte'ları silver
//   * Pass 2: 0xFF (bir, 0x00'ın tümleyeni)
//   * Pass 3: pseudo-rastgele bayt (xorshift64 PRNG, sistem saatinden
//     seed). Cryptographic random gerekmez — amaç forensic recovery'yi
//     bozmak, gizlilik koruma değil.
//
// Her geçiş sonrası `sync_all` ile OS buffer flush + fiziksel disk
// commit. SSD'lerde wear leveling nedeniyle %100 garanti yok (kontrolör
// gizli blok kopyaları tutabilir); bu sınırı kullanıcıya UI'da belirtmek
// caller sorumluluğu (kontrat dokümantasyon).
//
// Güvenlik kontrolü: caller `confirm_phrase` parametresinde dosya adının
// tam olarak yazılmasını ister (Bölüm 12.4 double-confirm pattern). Bu
// modülün ham `dod_wipe_file` fonksiyonu confirm yapmaz — `permanent`
// modülündeki sarmalayıcı bunu yapar; ham fonksiyonu kullanmamalı.

use crate::error::{Error, Result};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

/// Üzerine yazma chunk boyutu — 64 KB ile cluster sınırlarına oturur,
/// büyük dosyalarda RAM'i şişirmez.
const WIPE_CHUNK: usize = 64 * 1024;

/// `xorshift64` — Marsaglia 2003. Cryptographic değil; forensic recovery
/// için yeterli. State 0 olmamalı (sıfır kalıcı).
struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    fn from_time_seed() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xDEAD_BEEF_CAFE_F00D);
        Self { state: seed.max(1) }
    }

    #[cfg(test)]
    fn from_seed(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn fill(&mut self, buf: &mut [u8]) {
        let mut i = 0;
        while i + 8 <= buf.len() {
            let v = self.next_u64().to_le_bytes();
            buf[i..i + 8].copy_from_slice(&v);
            i += 8;
        }
        if i < buf.len() {
            let v = self.next_u64().to_le_bytes();
            let remaining = buf.len() - i;
            buf[i..].copy_from_slice(&v[..remaining]);
        }
    }
}

/// Geçiş türü — log'a ne yazıldığı görünür.
enum Pass {
    Zero,
    Ones,
    Random,
}

/// Bir dosyanın tüm uzunluğunu verilen pattern ile baştan sona yaz +
/// sync_all. File handle çağırıcıdan; pozisyon başa sarılır.
fn overwrite_pass(file: &mut File, len: u64, pass: &Pass) -> std::io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = vec![0u8; WIPE_CHUNK];
    let mut prng = Xorshift64::from_time_seed();
    let mut remaining = len;
    while remaining > 0 {
        let n = remaining.min(WIPE_CHUNK as u64) as usize;
        match pass {
            Pass::Zero => buf[..n].fill(0x00),
            Pass::Ones => buf[..n].fill(0xFF),
            Pass::Random => prng.fill(&mut buf[..n]),
        }
        file.write_all(&buf[..n])?;
        remaining -= n as u64;
    }
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

/// DoD 5220.22-M secure wipe + delete. 3 geçiş overwrite + sync, sonra
/// dosyayı sil. Path bir DOSYA olmalı (klasör veya symlink reddedilir —
/// caller permanent.rs'de bunu doğrular).
///
/// **DİKKAT**: Bu işlem geri alınamaz. Caller `permanent::permanent_delete`
/// double-confirm yaptıktan SONRA çağırır.
pub fn dod_wipe_file(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| Error::Staging(format!("wipe: meta '{}': {}", path.display(), e)))?;
    if !metadata.is_file() {
        return Err(Error::Staging(format!(
            "wipe yalnız düz dosya kabul eder, klasör/symlink değil: '{}'",
            path.display()
        )));
    }
    let len = metadata.len();
    // Gemini review B17 — FILE_ATTRIBUTE_READONLY dosyalar OpenOptions
    // .write(true) ile ACCESS_DENIED verir. Staging'e MOVE attribute'yi
    // korur; wipe öncesi readonly bayrağını temizle (best-effort).
    {
        let perms = std::fs::metadata(path)
            .map_err(|e| Error::Staging(format!("wipe perms '{}': {}", path.display(), e)))?
            .permissions();
        if perms.readonly() {
            // Windows hedefi: FILE_ATTRIBUTE_READONLY temizliği. Clippy'nin
            // Unix "world writable" uyarısı bu platformda geçerli değil.
            #[allow(clippy::permissions_set_readonly_false)]
            {
                let mut cleared = perms;
                cleared.set_readonly(false);
                if let Err(e) = std::fs::set_permissions(path, cleared) {
                    debug!(path = %path.display(), error = %e, "readonly temizleme başarısız");
                }
            }
        }
    }

    // 0-byte dosya: passes anlamsız, direkt sil.
    if len == 0 {
        std::fs::remove_file(path).map_err(|e| Error::Staging(format!("wipe remove: {}", e)))?;
        debug!(path = %path.display(), "0-byte dosya — pass'sız sil");
        return Ok(());
    }

    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| Error::Staging(format!("wipe aç '{}': {}", path.display(), e)))?;

    for (i, pass) in [Pass::Zero, Pass::Ones, Pass::Random].iter().enumerate() {
        overwrite_pass(&mut file, len, pass)
            .map_err(|e| Error::Staging(format!("wipe pass {}: {}", i + 1, e)))?;
        debug!(path = %path.display(), pass = i + 1, "DoD pass tamam");
    }
    // Gemini review B18 — geçişler bittiğinde dosyayı 0'a truncate et ki
    // son uzunluk + son geçişin içerik kalıntısı MFT resident verisinden
    // okunamasın.
    file.set_len(0)
        .map_err(|e| Error::Staging(format!("wipe truncate: {}", e)))?;
    drop(file);

    // Gemini review B18 — silmeden önce rastgele kısa isme rename: NTFS
    // directory entry'sinde orijinal dosya adı forensic kalıntısı bırakır.
    let random_suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let renamed = parent.join(format!(".dspace_wiped_{:x}", random_suffix));
    let delete_target = if std::fs::rename(path, &renamed).is_ok() {
        renamed
    } else {
        path.to_path_buf()
    };
    std::fs::remove_file(&delete_target)
        .map_err(|e| Error::Staging(format!("wipe remove: {}", e)))?;

    info!(
        path = %path.display(),
        bytes = len,
        "DoD 5220.22-M secure wipe + delete tamam"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[allow(unused_imports)]
    use std::io::Write as _;

    #[test]
    fn xorshift_repeatable_with_seed() {
        let mut a = Xorshift64::from_seed(42);
        let mut b = Xorshift64::from_seed(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn xorshift_fill_different_seeds_differ() {
        let mut a = Xorshift64::from_seed(1);
        let mut b = Xorshift64::from_seed(2);
        let mut buf_a = [0u8; 256];
        let mut buf_b = [0u8; 256];
        a.fill(&mut buf_a);
        b.fill(&mut buf_b);
        assert_ne!(buf_a, buf_b);
    }

    #[test]
    fn wipe_actually_deletes_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("victim.bin");
        let content = b"sensitive data 0123456789".repeat(2000);
        fs::File::create(&p).unwrap().write_all(&content).unwrap();
        assert!(p.exists());

        dod_wipe_file(&p).unwrap();
        assert!(!p.exists(), "dosya silinmiş olmalı");
    }

    #[test]
    fn wipe_zero_byte_file_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("empty.bin");
        fs::File::create(&p).unwrap();
        assert!(p.exists());
        assert_eq!(fs::metadata(&p).unwrap().len(), 0);

        dod_wipe_file(&p).unwrap();
        assert!(!p.exists());
    }

    #[test]
    fn wipe_rejects_directory() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("sub");
        fs::create_dir(&inner).unwrap();
        let err = dod_wipe_file(&inner).unwrap_err();
        match err {
            Error::Staging(msg) => assert!(msg.contains("klasör") || msg.contains("dosya")),
            _ => panic!("Staging hata bekleniyor"),
        }
        // Klasör hâlâ varlığını korumalı
        assert!(inner.exists());
    }

    #[test]
    fn wipe_rejects_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("ghost.bin");
        let err = dod_wipe_file(&p).unwrap_err();
        assert!(matches!(err, Error::Staging(_)));
    }

    /// 3-geçiş üzerine yazma bayt değerlerini sırayla değiştirir mi
    /// kanıtla — overwrite_pass'i doğrudan çağır, file content'i her
    /// pass sonrası kontrol et.
    #[test]
    fn overwrite_pass_writes_expected_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pattern.bin");
        let original = vec![0xAB; 1024];
        fs::write(&p, &original).unwrap();

        let mut file = OpenOptions::new().write(true).read(true).open(&p).unwrap();

        overwrite_pass(&mut file, 1024, &Pass::Zero).unwrap();
        let after_zero = fs::read(&p).unwrap();
        assert!(after_zero.iter().all(|&b| b == 0x00));

        let mut file2 = OpenOptions::new().write(true).read(true).open(&p).unwrap();
        overwrite_pass(&mut file2, 1024, &Pass::Ones).unwrap();
        let after_ones = fs::read(&p).unwrap();
        assert!(after_ones.iter().all(|&b| b == 0xFF));
    }
}
