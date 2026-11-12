// SPDX-License-Identifier: GPL-3.0-or-later
//
// FAT12/16/32 + exFAT deleted-file engine — DiskWizard.
//
// Engine logic adapted from SectorForge (MIT licensed,
// Copyright (c) 2026 terra seraph — https://github.com/terraseraph/
// sectorforge), reworked for DiskWizard's types (RawMftEntry /
// DeletedEntry tree model, ReadSeek raw volume handles, progress/pause/
// cancel flags) and GPL-3.0 relicensed with the MIT notice retained here.
//
// Honesty model (identical rules to the NTFS engine):
//   * Live files follow their cluster chain (FAT) or NoFatChain run
//     (exFAT).
//   * FAT zeroes a deleted file's chain, so a deleted file is read as one
//     contiguous run from its first cluster (the classic FAT undelete
//     assumption) — clusters allocated again count as reused and grade
//     the file down.
//   * Deleted directories are harvested from free clusters after their
//     first cluster (strict parse: the first entry must be ".").
//   * exFAT's allocation bitmap is the allocation authority (its FAT
//     entries go stale after deletes); plain FAT uses the table.

use crate::error::{Error, Result};
use crate::recovery::collect::{CollectProgress, DeletedEntry, DwRun, RunFlags};
use crate::scan::walk::RawMftEntry;
use std::collections::{HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::time::Instant;

mod dir;

/// Live directories bigger than this are corrupt chains, not directories.
const MAX_DIR_BYTES: u64 = 64 * 1024 * 1024;
/// A deleted FAT directory has no size; read at most this many free
/// clusters after its first one.
const DELETED_DIR_CLUSTERS: u64 = 64;

/// First synthetic id for FAT/exFAT entries (mirrors the NTFS record-16
/// convention; the tree builder synthesizes root id 5).
pub const FIRST_FAT_RECORD: u64 = 16;
/// Synthetic root id (shared with the NTFS tree convention).
pub const FAT_ROOT_RECORD: u64 = 5;

/// FAT chain states (normalized).
const FREE: u32 = 0;
const EOC: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatKind {
    Fat12,
    Fat16,
    Fat32,
    ExFat,
}

impl FatKind {
    pub fn wire_name(self) -> &'static str {
        match self {
            FatKind::Fat12 => "fat12",
            FatKind::Fat16 => "fat16",
            FatKind::Fat32 => "fat32",
            FatKind::ExFat => "exfat",
        }
    }
}

/// Where a file's (or directory's) data starts and how to follow it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Loc {
    pub first: u32,
    pub size: u64,
    /// Bytes past this read as zeros (exFAT's valid data length).
    pub valid: u64,
    /// No chain to follow: exFAT's NoFatChain flag.
    pub contiguous: bool,
    pub deleted: bool,
}

// ---------------------------------------------------------------------------
// Geometry (persisted with sessions; powers read-back)
// ---------------------------------------------------------------------------

/// Serializable FatKind for the geometry blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatKindWire {
    Fat12,
    Fat16,
    Fat32,
    ExFat,
}

impl From<FatKind> for FatKindWire {
    fn from(k: FatKind) -> Self {
        match k {
            FatKind::Fat12 => FatKindWire::Fat12,
            FatKind::Fat16 => FatKindWire::Fat16,
            FatKind::Fat32 => FatKindWire::Fat32,
            FatKind::ExFat => FatKindWire::ExFat,
        }
    }
}

impl From<FatKindWire> for FatKind {
    fn from(k: FatKindWire) -> Self {
        match k {
            FatKindWire::Fat12 => FatKind::Fat12,
            FatKindWire::Fat16 => FatKind::Fat16,
            FatKindWire::Fat32 => FatKind::Fat32,
            FatKindWire::ExFat => FatKind::ExFat,
        }
    }
}

/// Everything needed to re-open and re-check a FAT/exFAT volume at
/// read-back time (slot re-parse + fresh allocation state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FatGeometry {
    pub kind: FatKindWire,
    /// Cluster size in bytes.
    pub cluster_size: u64,
    /// Byte offset of cluster 2 (start of the data area).
    pub heap_off: u64,
    /// Byte offsets of the FAT copies (relative to the volume start).
    pub fat_copies: Vec<u64>,
    /// Length of one FAT in bytes.
    pub fat_bytes: u64,
    /// Highest cluster number + 1.
    pub cluster_end: u32,
    /// exFAT only: (first cluster, byte length) of the allocation bitmap
    /// as recorded in the root directory.
    pub exfat_bitmap: Option<(u32, u64)>,
}

