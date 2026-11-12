// SPDX-License-Identifier: GPL-3.0-or-later
//
// USN_RECORD_V2 parser + FSCTL bağlantıları — Sprint 3.8.
//
// Discovery Log #005 — Bölüm 5.6: USN Journal modeli "Everything"in temel
// mekanizmasıdır. NTFS USN_RECORD_V2 binary formatı:
//
//   DWORD  RecordLength;
//   WORD   MajorVersion;       // 2
//   WORD   MinorVersion;       // 0
//   DWORDLONG FileReferenceNumber;
//   DWORDLONG ParentFileReferenceNumber;
//   USN     Usn;               // i64
//   LARGE_INTEGER TimeStamp;   // FILETIME 100ns since 1601
//   DWORD   Reason;
//   DWORD   SourceInfo;
//   DWORD   SecurityId;
//   DWORD   FileAttributes;
//   WORD    FileNameLength;    // bytes (NOT chars)
//   WORD    FileNameOffset;
//   WCHAR   FileName[1];       // UTF-16 LE, NOT null-terminated
//
// NTFS file reference number 64-bit, alt 48 bit segment numarası (MFT
// record index), üst 16 bit sequence numarası. Persistent index için
// yalnız segment numarasını saklıyoruz (rename sırasında sequence değişir
// ama segment kalır — kayıt sürekliliği için tercih edilir).
//
// FSCTL constants `windows-rs` 0.61'in `Win32::System::Ioctl`'ünde:
//   * FSCTL_ENUM_USN_DATA       = 0x000900B3 (baseline enumerate)
//   * FSCTL_READ_USN_JOURNAL    = 0x000900BB (delta read)
//   * FSCTL_QUERY_USN_JOURNAL   = 0x000900F4 (journal_id okumak için)
// Windows 11'de bu sembolik isimler `windows` crate'inde değil; hand-roll
// edilir (FSCTL kodları kararlı, Win2K'dan beri değişmedi).
//
// USN reason bayrakları (`winioctl.h`):
//   USN_REASON_FILE_CREATE     = 0x00000100
//   USN_REASON_FILE_DELETE     = 0x00000200
//   USN_REASON_DATA_OVERWRITE  = 0x00000001
//   USN_REASON_RENAME_NEW_NAME = 0x00002000
//   USN_REASON_RENAME_OLD_NAME = 0x00001000

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// USN reason mask — delta uygularken yalnız bu olaylara tepki veriyoruz
/// (data değişim, oluşturma, silme, ad değişimi). Discovery #005 — diğer
/// reason'lar (REASON_SECURITY_CHANGE, REASON_BASIC_INFO_CHANGE vb.)
/// dosya isim/varlık bilgisini değiştirmez, görmezden gelinir.
pub const USN_REASON_MASK: u32 = USN_REASON_FILE_CREATE
    | USN_REASON_FILE_DELETE
    | USN_REASON_DATA_OVERWRITE
    | USN_REASON_RENAME_NEW_NAME
    | USN_REASON_RENAME_OLD_NAME
    | USN_REASON_CLOSE;

pub const USN_REASON_DATA_OVERWRITE: u32 = 0x0000_0001;
pub const USN_REASON_FILE_CREATE: u32 = 0x0000_0100;
pub const USN_REASON_FILE_DELETE: u32 = 0x0000_0200;
pub const USN_REASON_RENAME_OLD_NAME: u32 = 0x0000_1000;
pub const USN_REASON_RENAME_NEW_NAME: u32 = 0x0000_2000;
pub const USN_REASON_CLOSE: u32 = 0x8000_0000;

/// FSCTL_ENUM_USN_DATA — baseline enumerate.
pub const FSCTL_ENUM_USN_DATA: u32 = 0x0009_00B3;
/// FSCTL_READ_USN_JOURNAL — delta okuma.
pub const FSCTL_READ_USN_JOURNAL: u32 = 0x0009_00BB;
/// FSCTL_QUERY_USN_JOURNAL — journal_id sorgu.
pub const FSCTL_QUERY_USN_JOURNAL: u32 = 0x0009_00F4;

