// SPDX-License-Identifier: GPL-3.0-or-later
//
// Physical disk + partition layer — DiskWizard recovery engine.
//
// * `OffsetReader` — a `Read + Seek` view of a byte range inside a
//   physical drive (the NTFS engine then works over a lost partition
//   exactly like over a mounted volume).
// * GPT/MBR layout parsing + unallocated-gap detection with NTFS/FAT
//   boot-signature probing (lost partition rescue).
// * Shared helpers: raw target readers, `$Bitmap` reads, UNC space.
//
// The parsing functions are pure (fixture-testable on any host); the IO
// wrappers are Windows-only in practice (raw `\\.\PhysicalDriveN` access
// requires administrator).

use crate::error::{Error, Result};
use crate::recovery::collect::{ClusterBitmap, ReadSeek};
use crate::recovery::{ScanMeta, ScanTarget};
use ntfs::Ntfs;
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// OffsetReader
// ---------------------------------------------------------------------------

/// A `Read + Seek` view over `[base, base + len)` of an inner reader.
/// The NTFS engine sees the lost partition as if it were a volume
/// starting at byte 0.
pub struct OffsetReader<R> {
    inner: R,
    base: u64,
}

impl<R: Read> Read for OffsetReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: Seek> Seek for OffsetReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let effective = match pos {
            SeekFrom::Start(p) => SeekFrom::Start(self.base.saturating_add(p)),
            other => other,
        };
        let abs = self.inner.seek(effective)?;
        Ok(abs.saturating_sub(self.base))
    }
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionWire {
    pub index: u32,
    /// Human-readable type ("EFI system partition", "Microsoft basic data",
    /// "MBR (0x07)", ...) or "unallocated".
    pub kind: String,
    pub start_byte: u64,
    pub size_bytes: u64,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskLayoutWire {
    pub disk_no: u32,
    pub size_bytes: u64,
    pub sector_size: u32,
    pub style: String, // gpt | mbr | unknown
    pub partitions: Vec<PartitionWire>,
    /// Unallocated gaps with a detectable filesystem signature.
    pub lost_partitions: Vec<LostPartitionWire>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LostPartitionWire {
    /// Stable scan target id: `lp:<disk>:<base>`.
    pub id: String,
    pub disk_no: u32,
    pub start_byte: u64,
    pub size_bytes: u64,
    pub probable_fs: String,
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Pure parsing (fixture-testable)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct GptEntry {
    pub type_guid: [u8; 16],
    pub first_lba: u64,
    pub last_lba: u64,
    pub name: String,
}

/// Parses a GPT header (LBA 1, 512 bytes). Returns
/// `(entries_start_lba, entry_count, entry_size, first_usable, last_usable)`.
pub fn parse_gpt_header(sector: &[u8]) -> Option<(u64, u32, u32, u64, u64)> {
    if sector.len() < 92 || &sector[0..8] != b"EFI PART" {
        return None;
    }
    let entries_start = u64::from_le_bytes(sector[72..80].try_into().ok()?);
    let count = u32::from_le_bytes(sector[80..84].try_into().ok()?);
    let size = u32::from_le_bytes(sector[84..88].try_into().ok()?);
    let first_usable = u64::from_le_bytes(sector[40..48].try_into().ok()?);
    let last_usable = u64::from_le_bytes(sector[48..56].try_into().ok()?);
    if size == 0 || size > 1024 || count > 1024 {
        return None;
    }
    Some((entries_start, count, size, first_usable, last_usable))
}

/// Parses one GPT partition entry (128 bytes typical).
pub fn parse_gpt_entry(entry: &[u8]) -> Option<GptEntry> {
    if entry.len() < 128 {
        return None;
    }
    let mut type_guid = [0u8; 16];
    type_guid.copy_from_slice(&entry[0..16]);
    let first_lba = u64::from_le_bytes(entry[32..40].try_into().ok()?);
    let last_lba = u64::from_le_bytes(entry[40..48].try_into().ok()?);
    let name_utf16: Vec<u16> = entry[56..128]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&c| c != 0)
        .collect();
    let name = String::from_utf16_lossy(&name_utf16).trim().to_string();
    Some(GptEntry {
        type_guid,
        first_lba,
        last_lba,
        name,
    })
}

/// Well-known GPT type GUIDs (mixed-endian bytes as stored on disk).
pub fn gpt_kind_name(guid: &[u8; 16]) -> &'static str {
    match guid {
        [0xC1, 0x2A, 0x73, 0x28, 0xF8, 0x1F, 0x11, 0xD2, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B] => {
            "EFI system partition"
        }
        [0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7] => {
            "Microsoft reserved (MSR)"
        }
        [0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC8] => {
            "Microsoft basic data"
        }
        [0x0F, 0x88, 0x9D, 0x24, 0x0A, 0x4C, 0xB1, 0x1B, 0xE4, 0xA2, 0x9B, 0x84, 0x94, 0x1F, 0x89, 0x12] => {
            "Windows recovery environment"
        }
        _ => "Unknown partition",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MbrEntry {
    pub type_byte: u8,
    pub start_lba: u64,
    pub sectors: u64,
}

/// Parses the 4 MBR partition entries out of an LBA-0 sector.
pub fn parse_mbr(sector: &[u8]) -> Option<Vec<MbrEntry>> {
    if sector.len() < 512 || sector[510] != 0x55 || sector[511] != 0xAA {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..4 {
        let off = 446 + i * 16;
        let type_byte = sector[off + 4];
        if type_byte == 0 {
            continue;
        }
        let start_lba = u32::from_le_bytes(sector[off + 8..off + 12].try_into().ok()?) as u64;
        let sectors = u32::from_le_bytes(sector[off + 12..off + 16].try_into().ok()?) as u64;
        out.push(MbrEntry {
            type_byte,
            start_lba,
            sectors,
        });
    }
    Some(out)
}

pub fn mbr_kind_name(type_byte: u8) -> String {
    match type_byte {
        0x07 => "NTFS/exFAT".to_string(),
        0x0B | 0x0C => "FAT32".to_string(),
        0x0E => "FAT16".to_string(),
        0xEE => "GPT protective".to_string(),
        0x27 => "Windows recovery".to_string(),
        _ => format!("MBR (0x{type_byte:02X})"),
    }
}

/// Filesystem signature detection inside a boot sector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FsKind {
    Ntfs,
    Fat32,
    ExFat,
}

impl FsKind {
    pub fn wire_name(self) -> &'static str {
        match self {
            FsKind::Ntfs => "ntfs",
            FsKind::Fat32 => "fat32",
            FsKind::ExFat => "exfat",
        }
    }
}

/// Detects an NTFS/FAT32/exFAT boot sector and (for NTFS) returns
/// `(volume_bytes, sectors_per_cluster)`.
pub fn detect_fs_boot(sector: &[u8]) -> Option<(FsKind, u64)> {
    if sector.len() < 512 {
        return None;
    }
    let oem = &sector[3..11];
    let bps = u16::from_le_bytes(sector[11..13].try_into().ok()?);
    let spc = sector[13];
    // Sanity: bytes per sector 512..4096 power of two; clusters 1..128
    // power of two.
    let sane_geometry =
        matches!(bps, 512 | 1024 | 2048 | 4096) && spc.is_power_of_two() && spc <= 128;
    if oem.starts_with(b"NTFS") && sector[510] == 0x55 && sector[511] == 0xAA && sane_geometry {
        let total_sectors = u64::from_le_bytes(sector[40..48].try_into().ok()?);
        let volume_bytes = total_sectors.saturating_mul(bps as u64);
        if volume_bytes > 1024 * 1024 {
            return Some((FsKind::Ntfs, volume_bytes));
        }
    }
    if oem.starts_with(b"MSDOS") && sane_geometry && sector[510] == 0x55 && sector[511] == 0xAA {
        let total_sectors = u32::from_le_bytes(sector[32..36].try_into().ok()?) as u64;
        let volume_bytes = total_sectors.saturating_mul(bps as u64);
        if volume_bytes > 1024 * 1024 {
            return Some((FsKind::Fat32, volume_bytes));
        }
    }
    if oem.starts_with(b"EXFAT") && sane_geometry {
        let total_sectors = u64::from_le_bytes(sector[64..72].try_into().ok()?);
        let volume_bytes = total_sectors.saturating_mul(bps as u64);
        if volume_bytes > 1024 * 1024 {
            return Some((FsKind::ExFat, volume_bytes));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// IO wrappers
// ---------------------------------------------------------------------------

/// Opens a raw reader for a scan target.
/// * Volume → `\\.\X:`
/// * LostPartition → `\\.\PhysicalDriveN` wrapped in an `OffsetReader`
/// * UNC → error (network targets never use raw readers)
pub fn open_target_reader(target: &ScanTarget) -> Result<Box<dyn ReadSeek>> {
    match target {
        ScanTarget::Volume { volume_path, .. } => {
            let f = std::fs::File::open(volume_path).map_err(|e| {
                Error::Scan(format!(
                    "cannot open volume '{volume_path}': {e} (administrator privileges are required)"
                ))
            })?;
            Ok(Box::new(f))
        }
        ScanTarget::LostPartition {
            physical_drive,
            base_byte,
            ..
        } => {
            let path = format!(r"\\.\PhysicalDrive{physical_drive}");
            let f = std::fs::File::open(&path)
                .map_err(|e| Error::Scan(format!("cannot open '{path}': {e}")))?;
            Ok(Box::new(OffsetReader {
                inner: f,
                base: *base_byte,
            }))
        }
        ScanTarget::Unc { .. } => Err(Error::Scan(
            "network shares do not support raw volume readers".into(),
        )),
    }
}

/// Volume label used for logging when reading a target.
pub fn reader_volume_label(target: &ScanTarget) -> String {
    match target {
        ScanTarget::Volume { volume_path, .. } => volume_path.clone(),
        ScanTarget::Unc { root } => root.clone(),
        ScanTarget::LostPartition {
            physical_drive,
            base_byte,
            ..
        } => format!(r"\\.\PhysicalDrive{physical_drive}+{base_byte}"),
    }
}

/// Reads the NTFS `$Bitmap` through a target reader.
pub fn bitmap_on_reader(
    reader: &mut Box<dyn ReadSeek>,
    volume_label: &str,
) -> Result<ClusterBitmap> {
    let ntfs = Ntfs::new(reader)
        .map_err(|e| Error::Scan(format!("NTFS parse error on '{volume_label}': {e:?}")))?;
    ClusterBitmap::read(&ntfs, reader)
}

/// `GetDiskFreeSpaceExW` on a UNC root (best-effort used-space estimate).
pub fn unc_space(root: &str) -> Option<(u64, u64)> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let mut free: u64 = 0;
        let mut total: u64 = 0;
        let mut _total_free: u64 = 0;
        let ok = unsafe {
            windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                PCWSTR(wide.as_ptr()),
                Some(&mut free),
                Some(&mut total),
                Some(&mut _total_free),
            )
        };
        if ok.is_ok() {
            Some((free, total))
        } else {
            None
        }
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        None
    }
}

/// Enumerates physical drive numbers present in the system.
pub fn list_physical_drives() -> Vec<u32> {
    let mut out = Vec::new();
    for n in 0..64 {
        let path = format!(r"\\.\PhysicalDrive{n}");
        match std::fs::File::open(&path) {
            Ok(mut f) => {
                if f.seek(SeekFrom::End(0)).is_ok() {
                    out.push(n);
                }
            }
            Err(_) => continue,
        }
    }
    out
}

fn physical_drive_size(handle: &mut std::fs::File) -> u64 {
    handle.seek(SeekFrom::End(0)).unwrap_or(0)
}

fn read_sector<R: Read + Seek>(handle: &mut R, lba: u64, sector_size: u32) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; sector_size as usize];
    handle
        .seek(SeekFrom::Start(lba * sector_size as u64))
        .ok()?;
    handle.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Reads the full layout of a physical drive: partitions + unallocated
/// gaps + lost-partition probes.
pub fn read_disk_layout(disk_no: u32) -> Result<DiskLayoutWire> {
    let path = format!(r"\\.\PhysicalDrive{disk_no}");
    let mut handle = std::fs::File::open(&path)
        .map_err(|e| Error::Volume(format!("cannot open '{path}': {e}")))?;
    let size_bytes = physical_drive_size(&mut handle);
    if size_bytes == 0 {
        return Err(Error::Volume(format!("disk {disk_no} reports zero size")));
    }
    let sector_size: u32 = 512; // probe sectors use the classic geometry

    let lba0 = read_sector(&mut handle, 0, sector_size)
        .ok_or_else(|| Error::Volume("cannot read LBA 0".into()))?;

    let mut partitions: Vec<PartitionWire> = Vec::new();
    let mut style = "unknown".to_string();
    let mut gaps: Vec<(u64, u64)> = Vec::new(); // (start_byte, size)

    if let Some((entries_lba, count, entry_size, first_usable, last_usable)) =
        parse_gpt_header(lba0.as_slice())
    {
        // GPT signature is on LBA1, not LBA0 — re-check properly.
        let lba1 = read_sector(&mut handle, 1, sector_size);
        let gpt = lba1.as_deref().and_then(parse_gpt_header).or(Some((
            entries_lba,
            count,
            entry_size,
            first_usable,
            last_usable,
        )));
        if let Some((entries_lba, count, entry_size, first_usable, last_usable)) = gpt {
            style = "gpt".into();
            let mut used: Vec<(u64, u64)> = Vec::new(); // (start_lba, end_lba)
            let mut idx = 0u32;
            'entries: for lba in (entries_lba..entries_lba + 16).take(32) {
                let sector = match read_sector(&mut handle, lba, sector_size) {
                    Some(s) => s,
                    None => break 'entries,
                };
                for off in (0..sector.len()).step_by(entry_size as usize) {
                    if idx >= count {
                        break 'entries;
                    }
                    if off + entry_size as usize > sector.len() {
                        break;
                    }
                    if let Some(entry) = parse_gpt_entry(&sector[off..off + entry_size as usize]) {
                        if entry.first_lba == 0 && entry.last_lba == 0 {
                            idx += 1;
                            continue;
                        }
                        if entry.last_lba >= entry.first_lba {
                            used.push((entry.first_lba, entry.last_lba));
                            partitions.push(PartitionWire {
                                index: idx,
                                kind: gpt_kind_name(&entry.type_guid).to_string(),
                                start_byte: entry.first_lba * sector_size as u64,
                                size_bytes: (entry.last_lba - entry.first_lba + 1)
                                    * sector_size as u64,
                                label: if entry.name.is_empty() {
                                    None
                                } else {
                                    Some(entry.name.clone())
                                },
                            });
                        }
                    }
                    idx += 1;
                }
            }
            used.sort_unstable();
            // Gaps inside [first_usable, last_usable].
            let first_usable_byte = first_usable * sector_size as u64;
            let last_usable_byte = (last_usable + 1) * sector_size as u64;
            let mut cursor = first_usable_byte;
            for (start, end) in &used {
                let s = start * sector_size as u64;
                if s > cursor {
                    gaps.push((cursor, s - cursor));
                }
                cursor = (end + 1) * sector_size as u64;
            }
            if last_usable_byte > cursor {
                gaps.push((cursor, last_usable_byte - cursor));
            }
        }
    }

    if style == "unknown" {
        if let Some(entries) = parse_mbr(lba0.as_slice()) {
            style = "mbr".into();
            let mut used: Vec<(u64, u64)> = Vec::new();
            for (i, e) in entries.iter().enumerate() {
                if e.type_byte == 0xEE {
                    style = "gpt".into(); // protective MBR — handled above; keep kind
                }
                if e.sectors == 0 {
                    continue;
                }
                used.push((e.start_lba, e.start_lba + e.sectors.saturating_sub(1)));
                partitions.push(PartitionWire {
                    index: i as u32,
                    kind: mbr_kind_name(e.type_byte),
                    start_byte: e.start_lba * sector_size as u64,
                    size_bytes: e.sectors * sector_size as u64,
                    label: None,
                });
            }
            used.sort_unstable();
            let mut cursor = sector_size as u64; // after MBR
            let disk_end = size_bytes;
            for (start, end) in &used {
                let s = start * sector_size as u64;
                if s > cursor {
                    gaps.push((cursor, s - cursor));
                }
                cursor = (end + 1) * sector_size as u64;
            }
            if disk_end > cursor {
                gaps.push((cursor, disk_end - cursor));
            }
        }
    }

    // Probe gaps for lost filesystems.
    let mut lost = Vec::new();
    for (start, size) in gaps.into_iter().filter(|(_, s)| *s >= 1024 * 1024) {
        if let Some(hit) = probe_gap(&mut handle, start, size, sector_size) {
            lost.push(hit);
        }
    }

    Ok(DiskLayoutWire {
        disk_no,
        size_bytes,
        sector_size,
        style,
        partitions,
        lost_partitions: lost,
    })
}

