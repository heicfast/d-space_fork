// SPDX-License-Identifier: GPL-3.0-or-later
//
// ext2/3/4 file listing + deleted-file recovery — DiskWizard.
//
// Engine logic adapted from SectorForge (MIT licensed,
// Copyright (c) 2026 terra seraph — https://github.com/terraseraph/
// sectorforge), reworked for DiskWizard's types (RawMftEntry /
// DeletedEntry tree model, ReadSeek raw volume handles, progress/pause/
// cancel flags) and GPL-3.0 relicensed with the MIT notice retained.
//
// Model:
//   * Live files are read through their extent tree (ext4) or direct /
//     indirect block map (ext2), or inline data.
//   * Deleting a file on ext3/ext4 normally wipes its block pointers, so
//     deleted names — found in the slack space of directory entries —
//     are only recoverable when their freed inode still maps data;
//     otherwise they grade Poor instead of being hidden.
//   * Names whose inode is in use again (link count != 0) are rename
//     leftovers and are skipped.
//   * Holes in the block map read back as zeros (sparse runs).

use crate::error::{Error, Result};
use crate::recovery::collect::{CollectProgress, DeletedEntry, DwRun, RunFlags};
use crate::scan::walk::RawMftEntry;
use std::collections::{HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::time::Instant;

/// ext2 root inode.
const ROOT_INODE: u32 = 2;
/// Live directories bigger than this are corrupt chains, not directories.
const MAX_DIR_BYTES: u64 = 64 * 1024 * 1024;
/// Cap on extent-tree nodes walked per inode (defense against loops).
const MAX_EXTENT_NODES: usize = 1 << 16;

const INCOMPAT_FILETYPE: u32 = 0x2;
const INCOMPAT_JOURNAL_DEV: u32 = 0x8;
const INCOMPAT_META_BG: u32 = 0x10;
const INCOMPAT_64BIT: u32 = 0x80;
const RO_COMPAT_SPARSE_SUPER: u32 = 0x1;

const FL_ENCRYPT: u32 = 0x800;
const FL_INDEX: u32 = 0x1000;
const FL_EXTENTS: u32 = 0x8_0000;
const FL_INLINE_DATA: u32 = 0x1000_0000;

const S_IFMT: u16 = 0xF000;
const S_IFDIR: u16 = 0x4000;
const S_IFREG: u16 = 0x8000;

/// First synthetic id for ext entries (mirrors the NTFS record-16
/// convention; the tree builder synthesizes root id 5).
pub const FIRST_EXT_RECORD: u64 = 16;
/// Synthetic root id (shared with the other engines).
pub const EXT_ROOT_RECORD: u64 = 5;

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap_or([0; 4]))
}

fn read_at<R: Read + Seek>(r: &mut R, abs: u64, out: &mut [u8]) -> std::io::Result<()> {
    r.seek(SeekFrom::Start(abs))?;
    r.read_exact(out)
}

// ---------------------------------------------------------------------------
// Geometry (persisted with sessions; powers read-back)
// ---------------------------------------------------------------------------

/// Everything needed to re-open and re-check an ext volume at read-back
/// time (inode re-parse + fresh block-bitmap allocation checks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtGeometry {
    pub block_size: u64,
    pub blocks: u64,
    pub first_data_block: u64,
    pub blocks_per_group: u64,
    pub inodes_per_group: u64,
    pub inode_size: u64,
    pub inodes: u64,
    pub filetype: bool,
    /// (block-bitmap block, inode-table block) per group.
    pub groups: Vec<(u64, u64)>,
}

/// Allocation view over the per-group block bitmaps (bit set = in use).
pub struct ExtAllocMap {
    /// Concatenated per-group bitmaps.
    pub data: Vec<u8>,
    pub blocks_per_group: u64,
    pub first_data_block: u64,
}

impl ExtAllocMap {
    /// `true` when the block belongs to something now (or can't be told).
    pub fn is_allocated_block(&self, block: u64) -> bool {
        let rel = block.saturating_sub(self.first_data_block);
        let (g, bit) = (rel / self.blocks_per_group, rel % self.blocks_per_group);
        let per = self.blocks_per_group.div_ceil(8) as usize;
        let byte = self
            .data
            .get((g as usize) * per + (bit / 8) as usize)
            .copied()
            .unwrap_or(0xFF);
        byte & (1 << (bit % 8)) != 0
    }
}

impl crate::recovery::deleted::AllocMap for ExtAllocMap {
    /// Ext run "cluster" indices are physical block numbers (the run list
    /// is materialized dense-logical, but allocation checks use the
    /// physical block stored in the run).
    fn is_allocated(&self, cluster: u64) -> bool {
        self.is_allocated_block(cluster)
    }
}