/// Allocation view over the FAT table (or the exFAT bitmap when present).
/// Cluster indices are 0-based from the data area (cluster number - 2).
pub struct FatAllocMap {
    pub table: Vec<u32>,
    /// exFAT allocation bitmap (bit 0 = cluster 2).
    pub bitmap: Option<Vec<u8>>,
}

impl FatAllocMap {
    /// `true` when the cluster belongs to something now (or can't be
    /// told). `idx` is 0-based (cluster number - 2).
    pub fn is_allocated_idx(&self, idx: u64) -> bool {
        let c = idx.saturating_add(2) as u32;
        match &self.bitmap {
            Some(bm) => bm
                .get(((c - 2) / 8) as usize)
                .map_or(true, |b| b & (1 << ((c - 2) % 8)) != 0),
            None => self
                .table
                .get(c as usize)
                .map(|v| *v != FREE)
                .unwrap_or(true),
        }
    }
}

/// Result of the unified FAT/exFAT walk.
pub struct FatCollected {
    pub kind: FatKind,
    pub cluster_size: u32,
    pub existing: Vec<RawMftEntry>,
    pub deleted: Vec<DeletedEntry>,
    pub alloc: FatAllocMap,
    pub geometry: FatGeometry,
    pub skipped_errors: u64,
    pub elapsed_ms: u64,
    pub cancelled: bool,
}

// ---------------------------------------------------------------------------
// Boot sector parse (pure)
// ---------------------------------------------------------------------------

/// Parsed FAT/exFAT boot sector.
struct FatBoot {
    kind: FatKind,
    cluster_size: u64,
    heap_off: u64,
    cluster_end: u32,
    fat_copies: Vec<u64>,
    fat_bytes: u64,
    /// FAT12/16 fixed root region: (offset, bytes). FAT32/exFAT: chain.
    root_fixed: Option<(u64, u64)>,
    root_cluster: u32,
    exfat: bool,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap_or([0; 4]))
}

fn parse_boot(bs: &[u8]) -> Result<FatBoot> {
    if bs.len() < 512 {
        return Err(Error::Scan("short boot sector".into()));
    }
    if &bs[3..11] == b"EXFAT   " {
        return parse_exfat_boot(bs);
    }
    parse_fat_boot(bs)
}

fn parse_fat_boot(bs: &[u8]) -> Result<FatBoot> {
    let bps = u16_at(bs, 11) as u64;
    let spc = bs[13] as u64;
    let reserved = u16_at(bs, 14) as u64;
    let nfats = bs[16] as u64;
    let root_ents = u16_at(bs, 17) as u64;
    let total = match u16_at(bs, 19) {
        0 => u32_at(bs, 32) as u64,
        n => n as u64,
    };
    let fat_sectors = match u16_at(bs, 22) {
        0 => u32_at(bs, 36) as u64,
        n => n as u64,
    };
    if !matches!(bps, 512 | 1024 | 2048 | 4096)
        || !spc.is_power_of_two()
        || reserved == 0
        || !(1..=4).contains(&nfats)
        || fat_sectors == 0
    {
        return Err(Error::Scan(
            "no valid FAT boot sector on this volume".into(),
        ));
    }
    let root_sectors = (root_ents * 32).div_ceil(bps);
    let first_data = reserved + nfats * fat_sectors + root_sectors;
    let count = total
        .checked_sub(first_data)
        .ok_or_else(|| Error::Scan("the FAT boot sector's sizes don't add up".into()))?
        / spc;
    let kind = match count {
        0..4085 => FatKind::Fat12,
        4085..65525 => FatKind::Fat16,
        _ => FatKind::Fat32,
    };
    Ok(FatBoot {
        kind,
        cluster_size: bps * spc,
        heap_off: first_data * bps,
        cluster_end: (count + 2).min(0x0FFF_FFF7) as u32,
        fat_copies: (0..nfats)
            .map(|i| (reserved + i * fat_sectors) * bps)
            .collect(),
        fat_bytes: fat_sectors * bps,
        root_fixed: match kind {
            FatKind::Fat32 => None,
            _ => Some(((reserved + nfats * fat_sectors) * bps, root_ents * 32)),
        },
        root_cluster: match kind {
            FatKind::Fat32 => u32_at(bs, 44),
            _ => 0,
        },
        exfat: false,
    })
}

