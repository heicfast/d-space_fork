// SPDX-License-Identifier: GPL-3.0-or-later
//
// USN baseline walker — Sprint 3.8.1.
//
// Discovery Log #005 — Bölüm 5.6: `FSCTL_ENUM_USN_DATA` ile NTFS MFT'nin
// tamamını gez, USN_RECORD_V2 sırasında dosya isim/parent/attr kayıtlarını
// `usn_index` tablosuna 1 MB blokları halinde upsert et. Tek bir cilt için
// tipik 1M dosyada ~5 sn — sonraki açılışlarda watcher (`watcher.rs`) yalnız
// delta'yı uygular.
//
// Akış (Windows):
//   1) Volume handle aç: `CreateFileW(\\.\X:, GENERIC_READ, FILE_SHARE_*,
//      OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS)` — admin gerekli.
//   2) `FSCTL_QUERY_USN_JOURNAL` → `JournalData` (journal_id + next_usn).
//      Watermark snapshot'ı baseline öncesi alınır (race olsa bile baseline
//      sırasında düşen kayıtlar watcher delta'sı ile yakalanır).
//   3) `FSCTL_ENUM_USN_DATA` döngüsü, 1 MB output buffer. Her yanıt:
//      `[u64 next_start_file_ref][USN_RECORD_V2 ... ]`. EOF: GetLastError
//      → ERROR_HANDLE_EOF.
//   4) Her batch → `apply_baseline_batch` (in-place transaction) → SQLite
//      upsert. Race: aynı file_ref watcher delta'da gelirse `save_entries`
//      ON CONFLICT idempotent (son yazan kazanır, kullanıcıdan gizli).
//   5) Bitince `save_watermark` — sonraki açılışta watcher yalnız delta okur.
//
// Modül iki katmana ayrılmıştır:
//   * Saf parser/encoder helper'lar — `cfg`-bağımsız, test edilebilir.
//   * `enumerate_volume_baseline` — yalnız `cfg(windows)` IO katmanı.

use super::persist::{save_entries, save_watermark, Watermark};
use super::usn::{parse_records, read_le_i64, read_le_u64, UsnRecord};
use super::{now_unix, IndexEntry};
use crate::error::{Error, Result};
use rusqlite::Connection;
use serde::Serialize;
use tracing::{debug, info};

/// `FSCTL_ENUM_USN_DATA` çıktısının ilk 8 baytı sonraki çağrıda
/// `StartFileReferenceNumber` olarak kullanılacak `u64`'tür.
pub const ENUM_BUFFER_HEADER: usize = 8;

/// Varsayılan baseline tampon boyutu — 1 MB. Tipik kayıt ~80 bayt → ~12k
/// kayıt/batch. Daha büyük tampon syscall sayısını düşürür ama bellek tepe
/// kullanımını artırır.
pub const DEFAULT_BASELINE_BUFFER: usize = 1 << 20;

/// `USN_JOURNAL_DATA_V0` sabit boyutu (`winioctl.h`): 7 × `DWORDLONG/USN` = 56 bayt.
pub const JOURNAL_DATA_V0_SIZE: usize = 56;

/// `MFT_ENUM_DATA_V0` istek bloğu boyutu (3 × `DWORDLONG/USN` = 24 bayt).
pub const ENUM_REQUEST_V0_SIZE: usize = 24;

/// `FSCTL_QUERY_USN_JOURNAL` çıktısının parse edilmiş hali. Yalnız bizi
/// ilgilendiren alanlar tutuluyor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct JournalData {
    pub journal_id: i64,
    pub first_usn: i64,
    pub next_usn: i64,
    pub lowest_valid_usn: i64,
    pub max_usn: i64,
    pub maximum_size: i64,
    pub allocation_delta: i64,
}