// ---------------------------------------------------------------------------
// Inodes + block maps (pure)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Inode {
    mode: u16,
    size: u64,
    links: u16,
    flags: u32,
    mtime: u32,
    block: [u8; 60],
}

impl Inode {
    fn is_dir(&self) -> bool {
        self.mode & S_IFMT == S_IFDIR
    }
}

/// Where a file's blocks are: (first logical block, first physical block,
/// count). Holes are simply absent.
#[derive(Debug, Clone, Default)]
struct Map {
    extents: Vec<(u64, u64, u64)>,
    /// Data stored in the inode itself (inline data).
    inline: Option<Vec<u8>>,
    /// Part of the map couldn't be read or made no sense.
    incomplete: bool,
}

/// Whether group `g` holds a superblock backup (sparse_super placement).
fn has_super(g: u64, sparse: bool) -> bool {
    if !sparse || g <= 1 {
        return true;
    }
    [3u64, 5, 7].iter().any(|&b| {
        let mut x = g;
        while x % b == 0 {
            x /= b;
        }
        x == 1
    })
}

// ---------------------------------------------------------------------------
// Volume
// ---------------------------------------------------------------------------

struct ExtVolume {
    block_size: u64,
    blocks: u64,
    first_data_block: u64,
    blocks_per_group: u64,
    inodes_per_group: u64,
    inode_size: u64,
    inodes: u64,
    filetype: bool,
    groups: Vec<(u64, u64)>,
}

impl ExtVolume {
    fn open<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut sb = vec![0u8; 1024];
        read_at(reader, 1024, &mut sb)
            .map_err(|e| Error::Scan(format!("the ext superblock can't be read: {e}")))?;
        if u16_at(&sb, 56) != 0xEF53 || u32_at(&sb, 24) > 6 {
            return Err(Error::Scan("no valid ext superblock on this volume".into()));
        }
        let incompat = u32_at(&sb, 96);
        if incompat & INCOMPAT_JOURNAL_DEV != 0 {
            return Err(Error::Scan(
                "this is an external ext journal device, which holds no files".into(),
            ));
        }
        let wide = incompat & INCOMPAT_64BIT != 0;
        let block_size = 1024u64 << u32_at(&sb, 24);
        let blocks = u32_at(&sb, 4) as u64
            | if wide {
                (u32_at(&sb, 0x150) as u64) << 32
            } else {
                0
            };
        let first_data_block = u32_at(&sb, 20) as u64;
        let blocks_per_group = u32_at(&sb, 32) as u64;
        let inodes_per_group = u32_at(&sb, 40) as u64;
        let inode_size = if u32_at(&sb, 76) == 0 {
            128
        } else {
            u16_at(&sb, 88) as u64
        };
        let desc_size = if wide {
            (u16_at(&sb, 254) as u64).max(64)
        } else {
            32
        };
        if blocks_per_group == 0
            || inodes_per_group == 0
            || !(128..=block_size).contains(&inode_size)
            || desc_size > block_size
        {
            return Err(Error::Scan(
                "the ext superblock's geometry makes no sense".into(),
            ));
        }
        let ngroups = blocks
            .saturating_sub(first_data_block)
            .div_ceil(blocks_per_group);