fn parse_exfat_boot(bs: &[u8]) -> Result<FatBoot> {
    let (bps_shift, spc_shift) = (bs[0x6C] as u32, bs[0x6D] as u32);
    if !(9..=12).contains(&bps_shift) || bps_shift + spc_shift > 25 || !(1..=2).contains(&bs[0x6E])
    {
        return Err(Error::Scan(
            "no valid exFAT boot sector on this volume".into(),
        ));
    }
    let bps = 1u64 << bps_shift;
    let (fat_off, fat_len) = (u32_at(bs, 0x50) as u64 * bps, u32_at(bs, 0x54) as u64 * bps);
    Ok(FatBoot {
        kind: FatKind::ExFat,
        cluster_size: bps << spc_shift,
        heap_off: u32_at(bs, 0x58) as u64 * bps,
        cluster_end: u32_at(bs, 0x5C).saturating_add(2).min(0xFFFF_FFF6),
        fat_copies: (0..bs[0x6E] as u64)
            .map(|i| fat_off + i * fat_len)
            .collect(),
        fat_bytes: fat_len,
        root_fixed: None,
        root_cluster: u32_at(bs, 0x60),
        exfat: true,
    })
}

// ---------------------------------------------------------------------------
// FAT table decode (pure)
// ---------------------------------------------------------------------------

fn decode_fat_table(kind: FatKind, raw: &[u8], entries: u64) -> Vec<u32> {
    let (eoc, bad) = match kind {
        FatKind::Fat12 => (0xFF8, 0xFF7),
        FatKind::Fat16 => (0xFFF8, 0xFFF7),
        FatKind::Fat32 => (0x0FFF_FFF8, 0x0FFF_FFF7),
        FatKind::ExFat => (0xFFFF_FFF8, 0xFFFF_FFF7),
    };
    (0..entries as usize)
        .map(|n| {
            let v = match kind {
                FatKind::Fat12 => {
                    let o = n + n / 2;
                    match raw.get(o..o + 2) {
                        Some(w) => {
                            let w = u16_at(w, 0) as u32;
                            if n % 2 == 1 {
                                w >> 4
                            } else {
                                w & 0xFFF
                            }
                        }
                        None => eoc,
                    }
                }
                FatKind::Fat16 => raw
                    .get(n * 2..n * 2 + 2)
                    .map_or(eoc, |w| u16_at(w, 0) as u32),
                FatKind::Fat32 => raw
                    .get(n * 4..n * 4 + 4)
                    .map_or(eoc, |w| u32_at(w, 0) & 0x0FFF_FFFF),
                FatKind::ExFat => raw.get(n * 4..n * 4 + 4).map_or(eoc, |w| u32_at(w, 0)),
            };
            if v >= eoc {
                EOC
            } else if v == bad {
                u32::MAX - 1 // BAD
            } else {
                v
            }
        })
        .collect()
}

fn table_len(kind: FatKind, entries: u64, fat_bytes: u64) -> usize {
    match kind {
        FatKind::Fat12 => (entries * 3).div_ceil(2),
        FatKind::Fat16 => entries * 2,
        FatKind::Fat32 | FatKind::ExFat => entries * 4,
    }
    .min(fat_bytes) as usize
}

// ---------------------------------------------------------------------------
// Raw reads
// ---------------------------------------------------------------------------

fn read_at<R: Read + Seek>(r: &mut R, abs: u64, out: &mut [u8]) -> std::io::Result<()> {
    r.seek(SeekFrom::Start(abs))?;
    r.read_exact(out)
}

fn in_heap(cluster_end: u32, c: u32) -> bool {
    (2..cluster_end).contains(&c)
}

/// Whether cluster `c` currently belongs to something (FAT table or the
/// exFAT bitmap when present).
fn fat_allocated(table: &[u32], bitmap: &Option<Vec<u8>>, cluster_end: u32, c: u32) -> bool {
    if !in_heap(cluster_end, c) {
        return true;
    }
    match bitmap {
        Some(bm) => bm
            .get(((c - 2) / 8) as usize)
            .map_or(true, |b| b & (1 << ((c - 2) % 8)) != 0),
        None => table.get(c as usize).map(|v| *v != FREE).unwrap_or(true),
    }
}