/// Probes the first 128 sectors of an unallocated gap for a filesystem
/// boot sector; returns a lost-partition candidate on a hit.
fn probe_gap(
    handle: &mut std::fs::File,
    gap_start: u64,
    gap_size: u64,
    sector_size: u32,
) -> Option<LostPartitionWire> {
    let probe_lbas: u64 = 128.min(gap_size / sector_size as u64);
    for l in 0..probe_lbas {
        let abs = gap_start + l * sector_size as u64;
        handle.seek(SeekFrom::Start(abs)).ok()?;
        let mut buf = vec![0u8; sector_size as usize];
        handle.read_exact(&mut buf).ok()?;
        if let Some((kind, volume_bytes)) = detect_fs_boot(&buf) {
            // Lost volume may be smaller than the gap (or clipped by it).
            let size = volume_bytes.min(gap_size - l * sector_size as u64);
            if size < 1024 * 1024 {
                continue;
            }
            let start_byte = abs;
            info!(
                disk_lba = l,
                start_byte,
                size,
                fs = kind.wire_name(),
                "lost partition detected"
            );
            return Some(LostPartitionWire {
                id: format!("lp:{start_byte}"),
                disk_no: 0, // filled by caller
                start_byte,
                size_bytes: size,
                probable_fs: kind.wire_name().to_string(),
                note: Some("Unallocated area with a valid boot sector".to_string()),
            });
        }
    }
    None
}