/// USN_RECORD_V2 sabit alan boyutu (FileName hariç) = 60 bayt.
pub const USN_RECORD_V2_HEADER_SIZE: usize = 60;

/// FILE_ATTRIBUTE_DIRECTORY (winnt.h).
pub const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;

/// Win32 ERROR_JOURNAL_NOT_ACTIVE — USN journal kapalı volume için
/// `DeviceIoControl` bu hatayı verir. Caller wraparound veya re-create
/// kararı verir (D-Space: watcher başlatılmaz).
pub const ERROR_JOURNAL_NOT_ACTIVE: u32 = 1_179;
/// Win32 ERROR_JOURNAL_DELETE_IN_PROGRESS.
pub const ERROR_JOURNAL_DELETE_IN_PROGRESS: u32 = 1_178;
/// Win32 ERROR_JOURNAL_ENTRY_DELETED — istenen USN penceresi düşmüş
/// (truncation), wraparound → full re-enumerate.
pub const ERROR_JOURNAL_ENTRY_DELETED: u32 = 1_181;

/// Slice'tan sabit boyutlu array'e güvenli kopya — `from_le_bytes`
/// helper'ları. Caller bounds'ı kontrol eder; `copy_from_slice` length
/// mismatch'inde panic eder ama yapı kanıtlanabilir şekilde güvenli.
/// `.try_into().unwrap()` pattern'ine göre clippy ve okur için temiz.
#[inline]
pub(super) fn read_le_i64(buf: &[u8], offset: usize) -> i64 {
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&buf[offset..offset + 8]);
    i64::from_le_bytes(arr)
}

#[inline]
pub(super) fn read_le_u64(buf: &[u8], offset: usize) -> u64 {
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&buf[offset..offset + 8]);
    u64::from_le_bytes(arr)
}

#[inline]
pub(super) fn read_le_u32(buf: &[u8], offset: usize) -> u32 {
    let mut arr = [0u8; 4];
    arr.copy_from_slice(&buf[offset..offset + 4]);
    u32::from_le_bytes(arr)
}

#[inline]
pub(super) fn read_le_u16(buf: &[u8], offset: usize) -> u16 {
    let mut arr = [0u8; 2];
    arr.copy_from_slice(&buf[offset..offset + 2]);
    u16::from_le_bytes(arr)
}

/// `READ_USN_JOURNAL_DATA_V0` input boyutu — 6 alan, 40 bayt:
///   USN StartUsn (8) + DWORD ReasonMask (4) + DWORD ReturnOnlyOnClose (4)
///   + DWORDLONG Timeout (8) + DWORDLONG BytesToWaitFor (8)
///   + DWORDLONG UsnJournalID (8) = 40
pub const READ_USN_JOURNAL_DATA_V0_SIZE: usize = 40;

/// `READ_USN_JOURNAL_DATA_V0` input bloğunu inşa eder. Gemini review 2.4 —
/// event-driven watcher için: `timeout_100ns > 0` + `bytes_to_wait > 0`
/// ile kernel IOCTL'i yeni USN entry beklemek üzere bloklar; polling
/// gerek yok, batarya dostu.
///
/// `start_usn` = mevcut watermark next_usn.
/// `reason_mask` = `USN_REASON_MASK` (filtre).
/// `return_only_on_close` = 0 (her event'i yakala).
/// `timeout_100ns` = 0 → no wait (eski polling); >0 → blocking.
///   Birim 100 ns; 1 dk = 600_000_000.
/// `bytes_to_wait_for` = 1 → herhangi bir kayıt gelince çık.
/// `journal_id` = mevcut watermark journal_id.
pub fn build_read_journal_request(
    start_usn: i64,
    reason_mask: u32,
    return_only_on_close: u32,
    timeout_100ns: u64,
    bytes_to_wait_for: u64,
    journal_id: i64,
) -> [u8; READ_USN_JOURNAL_DATA_V0_SIZE] {
    let mut buf = [0u8; READ_USN_JOURNAL_DATA_V0_SIZE];
    buf[0..8].copy_from_slice(&start_usn.to_le_bytes());
    buf[8..12].copy_from_slice(&reason_mask.to_le_bytes());
    buf[12..16].copy_from_slice(&return_only_on_close.to_le_bytes());
    buf[16..24].copy_from_slice(&timeout_100ns.to_le_bytes());
    buf[24..32].copy_from_slice(&bytes_to_wait_for.to_le_bytes());
    buf[32..40].copy_from_slice(&journal_id.to_le_bytes());
    buf
}