        let per_block = block_size / desc_size;
        let meta_bg = incompat & INCOMPAT_META_BG != 0;
        let first_meta_bg = if meta_bg {
            u32_at(&sb, 0x104) as u64
        } else {
            u64::MAX
        };
        let sparse = u32_at(&sb, 100) & RO_COMPAT_SPARSE_SUPER != 0;
        let mut groups: Vec<(u64, u64)> = Vec::new();
        let mut blk = vec![0u8; block_size as usize];
        for i in 0..ngroups.div_ceil(per_block) {
            let at = if i < first_meta_bg {
                first_data_block + 1 + i
            } else {
                let g = i * per_block;
                first_data_block + g * blocks_per_group + u64::from(has_super(g, sparse))
            };
            if read_at(reader, at * block_size, &mut blk).is_err() && i == 0 {
                return Err(Error::Scan(
                    "the ext group descriptor table can't be read".into(),
                ));
            }
            for d in blk.chunks_exact(desc_size as usize) {
                if groups.len() as u64 == ngroups {
                    break;
                }
                let hi = |o: usize| {
                    if desc_size >= 64 {
                        (u32_at(d, o) as u64) << 32
                    } else {
                        0
                    }
                };
                groups.push((
                    u32_at(d, 0) as u64 | hi(0x20),
                    u32_at(d, 8) as u64 | hi(0x28),
                ));
            }
        }
        Ok(ExtVolume {
            block_size,
            blocks,
            first_data_block,
            blocks_per_group,
            inodes_per_group,
            inode_size,
            inodes: u32_at(&sb, 0) as u64,
            filetype: incompat & INCOMPAT_FILETYPE != 0,
            groups,
        })
    }

    fn read_block<R: Read + Seek>(&self, reader: &mut R, b: u64, out: &mut [u8]) -> bool {
        // Returns true when the block is unusable (beyond the volume or
        // read failure); callers zero-fill.
        if b >= self.blocks {
            out.fill(0);
            return true;
        }
        if read_at(reader, b * self.block_size, out).is_err() {
            out.fill(0);
            return true;
        }
        false
    }

    fn inode<R: Read + Seek>(&self, reader: &mut R, n: u32) -> Option<Inode> {
        if n == 0 || u64::from(n) > self.inodes {
            return None;
        }
        let (g, i) = (
            (u64::from(n) - 1) / self.inodes_per_group,
            (u64::from(n) - 1) % self.inodes_per_group,
        );
        let table = self.groups.get(g as usize)?.1;
        let mut b = vec![0u8; self.inode_size as usize];
        if table == 0
            || read_at(
                reader,
                table * self.block_size + i * self.inode_size,
                &mut b,
            )
            .is_err()
        {
            return None;
        }
        Some(Inode {
            mode: u16_at(&b, 0),
            size: u32_at(&b, 4) as u64 | (u64::from(u32_at(&b, 108)) << 32),
            links: u16_at(&b, 26),
            flags: u32_at(&b, 32),
            mtime: u32_at(&b, 16),
            block: b[40..100].try_into().ok()?,
        })
    }

    fn map<R: Read + Seek>(&self, reader: &mut R, ino: &Inode) -> Map {
        let needed = ino.size.div_ceil(self.block_size);
        let mut m = Map::default();
        if ino.flags & FL_INLINE_DATA != 0 {
            let n = (ino.size as usize).min(60);
            m.inline = Some(ino.block[..n].to_vec());
            // The rest of inline data lives in an extended attribute.
            m.incomplete = ino.size > 60;
        } else if ino.flags & FL_EXTENTS != 0 {
            let mut nodes = 0usize;
            self.walk_extents(reader, &ino.block, 0, &mut m, &mut nodes);
        } else {
            let ptr = |i: usize| u32_at(&ino.block, i * 4) as u64;
            for i in 0..12u64 {
                self.push_block(&mut m, i, ptr(i as usize));
            }
            let per = self.block_size / 4;
            let mut first = 12u64;
            for (level, slot) in [(1u32, 12usize), (2, 13), (3, 14)] {
                if first >= needed {
                    break;
                }
                self.walk_indirect(reader, ptr(slot), level, first, needed, &mut m);
                first += per.pow(level);
            }
        }
        // Blocks beyond what the size needs aren't data; past the end of
        // the volume they're garbage.
        m.extents.retain(|e| e.0 < needed);
        let blocks = self.blocks;
        if m.extents.iter().any(|e| e.1 + e.2 > blocks) {
            m.incomplete = true;
            m.extents.retain(|e| e.1 + e.2 <= blocks);
        }
        m
    }

    fn push_block(&self, m: &mut Map, logical: u64, phys: u64) {
        if phys == 0 {
            return;
        }
        match m.extents.last_mut() {
            Some((l, p, n)) if *l + *n == logical && *p + *n == phys => *n += 1,
            _ => m.extents.push((logical, phys, 1)),
        }
    }

    fn walk_indirect<R: Read + Seek>(
        &self,
        reader: &mut R,
        blk: u64,
        level: u32,
        first: u64,
        needed: u64,
        m: &mut Map,
    ) {
        if blk == 0 {
            return;
        }
        let per = self.block_size / 4;
        let mut buf = vec![0u8; self.block_size as usize];
        if self.read_block(reader, blk, &mut buf) {
            m.incomplete = true;
            return;
        }
        let span = per.pow(level - 1);
        for i in 0..per {
            let logical = first + i * span;
            if logical >= needed {
                break;
            }
            let p = u32_at(&buf, (i * 4) as usize) as u64;
            if level == 1 {
                self.push_block(m, logical, p);
            } else {
                self.walk_indirect(reader, p, level - 1, logical, needed, m);
            }
        }
    }

    fn walk_extents<R: Read + Seek>(
        &self,
        reader: &mut R,
        node: &[u8],
        depth_seen: u32,
        m: &mut Map,
        nodes: &mut usize,
    ) {
        *nodes += 1;
        if node.len() < 12
            || u16_at(node, 0) != 0xF30A
            || depth_seen > 5
            || *nodes > MAX_EXTENT_NODES
        {
            m.incomplete |= depth_seen > 0 || (node.len() >= 2 && u16_at(node, 0) == 0xF30A);
            return;
        }
        let entries = (u16_at(node, 2) as usize).min((node.len() - 12) / 12);
        let depth = u16_at(node, 6);
        for i in 0..entries {
            let e = &node[12 + i * 12..24 + i * 12];
            if depth == 0 {
                let raw_len = u16_at(e, 4) as u64;
                // Above 32768: preallocated but never written, reads as
                // zeros.
                if raw_len > 32768 {
                    continue;
                }
                let phys = (u16_at(e, 6) as u64) << 32 | u32_at(e, 8) as u64;
                m.extents.push((u32_at(e, 0) as u64, phys, raw_len));
            } else {
                let child = (u16_at(e, 8) as u64) << 32 | u32_at(e, 4) as u64;
                let mut buf = vec![0u8; self.block_size as usize];
                if self.read_block(reader, child, &mut buf) {
                    m.incomplete = true;
                    continue;
                }
                self.walk_extents(reader, &buf, depth_seen + 1, m, nodes);
            }
        }
    }

    /// Concatenated per-group block bitmaps (unreadable groups read as
    /// fully allocated — never optimistic).
    fn load_bitmap<R: Read + Seek>(&self, reader: &mut R) -> Vec<u8> {
        let per = self.blocks_per_group.div_ceil(8) as usize;
        let mut bm = vec![0u8; per * self.groups.len()];
        let mut blk = vec![0u8; self.block_size as usize];
        for (g, &(b, _)) in self.groups.iter().enumerate() {
            let dst = &mut bm[g * per..(g + 1) * per];
            if b == 0 || self.read_block(reader, b, &mut blk) {
                dst.fill(0xFF);
            } else {
                let n = per.min(blk.len());
                dst[..n].copy_from_slice(&blk[..n]);
            }
        }
        bm
    }

    /// Reads `out.len()` bytes at byte `pos` of a mapped file. Holes are
    /// zeros. Returns true when part of the read failed.
    fn read_mapped<R: Read + Seek>(
        &self,
        reader: &mut R,
        m: &Map,
        pos: u64,
        out: &mut [u8],
    ) -> bool {
        out.fill(0);
        if let Some(d) = &m.inline {
            let s = (pos as usize).min(d.len());
            let n = (d.len() - s).min(out.len());
            out[..n].copy_from_slice(&d[s..s + n]);
            return false;
        }
        let bs = self.block_size;
        let end = pos + out.len() as u64;
        let mut bad = false;
        for &(l, p, n) in &m.extents {
            let (rs, re) = (l * bs, (l + n) * bs);
            let (s, e) = (rs.max(pos), re.min(end));
            if s < e {
                let abs = p * bs + (s - rs);
                if read_at(
                    reader,
                    abs,
                    &mut out[(s - pos) as usize..(e - pos) as usize],
                )
                .is_err()
                {
                    bad = true;
                }
            }
        }
        bad
    }

    /// Directory bytes of an inode (bounded).
    fn dir_bytes<R: Read + Seek>(&self, reader: &mut R, ino: &Inode) -> Vec<u8> {
        let m = self.map(reader, ino);
        let mut buf = vec![0u8; ino.size.min(MAX_DIR_BYTES) as usize];
        self.read_mapped(reader, &m, 0, &mut buf);
        buf
    }

    /// Live entries and deleted ones left in the slack after a live
    /// entry's name: (inode, name, deleted).
    fn parse_dir(&self, buf: &[u8], hashed: bool) -> Vec<(u32, String, bool)> {
        let bs = self.block_size as usize;
        let name_len = |e: &[u8]| {
            if self.filetype {
                e[6] as usize
            } else {
                u16_at(e, 6) as usize
            }
        };
        let mut out = Vec::new();
        let mut pos = 0;
        while pos + 8 <= buf.len() {
            let e = &buf[pos..];
            let (ino, rec_len, nl) = (u32_at(e, 0), u16_at(e, 4) as usize, name_len(e));
            if rec_len < 8
                || rec_len % 4 != 0
                || pos + rec_len > buf.len()
                || pos / bs != (pos + rec_len - 1) / bs
            {
                pos = (pos / bs + 1) * bs; // damaged block: try the next one
                continue;
            }
            let name = &e[8..(8 + nl).min(rec_len)];
            if ino != 0 && nl > 0 && 8 + nl <= rec_len {
                out.push((ino, String::from_utf8_lossy(name).into_owned(), false));
            }
            // A hashed directory hides its index in ".." and in empty
            // entries; that isn't slack.
            let index_block = hashed && ((ino == 0 && nl == 0) || name == b"..");
            let used = if ino == 0 { 0 } else { (8 + nl + 3) & !3 };
            if !index_block {
                let mut q = pos + used;
                while q + 12 <= pos + rec_len {
                    match self.deleted_entry(&buf[q..pos + rec_len]) {
                        Some((i, n, len)) => {
                            out.push((i, n, true));
                            q += len;
                        }
                        None => q += 4,
                    }
                }
            }
            pos += rec_len;
        }
        out.retain(|(_, n, _)| n != "." && n != "..");
        out
    }

    fn deleted_entry(&self, b: &[u8]) -> Option<(u32, String, usize)> {
        let (ino, rec_len) = (u32_at(b, 0), u16_at(b, 4) as usize);
        let nl = if self.filetype {
            b[6] as usize
        } else {
            u16_at(b, 6) as usize
        };
        let ok = ino != 0
            && (u64::from(ino) <= self.inodes)
            && rec_len >= 8 + nl
            && rec_len % 4 == 0
            && nl > 0
            && 8 + nl <= b.len()
            && (!self.filetype || (1..=7).contains(&b[7]))
            && b[8..8 + nl].iter().all(|&c| c != 0 && c != b'/');
        ok.then(|| {
            (
                ino,
                String::from_utf8_lossy(&b[8..8 + nl]).into_owned(),
                (8 + nl + 3) & !3,
            )
        })
    }
}