/// Full disk inventory (layouts for every physical drive).
pub fn list_disk_layouts() -> Result<Vec<DiskLayoutWire>> {
    let mut out = Vec::new();
    for n in list_physical_drives() {
        match read_disk_layout(n) {
            Ok(mut layout) => {
                for lp in layout.lost_partitions.iter_mut() {
                    lp.id = format!("lp:{n}:{}", lp.start_byte);
                    lp.disk_no = n;
                }
                out.push(layout);
            }
            Err(e) => {
                warn!(disk = n, error = %e, "layout read failed");
            }
        }
    }
    debug!(disks = out.len(), "disk inventory complete");
    Ok(out)
}

/// Resolves an `lp:<disk>:<base>` id into scan metadata (light boot
/// re-probe to confirm the filesystem).
pub fn resolve_lost_target(rest: &str) -> Result<ScanMeta> {
    let (disk_s, base_s) = rest
        .split_once(':')
        .ok_or_else(|| Error::Volume(format!("invalid lost-partition id 'lp:{rest}'")))?;
    let disk: u32 = disk_s
        .parse()
        .map_err(|_| Error::Volume(format!("invalid disk number in 'lp:{rest}'")))?;
    let base: u64 = base_s
        .parse()
        .map_err(|_| Error::Volume(format!("invalid offset in 'lp:{rest}'")))?;

    let path = format!(r"\\.\PhysicalDrive{disk}");
    let mut handle = std::fs::File::open(&path)
        .map_err(|e| Error::Volume(format!("cannot open '{path}': {e}")))?;
    handle
        .seek(SeekFrom::Start(base))
        .map_err(|e| Error::Volume(format!("seek in '{path}' failed: {e}")))?;
    let mut buf = vec![0u8; 512];
    handle
        .read_exact(&mut buf)
        .map_err(|e| Error::Volume(format!("boot read failed: {e}")))?;
    let (kind, volume_bytes) = detect_fs_boot(&buf)
        .ok_or_else(|| Error::Volume("no filesystem signature at that offset".into()))?;

    let size_gb = volume_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let label = format!(
        "Disk {disk} · Lost {fs} volume ({gb:.1} GB)",
        fs = kind.wire_name().to_uppercase(),
        gb = size_gb
    );
    Ok(ScanMeta {
        target: ScanTarget::LostPartition {
            physical_drive: disk,
            base_byte: base,
            size_bytes: volume_bytes,
            label: label.clone(),
        },
        target_id: format!("lp:{disk}:{base}"),
        path_scope: None,
        target_label: label,
        drive_letter: None,
        fs: kind.wire_name().to_string(),
        volume_id: format!(r"\\.\PhysicalDrive{disk}+{base}"),
    })
}

