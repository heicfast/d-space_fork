// SPDX-License-Identifier: GPL-3.0-or-later
//
// MFT full walker — Bölüm 5.1, Bölüm 4.3 (Adım 2: Rust MFT'yi sıralı okur).
//
// Bu sprint hedefi: tüm MFT record'larını sırayla gezip in-use / dir / file
// sayımı ve örnek dosya adları çıkarma. Hiyerarşi kurma ve `Arc<ScanTree>`
// build sonraki sprint (Bölüm 4.3 Adım 3 + rayon paralel aggregation).
//
// Performans notu (Bölüm 5.4): 1TB SSD < 5sn hedefine ulaşmak için
// gerçek production'da bulk MFT read (raw IOCTL) ve rayon paralel parse
// gerekecek. Bu v0.1 walker tek thread ve `ntfs::Ntfs::file()` per-record
// çağırır — yeterli, sonraki turda optimize.

use crate::error::{Error, Result};
use ntfs::structured_values::NtfsFileNamespace;
use ntfs::{KnownNtfsFileRecordNumber, Ntfs, NtfsFile, NtfsFileFlags};
use serde::Serialize;
use std::fs::File;
use std::time::Instant;
use tracing::{debug, info};

const SAMPLE_NAME_LIMIT: usize = 25;
const MAX_RECORD_HARD_LIMIT: u64 = 50_000_000; // güvenlik tavanı

/// NTFS FILETIME (1601-01-01 epoch, 100-ns intervals) → Unix saniye.
/// ntfs crate `NtfsTime::nt_timestamp()` u64 değerini bu fonksiyona besler.
/// 0 girdisi 0 döner (epoch placeholder).
pub(crate) fn nt_to_unix(nt_ts: u64) -> i64 {
    if nt_ts == 0 {
        return 0;
    }
    ((nt_ts / 10_000_000) as i64) - 11_644_473_600i64
}

/// Tek bir MFT kaydının özet metadata'sı — hiyerarşi + Timeline (Bölüm 9.1
/// mod 4/4) için yeterli.
#[derive(Debug, Clone, Serialize)]
pub struct RawMftEntry {
    pub record_no: u64,
    pub parent_record_no: u64,
    pub name: String,
    pub data_size: u64,
    pub is_dir: bool,
    /// `$STANDARD_INFORMATION.modification_time` (Windows yazıda güncelliyor).
    /// Fallback `$FILE_NAME` lag yapabilir — StandardInformation canonical.
    pub modified_unix: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MftEntries {
    pub drive: String,
    pub volume_path: String,
    pub entries: Vec<RawMftEntry>,
    pub skipped_errors: u64,
    pub elapsed_ms: u64,
}

/// Bölüm 9.6.5 — tarama ilerlemesi event'i. Backend her N entry'de bir emit eder.
/// `partial_tree`: Sprint 3.7 — her 10k entry'de canlı sunburst için root +
/// 2 seviye + top-200 düğüm snapshot. Diğer event'lerde `None` (payload küçük).
#[derive(Debug, Clone, Serialize)]
pub struct ScanProgress {
    pub phase: &'static str,
    pub visited: u64,
    pub total_estimate: u64,
    pub in_use: u64,
    pub last_name: String,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_tree: Option<Vec<crate::scan::tree::PartialNode>>,
}

/// `Fn(&ScanProgress)` callback alias — heap-allocated closure'lardan kaçınmak için.
pub type ProgressCb<'a> = &'a (dyn Fn(&ScanProgress) + Send + Sync);

const MFT_PROGRESS_INTERVAL: u64 = 5000;
/// Sprint 3.7 — canlı sunburst snapshot başlangıç aralığı. Gemini review
/// (partial-tree O(N²)): sabit 10k aralıkla full-rebuild, büyük volumelerde
/// karesel maliyet üretiyordu. Artık geometrik büyüme (×4) kullanılır —
/// 10k, 40k, 160k, 640k, 2.5M, sonra her 2.5M'de bir. Toplam rebuild
/// sayısı ~O(log N), her biri O(N) → maliyet ~O(N log N) sınırlı kalır.
pub(crate) const PARTIAL_TREE_INTERVAL: u64 = 10_000;
/// Geometrik büyüme katsayısı (aralık × 4).
const PARTIAL_TREE_GROWTH: u64 = 4;
/// Geometrik aralığın tavanı — çok büyük taramalarda bile makul sıklık.
const PARTIAL_TREE_MAX_INTERVAL: u64 = 2_500_000;
/// Sprint 3.7 — canlı sunburst max derinlik (root + 2 seviye).
pub(crate) const PARTIAL_TREE_MAX_DEPTH: usize = 2;
/// Sprint 3.7 — canlı sunburst top-N düğüm (root + 199).
pub(crate) const PARTIAL_TREE_TOP_N: usize = 200;

/// Bir sonraki partial-tree snapshot eşiğini geometrik olarak ilerletir.
#[inline]
pub(crate) fn advance_partial_threshold(current: u64) -> u64 {
    (current.saturating_mul(PARTIAL_TREE_GROWTH)).min(PARTIAL_TREE_MAX_INTERVAL)
}

#[derive(Debug, Clone, Serialize)]
pub struct MftWalkStats {
    pub drive: String,
    pub volume_path: String,
    pub total_records_estimate: u64,
    pub records_visited: u64,
    pub in_use_records: u64,
    pub directory_records: u64,
    pub file_records: u64,
    pub skipped_errors: u64,
    pub bytes_aggregate: u64,
    pub sample_names: Vec<String>,
    pub elapsed_ms: u64,
}

/// "C", "c:", "C:\Users" → r"\\.\C:" — katı doğrulama (UNC reddedilir).
pub(crate) fn normalize_volume_path(drive: &str) -> Result<String> {
    let letter = crate::volume::parse_drive_letter(drive)
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| {
            Error::Scan(format!(
                "Invalid drive parameter: '{}' (the MFT path does not accept UNC)",
                drive
            ))
        })?;
    Ok(format!(r"\\.\{}:", letter))
}

