// SPDX-License-Identifier: GPL-3.0-or-later
//
// Directory entry parsing for FAT (8.3 names plus VFAT long names) and
// exFAT (file entry sets) — DiskWizard.
//
// Adapted from SectorForge (MIT licensed, Copyright (c) 2026 terra
// seraph); both formats keep deleted entries in place with one byte
// changed, so deleted names come back too.
//
// Differences from the original: entries carry their byte offset inside
// the directory stream (for slot re-checks at read-back time) and
// timestamps are Unix seconds rather than FILETIME.

use super::{u16_at, u32_at, Loc};
use std::time::{SystemTime, UNIX_EPOCH};

const DELETED: u8 = 0xE5;
const ATTR_DIR: u8 = 0x10;
const ATTR_VOLUME: u8 = 0x08;
const ATTR_LFN: u8 = 0x0F;

/// One parsed directory entry (live or deleted).
#[derive(Debug, Clone, Default)]
pub(super) struct RawEntry {
    pub name: String,
    pub is_dir: bool,
    pub deleted: bool,
    pub loc: Loc,
    pub modified_unix: i64,
}

/// FAT date/time to Unix seconds. FAT stores local time with no zone;
/// the value is labelled UTC (documented limitation, same as every FAT
/// undelete tool). 0 for an invalid date.
pub(super) fn dos_to_unix(date: u16, time: u16) -> i64 {
    let (y, m, d) = (
        1980 + (date >> 9) as i64,
        ((date >> 5) & 15) as u32,
        (date & 31) as u32,
    );
    let (h, mi, s) = (
        (time >> 11) as i64,
        ((time >> 5) & 63) as i64,
        ((time & 31) * 2) as i64,
    );
    if !(1..=12).contains(&m) || d == 0 || h > 23 || mi > 59 || s > 59 {
        return 0;
    }
    // Days since 1970 (Howard Hinnant's algorithm).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2.rem_euclid(400);
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + h * 3600 + mi * 60 + s
}

/// Plausible as an 8.3 name: printable, none of the characters FAT forbids.
fn valid_short(e: &[u8], deleted: bool) -> bool {
    let first_ok = deleted || e[0] == 0x05 || e[0] > 0x20;
    first_ok
        && e[1..11]
            .iter()
            .all(|&c| c >= 0x20 && !b"\"*+,/:;<=>?[\\]|".contains(&c))
        && e[0..8] != [b' '; 8]
}

fn short_name(e: &[u8], deleted: bool) -> String {
    let case = e[12];
    let part = |b: &[u8], lower: bool| -> String {
        let s: String = b
            .iter()
            .map(|&c| char::from(c))
            .collect::<String>()
            .trim_end()
            .to_string();
        if lower {
            s.to_lowercase()
        } else {
            s
        }
    };
    let mut base = e[0..8].to_vec();
    base[0] = match base[0] {
        0x05 => DELETED,
        _ if deleted => b'_', // the real first character is gone
        c => c,
    };
    let (base, ext) = (
        part(&base, case & 0x08 != 0),
        part(&e[8..11], case & 0x10 != 0),
    );
    if ext.is_empty() {
        base
    } else {
        format!("{base}.{ext}")
    }
}

pub(super) fn lfn_checksum(short: &[u8]) -> u8 {
    short
        .iter()
        .fold(0u8, |s, &c| s.rotate_right(1).wrapping_add(c))
}

/// The long name from the LFN entries just before an 8.3 entry (in disk
/// order, so last part first), if they belong to it.
fn long_name(parts: &[&[u8]], short: &[u8], deleted: bool) -> Option<String> {
    let sum = parts.first()?[13];
    if parts.iter().any(|p| p[13] != sum) {
        return None;
    }
    // A deleted entry's first byte was overwritten, so neither the 8.3
    // checksum nor the sequence numbers can be checked; agreeing
    // checksums among the parts have to do.
    if deleted {
        if parts.iter().any(|p| p[0] != DELETED) {
            return None;
        }
    } else {
        let ords_ok = parts.iter().enumerate().all(|(i, p)| {
            let want = (parts.len() - i) as u8 | if i == 0 { 0x40 } else { 0 };
            p[0] == want
        });
        if !ords_ok || lfn_checksum(&short[0..11]) != sum {
            return None;
        }
    }
    let mut units = Vec::with_capacity(parts.len() * 13);
    for p in parts.iter().rev() {
        for r in [1..11, 14..26, 28..32] {
            units.extend(
                p[r].chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]])),
            );
        }
    }
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    let name = String::from_utf16_lossy(&units[..end]);
    (!name.is_empty()).then_some(name)
}