/// Directory bytes for a source (bounded by MAX_DIR_BYTES).
fn dir_bytes_of<R: Read + Seek>(
    reader: &mut R,
    boot: &FatBoot,
    table: &[u32],
    bitmap: &Option<Vec<u8>>,
    src: DirSrc,
) -> Vec<u8> {
    let cluster_size = boot.cluster_size;
    let cluster_end = boot.cluster_end;
    let max = MAX_DIR_BYTES / cluster_size;
    let (runs, n) = match src {
        DirSrc::FixedRoot(off, len) => {
            let mut buf = vec![0u8; len as usize];
            let _ = read_at(reader, off, &mut buf);
            return buf;
        }
        DirSrc::Chain(first) => chain(table, cluster_end, first, max),
        DirSrc::Free(first) => {
            if !in_heap(cluster_end, first) || fat_allocated(table, bitmap, cluster_end, first) {
                return Vec::new();
            }
            let n = (first..cluster_end)
                .take(DELETED_DIR_CLUSTERS as usize)
                .take_while(|&c| !fat_allocated(table, bitmap, cluster_end, c))
                .count();
            (vec![(first, n as u32)], n as u64)
        }
        DirSrc::Loc(loc) => {
            if loc.deleted && fat_allocated(table, bitmap, cluster_end, loc.first) {
                return Vec::new(); // reused: whatever is there now isn't this directory
            }
            let loc = Loc {
                size: loc.size.min(MAX_DIR_BYTES),
                ..loc
            };
            // exFAT directories follow the chain (or NoFatChain run).
            let needed = loc.size.div_ceil(cluster_size);
            if needed == 0 || !in_heap(cluster_end, loc.first) {
                return Vec::new();
            }
            if !loc.contiguous {
                let (runs, n) = chain(table, cluster_end, loc.first, needed);
                if n >= needed || !loc.deleted {
                    (runs, n)
                } else {
                    let avail = needed.min((cluster_end - loc.first) as u64);
                    (vec![(loc.first, avail as u32)], avail)
                }
            } else {
                let avail = needed.min((cluster_end - loc.first) as u64);
                (vec![(loc.first, avail as u32)], avail)
            }
        }
    };
    let mut buf = vec![0u8; (n * cluster_size) as usize];
    let mut vpos = 0u64;
    for (c, len) in &runs {
        let (rs, re) = (vpos, vpos + *len as u64 * cluster_size);
        vpos = re;
        let abs = boot.heap_off + (*c as u64 - 2) * cluster_size;
        let end = (re as usize).min(buf.len());
        let start = rs as usize;
        if start < end {
            let _ = read_at(reader, abs, &mut buf[start..end]);
        }
    }
    buf
}

fn chain(table: &[u32], cluster_end: u32, first: u32, max: u64) -> (Vec<(u32, u32)>, u64) {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    let mut n = 0u64;
    let mut c = first;
    // Never more steps than there are clusters: a looped chain ends here.
    while in_heap(cluster_end, c) && n < max && n < cluster_end as u64 {
        match runs.last_mut() {
            Some((s, l)) if *s + *l == c => *l += 1,
            _ => runs.push((c, 1)),
        }
        n += 1;
        match table.get(c as usize).copied() {
            Some(next) if in_heap(cluster_end, next) => c = next,
            _ => break,
        }
    }
    (runs, n)
}

// ---------------------------------------------------------------------------
// Unified walk
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum DirSrc {
    FixedRoot(u64, u64),
    Chain(u32),
    Free(u32),
    Loc(Loc),
}

