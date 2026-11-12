// SPDX-License-Identifier: GPL-3.0-or-later
//
// 26.3 Cross-Platform Volume Reader — v2 stub.
//
// Phase 1: Windows only. Direct MFT reads (ntfs crate) + FindFirstFile
// fallback. All of Chapter 5 is Win32 dependent.
//
// Phase 2: Linux ext4/btrfs/zfs and macOS APFS support. A VolumeBackend
// impl per platform. Must return the same `ScanTree` data structure.

use crate::error::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub enum VolumeBackend {
    /// Chapter 5 Windows MFT.
    WindowsNtfs,
    /// FindFirstFile / readdir generic.
    GenericReadDir,
    /// Linux ext4/btrfs/zfs (v2).
    LinuxFs,
    /// macOS APFS (v2).
    AppleApfs,
}

/// Chapter 26.3 — cross-platform reader. Each impl must produce a stream
/// of `RawMftEntry`-like items so `build_tree` can consume it directly.
pub trait CrossPlatformVolumeReader: Send + Sync {
    fn backend(&self) -> VolumeBackend;

    /// Volume root path (Windows `C:\`, Linux `/mnt/...`, macOS `/Volumes/...`).
    /// Stream-typed — prevents memory blowup on large volumes.
    fn enumerate_entries(&self, root: &str) -> Result<Box<dyn Iterator<Item = String>>>;
}