/// USN_RECORD_V2 binary kaydının parse edilmiş hali. Sadece bizi
/// ilgilendiren alanlar tutuluyor — TimeStamp/SecurityId atılıyor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsnRecord {
    pub file_ref: i64,
    pub parent_ref: i64,
    pub usn_id: i64,
    pub reason: u32,
    pub attributes: u32,
    pub name: String,
}

impl UsnRecord {
    pub fn is_directory(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
    }
}

/// NTFS file reference number → persistent segment kısmı (alt 48 bit).
/// Üst 16 bit sequence number; rename'de değişir, persist için tercih
/// edilmez.
#[inline]
pub fn segment_of(file_ref: u64) -> i64 {
    (file_ref & 0x0000_FFFF_FFFF_FFFF) as i64
}

/// Tek bir USN_RECORD_V2'yi binary'den parse eder. `buf` kayıt başlangıcı.
/// Dönen değer: (kayıt, kayıt boyutu — caller advance için).
pub fn parse_record_v2(buf: &[u8]) -> Result<(UsnRecord, usize)> {
    if buf.len() < USN_RECORD_V2_HEADER_SIZE {
        return Err(Error::Index(format!(
            "USN_RECORD_V2 buffer too short: {} < {}",
            buf.len(),
            USN_RECORD_V2_HEADER_SIZE
        )));
    }
    let record_length = read_le_u32(buf, 0) as usize;
    let major = read_le_u16(buf, 4);
    let _minor = read_le_u16(buf, 6);
    if major != 2 {
        return Err(Error::Index(format!(
            "only USN_RECORD_V2 is supported, got MajorVersion={}",
            major
        )));
    }
    if record_length < USN_RECORD_V2_HEADER_SIZE || record_length > buf.len() {
        return Err(Error::Index(format!(
            "USN_RECORD_V2 record_length invalid: {} (buf={})",
            record_length,
            buf.len()
        )));
    }
    let file_ref_raw = read_le_u64(buf, 8);
    let parent_ref_raw = read_le_u64(buf, 16);
    let usn_id = read_le_i64(buf, 24);
    // 32..40 TimeStamp — atlanıyor.
    let reason = read_le_u32(buf, 40);
    // 44..48 SourceInfo, 48..52 SecurityId — atlanıyor.
    let attributes = read_le_u32(buf, 52);
    let name_length = read_le_u16(buf, 56) as usize;
    let name_offset = read_le_u16(buf, 58) as usize;

    if name_offset + name_length > record_length {
        return Err(Error::Index(format!(
            "USN_RECORD_V2 name range outside the record: offset={} len={} record={}",
            name_offset, name_length, record_length
        )));
    }
    if name_length % 2 != 0 {
        return Err(Error::Index(format!(
            "USN_RECORD_V2 name length is odd (UTF-16 LE is 2-byte): {}",
            name_length
        )));
    }
    let name_bytes = &buf[name_offset..name_offset + name_length];
    let utf16: Vec<u16> = name_bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let name = String::from_utf16(&utf16)
        .map_err(|e| Error::Index(format!("UTF-16 decode error: {}", e)))?;

    Ok((
        UsnRecord {
            file_ref: segment_of(file_ref_raw),
            parent_ref: segment_of(parent_ref_raw),
            usn_id,
            reason,
            attributes,
            name,
        },
        record_length,
    ))
}

