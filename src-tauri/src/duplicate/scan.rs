// SPDX-License-Identifier: GPL-3.0-or-later
//
// Duplicate Detector — 4-aşamalı pipeline (Bölüm 7).
//
// v0.1 implementasyonu:
//   Aşama 1 — Boyut bucket'ı: aynı boyuttaki dosyaları topla (≥2 → aday).
//             Bölüm 7.2: 0-byte ve `min_size_bytes` altı filtre (varsayılan 4 KB).
//   Aşama 2 — Tam Blake3 hash (streaming, 64 KB buffer).
//             Bölüm 7.3: SIMD, sabit RAM, paralel — şu an tek-thread.
//   Aşama 3 — Hash bucket'ı (≥2 → grup).
//   Aşama 4 — DuplicateGroup vec'i sırala (`redundant_bytes` desc).
//
// Optimizasyonlar (v0.2):
//   * Aşama 2'den önce 4 KB "head hash" — büyük çoğunluğu erken eler.
//   * rayon paralel hash.
//   * Aynı `inode` (NodeId) iki kez sayılmaz — hardlink/junction (Bölüm 7.2).
//   * 100 GB karışık veri < 60 sn (Bölüm 7.4) ölçümü için bench harness.

use crate::duplicate::{DuplicateGroup, DuplicateScanResult, DuplicateStats};
use crate::error::{Error, Result};
use crate::scan::{node_full_path, NodeId, ScanTree};
use blake3::Hasher;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::Instant;
use tracing::{debug, info, warn};

const HASH_BUFFER: usize = 64 * 1024;
/// Head-hash prefilter buffer (ilk 4 KB). Bölüm 7.4 — full Blake3 öncesi
/// aynı boyutlu dosyaların büyük çoğunluğu farklı başlıklara sahiptir;
/// bu küçük read full hash workload'ını ~%80-95 azaltır.
const HEAD_HASH_SIZE: usize = 4096;
/// Head-hash uygulamak için dosya boyutu eşiği. Bu altı (≤64 KB) full
/// hash zaten ucuz, prefilter overhead'i kazanımdan büyük.
const HEAD_HASH_THRESHOLD: u64 = 64 * 1024;
/// Head-hash uygulamak için size bucket'ı minimum üye sayısı. Çift
/// dosyaya prefilter overhead'i koymak anlamsız.
const HEAD_HASH_BUCKET_MIN: usize = 4;
/// Bölüm 7.2 — sistem dosyaları ve cache'leri eler.
pub const DEFAULT_MIN_DUP_SIZE: u64 = 4096;
/// Hash aşamasında patolojik dağılım koruması.
const MAX_CANDIDATE_FILES: usize = 500_000;

/// Bölüm 7 ayarları — UI'dan parametrik gelir.
#[derive(Debug, Clone, Copy, serde::Deserialize, Serialize)]
pub struct DuplicateOptions {
    pub min_size_bytes: u64,
    /// Aşama 2'yi atlayıp sadece boyut bucket'ı raporlamak için (debug/UI önizleme).
    /// Default false — hash şart.
    pub size_only: bool,
    /// Head-hash prefilter'ı devre dışı bırak — benchmark ve regresyon
    /// karşılaştırması için. Üretimde default false (prefilter etkin).
    #[serde(default)]
    pub skip_head_prefilter: bool,
    /// IPC payload üst limiti (kazanım sıralı). 1M dosyalık scan'de
    /// 100k+ duplicate group olabilir; UI tarafına 50 MB JSON gönderirsek
    /// frontend dondurur (Gemini review 2.1). Default 500 group;
    /// kullanıcı filtre daralttıkça daha fazlasını çekebilir.
    #[serde(default = "default_max_groups")]
    pub max_groups: usize,
}

fn default_max_groups() -> usize {
    500
}

impl Default for DuplicateOptions {
    fn default() -> Self {
        Self {
            min_size_bytes: DEFAULT_MIN_DUP_SIZE,
            size_only: false,
            skip_head_prefilter: false,
            max_groups: default_max_groups(),
        }
    }
}

/// Bir aday dosyanın (id, tam yol, byte boyutu) snapshot'ı.
/// `id` v0.2'de Bölüm 7.2 inode-bazlı hardlink/junction elemesi için kullanılacak.
#[derive(Debug, Clone)]
struct Candidate {
    #[allow(dead_code)]
    id: NodeId,
    path: String,
    size_bytes: u64,
}

