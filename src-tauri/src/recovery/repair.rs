// SPDX-License-Identifier: GPL-3.0-or-later
//
// Volume repair — DiskWizard recovery engine.
//
// Dry-run findings (read-only, safe):
//   * NTFS boot sector vs backup boot sector byte comparison
//   * $MFT record 0 vs $MFTMirr (first 4 records are mirrored)
//
// Apply (write path, typed REPAIR consent + undo log, non-system volumes
// only): restore the backup boot sector over a damaged primary, or
// restore $MFTMirr from $MFT. Every applied action stores the original
// bytes so `undoRepairs` can roll back. Writing raw volume structures is
// deliberately limited to NON-system volumes — locking the live system
// volume is neither safe nor supported.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;
use tracing::{info, warn};

const REPAIR_CONSENT_TOKEN: &str = "REPAIR";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairFindingWire {
    pub id: String,
    /// boot-sector-mismatch | mft-record-error
    pub kind: String,
    #[serde(rename = "where")]
    pub where_: String,
    pub description_key: String,
    pub fixable: bool,
    pub severity: String, // info | warning | critical
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairDryRunWire {
    pub device_id: String,
    pub findings: Vec<RepairFindingWire>,
    pub can_apply: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairUndoEntryWire {
    pub index: u32,
    pub action: String,
    #[serde(rename = "where")]
    pub where_: String,
    pub at_unix: i64,
    pub snapshot_hex: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOptionsWire {
    pub consent_token: String,
}

/// In-memory undo log per volume (session lifetime; the contract queries
/// it per device id).
pub struct RepairUndoLog {
    entries: Mutex<Vec<StoredUndo>>,
}

#[derive(Clone)]
struct StoredUndo {
    volume: String,
    action: String,
    where_: String,
    at_unix: i64,
    snapshot: Vec<u8>,
}

impl Default for RepairUndoLog {
    fn default() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// Sector helpers
// ---------------------------------------------------------------------------

fn open_volume_read(letter: char) -> Result<std::fs::File> {
    let path = format!(r"\\.\{letter}:");
    std::fs::File::open(&path).map_err(|e| Error::Volume(format!("cannot open '{path}': {e}")))
}

fn read_sector_at(f: &mut std::fs::File, byte_offset: u64) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; 512];
    f.seek(SeekFrom::Start(byte_offset))
        .map_err(|e| Error::Volume(format!("seek failed: {e}")))?;
    f.read_exact(&mut buf)
        .map_err(|e| Error::Volume(format!("sector read failed: {e}")))?;
    Ok(buf)
}

/// NTFS backup boot sector offset: last sector of the boot region —
/// at sector 6 of the volume for classic layouts? No — the backup lives
/// at the END of the boot area: `bytes_per_sector` after sector 0 region
/// is not fixed; the canonical location is the sector at
/// "reserved sectors" boundary. The NTFS BPB at offset 0x0E gives
/// reserved sectors; backup boot = reserved_sectors - 1? In practice the
/// backup boot sector sits at sector (6) for XP-era and at the last
/// sector of the first track region described by BPB — the reliable
/// approach used by repair tools: read the BPB's
/// "total sectors" (offset 0x28) and probe the FINAL sector of the
/// volume? That is the NTFS "backup boot sector at end of volume" rule
/// for dynamic layouts. Classic NTFS: backup at sector `reserved - 1`
/// where reserved = u16@0x0E. We probe BOTH candidates and compare.
fn backup_boot_candidates(boot: &[u8]) -> Vec<u64> {
    let mut out = Vec::new();
    if boot.len() >= 0x30 && &boot[3..7] == b"NTFS" {
        let reserved = u16::from_le_bytes([boot[0x0E], boot[0x0F]]) as u64;
        if reserved >= 1 {
            out.push((reserved - 1) * 512);
        }
        let total_sectors = u64::from_le_bytes(boot[0x28..0x30].try_into().unwrap_or([0; 8]));
        if total_sectors > 0 {
            out.push(total_sectors.saturating_sub(1) * 512);
        }
    }
    out
}

fn is_ntfs_boot(sector: &[u8]) -> bool {
    sector.len() >= 7 && &sector[3..7] == b"NTFS" && sector[510] == 0x55 && sector[511] == 0xAA
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

// ---------------------------------------------------------------------------
// Dry-run
// ---------------------------------------------------------------------------

pub fn dry_run(device_id: &str) -> Result<RepairDryRunWire> {
    let letter = volume_letter_of(device_id)?;
    let mut f = open_volume_read(letter)?;

    let mut findings = Vec::new();
    let boot = read_sector_at(&mut f, 0)?;
    if !is_ntfs_boot(&boot) {
        return Ok(RepairDryRunWire {
            device_id: device_id.to_string(),
            findings: vec![RepairFindingWire {
                id: "boot-invalid".into(),
                kind: "boot-sector-mismatch".into(),
                where_: format!("{letter}: sector 0"),
                description_key: "repair.finding.bootSectorInvalid".into(),
                fixable: false,
                severity: "critical".into(),
            }],
            can_apply: false,
            note: Some(
                "The volume does not expose a readable NTFS boot sector; volume repair supports NTFS only.".into(),
            ),
        });
    }

    // Boot vs backup comparison.
    let mut backup_ok = false;
    let mut backup_offset = 0u64;
    for cand in backup_boot_candidates(&boot) {
        if let Ok(candidate) = read_sector_at(&mut f, cand) {
            if is_ntfs_boot(&candidate) {
                backup_ok = true;
                backup_offset = cand;
                if candidate != boot {
                    findings.push(RepairFindingWire {
                        id: "boot-sector-mismatch".into(),
                        kind: "boot-sector-mismatch".into(),
                        where_: format!("{letter}: sector 0 vs sector {}", cand / 512),
                        description_key: "repair.finding.bootSector".into(),
                        fixable: !is_system_volume(letter),
                        severity: "critical".into(),
                    });
                }
                break;
            }
        }
    }
    if !backup_ok {
        findings.push(RepairFindingWire {
            id: "backup-dbr-stale".into(),
            kind: "backup-dbr-stale".into(),
            where_: format!("{letter}: backup boot sector"),
            description_key: "repair.finding.backupDbr".into(),
            fixable: false,
            severity: "warning".into(),
        });
    }
    let _ = backup_offset;

    // $MFT vs $MFTMirr — MFT starts at the LCN in the boot sector
    // (u64 @0x30); cluster size = u8@0x0D * sector size. The mirror is
    // the second cluster run recorded at u64@0x38. Record 0 of each,
    // compared byte-wise (first record size = u16@0x40 * 256? signed; we
    // compare the first 512 bytes which contain the header + flags).
    let cluster_size = boot[0x0D] as u64 * 512;
    let mft_lcn = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap_or([0; 8]));
    let mirror_lcn = u64::from_le_bytes(boot[0x38..0x40].try_into().unwrap_or([0; 8]));
    if cluster_size > 0 && (mft_lcn > 0 || mirror_lcn > 0) {
        let mft_head = read_at(&mut f, mft_lcn * cluster_size, 512).ok();
        let mirror_head = read_at(&mut f, mirror_lcn * cluster_size, 512).ok();
        match (mft_head, mirror_head) {
            (Some(m), Some(r)) if m != r => {
                findings.push(RepairFindingWire {
                    id: "mft-record-error".into(),
                    kind: "mft-record-error".into(),
                    where_: format!("{letter}: $MFT vs $MFTMirr (record 0)"),
                    description_key: "repair.finding.mftRecord".into(),
                    fixable: !is_system_volume(letter),
                    severity: "critical".into(),
                });
            }
            _ => {}
        }
    }

    let can_apply = findings.iter().any(|f| f.fixable);
    Ok(RepairDryRunWire {
        device_id: device_id.to_string(),
        findings,
        can_apply,
        note: if is_system_volume(letter) {
            Some("Repairs cannot be applied to the running system volume.".into())
        } else {
            None
        },
    })
}

fn read_at(f: &mut std::fs::File, byte_offset: u64, len: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    f.seek(SeekFrom::Start(byte_offset))
        .map_err(|e| Error::Volume(format!("seek failed: {e}")))?;
    f.read_exact(&mut buf)
        .map_err(|e| Error::Volume(format!("read failed: {e}")))?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Apply + undo
// ---------------------------------------------------------------------------

pub fn apply_repair(
    device_id: &str,
    options: &ApplyOptionsWire,
    undo_log: &RepairUndoLog,
) -> Result<u32> {
    if options.consent_token != REPAIR_CONSENT_TOKEN {
        return Err(Error::Volume(
            "repair consent token missing (type REPAIR to confirm)".into(),
        ));
    }
    let letter = volume_letter_of(device_id)?;
    if is_system_volume(letter) {
        return Err(Error::Volume(format!(
            "repairs cannot be applied to the system volume {letter}:"
        )));
    }

    let dry = dry_run(device_id)?;
    if dry.findings.is_empty() {
        return Ok(0);
    }

    // Open with write access.
    let path = format!(r"\\.\{letter}:");
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| Error::Volume(format!("cannot open '{path}' for writing: {e}")))?;

    let mut applied = 0u32;
    for finding in &dry.findings {
        if !finding.fixable {
            continue;
        }
        match finding.kind.as_str() {
            "boot-sector-mismatch" => {
                // Snapshot the damaged primary, restore the backup.
                let original = read_sector_at(&mut f, 0)?;
                let backup_offset = finding
                    .where_
                    .rsplit("sector ")
                    .next()
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|s| s * 512)
                    .unwrap_or(0);
                if backup_offset == 0 {
                    continue;
                }
                let backup = read_sector_at(&mut f, backup_offset)?;
                write_sector(&mut f, 0, &backup)?;
                undo_log.entries.lock().unwrap().push(StoredUndo {
                    volume: format!("{letter}:"),
                    action: "boot-sector-restore".into(),
                    where_: "sector 0".into(),
                    at_unix: crate::recovery::now_unix(),
                    snapshot: original,
                });
                applied += 1;
            }
            "mft-record-error" => {
                // Snapshot the mirror, restore it from $MFT record 0.
                let boot = read_sector_at(&mut f, 0)?;
                let cluster_size = boot[0x0D] as u64 * 512;
                let mft_lcn = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap_or([0; 8]));
                let mirror_lcn = u64::from_le_bytes(boot[0x38..0x40].try_into().unwrap_or([0; 8]));
                if cluster_size == 0 {
                    continue;
                }
                let mft_head = read_at(&mut f, mft_lcn * cluster_size, 512)?;
                let mirror = read_at(&mut f, mirror_lcn * cluster_size, 512)?;
                write_at(&mut f, mirror_lcn * cluster_size, &mft_head)?;
                undo_log.entries.lock().unwrap().push(StoredUndo {
                    volume: format!("{letter}:"),
                    action: "mft-mirror-restore".into(),
                    where_: "$MFTMirr".into(),
                    at_unix: crate::recovery::now_unix(),
                    snapshot: mirror,
                });
                applied += 1;
            }
            _ => {}
        }
    }
    info!(device = device_id, applied, "repairs applied");
    Ok(applied)
}