/// Baseline koşusu özet. UI'a dönen `IndexStatus`'tan ayrı — gözlem amaçlı.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BaselineSummary {
    pub volume_id: String,
    pub records_seen: usize,
    pub entries_written: usize,
    pub batches: usize,
    pub journal_id: i64,
    pub next_usn: i64,
}

/// `USN_JOURNAL_DATA_V0` binary buffer parse. Hata: kısa buffer.
pub fn parse_journal_data(buf: &[u8]) -> Result<JournalData> {
    if buf.len() < JOURNAL_DATA_V0_SIZE {
        return Err(Error::Index(format!(
            "USN_JOURNAL_DATA_V0 buffer too short: {} < {}",
            buf.len(),
            JOURNAL_DATA_V0_SIZE
        )));
    }
    let journal_id = read_le_i64(buf, 0);
    let first_usn = read_le_i64(buf, 8);
    let next_usn = read_le_i64(buf, 16);
    let lowest_valid_usn = read_le_i64(buf, 24);
    let max_usn = read_le_i64(buf, 32);
    let maximum_size = read_le_i64(buf, 40);
    let allocation_delta = read_le_i64(buf, 48);
    Ok(JournalData {
        journal_id,
        first_usn,
        next_usn,
        lowest_valid_usn,
        max_usn,
        maximum_size,
        allocation_delta,
    })
}

/// `MFT_ENUM_DATA_V0` istek bloğu — 24 bayt little-endian.
pub fn build_enum_request(
    start_file_ref: u64,
    low_usn: i64,
    high_usn: i64,
) -> [u8; ENUM_REQUEST_V0_SIZE] {
    let mut buf = [0u8; ENUM_REQUEST_V0_SIZE];
    buf[0..8].copy_from_slice(&start_file_ref.to_le_bytes());
    buf[8..16].copy_from_slice(&low_usn.to_le_bytes());
    buf[16..24].copy_from_slice(&high_usn.to_le_bytes());
    buf
}

/// `FSCTL_ENUM_USN_DATA` çıktısını parse et: ilk 8 bayt sonraki
/// `StartFileReferenceNumber`, geri kalan `USN_RECORD_V2` zinciri.
pub fn parse_enum_buffer(buf: &[u8]) -> Result<(u64, Vec<UsnRecord>)> {
    if buf.len() < ENUM_BUFFER_HEADER {
        return Err(Error::Index(format!(
            "ENUM output buffer too short: {} < {}",
            buf.len(),
            ENUM_BUFFER_HEADER
        )));
    }
    let next_ref = read_le_u64(buf, 0);
    let records = parse_records(buf, ENUM_BUFFER_HEADER)?;
    Ok((next_ref, records))
}

/// USN kayıtlarını `IndexEntry`'ye dönüştür. Volume_id + timestamp çağrıcıdan.
pub fn records_to_entries(records: &[UsnRecord], volume_id: &str, ts: i64) -> Vec<IndexEntry> {
    records
        .iter()
        .map(|r| IndexEntry {
            volume_id: volume_id.into(),
            file_ref: r.file_ref,
            parent_ref: r.parent_ref,
            name: r.name.clone(),
            usn_id: r.usn_id,
            last_seen_unix: ts,
            attrs: r.attributes as i64,
        })
        .collect()
}

/// Tek bir batch USN kaydını DB'ye yaz. `save_entries` zaten tek
/// transaction'da upsert eder; baseline + watcher delta race idempotent.
pub fn apply_baseline_batch(
    conn: &mut Connection,
    volume_id: &str,
    records: &[UsnRecord],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let ts = now_unix();
    let entries = records_to_entries(records, volume_id, ts);
    save_entries(conn, &entries)
}

// ----------------------- Windows IO -----------------------