/// Walks the FAT/exFAT directory graph breadth-first from the root,
/// collecting live entries (tree feed) and deleted entries (recovery
/// feed) in one pass. Cancel returns the partial collection (browsable),
/// mirroring the MFT engine's contract.
pub fn collect_fat<R: Read + Seek>(
    reader: &mut R,
    volume_path: &str,
    flags: &RunFlags,
    progress: Option<&dyn Fn(&CollectProgress)>,
) -> Result<FatCollected> {
    let start = Instant::now();

    // Boot sector.
    let mut bs = vec![0u8; 512];
    read_at(reader, 0, &mut bs).map_err(|e| {
        Error::Scan(format!(
            "cannot read the boot sector of '{volume_path}': {e}"
        ))
    })?;
    let boot = parse_boot(&bs)?;

    // FAT table (primary copy; a failed read leaves 0xFF bytes that
    // decode as end-of-chain = allocated — never optimistic).
    let mut raw = vec![0u8; table_len(boot.kind, boot.cluster_end as u64, boot.fat_bytes)];
    if read_at(reader, boot.fat_copies[0], &mut raw).is_err() {
        raw.fill(0xFF);
    }
    let table = decode_fat_table(boot.kind, &raw, boot.cluster_end as u64);
    let cluster_size = boot.cluster_size;
    let cluster_end = boot.cluster_end;

    // exFAT: load the allocation bitmap from the root directory.
    let mut bitmap: Option<Vec<u8>> = None;
    let mut exfat_bitmap_loc: Option<(u32, u64)> = None;
    if boot.exfat {
        let root = dir_bytes_of(
            reader,
            &boot,
            &table,
            &bitmap,
            DirSrc::Chain(boot.root_cluster),
        );
        if let Some((first, len)) = dir::exfat_bitmap(&root) {
            exfat_bitmap_loc = Some((first, len));
            let mut bm = vec![0u8; len as usize];
            // The bitmap file's own clusters: follow the chain.
            let needed = len.div_ceil(cluster_size);
            let (runs, n) = chain(&table, cluster_end, first, needed);
            let mut vpos = 0u64;
            for (c, l) in &runs {
                let (rs, re) = (vpos, vpos + *l as u64 * cluster_size);
                vpos = re;
                let abs = boot.heap_off + (*c as u64 - 2) * cluster_size;
                let end = (re as usize).min(bm.len());
                if rs < re && (rs as usize) < end {
                    let _ = read_at(reader, abs, &mut bm[rs as usize..end]);
                }
            }
            let _ = n;
            bitmap = Some(bm);
        }
    }

    let geometry = FatGeometry {
        kind: boot.kind.into(),
        cluster_size,
        heap_off: boot.heap_off,
        fat_copies: boot.fat_copies.clone(),
        fat_bytes: boot.fat_bytes,
        cluster_end,
        exfat_bitmap: exfat_bitmap_loc,
    };

    let mut existing: Vec<RawMftEntry> = Vec::new();
    let mut deleted: Vec<DeletedEntry> = Vec::new();
    let mut skipped = 0u64;
    let mut last_name = String::new();
    let mut cancelled = false;

    // Root source + its absolute byte offset (for slot re-checks).
    let (root_src, root_abs) = match boot.root_fixed {
        Some((off, len)) => (DirSrc::FixedRoot(off, len), off),
        None => (DirSrc::Chain(boot.root_cluster), 0),
    };

    // The root itself is an existing entry (id 5) so the tree builder has
    // a real node to attach to.
    existing.push(RawMftEntry {
        record_no: FAT_ROOT_RECORD,
        parent_record_no: FAT_ROOT_RECORD,
        name: String::new(),
        data_size: 0,
        is_dir: true,
        modified_unix: 0,
    });

    let mut next_id = FIRST_FAT_RECORD;
    // (dir_id, source, parent_deleted, dir_data_byte_offset)
    let mut queue: VecDeque<(u64, DirSrc, bool, u64)> = VecDeque::new();
    let mut seen: HashSet<u32> = HashSet::new();
    if let DirSrc::Chain(c) = root_src {
        seen.insert(c);
    }
    queue.push_back((FAT_ROOT_RECORD, root_src, false, root_abs));

    let mut dir_count = 1u64;
    while let Some((dir_id, src, parent_deleted, dir_abs)) = queue.pop_front() {
        if flags.wait_while_paused() || flags.is_cancelled() {
            cancelled = true;
            break;
        }
        if let Some(cb) = progress {
            cb(&CollectProgress {
                visited: dir_count,
                total: dir_count + queue.len() as u64,
                existing: existing.len() as u64,
                deleted: deleted.len() as u64,
                last_name: last_name.clone(),
            });
        }
        let bytes = dir_bytes_of(reader, &boot, &table, &bitmap, src);
        let raw_entries = match boot.kind {
            FatKind::ExFat => dir::parse_exfat(&bytes),
            _ => dir::parse_fat(
                &bytes,
                boot.kind == FatKind::Fat32,
                matches!(src, DirSrc::Free(_)),
            ),
        };
        for (entry_off, r) in raw_entries {
            let deleted_flag = r.deleted || parent_deleted;
            let mut loc = r.loc;
            loc.deleted = deleted_flag;
            // FAT keeps no chain for a deleted file; exFAT may still have one.
            loc.contiguous |= deleted_flag && boot.kind != FatKind::ExFat;
            if !in_heap(cluster_end, loc.first) && (r.is_dir || loc.size > 0) && loc.first != 0 {
                skipped += 1;
                continue; // points outside the volume: garbage
            }
            let id = next_id;
            next_id += 1;
            last_name = r.name.clone();

            let slot_offset = dir_abs + entry_off as u64;

            if !deleted_flag {
                existing.push(RawMftEntry {
                    record_no: id,
                    parent_record_no: dir_id,
                    name: r.name.clone(),
                    data_size: if r.is_dir { 0 } else { loc.size },
                    is_dir: r.is_dir,
                    modified_unix: r.modified_unix,
                });
            } else {
                // Deleted entry: materialize runs in the shared
                // DeletedEntry shape. LCNs are 0-based data-area cluster
                // indices; read-back adds heap_off from the geometry.
                let needed = loc.size.div_ceil(cluster_size.max(1));
                let runs: Vec<(u32, u32)> = if needed == 0 || !in_heap(cluster_end, loc.first) {
                    Vec::new()
                } else if !loc.contiguous {
                    let (runs, n) = chain(&table, cluster_end, loc.first, needed);
                    if n >= needed {
                        runs
                    } else {
                        let avail = needed.min((cluster_end - loc.first) as u64);
                        vec![(loc.first, avail as u32)]
                    }
                } else {
                    let avail = needed.min((cluster_end - loc.first) as u64);
                    vec![(loc.first, avail as u32)]
                };
                deleted.push(DeletedEntry {
                    record_no: id,
                    parent_record_no: dir_id,
                    name: r.name.clone(),
                    data_size: if r.is_dir { 0 } else { loc.size },
                    is_dir: r.is_dir,
                    modified_unix: r.modified_unix,
                    resident: false,
                    runs: runs
                        .into_iter()
                        .map(|(c, len)| DwRun {
                            lcn: Some((c as u64).saturating_sub(2)),
                            clusters: len as u64,
                        })
                        .collect(),
                    compressed: false,
                    encrypted: false,
                    slot_offset: Some(slot_offset),
                });
            }

            if r.is_dir && in_heap(cluster_end, loc.first) && seen.insert(loc.first) {
                let src = match (boot.kind, deleted_flag) {
                    (FatKind::ExFat, _) => DirSrc::Loc(loc),
                    (_, false) => DirSrc::Chain(loc.first),
                    (_, true) => DirSrc::Free(loc.first),
                };
                queue.push_back((id, src, deleted_flag, slot_offset));
            }
        }
        dir_count += 1;
    }

    Ok(FatCollected {
        kind: boot.kind,
        cluster_size: cluster_size as u32,
        existing,
        deleted,
        alloc: FatAllocMap { table, bitmap },
        geometry,
        skipped_errors: skipped,
        elapsed_ms: start.elapsed().as_millis() as u64,
        cancelled,
    })
}