/// Entries of a FAT directory, with their byte offset in the stream.
/// With `strict` (a deleted directory, whose clusters may run on into
/// other data), stops at the first entry that doesn't look like one
/// instead of skipping it.
pub(super) fn parse_fat(buf: &[u8], fat32: bool, strict: bool) -> Vec<(usize, RawEntry)> {
    let mut out = Vec::new();
    if strict && !buf.starts_with(b".          ") {
        return out; // its first cluster no longer holds this directory
    }
    let mut lfn: Vec<&[u8]> = Vec::new();
    for (off, e) in buf.chunks_exact(32).enumerate() {
        if e[0] == 0 {
            break;
        }
        let attr = e[11];
        if attr & 0x3F == ATTR_LFN {
            lfn.push(e);
            continue;
        }
        let parts = std::mem::take(&mut lfn);
        let deleted = e[0] == DELETED;
        if attr & ATTR_VOLUME != 0 || e[0] == b'.' {
            continue;
        }
        if attr & 0xC0 != 0 || !valid_short(e, deleted) {
            if strict {
                break;
            }
            continue;
        }
        let is_dir = attr & ATTR_DIR != 0;
        let hi = if fat32 {
            (u16_at(e, 20) as u32) << 16
        } else {
            0
        };
        let size = if is_dir { 0 } else { u32_at(e, 28) as u64 };
        out.push((
            off * 32,
            RawEntry {
                name: long_name(&parts, e, deleted).unwrap_or_else(|| short_name(e, deleted)),
                is_dir,
                deleted,
                loc: Loc {
                    first: hi | u16_at(e, 26) as u32,
                    size,
                    valid: size,
                    contiguous: false,
                    deleted,
                },
                modified_unix: dos_to_unix(u16_at(e, 24), u16_at(e, 22)),
            },
        ));
    }
    out
}

/// (first cluster, length) of the allocation bitmap, from the root.
pub(super) fn exfat_bitmap(buf: &[u8]) -> Option<(u32, u64)> {
    buf.chunks_exact(32)
        .take_while(|e| e[0] != 0)
        .find(|e| e[0] & 0x7F == 0x01)
        .map(|e| (u32_at(e, 20), u64_at_ex(e, 24)))
}

fn u64_at_ex(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap_or([0; 8]))
}

fn set_checksum(set: &[u8]) -> u16 {
    set.iter()
        .enumerate()
        .filter(|(i, _)| !matches!(i, 2 | 3))
        .fold(0u16, |s, (i, &b)| {
            // Deleting clears bit 7 of each type byte; check as if it
            // were set.
            let b = if i % 32 == 0 { b | 0x80 } else { b };
            s.rotate_right(1).wrapping_add(u16::from(b))
        })
}

/// exFAT timestamp: packed date/time, 10 ms increment, UTC offset byte.
fn exfat_time_unix(ts: u32, off: u8) -> i64 {
    let minutes = if off & 0x80 != 0 {
        (i16::from(off << 1) >> 1) as i32 * 15
    } else {
        0
    };
    let unix = dos_to_unix((ts >> 16) as u16, ts as u16);
    if unix == 0 {
        return 0;
    }
    unix - i64::from(minutes) * 60
}

/// File entry sets (file + stream extension + names) in an exFAT
/// directory, live and deleted, with byte offsets. Sets whose checksum
/// fails are skipped.
pub(super) fn parse_exfat(buf: &[u8]) -> Vec<(usize, RawEntry)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 32 <= buf.len() {
        let t = buf[i];
        if t == 0 {
            break;
        }
        let in_use = t & 0x80;
        let count = buf[i + 1] as usize;
        let end = i + 32 * (count + 1);
        if t & 0x7F != 0x05 || !(2..=18).contains(&count) || end > buf.len() {
            i += 32;
            continue;
        }
        let set = &buf[i..end];
        let types_ok =
            set[32] == 0x40 | in_use && set[64..].chunks_exact(32).all(|n| n[0] == 0x41 | in_use);
        if !types_ok || set_checksum(set) != u16_at(set, 2) {
            i += 32;
            continue;
        }
        let stream = &set[32..64];
        let name_len = stream[3] as usize;
        let units: Vec<u16> = set[64..]
            .chunks_exact(32)
            .flat_map(|n| {
                n[2..32]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
            })
            .take(name_len)
            .collect();
        let size = u64_at_ex(stream, 24);
        let deleted = in_use == 0;
        out.push((
            i,
            RawEntry {
                name: String::from_utf16_lossy(&units),
                is_dir: u16_at(set, 4) & u16::from(ATTR_DIR) != 0,
                deleted,
                loc: Loc {
                    first: u32_at(stream, 20),
                    size,
                    valid: u64_at_ex(stream, 8).min(size),
                    contiguous: stream[1] & 2 != 0,
                    deleted,
                },
                modified_unix: exfat_time_unix(u32_at(set, 12), set[23]),
            },
        ));
        i = end;
    }
    out
}