// ---------------------------------------------------------------------------
// Tests — pure parsing against fixtures
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn gpt_header_fixture(entries_lba: u64, count: u32, entry_size: u32) -> [u8; 512] {
        let mut s = [0u8; 512];
        s[0..8].copy_from_slice(b"EFI PART");
        s[40..48].copy_from_slice(&34u64.to_le_bytes()); // first usable
        s[48..56].copy_from_slice(&1000u64.to_le_bytes()); // last usable
        s[72..80].copy_from_slice(&entries_lba.to_le_bytes());
        s[80..84].copy_from_slice(&count.to_le_bytes());
        s[84..88].copy_from_slice(&entry_size.to_le_bytes());
        s
    }

    #[test]
    fn gpt_header_parse_roundtrip() {
        let s = gpt_header_fixture(2, 8, 128);
        let (entries, count, size, first, last) = parse_gpt_header(&s).unwrap();
        assert_eq!((entries, count, size, first, last), (2, 8, 128, 34, 1000));
    }

    #[test]
    fn gpt_header_rejects_bad_signature() {
        let mut s = [0u8; 512];
        s[0..8].copy_from_slice(b"NOT-GPT!");
        assert!(parse_gpt_header(&s).is_none());
        assert!(parse_gpt_header(&[0u8; 16]).is_none());
    }

    #[test]
    fn gpt_entry_parse() {
        let mut e = [0u8; 128];
        // Microsoft basic data GUID (mixed endian)
        e[0..16].copy_from_slice(&[
            0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26,
            0x99, 0xC8,
        ]);
        e[32..40].copy_from_slice(&2048u64.to_le_bytes());
        e[40..48].copy_from_slice(&4096u64.to_le_bytes());
        e[56..58].copy_from_slice(&0x4100u16.to_le_bytes()); // 'A'
        let parsed = parse_gpt_entry(&e).unwrap();
        assert_eq!(parsed.first_lba, 2048);
        assert_eq!(parsed.last_lba, 4096);
        assert_eq!(parsed.name, "A");
        assert_eq!(gpt_kind_name(&parsed.type_guid), "Microsoft basic data");
    }

    #[test]
    fn mbr_parse_roundtrip() {
        let mut s = [0u8; 512];
        s[510] = 0x55;
        s[511] = 0xAA;
        // Entry 0: type 0x07 NTFS, start 2048, 1000 sectors
        s[446 + 4] = 0x07;
        s[446 + 8..446 + 12].copy_from_slice(&2048u32.to_le_bytes());
        s[446 + 12..446 + 16].copy_from_slice(&1000u32.to_le_bytes());
        let entries = parse_mbr(&s).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].start_lba, 2048);
        assert_eq!(entries[0].sectors, 1000);
        assert_eq!(mbr_kind_name(0x07), "NTFS/exFAT");
    }

    #[test]
    fn mbr_rejects_bad_signature() {
        assert!(parse_mbr(&[0u8; 512]).is_none());
    }

    fn ntfs_boot_fixture() -> [u8; 512] {
        let mut s = [0u8; 512];
        s[3..11].copy_from_slice(b"NTFS    ");
        s[11..13].copy_from_slice(&512u16.to_le_bytes()); // bytes/sector
        s[13] = 8; // sectors/cluster
        s[40..48].copy_from_slice(&1_000_000u64.to_le_bytes()); // total sectors
        s[510] = 0x55;
        s[511] = 0xAA;
        s
    }

    #[test]
    fn detect_ntfs_boot() {
        let (kind, bytes) = detect_fs_boot(&ntfs_boot_fixture()).unwrap();
        assert_eq!(kind, FsKind::Ntfs);
        assert_eq!(bytes, 512_000_000);
    }

    #[test]
    fn detect_rejects_garbage() {
        assert!(detect_fs_boot(&[0u8; 512]).is_none());
        // NTFS signature but absurd geometry
        let mut s = ntfs_boot_fixture();
        s[13] = 200; // not a power of two
        assert!(detect_fs_boot(&s).is_none());
    }

    #[test]
    fn offset_reader_maps_positions() {
        let mut inner = std::io::Cursor::new(vec![0u8; 4096]);
        {
            let mut r = OffsetReader {
                inner: &mut inner,
                base: 512,
            };
            assert_eq!(r.seek(SeekFrom::Start(0)).unwrap(), 0);
            let mut probe = [0u8; 4];
            assert_eq!(r.read(&mut probe).unwrap(), 4);
            assert_eq!(r.seek(SeekFrom::Start(16)).unwrap(), 16);
            assert_eq!(r.stream_position().unwrap(), 16);
        }
        // The underlying cursor sits at 512 + 16.
        assert_eq!(inner.stream_position().unwrap(), 512 + 16);
    }
}