/// `FSCTL_QUERY_USN_JOURNAL` — yalnız journal metadata (journal_id +
/// next_usn). Watcher/incremental sync wraparound kontrolü için kullanır;
/// tam baseline walker'ı çalıştırmaz.
#[cfg(windows)]
pub fn query_journal(volume_id: &str) -> Result<JournalData> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    use super::usn::FSCTL_QUERY_USN_JOURNAL;

    /// GENERIC_READ ham değer — windows-rs sürüm farklarından bağımsız.
    const GENERIC_READ: u32 = 0x8000_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    let wide: Vec<u16> = OsStr::new(volume_id)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let pcwstr = PCWSTR(wide.as_ptr());
    let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

    let handle = unsafe {
        CreateFileW(
            pcwstr,
            GENERIC_READ,
            share,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_BACKUP_SEMANTICS),
            None,
        )
    }
    .map_err(|e| {
        Error::Index(format!(
            "query_journal volume open ({}): {:?}",
            volume_id, e
        ))
    })?;
    let _guard = HandleGuard(handle);

    let mut buf = [0u8; JOURNAL_DATA_V0_SIZE];
    let mut returned: u32 = 0;
    unsafe {
        DeviceIoControl(
            handle,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some(buf.as_mut_ptr() as *mut _),
            JOURNAL_DATA_V0_SIZE as u32,
            Some(&mut returned),
            None,
        )
    }
    .map_err(|e| Error::Index(format!("query_journal DeviceIoControl: {:?}", e)))?;
    if (returned as usize) < JOURNAL_DATA_V0_SIZE {
        return Err(Error::Index(format!(
            "query_journal short response: {} bytes",
            returned
        )));
    }
    parse_journal_data(&buf)
}

/// Non-Windows — journal sorgusu desteklenmez.
#[cfg(not(windows))]
pub fn query_journal(_volume_id: &str) -> Result<JournalData> {
    Err(Error::NotImplemented("query_journal is Windows-only"))
}