// ---------------------------------------------------------------------------
// Read-back helpers (preview / recovery jobs)
// ---------------------------------------------------------------------------

/// Re-reads the FAT from raw geometry and returns the allocation view.
pub fn reload_alloc<R: Read + Seek>(geometry: &FatGeometry, reader: &mut R) -> FatAllocMap {
    let kind: FatKind = geometry.kind.into();
    let mut raw = vec![0u8; table_len(kind, geometry.cluster_end as u64, geometry.fat_bytes)];
    if geometry
        .fat_copies
        .first()
        .map(|&off| read_at(reader, off, &mut raw).is_err())
        .unwrap_or(true)
    {
        raw.fill(0xFF);
    }
    let table = decode_fat_table(kind, &raw, geometry.cluster_end as u64);

    // exFAT: re-read the allocation bitmap through the root directory's
    // recorded location.
    let mut bitmap = None;
    if kind == FatKind::ExFat {
        if let Some((first, len)) = geometry.exfat_bitmap {
            let needed = len.div_ceil(geometry.cluster_size);
            let (runs, _) = chain(&table, geometry.cluster_end, first, needed);
            let mut bm = vec![0u8; len as usize];
            let mut vpos = 0u64;
            for (c, l) in &runs {
                let (rs, re) = (vpos, vpos + *l as u64 * geometry.cluster_size);
                vpos = re;
                let abs = geometry.heap_off + (*c as u64 - 2) * geometry.cluster_size;
                let end = (re as usize).min(bm.len());
                if (rs as usize) < end {
                    let _ = read_at(reader, abs, &mut bm[rs as usize..end]);
                }
            }
            bitmap = Some(bm);
        }
    }
    FatAllocMap { table, bitmap }
}

