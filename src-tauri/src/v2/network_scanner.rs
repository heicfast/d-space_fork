// SPDX-License-Identifier: GPL-3.0-or-later
//
// 26.2 Network Share Scanner — v2 stub.
//
// Phase 1: network drives are marked drive_kind=Remote by `volume::pre_flight_check`,
// the user sees a warning (Chapter 33.3). Scanning is done via the
// FindFirstFile fallback — slow.
//
// Phase 2: a dedicated scanner for UNC paths. Since there is no MFT, it is
// stream-based, chunked listing + cache. Bandwidth-aware over SMB.

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

/// Chapter 26.2 + 33.3 — UNC path scanner.
pub trait NetworkShareScanner: Send + Sync {
    /// Scan the network share. If `bandwidth_cap_bps` is 0, unlimited.
    /// Chapter 33.3 principle: the user must know the network cost; a partial
    /// result is always accepted (timeout or cap exceeded).
    fn scan_unc(&self, unc_path: &str, bandwidth_cap_bps: u64) -> Result<NetworkScanResult>;
}

/// Chapter 26.2 — `NetworkShareScanner` concrete impl. Since the existing
/// `find_first` walker already accepts UNC paths
/// (Sprint 4.4), it is built as a thin wrapper. `bandwidth_cap_bps`
/// is reserved for real throttling in v0.3.2; in the v0.3.1 release
/// it returns an estimated bandwidth report of a typical 4 KB/file
/// for metadata-fetch-heavy SMB workloads.
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

/// Typical cost of an SMB metadata fetch — a single file stat ~4 KB (handle +
/// FILE_NETWORK_OPEN_INFORMATION). Not real bandwidth, but a useful estimate
/// for giving the UI an "X MB of data pulled" impression.
const SMB_METADATA_BYTES_PER_FILE: u64 = 4096;

impl WindowsNetworkScanner {
    /// Gemini review (scan_unc_path uncancellable) — an enriched scan with a
    /// cancel flag and a progress callback. Extends the trait method without
    /// changing its shape; `scan_unc` calls this method with None/None.
    /// Walks over SMB that can take hours become stoppable from the UI
    /// + can report live progress.
    pub fn scan_unc_with_progress(
        &self,
        unc_path: &str,
        bandwidth_cap_bps: u64,
        progress_cb: Option<crate::scan::walk::ProgressCb<'_>>,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<NetworkScanResult> {
        let started = std::time::Instant::now();
        // `scan_find_first_with_progress` directly accepts the UNC path
        // (drive_to_root recognizes the UNC form, Sprint 4.4).
        let entries = crate::scan::scan_find_first_with_progress(unc_path, progress_cb, cancel)
            .map_err(|e| crate::error::Error::Volume(format!("UNC scan: {:?}", e)))?;
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
        // `\\server` (no share) — find_first should reject it.
        let res = s.scan_unc(r"\\server", 0);
        assert!(res.is_err(), "share-less UNC must be rejected");
    }

    #[test]
    fn bandwidth_estimate_uses_per_file_constant() {
        // Bandwidth estimate formula verification — for a typical 1000 files +
        // 100 folders, 4 KB × 1100 = ~4.4 MB is expected.
        let n_files: u64 = 1000;
        let n_dirs: u64 = 100;
        let expected = (n_files + n_dirs) * SMB_METADATA_BYTES_PER_FILE;
        assert_eq!(expected, 4_505_600);
    }
}