/// MFT'nin kendi `$DATA` boyutundan toplam record sayısını tahmin eder.
pub(crate) fn estimate_record_count<T: std::io::Read + std::io::Seek>(
    ntfs: &Ntfs,
    handle: &mut T,
) -> Result<u64> {
    let mft_file = ntfs
        .file(handle, KnownNtfsFileRecordNumber::MFT as u64)
        .map_err(|e| Error::Scan(format!("cannot read MFT record 0: {:?}", e)))?;

    let data_attr_item = mft_file
        .data(handle, "")
        .ok_or_else(|| Error::Scan("MFT $DATA not found".into()))?
        .map_err(|e| Error::Scan(format!("MFT $DATA error: {:?}", e)))?;

    let attr = data_attr_item
        .to_attribute()
        .map_err(|e| Error::Scan(format!("MFT $DATA attribute: {:?}", e)))?;

    let total_bytes = attr.value_length();
    let record_size = ntfs.file_record_size() as u64;
    if record_size == 0 {
        return Err(Error::Scan("file_record_size = 0".into()));
    }
    Ok(total_bytes / record_size)
}

/// Tek bir `NtfsFile` için isim çıkarmaya çalışır. Win32 namespace tercih
/// edilir (DOS 8.3'ün ikincil görünümlerini atlamak için).
fn extract_name<T: std::io::Read + std::io::Seek>(
    file: &NtfsFile,
    handle: &mut T,
) -> Option<String> {
    // Önce Win32 namespace
    if let Some(Ok(name)) = file.name(handle, Some(NtfsFileNamespace::Win32), None) {
        return Some(name.name().to_string_lossy());
    }
    // Fallback: Win32AndDos (3) — birleşik isim
    if let Some(Ok(name)) = file.name(handle, None, None) {
        // DOS'u atla
        if !matches!(name.namespace(), NtfsFileNamespace::Dos) {
            return Some(name.name().to_string_lossy());
        }
    }
    None
}

/// İsim + parent_ref + data_size + is_dir + mtime tek seferde çıkarır.
/// Win32 namespace tercih edilir. mtime için `$STANDARD_INFORMATION`
/// (Windows yazıda update eder — canonical), başarısızsa `$FILE_NAME` fallback.
pub(crate) fn extract_full<T: std::io::Read + std::io::Seek>(
    file: &NtfsFile,
    handle: &mut T,
) -> Option<(String, u64, u64, bool, i64)> {
    let si_mtime = file
        .info()
        .ok()
        .map(|si| nt_to_unix(si.modification_time().nt_timestamp()));

    if let Some(Ok(fname)) = file.name(handle, Some(NtfsFileNamespace::Win32), None) {
        let mtime =
            si_mtime.unwrap_or_else(|| nt_to_unix(fname.modification_time().nt_timestamp()));
        return Some((
            fname.name().to_string_lossy(),
            fname.parent_directory_reference().file_record_number(),
            fname.data_size(),
            fname.is_directory(),
            mtime,
        ));
    }
    if let Some(Ok(fname)) = file.name(handle, None, None) {
        if !matches!(fname.namespace(), NtfsFileNamespace::Dos) {
            let mtime =
                si_mtime.unwrap_or_else(|| nt_to_unix(fname.modification_time().nt_timestamp()));
            return Some((
                fname.name().to_string_lossy(),
                fname.parent_directory_reference().file_record_number(),
                fname.data_size(),
                fname.is_directory(),
                mtime,
            ));
        }
    }
    None
}