/// Re-reads a deleted entry's directory slot and reports whether it is
/// still deleted. `slot_offset` is the absolute byte offset recorded at
/// scan time.
pub fn reparse_slot<R: Read + Seek>(
    geometry: &FatGeometry,
    reader: &mut R,
    slot_offset: u64,
) -> Result<bool> {
    let kind: FatKind = geometry.kind.into();
    let mut entry = [0u8; 32];
    read_at(reader, slot_offset, &mut entry)
        .map_err(|e| Error::Scan(format!("directory slot re-read failed: {e}")))?;
    Ok(match kind {
        FatKind::ExFat => entry[0] & 0x80 == 0,
        _ => entry[0] == 0xE5,
    })
}

// ---------------------------------------------------------------------------
// Tests — pure parsing against synthetic FAT images
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fat12_image() -> Vec<u8> {
        // Minimal FAT12: 512 B/sector, 1 sector/cluster, 1 reserved,
        // 1 FAT of 1 sector, 16 root entries, 32 total sectors.
        let mut img = vec![0u8; 512 * 32];
        img[0] = 0xEB;
        img[1] = 0x3C;
        img[2] = 0x90;
        img[3..11].copy_from_slice(b"MSDOS5.0");
        img[11..13].copy_from_slice(&512u16.to_le_bytes());
        img[13] = 1; // sectors per cluster
        img[14..16].copy_from_slice(&1u16.to_le_bytes()); // reserved
        img[16] = 1; // fats
        img[17..19].copy_from_slice(&16u16.to_le_bytes()); // root entries
        img[19..21].copy_from_slice(&0u16.to_le_bytes()); // total16 (use 32)
        img[21] = 0xF8; // media
        img[22..24].copy_from_slice(&1u16.to_le_bytes()); // fat sectors
        img[32..36].copy_from_slice(&32u32.to_le_bytes()); // total32
        img[510] = 0x55;
        img[511] = 0xAA;
        // FAT12 packing: entry n lives at byte offset n + n/2 (1.5 bytes
        // each). Entry 2 (even) = b3 | (b4_low << 8); entry 3 (odd) =
        // b4_high | (b5 << 4). b3=0xF8, b4=0x0F, b5=0x00 gives
        // entry2 = 0xFF8 (EOC, allocated) and entry3 = 0 (free).
        img[512 + 3] = 0xF8;
        img[512 + 4] = 0x0F;
        img[512 + 5] = 0x00;
        // Root dir at sector 2 (reserved 1 + fat 1): one live file entry
        // (FAT pads names with spaces, never NULs).
        let root = 2 * 512;
        img[root] = b'H';
        img[root + 1] = b'I';
        for b in &mut img[root + 2..root + 11] {
            *b = b' ';
        }
        img[root + 11] = 0x20; // archive
        img[root + 26..root + 28].copy_from_slice(&2u16.to_le_bytes()); // first cluster
        img[root + 28..root + 32].copy_from_slice(&4u32.to_le_bytes()); // size
                                                                        // A deleted entry right after.
        img[root + 32] = 0xE5;
        img[root + 33] = b'X';
        for b in &mut img[root + 34..root + 32 + 11] {
            *b = b' ';
        }
        img[root + 32 + 11] = 0x20;
        img[root + 32 + 26..root + 32 + 28].copy_from_slice(&3u16.to_le_bytes());
        img[root + 32 + 28..root + 32 + 32].copy_from_slice(&6u32.to_le_bytes());
        img
    }

    #[test]
    fn fat12_open_and_walk() {
        let mut cursor = std::io::Cursor::new(fat12_image());
        let flags = RunFlags::default();
        let out = collect_fat(&mut cursor, "test://fat12", &flags, None).unwrap();
        assert_eq!(out.kind, FatKind::Fat12);
        assert_eq!(out.cluster_size, 512);
        // Root + the live HI file.
        assert!(out.existing.len() >= 2);
        assert!(out.existing.iter().any(|e| e.name == "HI"));
        // The deleted X entry is collected with a contiguous run from
        // cluster 3 (data idx 1), size 6 bytes -> 1 cluster.
        assert_eq!(out.deleted.len(), 1);
        assert_eq!(out.deleted[0].name, "_X");
        assert!(out.deleted[0].slot_offset.is_some());
        assert_eq!(out.deleted[0].runs.len(), 1);
        assert_eq!(out.deleted[0].runs[0].lcn, Some(1));
        // Cluster 2 (idx 0) is allocated (EOC in FAT); idx 1 free.
        assert!(out.alloc.is_allocated_idx(0));
        assert!(!out.alloc.is_allocated_idx(1));
    }

    #[test]
    fn geometry_roundtrip_kind() {
        let g = FatGeometry {
            kind: FatKindWire::ExFat,
            cluster_size: 128 * 1024,
            heap_off: 0x100000,
            fat_copies: vec![0x8000, 0x108000],
            fat_bytes: 0x100000,
            cluster_end: 1024,
            exfat_bitmap: Some((2, 128)),
        };
        let round: FatKind = g.kind.into();
        assert_eq!(round, FatKind::ExFat);
    }

    #[test]
    fn reparse_slot_detects_fat_deletion() {
        let img = fat12_image();
        let mut cursor = std::io::Cursor::new(img.clone());
        let out = collect_fat(&mut cursor, "x", &RunFlags::default(), None).unwrap();
        let g = out.geometry.clone();
        let slot = out.deleted[0].slot_offset.unwrap();
        let mut reread = std::io::Cursor::new(img.clone());
        // Still deleted (0xE5 first byte).
        assert!(reparse_slot(&g, &mut reread, slot).unwrap());
        // Simulate reuse: overwrite the slot's first byte.
        let mut reused = img.clone();
        let si = slot as usize;
        reused[si] = b'Z';
        let mut r2 = std::io::Cursor::new(reused);
        assert!(!reparse_slot(&g, &mut r2, slot).unwrap());
    }

    #[test]
    fn reload_alloc_matches_scan_time_table() {
        let img = fat12_image();
        let mut cursor = std::io::Cursor::new(img.clone());
        let out = collect_fat(&mut cursor, "x", &RunFlags::default(), None).unwrap();
        let mut reread = std::io::Cursor::new(img);
        let reloaded = reload_alloc(&out.geometry, &mut reread);
        for idx in 0..10u64 {
            assert_eq!(
                out.alloc.is_allocated_idx(idx),
                reloaded.is_allocated_idx(idx),
                "idx {idx}"
            );
        }
    }

    #[test]
    fn fat16_classification_by_cluster_count() {
        // Cluster count between 4085 and 65525 => FAT16.
        let mut bs = [0u8; 512];
        bs[3..11].copy_from_slice(b"MSDOS5.0");
        bs[11..13].copy_from_slice(&512u16.to_le_bytes());
        bs[13] = 1;
        bs[14..16].copy_from_slice(&1u16.to_le_bytes());
        bs[16] = 1;
        bs[17..19].copy_from_slice(&16u16.to_le_bytes());
        bs[21] = 0xF8;
        bs[22..24].copy_from_slice(&8u16.to_le_bytes()); // fat sectors
        let total = 1 + 8 + 4 + 20_000; // data clusters ~20000
        bs[32..36].copy_from_slice(&(total as u32).to_le_bytes());
        let boot = parse_boot(&bs).unwrap();
        assert_eq!(boot.kind, FatKind::Fat16);
    }

    #[test]
    fn invalid_boot_rejected() {
        assert!(parse_boot(&[0u8; 512]).is_err());
        let mut bs = [0u8; 512];
        bs[3..11].copy_from_slice(b"MSDOS5.0");
        bs[13] = 3; // not a power of two
        assert!(parse_boot(&bs).is_err());
    }
}