/// Result of the unified ext walk.
pub struct ExtCollected {
    pub existing: Vec<RawMftEntry>,
    pub deleted: Vec<DeletedEntry>,
    pub alloc: ExtAllocMap,
    pub geometry: ExtGeometry,
    pub skipped_errors: u64,
    pub elapsed_ms: u64,
    pub cancelled: bool,
}

/// Materializes an ext block map into the shared dense run list: physical
/// extents become real runs, logical gaps become sparse (zero) runs.
fn map_to_runs(m: &Map, needed_blocks: u64) -> Vec<DwRun> {
    let mut runs: Vec<DwRun> = Vec::new();
    let mut cursor = 0u64;
    for &(l, p, n) in &m.extents {
        if l > cursor {
            // Hole: reads back as zeros.
            runs.push(DwRun {
                lcn: None,
                clusters: l - cursor,
            });
        }
        runs.push(DwRun {
            lcn: Some(p),
            clusters: n,
        });
        cursor = l + n;
    }
    if cursor < needed_blocks {
        runs.push(DwRun {
            lcn: None,
            clusters: needed_blocks - cursor,
        });
    }
    runs
}

/// Walks the ext directory tree breadth-first from the root inode,
/// collecting live entries (tree feed) and deleted slack names (recovery
/// feed). Cancel returns the partial collection (browsable).
pub fn collect_ext<R: Read + Seek>(
    reader: &mut R,
    volume_path: &str,
    flags: &RunFlags,
    progress: Option<&dyn Fn(&CollectProgress)>,
) -> Result<ExtCollected> {
    let start = Instant::now();
    let vol = ExtVolume::open(reader).map_err(|e| Error::Scan(format!("'{volume_path}': {e}")))?;

    let geometry = ExtGeometry {
        block_size: vol.block_size,
        blocks: vol.blocks,
        first_data_block: vol.first_data_block,
        blocks_per_group: vol.blocks_per_group,
        inodes_per_group: vol.inodes_per_group,
        inode_size: vol.inode_size,
        inodes: vol.inodes,
        filetype: vol.filetype,
        groups: vol.groups.clone(),
    };

    let mut existing: Vec<RawMftEntry> = Vec::new();
    let mut deleted: Vec<DeletedEntry> = Vec::new();
    let mut skipped = 0u64;
    let mut last_name = String::new();
    let mut cancelled = false;

    // The root itself (id 5) so the tree builder has a real node.
    existing.push(RawMftEntry {
        record_no: EXT_ROOT_RECORD,
        parent_record_no: EXT_ROOT_RECORD,
        name: String::new(),
        data_size: 0,
        is_dir: true,
        modified_unix: 0,
    });

    let mut next_id = FIRST_EXT_RECORD;
    // (dir_id, dir_inode, dir_deleted)
    let mut queue: VecDeque<(u64, u32, bool)> = VecDeque::new();
    let mut seen: HashSet<u32> = HashSet::from([ROOT_INODE]);
    queue.push_back((EXT_ROOT_RECORD, ROOT_INODE, false));

    let mut dir_count = 1u64;
    while let Some((dir_id, dir_ino, dir_deleted)) = queue.pop_front() {
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
        let Some(d) = vol.inode(reader, dir_ino) else {
            skipped += 1;
            continue;
        };
        let bytes = vol.dir_bytes(reader, &d);
        for (n, name, slack) in vol.parse_dir(&bytes, d.flags & FL_INDEX != 0) {
            let Some(ino) = vol.inode(reader, n) else {
                skipped += 1;
                continue;
            };
            let deleted_flag = slack || dir_deleted;
            // A deleted name whose inode is in use again is a rename
            // leftover or points at someone else's file now.
            if slack && ino.links != 0 {
                continue;
            }
            let kind = ino.mode & S_IFMT;
            if kind != S_IFDIR && kind != S_IFREG {
                continue; // symlinks, devices, sockets, fifos
            }
            let m = vol.map(reader, &ino);
            let id = next_id;
            next_id += 1;
            last_name = name.clone();
            let mtime = i64::from(ino.mtime);

            if !deleted_flag {
                existing.push(RawMftEntry {
                    record_no: id,
                    parent_record_no: dir_id,
                    name,
                    data_size: if ino.is_dir() { 0 } else { ino.size },
                    is_dir: ino.is_dir(),
                    modified_unix: mtime,
                });
            } else {
                let needed = ino.size.div_ceil(vol.block_size);
                deleted.push(DeletedEntry {
                    record_no: id,
                    parent_record_no: dir_id,
                    name,
                    data_size: if ino.is_dir() { 0 } else { ino.size },
                    is_dir: ino.is_dir(),
                    modified_unix: mtime,
                    resident: m.inline.is_some(),
                    runs: map_to_runs(&m, needed),
                    compressed: false,
                    encrypted: ino.flags & FL_ENCRYPT != 0,
                    // ext read-back key: the inode number (link count is
                    // re-checked).
                    slot_key: Some(u64::from(n)),
                });
            }

            if ino.is_dir() && seen.insert(n) {
                queue.push_back((id, n, deleted_flag));
            }
        }
        dir_count += 1;
    }

    let alloc = ExtAllocMap {
        data: vol.load_bitmap(reader),
        blocks_per_group: vol.blocks_per_group,
        first_data_block: vol.first_data_block,
    };

    Ok(ExtCollected {
        existing,
        deleted,
        alloc,
        geometry,
        skipped_errors: skipped,
        elapsed_ms: start.elapsed().as_millis() as u64,
        cancelled,
    })
}