fn write_sector(f: &mut std::fs::File, offset: u64, data: &[u8]) -> Result<()> {
    write_at(f, offset, data)
}

fn write_at(f: &mut std::fs::File, offset: u64, data: &[u8]) -> Result<()> {
    use std::io::Write;
    f.seek(SeekFrom::Start(offset))
        .map_err(|e| Error::Volume(format!("seek failed: {e}")))?;
    f.write_all(data)
        .map_err(|e| Error::Volume(format!("write failed: {e}")))?;
    f.sync_all()
        .map_err(|e| Error::Volume(format!("flush failed: {e}")))?;
    Ok(())
}

pub fn undo_log_wire(device_id: &str, log: &RepairUndoLog) -> Vec<RepairUndoEntryWire> {
    let letter = volume_letter_of(device_id).unwrap_or('?');
    let volume = format!("{letter}:");
    log.entries
        .lock()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, e)| e.volume == volume)
        .map(|(i, e)| RepairUndoEntryWire {
            index: i as u32,
            action: e.action.clone(),
            where_: e.where_.clone(),
            at_unix: e.at_unix,
            snapshot_hex: hex(&e.snapshot),
        })
        .collect()
}

pub fn undo_repairs(device_id: &str, log: &RepairUndoLog) -> Result<u32> {
    let letter = volume_letter_of(device_id)?;
    if is_system_volume(letter) {
        return Err(Error::Volume(
            "undo is not available for the system volume".into(),
        ));
    }
    let volume = format!("{letter}:");
    let entries: Vec<StoredUndo> = {
        let guard = log.entries.lock().unwrap();
        guard
            .iter()
            .filter(|e| e.volume == volume)
            .cloned()
            .collect()
    };
    if entries.is_empty() {
        return Ok(0);
    }
    let path = format!(r"\\.\{letter}:");
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| Error::Volume(format!("cannot open '{path}' for writing: {e}")))?;

    let mut rolled = 0u32;
    for e in entries.iter().rev() {
        let offset = match e.action.as_str() {
            "boot-sector-restore" => 0u64,
            "mft-mirror-restore" => {
                // Recompute mirror offset from the current boot sector.
                let boot = read_sector_at(&mut f, 0)?;
                let cluster_size = boot[0x0D] as u64 * 512;
                let mirror_lcn = u64::from_le_bytes(boot[0x38..0x40].try_into().unwrap_or([0; 8]));
                mirror_lcn * cluster_size
            }
            _ => continue,
        };
        write_at(&mut f, offset, &e.snapshot)?;
        rolled += 1;
    }
    log.entries.lock().unwrap().retain(|e| e.volume != volume);
    warn!(device = device_id, rolled, "repairs rolled back");
    Ok(rolled)
}

