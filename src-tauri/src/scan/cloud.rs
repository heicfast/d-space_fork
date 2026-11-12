// SPDX-License-Identifier: GPL-3.0-or-later
//
// Cloud placeholder detection — Master architecture Chapter 11.1.
//
// Phase 1: path-based rules (OneDrive/Dropbox/GoogleDrive folder match,
// Chapter 6.2). Useful but it cannot tell "cloud-only" files (placeholders)
// apart from ones that have a local copy.
//
// Phase 2 v0.1 (this sprint): real flag checking with Win32
// GetFileAttributesW:
//   * FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS (0x00400000) → online-only
//     placeholder. Opening it downloads from the cloud, disk usage 0.
//   * FILE_ATTRIBUTE_RECALL_ON_OPEN (0x00040000) → "always-available
//     offline" marked but tied to the cloud.
//   * FILE_ATTRIBUTE_REPARSE_POINT (0x00000400) → reparse point (cloud
//     or something else).
//
// Phase 2 v0.2: read the reparse tag (IO_REPARSE_TAG_CLOUD = 0x9000001A)
// via FindFirstFileEx + dwReserved0 — to tell providers apart (OneDrive
// vs Dropbox vs Google Drive).
//
// Principle: never on the scan-time hot path — on-demand drilldown probe
// only (Chapter 11.1 + 34.5.1 hot-path isolation pattern).

use crate::error::Result;
use serde::Serialize;
use std::path::Path;
#[cfg(windows)]
use tracing::debug;

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudPlaceholderState {
    /// Flags clean — a local file, not cloud nor a reparse point.
    LocalOnly,
    /// `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` set — online-only placeholder.
    /// Opening it downloads from the cloud, disk usage 0 (logical size > 0).
    OnlineOnly,
    /// `FILE_ATTRIBUTE_RECALL_ON_OPEN` set — "always available offline"
    /// marked but tied to the cloud; deleting it may also remove the cloud copy.
    AlwaysAvailable,
    /// REPARSE_POINT set but no cloud flags — may be a symlink, junction,
    /// Windows Dedup, etc.
    OtherReparse,
    /// Path not found or inaccessible.
    NotFound,
    /// Win32 error code.
    OtherError(u32),
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudProbe {
    pub path: String,
    pub state: CloudPlaceholderState,
    pub raw_attributes: u32,
    pub probe_elapsed_ms: u64,
}

#[cfg(windows)]
pub fn probe_cloud_state(path: &Path) -> Result<CloudProbe> {
    use std::os::windows::ffi::OsStrExt;
    use std::time::Instant;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::GetLastError;
    use windows::Win32::Storage::FileSystem::GetFileAttributesW;

    let start = Instant::now();
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let pcwstr = PCWSTR(wide.as_ptr());

    let attrs = unsafe { GetFileAttributesW(pcwstr) };
    let state = if attrs == INVALID_FILE_ATTRIBUTES {
        let code = unsafe { GetLastError().0 };
        match code {
            // ERROR_FILE_NOT_FOUND = 2, ERROR_PATH_NOT_FOUND = 3
            2 | 3 => CloudPlaceholderState::NotFound,
            other => CloudPlaceholderState::OtherError(other),
        }
    } else {
        classify(attrs)
    };

    let probe = CloudProbe {
        path: path.display().to_string(),
        state,
        raw_attributes: attrs,
        probe_elapsed_ms: start.elapsed().as_millis() as u64,
    };
    debug!(
        path = %path.display(),
        ?probe.state,
        raw_hex = format!("0x{:08X}", attrs),
        "cloud probe"
    );
    Ok(probe)
}

#[cfg(not(windows))]
pub fn probe_cloud_state(_path: &Path) -> Result<CloudProbe> {
    Err(crate::error::Error::Scan(
        "cloud probe is only supported on Windows targets".into(),
    ))
}

fn classify(attrs: u32) -> CloudPlaceholderState {
    if attrs & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS != 0 {
        return CloudPlaceholderState::OnlineOnly;
    }
    if attrs & FILE_ATTRIBUTE_RECALL_ON_OPEN != 0 {
        return CloudPlaceholderState::AlwaysAvailable;
    }
    if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return CloudPlaceholderState::OtherReparse;
    }
    CloudPlaceholderState::LocalOnly
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_recognizes_each_flag() {
        // 0x00400000 → OnlineOnly
        assert!(matches!(
            classify(FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS),
            CloudPlaceholderState::OnlineOnly
        ));
        // 0x00040000 → AlwaysAvailable
        assert!(matches!(
            classify(FILE_ATTRIBUTE_RECALL_ON_OPEN),
            CloudPlaceholderState::AlwaysAvailable
        ));
        // 0x00000400 → OtherReparse (no cloud flags)
        assert!(matches!(
            classify(FILE_ATTRIBUTE_REPARSE_POINT),
            CloudPlaceholderState::OtherReparse
        ));
        // Cloud takes priority (if REPARSE is also set, OnlineOnly is returned)
        assert!(matches!(
            classify(FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_REPARSE_POINT),
            CloudPlaceholderState::OnlineOnly
        ));
        // 0x00000020 (ARCHIVE) → LocalOnly
        assert!(matches!(classify(0x20), CloudPlaceholderState::LocalOnly));
    }

    #[cfg(windows)]
    #[test]
    fn probe_local_file_returns_local_only() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("plain.txt");
        std::fs::write(&p, b"plain content").unwrap();
        let probe = probe_cloud_state(&p).unwrap();
        assert!(matches!(probe.state, CloudPlaceholderState::LocalOnly));
    }

    #[cfg(windows)]
    #[test]
    fn probe_missing_path_returns_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("ghost.x");
        let probe = probe_cloud_state(&p).unwrap();
        assert!(matches!(probe.state, CloudPlaceholderState::NotFound));
    }
}