// ---------------------------------------------------------------------------
// Read-back helpers (preview / recovery jobs)
// ---------------------------------------------------------------------------

/// Re-reads an inode by number and returns its fresh state. `still_deleted`
/// is derived from the link count (deleted names keep links == 0 on ext;
/// a nonzero link count means the inode was reused).
pub fn reparse_inode<R: Read + Seek>(
    geometry: &ExtGeometry,
    reader: &mut R,
    inode_no: u32,
    stored_runs: Vec<DwRun>,
    stored_size: u64,
) -> Result<crate::recovery::deleted::FreshRecord> {
    let vol = ExtVolume {
        block_size: geometry.block_size,
        blocks: geometry.blocks,
        first_data_block: geometry.first_data_block,
        blocks_per_group: geometry.blocks_per_group,
        inodes_per_group: geometry.inodes_per_group,
        inode_size: geometry.inode_size,
        inodes: geometry.inodes,
        filetype: geometry.filetype,
        groups: geometry.groups.clone(),
    };
    let ino = vol
        .inode(reader, inode_no)
        .ok_or_else(|| Error::Scan("inode can no longer be read".into()))?;
    let still_deleted = ino.links == 0;
    let mut fresh = crate::recovery::deleted::FreshRecord {
        still_deleted,
        name: None,
        data_size: if ino.is_dir() { 0 } else { ino.size },
        resident: false,
        compressed: false,
        encrypted: ino.flags & FL_ENCRYPT != 0,
        runs: Vec::new(),
        resident_data: Vec::new(),
    };
    if still_deleted {
        let m = vol.map(reader, &ino);
        if m.inline.is_some() {
            fresh.resident = true;
            fresh.resident_data = m.inline.unwrap_or_default();
        } else if !m.extents.is_empty() {
            let needed = ino.size.div_ceil(vol.block_size);
            fresh.runs = map_to_runs(&m, needed);
        } else {
            // Block pointers were wiped when deleted (ext3/ext4) — the
            // scan-time runs are the only knowledge left.
            fresh.runs = stored_runs;
            fresh.data_size = stored_size;
        }
    }
    Ok(fresh)
}