#[cfg(windows)]
pub fn enumerate_volume_baseline(
    volume_id: &str,
    conn: &mut Connection,
    buffer_size: usize,
) -> Result<BaselineSummary> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_HANDLE_EOF, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    use super::usn::{FSCTL_ENUM_USN_DATA, FSCTL_QUERY_USN_JOURNAL};

    /// GENERIC_READ ham değer — windows-rs sürüm farklarından bağımsız.
    const GENERIC_READ: u32 = 0x8000_0000;
    /// FILE_FLAG_BACKUP_SEMANTICS — klasör/volume handle için gerekli.
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    let wide: Vec<u16> = OsStr::new(volume_id)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let pcwstr = PCWSTR(wide.as_ptr());
    let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

    let handle = unsafe {
        CreateFileW(
            pcwstr,
            GENERIC_READ,
            share,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_BACKUP_SEMANTICS),
            None,
        )
    }
    .map_err(|e| Error::Index(format!("volume open ({}): {:?}", volume_id, e)))?;
    let _guard = HandleGuard(handle);

    // Journal verilerini al — watermark için journal_id + next_usn snapshot.
    let mut query_buf = [0u8; JOURNAL_DATA_V0_SIZE];
    let mut bytes_returned: u32 = 0;
    unsafe {
        DeviceIoControl(
            handle,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some(query_buf.as_mut_ptr() as *mut _),
            JOURNAL_DATA_V0_SIZE as u32,
            Some(&mut bytes_returned),
            None,
        )
    }
    .map_err(|e| Error::Index(format!("FSCTL_QUERY_USN_JOURNAL: {:?}", e)))?;
    if (bytes_returned as usize) < JOURNAL_DATA_V0_SIZE {
        return Err(Error::Index(format!(
            "USN journal query too short: {} bytes",
            bytes_returned
        )));
    }
    let journal = parse_journal_data(&query_buf)?;
    info!(
        volume = volume_id,
        journal_id = journal.journal_id,
        next_usn = journal.next_usn,
        "USN baseline starting"
    );

    // Gemini review B6 — full rebuild öncesi eski satırları temizle.
    // Upsert-only walk, wraparound sonrası silinen dosyaların kayıtlarını
    // sonsuza dek bırakıyordu (hayalet arama sonuçları). purge_volume
    // watermark'ı da siler — aşağıda yeniden yazılır. Yarıda kalan walk
    // watermark yazmaz → sonraki açılış yine full rebuild ile toparlanır.
    let purged = super::persist::purge_volume(conn, volume_id)?;
    debug!(
        volume = volume_id,
        purged, "old index purged before baseline"
    );

    let mut summary = BaselineSummary {
        volume_id: volume_id.into(),
        journal_id: journal.journal_id,
        next_usn: journal.next_usn,
        ..Default::default()
    };
    let cap = buffer_size.max(ENUM_BUFFER_HEADER + 128);
    let mut buf = vec![0u8; cap];
    let mut start_file_ref: u64 = 0;

    loop {
        let request = build_enum_request(start_file_ref, 0, journal.next_usn);
        let mut returned: u32 = 0;
        let call = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_ENUM_USN_DATA,
                Some(request.as_ptr() as *const _),
                request.len() as u32,
                Some(buf.as_mut_ptr() as *mut _),
                buf.len() as u32,
                Some(&mut returned),
                None,
            )
        };
        if let Err(e) = call {
            let code = unsafe { GetLastError().0 };
            if code == ERROR_HANDLE_EOF.0 {
                debug!("FSCTL_ENUM_USN_DATA EOF");
                break;
            }
            return Err(Error::Index(format!(
                "FSCTL_ENUM_USN_DATA code={} err={:?}",
                code, e
            )));
        }
        if (returned as usize) < ENUM_BUFFER_HEADER {
            break;
        }
        let (next_ref, records) = parse_enum_buffer(&buf[..returned as usize])?;
        let written = apply_baseline_batch(conn, volume_id, &records)?;
        summary.records_seen += records.len();
        summary.entries_written += written;
        summary.batches += 1;
        if next_ref == start_file_ref {
            debug!(start_file_ref, "FSCTL_ENUM_USN_DATA ilerleme durdu");
            break;
        }
        start_file_ref = next_ref;
    }

    save_watermark(
        conn,
        volume_id,
        Watermark {
            next_usn: journal.next_usn,
            journal_id: journal.journal_id,
        },
    )?;
    info!(
        volume = volume_id,
        records = summary.records_seen,
        batches = summary.batches,
        "USN baseline complete"
    );
    Ok(summary)
}