/// Current Unix seconds (for tests that need a sanity bound).
#[allow(dead_code)]
pub(super) fn now_unix_bound() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dos_to_unix_known_values() {
        // 2024-03-11 09:12:00
        let date = (2024 - 1980) << 9 | 3 << 5 | 11;
        let time = 9 << 11 | 12 << 5;
        assert_eq!(dos_to_unix(date, time), 1_710_148_320);
        // 2000-02-29 23:59:58 (leap day)
        let date = (2000 - 1980) << 9 | 2 << 5 | 29;
        let time = 23 << 11 | 59 << 5 | 29;
        assert_eq!(dos_to_unix(date, time), 951_868_798);
        assert_eq!(dos_to_unix(0, 0), 0);
    }

    #[test]
    fn short_name_case_flags() {
        let mut e = [0u8; 32];
        e[0..8].copy_from_slice(b"HELLO   ");
        e[8..11].copy_from_slice(b"TXT");
        e[11] = 0x20;
        e[12] = 0x08; // base lowercase
        assert_eq!(short_name(&e, false), "hello.TXT");
        e[12] = 0x18; // both lowercase
        assert_eq!(short_name(&e, false), "hello.txt");
        e[12] = 0x00;
        assert_eq!(short_name(&e, false), "HELLO.TXT");
    }

    #[test]
    fn deleted_short_name_first_char_replaced() {
        let mut e = [0u8; 32];
        e[0..8].copy_from_slice(b"FILE    ");
        e[8..11].copy_from_slice(b"DAT");
        e[11] = 0x20;
        assert_eq!(short_name(&e, true), "_ILE.DAT");
    }

    #[test]
    fn lfn_assembly() {
        // One LFN part + 8.3 entry, checksum must match.
        let short = {
            let mut e = [0u8; 32];
            e[0..8].copy_from_slice(b"LONGFI~1");
            e[8..11].copy_from_slice(b"TXT");
            e[11] = 0x20;
            e
        };
        let mut lfn = [0u8; 32];
        lfn[0] = 0x41; // last part, seq 1
        lfn[11] = ATTR_LFN;
        lfn[13] = lfn_checksum(&short[0..11]);
        // UTF-16 "abc" + terminator + padding 0xFFFF.
        let units: Vec<u16> = vec![
            0x0061, 0x0062, 0x0063, 0, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
            0xFFFF, 0xFFFF,
        ];
        let all: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        lfn[1..1 + all.len().min(10)].copy_from_slice(&all[..10.min(all.len())]);
        lfn[14..14 + (all.len() - 10).min(12)]
            .copy_from_slice(&all[10..10 + (all.len() - 10).min(12)]);
        let parts: Vec<&[u8]> = vec![&lfn];
        assert_eq!(long_name(&parts, &short, false).as_deref(), Some("abc"));
    }

    #[test]
    fn exfat_entry_set_parse() {
        // Build a minimal 2-entry set: file + stream, no name entries
        // (count=1 means stream only; name needs count>=2 with one name
        // entry — use count=2).
        let mut set = vec![0u8; 96];
        // File entry (0x85 allocated).
        set[0] = 0x85;
        set[1] = 2; // 2 secondary entries
        set[4..6].copy_from_slice(&0u16.to_le_bytes()); // attributes: not dir
        set[8..12].copy_from_slice(&0u32.to_le_bytes()); // created
        set[12..16].copy_from_slice(&0u32.to_le_bytes()); // modified
                                                          // Stream entry (0xC0).
        set[32] = 0xC0;
        set[33] = 0x02; // NoFatChain
        set[35] = 0; // name length unused here (no name entries parsed)
        set[52..56].copy_from_slice(&7u32.to_le_bytes()); // first cluster
        set[56..64].copy_from_slice(&1234u64.to_le_bytes()); // size
                                                             // Name entry (0xC1) — content irrelevant (len 0).
        set[64] = 0xC1;
        // Checksum over the set (bytes 2,3 excluded).
        let cs = set_checksum(&set);
        set[2..4].copy_from_slice(&cs.to_le_bytes());
        let entries = parse_exfat(&set);
        assert_eq!(entries.len(), 1);
        let (_, e) = &entries[0];
        assert!(!e.deleted);
        assert!(e.is_dir == false);
        assert_eq!(e.loc.first, 7);
        assert_eq!(e.loc.size, 1234);
        assert!(e.loc.contiguous);

        // Deleted variant: clear bit 7 of all type bytes (checksum treats
        // them as set, so it stays valid).
        let mut del = set.clone();
        del[0] = 0x05;
        del[32] = 0x40;
        del[64] = 0x41;
        let cs2 = set_checksum(&del);
        del[2..4].copy_from_slice(&cs2.to_le_bytes());
        let entries2 = parse_exfat(&del);
        assert_eq!(entries2.len(), 1);
        assert!(entries2[0].1.deleted);
    }

    #[test]
    fn exfat_bitmap_entry() {
        let mut root = [0u8; 128];
        root[0] = 0x81; // allocation bitmap
        root[20..24].copy_from_slice(&2u32.to_le_bytes()); // first cluster
        root[24..32].copy_from_slice(&128u64.to_le_bytes()); // length
        assert_eq!(exfat_bitmap(&root), Some((2, 128)));
        assert_eq!(exfat_bitmap(&[0u8; 128]), None);
    }
}