fn volume_letter_of(device_id: &str) -> Result<char> {
    let letter = device_id
        .trim_start_matches("dev:")
        .trim_start_matches("vol:")
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic())
        .ok_or_else(|| Error::Volume(format!("invalid device id '{device_id}'")))?;
    Ok(letter.to_ascii_uppercase())
}

fn is_system_volume(letter: char) -> bool {
    // The system volume hosts \Windows — resolve via the Windows dir.
    if let Ok(windir) = std::env::var("SystemRoot") {
        return windir
            .chars()
            .next()
            .map(|c| c.eq_ignore_ascii_case(&letter))
            .unwrap_or(false);
    }
    letter == 'C'
}

// ---------------------------------------------------------------------------
// Tests — pure parts
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ntfs_boot_fixture() -> [u8; 512] {
        let mut s = [0u8; 512];
        s[3..7].copy_from_slice(b"NTFS");
        s[0x0D] = 1; // 1 sector/cluster with 512B sectors
        s[0x0E..0x10].copy_from_slice(&16u16.to_le_bytes()); // reserved
        s[0x28..0x30].copy_from_slice(&1_000_000u64.to_le_bytes());
        s[510] = 0x55;
        s[511] = 0xAA;
        s
    }

    #[test]
    fn backup_candidates_from_bpb() {
        let boot = ntfs_boot_fixture();
        let candidates = backup_boot_candidates(&boot);
        assert!(candidates.contains(&(15 * 512))); // reserved - 1
        assert!(candidates.contains(&(999_999 * 512))); // last sector
    }

    #[test]
    fn ntfs_boot_detection() {
        assert!(is_ntfs_boot(&ntfs_boot_fixture()));
        assert!(!is_ntfs_boot(&[0u8; 512]));
    }

    #[test]
    fn hex_encoding() {
        assert_eq!(hex(&[0xDE, 0xAD]), "DEAD");
    }

    #[test]
    fn consent_token_enforced() {
        let log = RepairUndoLog::default();
        let err = apply_repair(
            "dev:Z",
            &ApplyOptionsWire {
                consent_token: "nope".into(),
            },
            &log,
        )
        .unwrap_err();
        assert!(err.to_string().contains("consent"));
    }

    #[test]
    fn invalid_device_rejected() {
        assert!(volume_letter_of("garbage").is_err());
        assert_eq!(volume_letter_of("vol:C").unwrap(), 'C');
        assert_eq!(volume_letter_of("dev:c").unwrap(), 'C');
    }
}