/// Re-loads the per-group block bitmaps from raw geometry.
pub fn reload_alloc<R: Read + Seek>(geometry: &ExtGeometry, reader: &mut R) -> ExtAllocMap {
    let vol = ExtVolume {
        block_size: geometry.block_size,
        blocks: geometry.blocks,
        first_data_block: geometry.first_data_block,
        blocks_per_group: geometry.blocks_per_group,
        inodes_per_group: geometry.inodes_per_group,
        inode_size: geometry.inode_size,
        inodes: geometry.inodes,
        filetype: geometry.filetype,
        groups: geometry.groups.clone(),
    };
    ExtAllocMap {
        data: vol.load_bitmap(reader),
        blocks_per_group: vol.blocks_per_group,
        first_data_block: vol.first_data_block,
    }
}

// ---------------------------------------------------------------------------
// Tests — synthetic tiny ext2 image
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal ext2 (rev 1, 1024-byte blocks, 1 group) with:
    /// root dir (inode 2) containing one live file (inode 12) and one
    /// deleted slack name (inode 13, links 0).
    fn ext2_image() -> Vec<u8> {
        const BS: usize = 1024;
        const BLOCKS: usize = 64;
        let mut img = vec![0u8; BS * BLOCKS];

        // ---- Superblock at byte 1024 (block 1). ----
        let sb = BS;
        img[sb + 0..sb + 4].copy_from_slice(&16u32.to_le_bytes()); // inodes
        img[sb + 4..sb + 8].copy_from_slice(&(BLOCKS as u32).to_le_bytes()); // blocks (lo)
        img[sb + 20..sb + 24].copy_from_slice(&1u32.to_le_bytes()); // first data block
        img[sb + 24..sb + 28].copy_from_slice(&0u32.to_le_bytes()); // log block size
        img[sb + 32..sb + 36].copy_from_slice(&32u32.to_le_bytes()); // blocks per group
        img[sb + 40..sb + 44].copy_from_slice(&16u32.to_le_bytes()); // inodes per group
        img[sb + 56..sb + 58].copy_from_slice(&0xEF53u16.to_le_bytes()); // magic
        img[sb + 76..sb + 80].copy_from_slice(&1u32.to_le_bytes()); // rev level (v1)
        img[sb + 88..sb + 90].copy_from_slice(&128u16.to_le_bytes()); // inode size
        img[sb + 96..sb + 100].copy_from_slice(&INCOMPAT_FILETYPE.to_le_bytes()); // incompat

        // ---- Group descriptor table at block 2. ----
        let gd = 2 * BS;
        img[gd..gd + 4].copy_from_slice(&3u32.to_le_bytes()); // block bitmap block
        img[gd + 8..gd + 12].copy_from_slice(&4u32.to_le_bytes()); // inode table block

        // ---- Block bitmap at block 3: superblock/GDT/bitmap/inode table
        //      blocks 0-4 allocated (bits set). ----
        let bb = 3 * BS;
        img[bb] = 0b0001_1111;

        // ---- Inode table at block 4 (16 inodes × 128 bytes = 2 blocks). ----
        let it = 4 * BS;
        let put_inode =
            |img: &mut Vec<u8>, n: usize, mode: u16, size: u64, links: u16, blocks: &[u32]| {
                let o = it + (n - 1) * 128; // inode table is 1-indexed
                img[o..o + 2].copy_from_slice(&mode.to_le_bytes());
                img[o + 4..o + 8].copy_from_slice(&(size as u32).to_le_bytes());
                img[o + 16..o + 20].copy_from_slice(&1_700_000_000u32.to_le_bytes()); // mtime
                img[o + 26..o + 28].copy_from_slice(&links.to_le_bytes());
                for (i, b) in blocks.iter().enumerate() {
                    let p = o + 40 + i * 4;
                    img[p..p + 4].copy_from_slice(&b.to_le_bytes());
                }
            };
        // Inode 2: root dir, 1 block (block 6), links 2.
        put_inode(
            &mut img,
            2,
            S_IFDIR | 0o755,
            BS as u64,
            2,
            &[6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        // Inode 12: live file, 1 block (block 8), links 1.
        put_inode(
            &mut img,
            12,
            S_IFREG | 0o644,
            100,
            1,
            &[8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        // Inode 13: deleted file, 1 block (block 9), links 0.
        put_inode(
            &mut img,
            13,
            S_IFREG | 0o644,
            50,
            0,
            &[9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );

        // ---- Root directory at block 6: one entry with slack. ----
        let dir = 6 * BS;
        // Entry 1: ".": inode 2, rec_len 12, name ".", filetype 2.
        img[dir..dir + 4].copy_from_slice(&2u32.to_le_bytes());
        img[dir + 4..dir + 6].copy_from_slice(&12u16.to_le_bytes());
        img[dir + 6] = 1; // name len
        img[dir + 7] = 2; // filetype dir
        img[dir + 8] = b'.';
        // Entry 2: live file "live.txt": inode 12, rec_len 32 (8 used, 24 slack).
        img[dir + 12..dir + 16].copy_from_slice(&12u32.to_le_bytes());
        img[dir + 16..dir + 18].copy_from_slice(&32u16.to_le_bytes());
        img[dir + 18] = 8; // name len
        img[dir + 19] = 1; // filetype reg
        img[dir + 20..dir + 28].copy_from_slice(b"live.txt");
        // Deleted slack entry inside entry 2's rec_len: "gone.bin", inode 13.
        img[dir + 28..dir + 32].copy_from_slice(&13u32.to_le_bytes());
        img[dir + 32..dir + 34].copy_from_slice(&16u16.to_le_bytes()); // rec_len (bogus but unused: parsed as slack)
        img[dir + 34] = 8; // name len
        img[dir + 35] = 1; // filetype reg
        img[dir + 36..dir + 44].copy_from_slice(b"gone.bin");
        // Entry 3: terminator ".." covering the rest of the block.
        let e3 = dir + 44;
        img[e3..e3 + 4].copy_from_slice(&2u32.to_le_bytes());
        img[e3 + 4..e3 + 6].copy_from_slice(&((BS - 44) as u16).to_le_bytes());
        img[e3 + 6] = 2;
        img[e3 + 7] = 2;
        img[e3 + 8..e3 + 10].copy_from_slice(b"..");

        // ---- File data: block 8 = live file, block 9 = deleted file. ----
        img[8 * BS..8 * BS + 100].copy_from_slice(&vec![0xABu8; 100]);
        img[9 * BS..9 * BS + 50].copy_from_slice(&vec![0xCDu8; 50]);

        img
    }

    #[test]
    fn ext2_open_and_walk() {
        let mut cursor = std::io::Cursor::new(ext2_image());
        let out = collect_ext(&mut cursor, "test://ext2", &RunFlags::default(), None).unwrap();
        // Root + live.txt exist.
        assert!(out.existing.iter().any(|e| e.name == "live.txt"));
        assert!(out.existing.iter().any(|e| e.record_no == EXT_ROOT_RECORD));
        // The slack name gone.bin is collected as deleted with inode 13.
        assert_eq!(out.deleted.len(), 1);
        assert_eq!(out.deleted[0].name, "gone.bin");
        assert_eq!(out.deleted[0].slot_key, Some(13));
        assert_eq!(out.deleted[0].data_size, 50);
        // One real run at block 9 (1 block of 1024).
        assert_eq!(out.deleted[0].runs.len(), 1);
        assert_eq!(out.deleted[0].runs[0].lcn, Some(9));
        assert_eq!(out.deleted[0].runs[0].clusters, 1);
        // Block 9 is free in the bitmap (bits 0-4 set only) → not
        // allocated → the deleted entry's block is reusable-grade.
        assert!(!out.alloc.is_allocated_block(9));
        assert!(out.alloc.is_allocated_block(3)); // bitmap block itself
    }

    #[test]
    fn ext2_reparse_inode_still_deleted() {
        let img = ext2_image();
        let mut cursor = std::io::Cursor::new(img.clone());
        let out = collect_ext(&mut cursor, "x", &RunFlags::default(), None).unwrap();
        let g = out.geometry.clone();
        let stored = out.deleted[0].clone();
        let mut reread = std::io::Cursor::new(img);
        let fresh = reparse_inode(&g, &mut reread, 13, stored.runs, stored.data_size).unwrap();
        assert!(fresh.still_deleted);
        assert_eq!(fresh.data_size, 50);
        assert_eq!(fresh.runs.len(), 1);
        assert_eq!(fresh.runs[0].lcn, Some(9));
    }

    #[test]
    fn ext2_reparse_detects_reuse() {
        let img = ext2_image();
        let mut cursor = std::io::Cursor::new(img.clone());
        let out = collect_ext(&mut cursor, "x", &RunFlags::default(), None).unwrap();
        let g = out.geometry.clone();
        // Simulate reuse: give inode 13 a nonzero link count.
        let mut reused = img.clone();
        let it = 4 * 1024;
        let o = it + (13 - 1) * 128 + 26; // links field of inode 13
        reused[o..o + 2].copy_from_slice(&1u16.to_le_bytes());
        let mut r = std::io::Cursor::new(reused);
        let fresh = reparse_inode(&g, &mut r, 13, Vec::new(), 0).unwrap();
        assert!(!fresh.still_deleted);
    }

    #[test]
    fn map_to_runs_fills_holes_as_sparse() {
        let m = Map {
            extents: vec![(0, 10, 2), (5, 20, 1)],
            inline: None,
            incomplete: false,
        };
        let runs = map_to_runs(&m, 8);
        // (0..2 real) (2..5 sparse) (5..6 real) (6..8 sparse)
        assert_eq!(runs.len(), 4);
        assert_eq!(
            runs[0],
            DwRun {
                lcn: Some(10),
                clusters: 2
            }
        );
        assert_eq!(
            runs[1],
            DwRun {
                lcn: None,
                clusters: 3
            }
        );
        assert_eq!(
            runs[2],
            DwRun {
                lcn: Some(20),
                clusters: 1
            }
        );
        assert_eq!(
            runs[3],
            DwRun {
                lcn: None,
                clusters: 2
            }
        );
    }

    #[test]
    fn invalid_superblock_rejected() {
        let mut cursor = std::io::Cursor::new(vec![0u8; 8192]);
        assert!(collect_ext(&mut cursor, "x", &RunFlags::default(), None).is_err());
    }
}