/// Geriye uyumlu — progress callback ve cancel flag olmadan.
pub fn collect_mft_entries(drive: &str) -> Result<MftEntries> {
    collect_mft_entries_with_progress(drive, None, None)
}

/// Sprint 3.7 — canlı sunburst için raw entries'den hafif partial view inşa
/// eder. Gemini review (partial-tree perf): eskiden `entries.to_vec()` + tam
/// `build_tree` (kural eşleştirme dahil) her snapshot'ta O(N) ekstra kopya +
/// 63 kural × N düğüm maliyeti üretiyordu. Artık:
///   * klon YOK — `build_tree_light` borrow ile çalışır,
///   * kural skorlaması YOK — partial view yalnız boyut/isim taşır.
///
/// Geometrik aralık (yukarıda) ile birlikte toplam maliyet ~O(N log N).
pub(crate) fn build_partial_from_raw(
    entries: &[RawMftEntry],
    max_depth: usize,
    top_n: usize,
) -> Vec<crate::scan::tree::PartialNode> {
    let tree = crate::scan::tree::build_tree_light(entries);
    crate::scan::tree::build_partial_view(&tree, max_depth, top_n)
}

/// Tam MFT entry koleksiyonu — `build_tree` için besin kaynağı.
/// System records (0-15) atlanır. Volume root NTFS'te record 5'tir.
/// `progress_cb` her `MFT_PROGRESS_INTERVAL` entry'de bir çağrılır (Bölüm 9.6.5).
pub fn collect_mft_entries_with_progress(
    drive: &str,
    progress_cb: Option<ProgressCb<'_>>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<MftEntries> {
    let start = Instant::now();
    let volume_path = normalize_volume_path(drive)?;

    debug!(volume = %volume_path, "MFT entry collection starting");
    let mut handle = File::open(&volume_path).map_err(|e| {
        Error::Scan(format!(
            "cannot open volume '{}': {} (administrator privileges may be required)",
            volume_path, e
        ))
    })?;

    let ntfs =
        Ntfs::new(&mut handle).map_err(|e| Error::Scan(format!("NTFS parse error: {:?}", e)))?;

    let estimated = estimate_record_count(&ntfs, &mut handle)?;
    let cap = estimated.min(MAX_RECORD_HARD_LIMIT);
    if estimated > MAX_RECORD_HARD_LIMIT {
        tracing::warn!(
            estimated,
            hard_limit = MAX_RECORD_HARD_LIMIT,
            "MFT record estimate exceeded the hard limit — scan truncated at 50M records"
        );
    }
    info!(estimated, cap, "MFT entry koleksiyonu kapasitesi");

    // Gemini review B12 — dev upfront rezervasyon (~GB'lar) yerine makul
    // başlangıç kapasitesi; Vec zaten ihtiyaç oldukça büyür.
    let mut entries: Vec<RawMftEntry> =
        Vec::with_capacity(((cap as usize) / 2).clamp(1024, 1_000_000));
    let mut skipped = 0u64;
    let mut last_name = String::new();
    // Sprint 3.7 — her PARTIAL_TREE_INTERVAL entry'de bir partial tree
    // snapshot üret. Sayım entries'in mutlak boyutu üzerinden — record
    // sayısı değil. Yeni eşik aşıldığında bir kez tetiklenir.
    let mut next_partial_at: u64 = PARTIAL_TREE_INTERVAL;

    // Root directory (record 5) sentetik düğüm olarak eklenmez —
    // collect aşamasında kullanıcı entries'i temiz görür; build_tree
    // gerekirse root sentetik düğüm üretir.
    for record_no in 16..cap {
        // Cancellation check — her MFT_PROGRESS_INTERVAL'da bir yeterli.
        if (record_no - 16) % MFT_PROGRESS_INTERVAL == 0 {
            if let Some(c) = cancel {
                if c.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(Error::Scan("scan-cancelled".into()));
                }
            }
        }
        // Progress event
        if let Some(cb) = progress_cb {
            if (record_no - 16) % MFT_PROGRESS_INTERVAL == 0 {
                let partial = if entries.len() as u64 >= next_partial_at {
                    next_partial_at = advance_partial_threshold(next_partial_at);
                    Some(build_partial_from_raw(
                        &entries,
                        PARTIAL_TREE_MAX_DEPTH,
                        PARTIAL_TREE_TOP_N,
                    ))
                } else {
                    None
                };
                cb(&ScanProgress {
                    phase: "mft_walk",
                    visited: record_no - 16,
                    total_estimate: cap - 16,
                    in_use: entries.len() as u64,
                    last_name: last_name.clone(),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    partial_tree: partial,
                });
            }
        }

        let file = match ntfs.file(&mut handle, record_no) {
            Ok(f) => f,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if !file.flags().contains(NtfsFileFlags::IN_USE) {
            continue;
        }
        if let Some((name, parent, size, is_dir, mtime)) = extract_full(&file, &mut handle) {
            last_name = name.clone();
            entries.push(RawMftEntry {
                record_no,
                parent_record_no: parent,
                name,
                data_size: if is_dir { 0 } else { size },
                is_dir,
                modified_unix: mtime,
            });
        }
    }

    Ok(MftEntries {
        drive: drive.to_string(),
        volume_path,
        entries,
        skipped_errors: skipped,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

/// Tüm MFT record'larını sırayla gezer. İlk 16 system file (record 0-15)
/// hariç tutulur — onlar `$MFT`, `$LogFile`, `$Volume`, `$AttrDef`, `$.`,
/// `$Bitmap`, `$Boot`, `$BadClus`, `$Secure`, `$UpCase`, `$Extend`, vb.
pub fn walk_mft(drive: &str) -> Result<MftWalkStats> {
    let start = Instant::now();
    let volume_path = normalize_volume_path(drive)?;

    debug!(volume = %volume_path, "MFT full walk starting");
    let mut handle = File::open(&volume_path).map_err(|e| {
        Error::Scan(format!(
            "cannot open volume '{}': {} (administrator privileges may be required)",
            volume_path, e
        ))
    })?;

    let ntfs =
        Ntfs::new(&mut handle).map_err(|e| Error::Scan(format!("NTFS parse error: {:?}", e)))?;

    let estimated = estimate_record_count(&ntfs, &mut handle)?;
    let cap = estimated.min(MAX_RECORD_HARD_LIMIT);
    info!(estimated, cap, "MFT record count estimate");

    let mut visited = 0u64;
    let mut in_use = 0u64;
    let mut dirs = 0u64;
    let mut files = 0u64;
    let mut skipped = 0u64;
    let mut total_bytes = 0u64;
    let mut samples: Vec<String> = Vec::with_capacity(SAMPLE_NAME_LIMIT);

    // System records (0-15) atlanır — gerçek kullanıcı içeriği 16'dan başlar.
    for record_no in 16..cap {
        visited += 1;

        let file = match ntfs.file(&mut handle, record_no) {
            Ok(f) => f,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let flags = file.flags();
        if !flags.contains(NtfsFileFlags::IN_USE) {
            continue;
        }
        in_use += 1;

        let is_dir = file.is_directory();
        if is_dir {
            dirs += 1;
        } else {
            files += 1;
            // Resident veya non-resident $DATA boyutu (Bölüm 7 uyumlu boyut).
            // `data_size()` u32 — büyük dosyalar için NtfsFileName.data_size()
            // 64-bit verir; ileride per-file metadata'da o kullanılır.
            total_bytes = total_bytes.saturating_add(file.data_size() as u64);
        }

        if samples.len() < SAMPLE_NAME_LIMIT {
            if let Some(name) = extract_name(&file, &mut handle) {
                samples.push(name);
            }
        }
    }

    let stats = MftWalkStats {
        drive: drive.to_string(),
        volume_path,
        total_records_estimate: estimated,
        records_visited: visited,
        in_use_records: in_use,
        directory_records: dirs,
        file_records: files,
        skipped_errors: skipped,
        bytes_aggregate: total_bytes,
        sample_names: samples,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    info!(
        visited = stats.records_visited,
        in_use = stats.in_use_records,
        dirs = stats.directory_records,
        files = stats.file_records,
        gb = stats.bytes_aggregate / 1_073_741_824,
        skipped = stats.skipped_errors,
        elapsed_ms = stats.elapsed_ms,
        "MFT walk complete"
    );

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_drive_paths() {
        assert_eq!(normalize_volume_path("E").unwrap(), r"\\.\E:");
        assert_eq!(normalize_volume_path("d:").unwrap(), r"\\.\D:");
        assert!(normalize_volume_path("").is_err());
    }
}