/// Bir buffer içindeki tüm USN_RECORD_V2 kayıtlarını dizi olarak çıkar.
/// `offset`: ilk record başlangıcı (FSCTL output buffer'da ilk 8 bayt
/// `NextUsn` üzerinden gelir; caller doğru offset'i geçer).
pub fn parse_records(buf: &[u8], mut offset: usize) -> Result<Vec<UsnRecord>> {
    let mut out = Vec::new();
    while offset + USN_RECORD_V2_HEADER_SIZE <= buf.len() {
        let (record, len) = parse_record_v2(&buf[offset..])?;
        if len == 0 {
            break;
        }
        out.push(record);
        offset += len;
    }
    Ok(out)
}

/// USN journal okuma sonucu — caller watermark'ı bu `next_usn` ile günceller.
#[derive(Debug, Clone)]
pub struct JournalReadResult {
    pub records: Vec<UsnRecord>,
    pub next_usn: i64,
    pub elapsed_ms: u64,
}

/// Gemini review 2.4 — event-driven USN journal okuma. Pozitif
/// `timeout_100ns` ve >= 1 `bytes_to_wait_for` ile kernel IOCTL
/// bekleyiş'i yapar; yeni USN kaydı gelene kadar **CPU kullanmadan**
/// bloklar. 5 sn polling tamamen gereksizleşir.
///
/// Kullanım: caller volume_id (`\\.\C:`), mevcut watermark (next_usn,
/// journal_id), timeout (örn. 60 sn = 600_000_000) verir; fonksiyon
/// blocking DeviceIoControl çağırır; dönen records `apply_delta` ile
/// DB'ye yazılır, `next_usn` yeni watermark olur.
///
/// Cancellation: timeout süresince blok eder. Hızlı iptal için
/// `CancelIoEx` kullanılabilir (ayrı thread'den); minimum viable
/// sürümünde timeout süresince dur.
///
/// Admin gerekir (`\\.\X:` raw volume open).
#[cfg(windows)]
pub fn read_journal_blocking(
    volume_id: &str,
    next_usn: i64,
    journal_id: i64,
    timeout_100ns: u64,
    buffer_size: usize,
) -> Result<JournalReadResult> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::time::Instant;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;

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

    let started = Instant::now();
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
    .map_err(|e| Error::Index(format!("read_journal volume open ({}): {:?}", volume_id, e)))?;
    let _guard = HandleGuard(handle);

    let request = build_read_journal_request(
        next_usn,
        USN_REASON_MASK,
        0,             // ReturnOnlyOnClose = 0 (her event)
        timeout_100ns, // Timeout: 0 = no-wait, >0 = blocking
        1,             // BytesToWaitFor: 1 → herhangi bir kayıt gelince çık
        journal_id,
    );

    let cap = buffer_size.max(8 + USN_RECORD_V2_HEADER_SIZE);
    let mut buf = vec![0u8; cap];
    let mut returned: u32 = 0;
    unsafe {
        DeviceIoControl(
            handle,
            FSCTL_READ_USN_JOURNAL,
            Some(request.as_ptr() as *const _),
            request.len() as u32,
            Some(buf.as_mut_ptr() as *mut _),
            buf.len() as u32,
            Some(&mut returned),
            None,
        )
    }
    .map_err(|e| Error::Index(format!("FSCTL_READ_USN_JOURNAL: {:?}", e)))?;

    if (returned as usize) < 8 {
        return Ok(JournalReadResult {
            records: Vec::new(),
            next_usn,
            elapsed_ms: started.elapsed().as_millis() as u64,
        });
    }

    let new_next_usn = read_le_i64(&buf, 0);
    let records = parse_records(&buf[..returned as usize], 8)?;
    Ok(JournalReadResult {
        records,
        next_usn: new_next_usn,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(not(windows))]
pub fn read_journal_blocking(
    _volume_id: &str,
    _next_usn: i64,
    _journal_id: i64,
    _timeout_100ns: u64,
    _buffer_size: usize,
) -> Result<JournalReadResult> {
    Err(Error::Index(
        "read_journal_blocking is only supported on Windows targets".into(),
    ))
}

/// Test fixture'larında V2 record bayt buffer'ı kurmak için yardımcı.
#[cfg(test)]
pub(crate) fn build_v2_record(
    file_ref: u64,
    parent_ref: u64,
    usn_id: i64,
    reason: u32,
    attributes: u32,
    name: &str,
) -> Vec<u8> {
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let name_bytes: Vec<u8> = utf16.iter().flat_map(|c| c.to_le_bytes()).collect();
    let name_offset = USN_RECORD_V2_HEADER_SIZE;
    let name_len_bytes = name_bytes.len();
    // 8-bayt hizalanmış kayıt — Windows USN dökümünden: kayıt uzunluğu
    // 8-bayt katlarına yuvarlanır.
    let unaligned = USN_RECORD_V2_HEADER_SIZE + name_len_bytes;
    let record_length = unaligned.div_ceil(8) * 8;
    let mut buf = vec![0u8; record_length];
    buf[0..4].copy_from_slice(&(record_length as u32).to_le_bytes());
    buf[4..6].copy_from_slice(&2u16.to_le_bytes());
    buf[6..8].copy_from_slice(&0u16.to_le_bytes());
    buf[8..16].copy_from_slice(&file_ref.to_le_bytes());
    buf[16..24].copy_from_slice(&parent_ref.to_le_bytes());
    buf[24..32].copy_from_slice(&usn_id.to_le_bytes());
    // 32..40 timestamp = 0
    buf[40..44].copy_from_slice(&reason.to_le_bytes());
    // 44..48 sourceinfo, 48..52 securityid = 0
    buf[52..56].copy_from_slice(&attributes.to_le_bytes());
    buf[56..58].copy_from_slice(&(name_len_bytes as u16).to_le_bytes());
    buf[58..60].copy_from_slice(&(name_offset as u16).to_le_bytes());
    buf[name_offset..name_offset + name_len_bytes].copy_from_slice(&name_bytes);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_record_v2_roundtrip() {
        // Klasik bir dosya oluşturma kaydı: report.docx, parent 5
        let buf = build_v2_record(
            0x0000_0000_0001_2345_u64, // file_ref segment 0x12345
            0x0000_0000_0000_0005_u64, // parent_ref segment 5
            42,
            USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
            0,
            "report.docx",
        );
        let (rec, sz) = parse_record_v2(&buf).expect("parse");
        assert_eq!(rec.file_ref, 0x12345);
        assert_eq!(rec.parent_ref, 5);
        assert_eq!(rec.usn_id, 42);
        assert_eq!(rec.name, "report.docx");
        assert_eq!(rec.reason & USN_REASON_FILE_CREATE, USN_REASON_FILE_CREATE);
        assert!(!rec.is_directory());
        // Kayıt boyutu 8-bayt hizalanmış olmalı
        assert!(sz % 8 == 0);
        assert!(sz >= USN_RECORD_V2_HEADER_SIZE + "report.docx".len() * 2);
    }

    #[test]
    fn parse_record_v2_utf16_unicode_name() {
        let buf = build_v2_record(7, 5, 1, USN_REASON_FILE_CREATE, 0, "rapor_çğüöş.txt");
        let (rec, _) = parse_record_v2(&buf).unwrap();
        assert_eq!(rec.name, "rapor_çğüöş.txt");
    }

    #[test]
    fn parse_record_v2_directory_attribute() {
        let buf = build_v2_record(
            10,
            5,
            1,
            USN_REASON_FILE_CREATE,
            FILE_ATTRIBUTE_DIRECTORY,
            "Documents",
        );
        let (rec, _) = parse_record_v2(&buf).unwrap();
        assert!(rec.is_directory());
        assert_eq!(
            rec.attributes & FILE_ATTRIBUTE_DIRECTORY,
            FILE_ATTRIBUTE_DIRECTORY
        );
    }

    #[test]
    fn parse_record_v2_short_buffer_errors() {
        let short = vec![0u8; 10];
        let err = parse_record_v2(&short).unwrap_err();
        assert!(matches!(err, Error::Index(_)));
    }

    #[test]
    fn parse_record_v2_wrong_major_version_errors() {
        let mut buf = build_v2_record(1, 2, 3, 0, 0, "x.txt");
        buf[4..6].copy_from_slice(&3u16.to_le_bytes()); // major=3
        let err = parse_record_v2(&buf).unwrap_err();
        match err {
            Error::Index(m) => assert!(m.contains("MajorVersion")),
            _ => panic!("Index variant beklendi"),
        }
    }

    #[test]
    fn parse_records_handles_multiple() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_v2_record(
            1,
            5,
            10,
            USN_REASON_FILE_CREATE,
            0,
            "a.txt",
        ));
        buf.extend_from_slice(&build_v2_record(
            2,
            5,
            11,
            USN_REASON_FILE_CREATE,
            0,
            "b.txt",
        ));
        buf.extend_from_slice(&build_v2_record(
            3,
            5,
            12,
            USN_REASON_FILE_CREATE,
            FILE_ATTRIBUTE_DIRECTORY,
            "subdir",
        ));
        let recs = parse_records(&buf, 0).unwrap();
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].name, "a.txt");
        assert_eq!(recs[1].name, "b.txt");
        assert!(recs[2].is_directory());
    }

    #[test]
    fn segment_strips_sequence_number() {
        // Üst 16 bit sequence, alt 48 bit segment
        let raw: u64 = 0x0012_3456_789A_BCDE;
        let seg = segment_of(raw);
        assert_eq!(seg, 0x0000_3456_789A_BCDE_i64);
    }

    #[test]
    fn build_read_journal_request_layout() {
        let req = build_read_journal_request(
            12_345_i64,      // StartUsn
            USN_REASON_MASK, // ReasonMask
            0,               // ReturnOnlyOnClose
            600_000_000_u64, // Timeout (60 sn, 100ns birim)
            1_u64,           // BytesToWaitFor
            0x42_i64,        // UsnJournalID
        );
        assert_eq!(req.len(), READ_USN_JOURNAL_DATA_V0_SIZE);
        assert_eq!(req.len(), 40);
        // Field offset doğrulamaları
        let start_usn = read_le_i64(&req, 0);
        assert_eq!(start_usn, 12_345);
        let mask = read_le_u32(&req, 8);
        assert_eq!(mask, USN_REASON_MASK);
        let timeout = read_le_u64(&req, 16);
        assert_eq!(timeout, 600_000_000);
        let journal_id = read_le_i64(&req, 32);
        assert_eq!(journal_id, 0x42);
    }

    #[test]
    fn reason_mask_includes_expected_events() {
        // Compile-time'da bit'lerin maskeye dahil olduğunu doğrula —
        // constant-fold edilen assertion'ları clippy const block ister.
        const _CHECK_CREATE: () = assert!(USN_REASON_MASK & USN_REASON_FILE_CREATE != 0);
        const _CHECK_DELETE: () = assert!(USN_REASON_MASK & USN_REASON_FILE_DELETE != 0);
        const _CHECK_RENAME_NEW: () = assert!(USN_REASON_MASK & USN_REASON_RENAME_NEW_NAME != 0);
        const _CHECK_RENAME_OLD: () = assert!(USN_REASON_MASK & USN_REASON_RENAME_OLD_NAME != 0);
        const _CHECK_OVERWRITE: () = assert!(USN_REASON_MASK & USN_REASON_DATA_OVERWRITE != 0);
    }
}
