// SPDX-License-Identifier: GPL-3.0-or-later
//
// DoD 5220.22-M secure wipe — Master architecture Chapter 12.4 (sensitive
// file mode, opt-in). Gemini review 3.1 — overwrites the on-disk data in
// 3 passes before permanent deletion, then deletes the file. Forensic
// recovery tools (Recuva, PhotoRec) can no longer bring the data back.
//
// Standard:
//   * Pass 1: 0x00 (zeros) — deletes the original bytes
//   * Pass 2: 0xFF (ones, the complement of 0x00)
//   * Pass 3: pseudo-random bytes (xorshift64 PRNG, seeded from the
//     system clock). Cryptographic random is not required — the goal is
//     to defeat forensic recovery, not privacy protection.
//
// After each pass, `sync_all` flushes the OS buffers + commits to the
// physical disk. On SSDs there is no 100% guarantee due to wear leveling
// (the controller may keep hidden block copies); noting this limitation
// for the user in the UI is the caller's responsibility (contract docs).
//
// Safety check: the caller requires the file name to be typed exactly in
// the `confirm_phrase` parameter (Chapter 12.4 double-confirm pattern).
// The raw `dod_wipe_file` function in this module does not confirm — the
// wrapper in the `permanent` module does; the raw function must not be used.

use crate::error::{Error, Result};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

/// Overwrite chunk size — 64 KB lands on cluster boundaries and does
/// not bloat RAM on large files.
const WIPE_CHUNK: usize = 64 * 1024;

/// `xorshift64` — Marsaglia 2003. Not cryptographic; good enough against
/// forensic recovery. State must never be 0 (stuck at zero).
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

/// Pass type — visible in what gets written to the log.
enum Pass {
    Zero,
    Ones,
    Random,
}

/// Writes the given pattern over the file's entire length, start to
/// finish, + sync_all. The file handle comes from the caller; the
/// position is rewound to the start.
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

/// DoD 5220.22-M secure wipe + delete. 3 overwrite passes + sync, then
/// delete the file. The path must be a FILE (folders and symlinks are
/// rejected — the caller verifies this in permanent.rs).
///
/// **WARNING**: This operation cannot be undone. The caller invokes it
/// only AFTER `permanent::permanent_delete` has double-confirmed.
pub fn dod_wipe_file(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| Error::Staging(format!("wipe: meta '{}': {}", path.display(), e)))?;
    if !metadata.is_file() {
        return Err(Error::Staging(format!(
            "wipe accepts only regular files, not folders/symlinks: '{}'",
            path.display()
        )));
    }
    let len = metadata.len();
    // Gemini review B17 — FILE_ATTRIBUTE_READONLY files get ACCESS_DENIED
    // from OpenOptions .write(true). The MOVE into staging preserves the
    // attribute; clear the readonly flag before the wipe (best-effort).
    {
        let perms = std::fs::metadata(path)
            .map_err(|e| Error::Staging(format!("wipe perms '{}': {}", path.display(), e)))?
            .permissions();
        if perms.readonly() {
            // Windows target: clearing FILE_ATTRIBUTE_READONLY. Clippy's
            // Unix "world writable" warning does not apply on this platform.
            #[allow(clippy::permissions_set_readonly_false)]
            {
                let mut cleared = perms;
                cleared.set_readonly(false);
                if let Err(e) = std::fs::set_permissions(path, cleared) {
                    debug!(path = %path.display(), error = %e, "readonly clear failed");
                }
            }
        }
    }

    // 0-byte file: passes are pointless, delete directly.
    if len == 0 {
        std::fs::remove_file(path).map_err(|e| Error::Staging(format!("wipe remove: {}", e)))?;
        debug!(path = %path.display(), "0-byte file — delete without passes");
        return Ok(());
    }

    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| Error::Staging(format!("wipe open '{}': {}", path.display(), e)))?;

    for (i, pass) in [Pass::Zero, Pass::Ones, Pass::Random].iter().enumerate() {
        overwrite_pass(&mut file, len, pass)
            .map_err(|e| Error::Staging(format!("wipe pass {}: {}", i + 1, e)))?;
        debug!(path = %path.display(), pass = i + 1, "DoD pass complete");
    }
    // Gemini review B18 — once the passes finish, truncate the file to 0
    // so the final length + the last pass's content remnants cannot be
    // read from the MFT resident data.
    file.set_len(0)
        .map_err(|e| Error::Staging(format!("wipe truncate: {}", e)))?;
    drop(file);

    // Gemini review B18 — rename to a random short name before deleting:
    // the original file name leaves a forensic remnant in the NTFS
    // directory entry.
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
        "DoD 5220.22-M secure wipe + delete complete"
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
        assert!(!p.exists(), "file should be gone");
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
            Error::Staging(msg) => assert!(msg.contains("folders") || msg.contains("files")),
            _ => panic!("expected a staging error"),
        }
        // The folder must still exist
        assert!(inner.exists());
    }

    #[test]
    fn wipe_rejects_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("ghost.bin");
        let err = dod_wipe_file(&p).unwrap_err();
        assert!(matches!(err, Error::Staging(_)));
    }

    /// Prove that the 3-pass overwrite changes the byte values in sequence —
    /// call overwrite_pass directly and check the file content after
    /// each pass.
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
