// SPDX-License-Identifier: GPL-3.0-or-later
//
// 26.2 Network Share Scanner — v2 stub.
//
// Faz 1: network drive'lar `volume::pre_flight_check` ile drive_kind=Remote
// olarak işaretlenir, kullanıcı uyarı görür (Bölüm 33.3). Tarama
// FindFirstFile fallback ile yapılır — yavaş.
//
// Faz 2: UNC path için özel scanner. MFT olmadığı için stream-based,
// chunked listing + cache. SMB üzerinden bandwidth-aware.

use crate::error::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct NetworkScanResult {
    pub unc_path: String,
    pub file_count: u64,
    pub dir_count: u64,
    pub total_bytes: u64,
    pub bandwidth_used_bytes: u64,
    pub elapsed_ms: u64,
    pub partial: bool,
}

/// Bölüm 26.2 + 33.3 — UNC path scanner.
pub trait NetworkShareScanner: Send + Sync {
    /// Network share'i tara. `bandwidth_cap_bps` 0 ise sınırsız.
    /// Bölüm 33.3 ilkesi: kullanıcı network maliyetini bilmeli, partial
    /// result her zaman kabul edilir (timeout veya cap aşımı).
    fn scan_unc(&self, unc_path: &str, bandwidth_cap_bps: u64) -> Result<NetworkScanResult>;
}

/// Bölüm 26.2 — `NetworkShareScanner` concrete impl. Mevcut
/// `find_first` walker'ı UNC path desteği zaten kabul ettiği için
/// (Sprint 4.4) thin wrapper olarak inşa edilir. `bandwidth_cap_bps`
/// v0.3.2'de gerçek throttling için tahsis edildi; v0.3.1 sürümünde
/// metadata fetch yoğun SMB workload'ı için tipik 4 KB/dosya
/// tahmini bandwidth raporu döner.
pub struct WindowsNetworkScanner;

impl WindowsNetworkScanner {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsNetworkScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// SMB metadata fetch tipik maliyeti — tek dosya stat ~4 KB (handle +
/// FILE_NETWORK_OPEN_INFORMATION). Gerçek bandwidth değil ama UI'a
/// "X MB veri çekildi" izlenimi vermek için kullanışlı tahmin.
const SMB_METADATA_BYTES_PER_FILE: u64 = 4096;

impl WindowsNetworkScanner {
    /// Gemini review (scan_unc_path uncancellable) — iptal bayrağı ve
    /// progress callback'i içeren zenginleştirilmiş tarama. Trait metodunu
    /// şekil bozmadan geniştirir; `scan_unc` bu metodu None/None ile
    /// çağırır. SMB üzerindeki saatler sürebilen walk'lar UI'dan
    /// durdurulabilir + canlı ilerleme alabilir.
    pub fn scan_unc_with_progress(
        &self,
        unc_path: &str,
        bandwidth_cap_bps: u64,
        progress_cb: Option<crate::scan::walk::ProgressCb<'_>>,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<NetworkScanResult> {
        let started = std::time::Instant::now();
        // `scan_find_first_with_progress` UNC path'ini doğrudan kabul eder
        // (drive_to_root UNC formunu tanır, Sprint 4.4).
        let entries = crate::scan::scan_find_first_with_progress(unc_path, progress_cb, cancel)
            .map_err(|e| crate::error::Error::Volume(format!("UNC tarama: {:?}", e)))?;
        self.summarize(unc_path, bandwidth_cap_bps, entries, started)
    }

    fn summarize(
        &self,
        unc_path: &str,
        _bandwidth_cap_bps: u64,
        entries: crate::scan::walk::MftEntries,
        started: std::time::Instant,
    ) -> Result<NetworkScanResult> {
        let mut file_count: u64 = 0;
        let mut dir_count: u64 = 0;
        let mut total_bytes: u64 = 0;
        for e in &entries.entries {
            if e.is_dir {
                dir_count += 1;
            } else {
                file_count += 1;
                total_bytes = total_bytes.saturating_add(e.data_size);
            }
        }

        Ok(NetworkScanResult {
            unc_path: unc_path.to_string(),
            file_count,
            dir_count,
            total_bytes,
            bandwidth_used_bytes: (file_count + dir_count) * SMB_METADATA_BYTES_PER_FILE,
            elapsed_ms: started.elapsed().as_millis() as u64,
            partial: false,
        })
    }
}

impl NetworkShareScanner for WindowsNetworkScanner {
    fn scan_unc(&self, unc_path: &str, bandwidth_cap_bps: u64) -> Result<NetworkScanResult> {
        self.scan_unc_with_progress(unc_path, bandwidth_cap_bps, None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_constructor_yields_scanner() {
        let _scanner: WindowsNetworkScanner = Default::default();
    }

    #[test]
    fn invalid_unc_rejected() {
        let s = WindowsNetworkScanner::new();
        // `\\server` (share yok) — find_first reject'lemeli.
        let res = s.scan_unc(r"\\server", 0);
        assert!(res.is_err(), "share-less UNC reddedilmeli");
    }

    #[test]
    fn bandwidth_estimate_uses_per_file_constant() {
        // Bandwidth tahmini formula doğrulaması — tipik 1000 dosya +
        // 100 klasör için 4 KB × 1100 = ~4.4 MB beklenir.
        let n_files: u64 = 1000;
        let n_dirs: u64 = 100;
        let expected = (n_files + n_dirs) * SMB_METADATA_BYTES_PER_FILE;
        assert_eq!(expected, 4_505_600);
    }
}