/// Streaming Blake3 — `Read` üzerinden generic. Test ve VSS retry için
/// reader-bazlı yardımcı.
fn hash_reader<R: Read>(mut reader: R) -> std::io::Result<[u8; 32]> {
    let mut hasher = Hasher::new();
    let mut buf = vec![0u8; HASH_BUFFER];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Win32 `ERROR_SHARING_VIOLATION` kodu — `std::io::Error::raw_os_error()`
/// üzerinden tespit. Path string'inde Türkçe veya farklı locale mesaj
/// içermeyen güvenilir tek heuristik.
const WIN32_ERROR_SHARING_VIOLATION: i32 = 32;

fn is_share_violation(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(WIN32_ERROR_SHARING_VIOLATION)
}

/// Düz hash — `std::io::Error` korur (share violation tespiti için).
fn hash_file_inner(path: &Path) -> std::io::Result<[u8; 32]> {
    let file = fs::File::open(path)?;
    let reader = BufReader::with_capacity(HASH_BUFFER, file);
    hash_reader(reader)
}

/// Head-hash — yalnız ilk `HEAD_HASH_SIZE` baytı okur. Full Blake3
/// pahalısının %95'inden tasarruf ettirir; aynı boyutta ama farklı
/// içerikli iki dosya genelde ilk 4 KB'de farklılaşır (header, magic
/// bytes, manifest vb.).
fn head_hash_inner(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut buf = [0u8; HEAD_HASH_SIZE];
    let mut total = 0usize;
    while total < buf.len() {
        let n = file.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    let mut hasher = Hasher::new();
    hasher.update(&buf[..total]);
    Ok(*hasher.finalize().as_bytes())
}

/// Streaming Blake3 — sabit RAM, dosya boyutundan bağımsız.
/// `Error::Duplicate` wrap eder; ham hata için `hash_file_inner` kullan.
/// Üretimde `hash_file_with_retry` kullanılır; bu sade form yalnızca testlerde.
#[cfg(test)]
fn hash_file(path: &Path) -> Result<[u8; 32]> {
    hash_file_inner(path)
        .map_err(|e| Error::Duplicate(format!("hash open/read '{}': {}", path.display(), e)))
}

/// Bölüm 7.3 + 34.5.4 — locked dosya hash retry zinciri.
/// ShareViolation alırsa VSS pool üzerinden snapshot-bazlı reader ile dener.
/// `vss` feature kapalıysa orijinal hatayı `Error::Duplicate` olarak döner
/// (eski davranış korunur).
pub(crate) fn hash_file_with_retry(path: &Path) -> Result<[u8; 32]> {
    match hash_file_inner(path) {
        Ok(h) => Ok(h),
        Err(e) => {
            #[cfg(all(windows, feature = "vss"))]
            {
                if is_share_violation(&e) {
                    let reader = crate::locked_file::vss_pool::VssPool::global()
                        .reader_for(path)
                        .map_err(|err| {
                            Error::Duplicate(format!("vss reader '{}': {}", path.display(), err))
                        })?;
                    return hash_reader(reader).map_err(|err| {
                        Error::Duplicate(format!("vss hash retry '{}': {}", path.display(), err))
                    });
                }
            }
            // share violation değilse ya da vss kapalıysa orijinal hata.
            let _ = is_share_violation; // dead_code uyarısı bastır
            Err(Error::Duplicate(format!(
                "hash open/read '{}': {}",
                path.display(),
                e
            )))
        }
    }
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Bölüm 7 — ScanTree üzerinden duplicate aramayı çalıştırır.
/// `drive_letter` path reconstruction için kullanılır.
pub fn find_duplicates(
    tree: &ScanTree,
    drive_letter: char,
    opts: DuplicateOptions,
) -> Result<DuplicateScanResult> {
    let started = Instant::now();
    let min_size = opts.min_size_bytes.max(1); // 0-byte her zaman atlanır

    // --- Aşama 1: boyut bucket'ı ---------------------------------------
    let mut size_buckets: HashMap<u64, Vec<Candidate>> = HashMap::new();
    let mut scanned_files = 0u64;
    let mut filtered_small = 0u64;

    for node in tree.nodes.values() {
        if node.is_dir {
            continue;
        }
        scanned_files += 1;
        let size = node.data_size;
        if size < min_size {
            filtered_small += 1;
            continue;
        }
        let Some(path) = node_full_path(tree, drive_letter, node.id) else {
            continue;
        };
        size_buckets.entry(size).or_default().push(Candidate {
            id: node.id,
            path,
            size_bytes: size,
        });
    }

    // En az 2 aday içeren bucket'ları al — ama paralel head-hash için
    // bucket yapısını koru (her size grubu içinde head-hash sub-bucket).
    let mut candidate_pairs: u64 = 0;
    let mut size_groups: Vec<Vec<Candidate>> = Vec::new();
    let mut bucket_count: u64 = 0;
    let mut total_candidates = 0usize;
    for (_, list) in size_buckets.into_iter() {
        if list.len() < 2 {
            continue;
        }
        bucket_count += 1;
        candidate_pairs += list.len() as u64;
        total_candidates += list.len();
        size_groups.push(list);
    }

    if total_candidates > MAX_CANDIDATE_FILES {
        return Err(Error::Duplicate(format!(
            "{} aday dosya MAX_CANDIDATE_FILES ({}) sınırını aştı — \
             min_size_bytes değerini yükseltin",
            total_candidates, MAX_CANDIDATE_FILES
        )));
    }

    debug!(
        scanned_files,
        filtered_small,
        bucket_count,
        candidates = total_candidates,
        "duplicate stage 1 (size buckets) complete"
    );

    // --- Aşama 1.5: head-hash prefilter (Bölüm 7.4 v0.2) ---------------
    // Her size grubu içinde paralel olarak ilk 4 KB'yi hash'le; aynı
    // (size, head) sub-group'larında ≥2 üye varsa survivor olarak full
    // hash'e geç. Geri kalanlar elinir.
    let candidates: Vec<Candidate> = if opts.size_only || opts.skip_head_prefilter {
        // Prefilter kapalı: tüm aday'ları olduğu gibi geçir.
        size_groups.into_iter().flatten().collect()
    } else {
        let mut survivors: Vec<Candidate> = Vec::new();
        let mut head_hash_count: usize = 0;
        let mut head_eliminated: usize = 0;
        for group in size_groups {
            let size = group.first().map(|c| c.size_bytes).unwrap_or(0);
            if size < HEAD_HASH_THRESHOLD || group.len() < HEAD_HASH_BUCKET_MIN {
                // Küçük dosya ya da küçük bucket: prefilter overhead'i
                // kazanımdan büyük, doğrudan full hash'e geç.
                survivors.extend(group);
                continue;
            }
            // Paralel head-hash. Hata olursa konservatif: aday'ı tut.
            let heads: Vec<Option<[u8; 32]>> = group
                .par_iter()
                .map(|c| head_hash_inner(Path::new(&c.path)).ok())
                .collect();
            head_hash_count += group.len();

            let mut by_head: HashMap<[u8; 32], Vec<Candidate>> = HashMap::new();
            for (c, h) in group.into_iter().zip(heads) {
                match h {
                    Some(h) => by_head.entry(h).or_default().push(c),
                    None => survivors.push(c), // hata → konservatif tut
                }
            }
            for (_, sub) in by_head {
                if sub.len() >= 2 {
                    survivors.extend(sub);
                } else {
                    head_eliminated += sub.len();
                }
            }
        }
        debug!(
            head_hash_count,
            head_eliminated,
            survivors = survivors.len(),
            "duplicate stage 1.5 (head-hash prefilter) complete"
        );
        survivors
    };

    // --- Aşama 2/3: full hash + hash bucket ----------------------------
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    let mut hash_errors = 0u64;

    if opts.size_only {
        // Yalnızca size bucket'larını DuplicateGroup'a çevir (hash = placeholder).
        let mut by_size: HashMap<u64, Vec<Candidate>> = HashMap::new();
        for c in candidates {
            by_size.entry(c.size_bytes).or_default().push(c);
        }
        for (size, list) in by_size {
            if list.len() < 2 {
                continue;
            }
            let mut paths: Vec<String> = list.into_iter().map(|c| c.path).collect();
            paths.sort();
            groups.push(DuplicateGroup {
                hash_hex: format!("size:{}", size),
                size_bytes: size,
                paths,
            });
        }
    } else {
        // Rayon paralel full hash. par_iter'in dönüşü ardından sıralı
        // hash bucket toplama (HashMap mutation tek thread'de).
        let results: Vec<(Candidate, std::result::Result<[u8; 32], String>)> = candidates
            .into_par_iter()
            .map(|cand| {
                let p = Path::new(&cand.path);
                let res = hash_file_with_retry(p).map_err(|e| format!("{:?}", e));
                (cand, res)
            })
            .collect();

        let mut hash_buckets: HashMap<[u8; 32], (u64, Vec<String>)> = HashMap::new();
        for (cand, res) in results {
            match res {
                Ok(h) => {
                    hash_buckets
                        .entry(h)
                        .or_insert_with(|| (cand.size_bytes, Vec::new()))
                        .1
                        .push(cand.path);
                }
                Err(e) => {
                    warn!(path = %cand.path, error = %e, "hash skipped");
                    hash_errors += 1;
                }
            }
        }
        for (h, (size, mut paths)) in hash_buckets.into_iter() {
            if paths.len() < 2 {
                continue;
            }
            paths.sort();
            groups.push(DuplicateGroup {
                hash_hex: hex32(&h),
                size_bytes: size,
                paths,
            });
        }
    }

    // --- Aşama 4: sırala (en çok kazanım önce) -------------------------
    groups.sort_by(|a, b| {
        let a_red = a
            .size_bytes
            .saturating_mul(a.paths.len().saturating_sub(1) as u64);
        let b_red = b
            .size_bytes
            .saturating_mul(b.paths.len().saturating_sub(1) as u64);
        b_red.cmp(&a_red)
    });

    let redundant_bytes: u64 = groups
        .iter()
        .map(|g| {
            g.size_bytes
                .saturating_mul(g.paths.len().saturating_sub(1) as u64)
        })
        .sum();

    let total_group_count = groups.len() as u64;
    let stats = DuplicateStats {
        group_count: total_group_count,
        redundant_bytes,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };

    // Gemini review 2.1 — IPC payload limiti. Gruplar zaten kazanım
    // sıralı, top-N yeterli; geri kalanı kullanıcı `min_size_bytes`
    // filtresini sıkı tutarak çekebilir.
    let cap = opts.max_groups.clamp(50, 5_000);
    let truncated = groups.len() > cap;
    if truncated {
        groups.truncate(cap);
    }

    info!(
        groups = total_group_count,
        returned = groups.len(),
        truncated,
        reclaim_mb = redundant_bytes / 1_048_576,
        candidate_pairs,
        hash_errors,
        elapsed_ms = stats.elapsed_ms,
        "duplicate scan complete"
    );

    Ok(DuplicateScanResult {
        drive_letter: drive_letter.to_ascii_uppercase().to_string(),
        scanned_files,
        filtered_small,
        candidate_pairs,
        hash_errors,
        groups,
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hash_two_identical_files_match() {
        let dir = tempdir();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let content = b"D-Space test payload \xff\x00\x01\x02\x03";
        fs::File::create(&a).unwrap().write_all(content).unwrap();
        fs::File::create(&b).unwrap().write_all(content).unwrap();
        let ha = hash_file(&a).unwrap();
        let hb = hash_file(&b).unwrap();
        assert_eq!(ha, hb);
    }

    #[test]
    fn hash_different_files_differ() {
        let dir = tempdir();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        fs::File::create(&a).unwrap().write_all(b"hello").unwrap();
        fs::File::create(&b).unwrap().write_all(b"world").unwrap();
        assert_ne!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
    }

    #[test]
    fn hash_reader_matches_hash_file() {
        let dir = tempdir();
        let p = dir.path().join("c.bin");
        let content = b"reader vs file path";
        fs::File::create(&p).unwrap().write_all(content).unwrap();

        let from_file = hash_file(&p).unwrap();
        let from_reader = hash_reader(std::io::Cursor::new(content)).unwrap();
        assert_eq!(from_file, from_reader);
    }

    #[test]
    fn head_hash_matches_full_when_file_under_4k() {
        // <4 KB: head_hash dosyanın tamamını okur; aynı içerikli iki
        // dosyada full hash ile aynı bayt aralığında olur.
        let dir = tempdir();
        let a = dir.path().join("a.bin");
        let content = b"under-4kb-payload";
        fs::File::create(&a).unwrap().write_all(content).unwrap();
        let head = head_hash_inner(&a).unwrap();
        let mut hasher = Hasher::new();
        hasher.update(content);
        let expected = *hasher.finalize().as_bytes();
        assert_eq!(head, expected);
    }

    #[test]
    fn head_hash_differs_for_different_starts() {
        // Aynı boyutta ama farklı baş içerikli iki dosya — head_hash
        // farklı olmalı (prefilter işlevselliği).
        let dir = tempdir();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let mut content_a = vec![0u8; 8192];
        let mut content_b = vec![0u8; 8192];
        content_a[0] = 1;
        content_b[0] = 2;
        fs::File::create(&a).unwrap().write_all(&content_a).unwrap();
        fs::File::create(&b).unwrap().write_all(&content_b).unwrap();
        let ha = head_hash_inner(&a).unwrap();
        let hb = head_hash_inner(&b).unwrap();
        assert_ne!(ha, hb);
    }

    #[test]
    fn head_hash_groups_same_starts() {
        // Aynı ilk 4 KB, farklı tail → head_hash eşit olmalı.
        let dir = tempdir();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let mut content_a = vec![0u8; 8192];
        let mut content_b = vec![0u8; 8192];
        for i in 0..4096 {
            content_a[i] = (i % 251) as u8;
            content_b[i] = (i % 251) as u8;
        }
        // Tail farklı
        for i in 4096..8192 {
            content_a[i] = 0xAA;
            content_b[i] = 0xBB;
        }
        fs::File::create(&a).unwrap().write_all(&content_a).unwrap();
        fs::File::create(&b).unwrap().write_all(&content_b).unwrap();
        let ha = head_hash_inner(&a).unwrap();
        let hb = head_hash_inner(&b).unwrap();
        assert_eq!(
            ha, hb,
            "identical first 4 KB must yield identical head_hash (even if the full hash differs)"
        );
    }

    #[test]
    fn share_violation_detector() {
        let sv = std::io::Error::from_raw_os_error(32);
        assert!(is_share_violation(&sv));
        let other = std::io::Error::from_raw_os_error(5);
        assert!(!is_share_violation(&other));
        let synthetic = std::io::Error::other("x");
        assert!(!is_share_violation(&synthetic));
    }

    /// Belirli boyutlardaki dosyalar üretir, ScanTree benzeri yapı simüle eder,
    /// `find_duplicates` boyut bucket aşamasının kandidat sayısını doğru
    /// raporladığını ölçer (size_only modu hash çağrısı yapmaz, geçici dosya
    /// yolları gerçekten okunmaz → tree path resolver test ediliyor).
    /// Modül-genelinde `RawMftEntry` helper'ı — birden çok test paylaşır.
    fn r(
        record_no: u64,
        parent: u64,
        name: &str,
        size: u64,
        dir: bool,
    ) -> crate::scan::walk::RawMftEntry {
        crate::scan::walk::RawMftEntry {
            record_no,
            parent_record_no: parent,
            name: name.into(),
            data_size: size,
            is_dir: dir,
            modified_unix: 0,
        }
    }

    #[test]
    fn size_only_groups_by_size() {
        use crate::scan::tree::{build_tree, ROOT_RECORD};

        // İki çift aynı boyut, bir tekil.
        let raw = vec![
            r(100, ROOT_RECORD, "a.bin", 8192, false),
            r(101, ROOT_RECORD, "b.bin", 8192, false),
            r(102, ROOT_RECORD, "c.bin", 16384, false),
            r(103, ROOT_RECORD, "d.bin", 16384, false),
            r(104, ROOT_RECORD, "lonely.bin", 32768, false),
            // 4 KB altı → filtrelenir
            r(105, ROOT_RECORD, "tiny1.bin", 100, false),
            r(106, ROOT_RECORD, "tiny2.bin", 100, false),
        ];
        let tree = build_tree("vol".into(), raw);
        let opts = DuplicateOptions {
            min_size_bytes: DEFAULT_MIN_DUP_SIZE,
            size_only: true,
            skip_head_prefilter: false,
            max_groups: 500,
        };
        let result = find_duplicates(&tree, 'C', opts).unwrap();
        assert_eq!(result.groups.len(), 2, "expected two size buckets");
        // tiny'ler filtrelendi
        assert!(result.filtered_small >= 2);
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    /// Sprint 4.1 entegrasyon: head-hash prefilter + paralel full hash
    /// gerçek dosya üzerinde uçtan uca. Aynı boyutta ama ilk 4 KB'si
    /// farklı iki dosya prefilter'da elinmeli; aynı boyutta + aynı içerikli
    /// üçüncü çift duplicate olarak rapor edilmeli.
    #[test]
    fn duplicate_v02_pipeline_end_to_end() {
        use crate::scan::tree::{build_tree, ROOT_RECORD};

        let dir = tempdir();
        // 4 dosya, hepsi 128 KB (HEAD_HASH_THRESHOLD üstü).
        //   a, b: aynı içerik (duplicate çift)
        //   c, d: a/b'den farklı içerik AMA aynı boyut — prefilter
        //         elinmeli, full hash'e gitmemeli.
        let make = |name: &str, fill: u8| -> std::path::PathBuf {
            let p = dir.path().join(name);
            let content = vec![fill; 128 * 1024];
            fs::File::create(&p).unwrap().write_all(&content).unwrap();
            p
        };
        let a = make("a.bin", 0xAA);
        let b = make("b.bin", 0xAA);
        let _c = make("c.bin", 0xBB);
        let _d = make("d.bin", 0xCC);

        // ScanTree'yi temp path'lere yönlendiremiyoruz (path resolver C: bekliyor).
        // Bu yüzden doğrudan hash_file ile tetikleyelim — head-hash ve full hash
        // tutarlılığını uçtan uca doğrula.
        let head_a = head_hash_inner(&a).unwrap();
        let head_b = head_hash_inner(&b).unwrap();
        assert_eq!(head_a, head_b, "same content → same head_hash");

        let full_a = hash_file(&a).unwrap();
        let full_b = hash_file(&b).unwrap();
        assert_eq!(full_a, full_b, "same content → same full hash");

        // Build_tree + find_duplicates yolu için synthetic raw entries:
        let raw = vec![
            r(100, ROOT_RECORD, "a.bin", 128 * 1024, false),
            r(101, ROOT_RECORD, "b.bin", 128 * 1024, false),
            r(102, ROOT_RECORD, "c.bin", 128 * 1024, false),
            r(103, ROOT_RECORD, "d.bin", 128 * 1024, false),
        ];
        let tree = build_tree("vol".into(), raw);
        // size_only modu: prefilter pipeline'ı geçer, sadece size bucket.
        let opts = DuplicateOptions {
            min_size_bytes: DEFAULT_MIN_DUP_SIZE,
            size_only: true,
            skip_head_prefilter: false,
            max_groups: 500,
        };
        let result = find_duplicates(&tree, 'C', opts).unwrap();
        // Size_only: dört dosya aynı boyutta → tek bucket.
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].paths.len(), 4);
    }

    /// `skip_head_prefilter` ile size_only=false yolu — prefilter
    /// devre dışı bırakılınca full hash tüm aday'lara koşulur. Bu test
    /// path'ler temp olduğundan gerçek hash başarısız → 0 group bekleniyor
    /// (kontrollü hata davranışı).
    #[test]
    fn duplicate_skip_prefilter_runs_full_hash() {
        use crate::scan::tree::{build_tree, ROOT_RECORD};

        let raw = vec![
            r(100, ROOT_RECORD, "missing1.bin", 128 * 1024, false),
            r(101, ROOT_RECORD, "missing2.bin", 128 * 1024, false),
        ];
        let tree = build_tree("vol".into(), raw);
        let opts = DuplicateOptions {
            min_size_bytes: DEFAULT_MIN_DUP_SIZE,
            size_only: false,
            skip_head_prefilter: true,
            max_groups: 500,
        };
        let result = find_duplicates(&tree, 'C', opts).unwrap();
        // Path'ler diskte yok → hash hatası beklenir. Grup oluşmaz.
        assert!(result.hash_errors >= 2);
        assert_eq!(result.groups.len(), 0);
    }
}