#[cfg(not(windows))]
pub fn enumerate_volume_baseline(
    _volume_id: &str,
    _conn: &mut Connection,
    _buffer_size: usize,
) -> Result<BaselineSummary> {
    Err(Error::Index(
        "USN baseline walker is only supported on Windows targets".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::persist::{load_entry, load_watermark, save_entries};
    use crate::index::usn::{build_v2_record, USN_REASON_CLOSE, USN_REASON_FILE_CREATE};
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

    /// Test fixture'ı için tam bir FSCTL_ENUM_USN_DATA çıktı buffer'ı kur.
    fn build_enum_buffer(next_ref: u64, records_raw: &[Vec<u8>]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&next_ref.to_le_bytes());
        for r in records_raw {
            buf.extend_from_slice(r);
        }
        buf
    }

    fn build_journal_buffer(jid: i64, first: i64, next: i64, max: i64) -> Vec<u8> {
        let mut buf = vec![0u8; JOURNAL_DATA_V0_SIZE];
        buf[0..8].copy_from_slice(&jid.to_le_bytes());
        buf[8..16].copy_from_slice(&first.to_le_bytes());
        buf[16..24].copy_from_slice(&next.to_le_bytes());
        buf[24..32].copy_from_slice(&first.to_le_bytes()); // lowest_valid_usn
        buf[32..40].copy_from_slice(&max.to_le_bytes());
        buf[40..48].copy_from_slice(&(32_i64 << 20).to_le_bytes()); // maximum_size 32 MB
        buf[48..56].copy_from_slice(&(8_i64 << 20).to_le_bytes()); // allocation_delta 8 MB
        buf
    }

    #[test]
    fn parse_journal_data_full_roundtrip() {
        let buf = build_journal_buffer(0xDEAD_BEEF, 1_000, 5_000, i64::MAX / 2);
        let j = parse_journal_data(&buf).unwrap();
        assert_eq!(j.journal_id, 0xDEAD_BEEF);
        assert_eq!(j.first_usn, 1_000);
        assert_eq!(j.next_usn, 5_000);
        assert_eq!(j.lowest_valid_usn, 1_000);
        assert_eq!(j.max_usn, i64::MAX / 2);
        assert_eq!(j.maximum_size, 32 << 20);
        assert_eq!(j.allocation_delta, 8 << 20);
    }

    #[test]
    fn parse_journal_data_short_buffer_errors() {
        let short = vec![0u8; 10];
        let err = parse_journal_data(&short).unwrap_err();
        assert!(matches!(err, Error::Index(_)));
    }

    #[test]
    fn build_enum_request_layout() {
        let req = build_enum_request(0x1234_5678_9ABC, 0, i64::MAX);
        assert_eq!(req.len(), ENUM_REQUEST_V0_SIZE);
        let start = read_le_u64(&req, 0);
        assert_eq!(start, 0x1234_5678_9ABC);
        let low = read_le_i64(&req, 8);
        assert_eq!(low, 0);
        let high = read_le_i64(&req, 16);
        assert_eq!(high, i64::MAX);
    }

    #[test]
    fn parse_enum_buffer_header_only_no_records() {
        let buf = build_enum_buffer(42_u64, &[]);
        let (next_ref, recs) = parse_enum_buffer(&buf).unwrap();
        assert_eq!(next_ref, 42);
        assert!(recs.is_empty());
    }

    #[test]
    fn parse_enum_buffer_three_records() {
        let raws = vec![
            build_v2_record(
                11,
                5,
                100,
                USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
                0,
                "a.txt",
            ),
            build_v2_record(
                12,
                5,
                101,
                USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
                0,
                "b.log",
            ),
            build_v2_record(
                13,
                5,
                102,
                USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
                crate::index::usn::FILE_ATTRIBUTE_DIRECTORY,
                "subdir",
            ),
        ];
        let buf = build_enum_buffer(13, &raws);
        let (next_ref, recs) = parse_enum_buffer(&buf).unwrap();
        assert_eq!(next_ref, 13);
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].name, "a.txt");
        assert_eq!(recs[1].name, "b.log");
        assert!(recs[2].is_directory());
    }

    #[test]
    fn parse_enum_buffer_short_header_errors() {
        let buf = vec![0u8; 4];
        let err = parse_enum_buffer(&buf).unwrap_err();
        assert!(matches!(err, Error::Index(_)));
    }

    #[test]
    fn apply_baseline_batch_writes_entries() {
        let mut conn = open_test_db();
        let v = r"\\.\C:";
        let raws = vec![
            build_v2_record(
                21,
                5,
                200,
                USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
                0,
                "x.dat",
            ),
            build_v2_record(
                22,
                5,
                201,
                USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
                0,
                "y.dat",
            ),
        ];
        let buf = build_enum_buffer(22, &raws);
        let (_, recs) = parse_enum_buffer(&buf).unwrap();
        let n = apply_baseline_batch(&mut conn, v, &recs).unwrap();
        assert_eq!(n, 2);
        let x = load_entry(&conn, v, 21).unwrap().unwrap();
        assert_eq!(x.name, "x.dat");
        let y = load_entry(&conn, v, 22).unwrap().unwrap();
        assert_eq!(y.name, "y.dat");
    }

    #[test]
    fn apply_baseline_batch_empty_no_op() {
        let mut conn = open_test_db();
        let n = apply_baseline_batch(&mut conn, r"\\.\C:", &[]).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn baseline_then_delta_idempotent_same_file_ref() {
        // Race senaryosu: baseline bir kaydı yazdı, watcher delta'da aynı
        // file_ref için bir güncelleme geldi → save_entries ON CONFLICT
        // upsert ile son yazan kazanır; ne çoğaltma ne hata.
        let mut conn = open_test_db();
        let v = r"\\.\C:";
        let raws = vec![build_v2_record(
            30,
            5,
            300,
            USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
            0,
            "race.txt",
        )];
        let buf = build_enum_buffer(30, &raws);
        let (_, recs) = parse_enum_buffer(&buf).unwrap();
        apply_baseline_batch(&mut conn, v, &recs).unwrap();

        // Watcher delta yolu: aynı file_ref, isim güncellendi.
        let delta_rec = crate::index::usn::UsnRecord {
            file_ref: 30,
            parent_ref: 5,
            usn_id: 305,
            reason: crate::index::usn::USN_REASON_RENAME_NEW_NAME,
            attributes: 0,
            name: "renamed.txt".into(),
        };
        crate::index::apply_delta(&mut conn, v, &[delta_rec]).unwrap();

        // Tek satır, son isim ile.
        let row = load_entry(&conn, v, 30).unwrap().unwrap();
        assert_eq!(row.name, "renamed.txt");
        assert_eq!(row.usn_id, 305);
        let count = crate::index::count_entries(&conn, v).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn records_to_entries_preserves_fields() {
        let r = UsnRecord {
            file_ref: 0x12345,
            parent_ref: 5,
            usn_id: 999,
            reason: USN_REASON_FILE_CREATE,
            attributes: crate::index::usn::FILE_ATTRIBUTE_DIRECTORY,
            name: "Documents".into(),
        };
        let entries = records_to_entries(&[r], r"\\.\C:", 1_700_000_000);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.volume_id, r"\\.\C:");
        assert_eq!(e.file_ref, 0x12345);
        assert_eq!(e.parent_ref, 5);
        assert_eq!(e.name, "Documents");
        assert_eq!(e.usn_id, 999);
        assert_eq!(e.last_seen_unix, 1_700_000_000);
        assert_eq!(e.attrs, crate::index::usn::FILE_ATTRIBUTE_DIRECTORY as i64);
    }

    #[test]
    fn baseline_after_existing_entries_upserts() {
        // Önceden DB'de kayıt var → baseline çalışınca aynı file_ref'i
        // ON CONFLICT upsert ile tazeler, satır sayısı sabit kalır.
        let mut conn = open_test_db();
        let v = r"\\.\C:";
        save_entries(
            &mut conn,
            &[IndexEntry {
                volume_id: v.into(),
                file_ref: 50,
                parent_ref: 5,
                name: "stale.txt".into(),
                usn_id: 1,
                last_seen_unix: 1,
                attrs: 0,
            }],
        )
        .unwrap();

        let raws = vec![build_v2_record(
            50,
            5,
            400,
            USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
            0,
            "fresh.txt",
        )];
        let buf = build_enum_buffer(50, &raws);
        let (_, recs) = parse_enum_buffer(&buf).unwrap();
        apply_baseline_batch(&mut conn, v, &recs).unwrap();

        let row = load_entry(&conn, v, 50).unwrap().unwrap();
        assert_eq!(row.name, "fresh.txt");
        assert_eq!(crate::index::count_entries(&conn, v).unwrap(), 1);
    }

    #[test]
    fn save_watermark_after_baseline_records_journal_state() {
        let conn = open_test_db();
        let v = r"\\.\C:";
        super::save_watermark(
            &conn,
            v,
            Watermark {
                next_usn: 12_345,
                journal_id: 0x42,
            },
        )
        .unwrap();
        let w = load_watermark(&conn, v).unwrap().unwrap();
        assert_eq!(w.next_usn, 12_345);
        assert_eq!(w.journal_id, 0x42);
    }
}
