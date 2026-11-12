// SPDX-License-Identifier: GPL-3.0-or-later
//
// Deleted-file layer — DiskWizard recovery engine, phase 2.
//
// Confidence grading (cluster-allocation based) and fresh re-parsing of
// deleted records at read/recovery time. The scan-time `$Bitmap` snapshot
// powers the grade shown in the UI; the actual byte reads always re-open
// the MFT record and re-check allocation so a recovery job never trusts
// stale scan-time state.

use crate::error::{Error, Result};
use crate::recovery::collect::{ClusterBitmap, DeletedEntry, DwRun};
use ntfs::Ntfs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::Ordering;

/// Recoverability grade, wire-serialized snake_case (matches the UI
/// contract's ConfidenceGrade).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Excellent,
    Good,
    Fair,
    Poor,
    Unknown,
}

/// Grades a deleted entry against the cluster bitmap.
///
/// Grading model (honest, allocation-based — we cannot know whether the
/// original bytes survived, only whether their clusters were reused):
/// * compressed / encrypted → `Poor` (raw runs cannot reconstruct content)
/// * 0-byte files → `Poor` (nothing to recover; matches the reference
///   product's grading of empty files)
/// * resident data → `Good` (the bytes live in the MFT record itself;
///   they survive until the record is reused — the record is still
///   deleted, so this is near-certain recovery)
/// * ≥ 95% clusters free → `Excellent`
/// * ≥ 60% free → `Good`
/// * > 0% free → `Fair`
/// * all clusters reallocated → `Poor`
pub fn grade_deleted(entry: &DeletedEntry, bitmap: &ClusterBitmap) -> Confidence {
    if entry.compressed || entry.encrypted {
        return Confidence::Poor;
    }
    if !entry.is_dir && entry.data_size == 0 {
        return Confidence::Poor;
    }
    if entry.is_dir {
        // Directories are just containers — "recoverability" of the entry
        // itself is not meaningful; children are graded individually.
        return Confidence::Unknown;
    }
    if entry.resident {
        return Confidence::Good;
    }
    if entry.runs.is_empty() {
        // Non-resident without parsable runs (attribute-list or damaged).
        return Confidence::Poor;
    }
    let free = bitmap.free_fraction(entry);
    if free >= 0.95 {
        Confidence::Excellent
    } else if free >= 0.60 {
        Confidence::Good
    } else if free > 0.0 {
        Confidence::Fair
    } else {
        Confidence::Poor
    }
}

/// A freshly re-parsed deleted record at read time.
pub struct FreshRecord {
    pub still_deleted: bool,
    pub name: Option<String>,
    pub data_size: u64,
    pub resident: bool,
    pub compressed: bool,
    pub encrypted: bool,
    pub runs: Vec<DwRun>,
    /// Resident `$DATA` bytes copied out of the record (small).
    pub resident_data: Vec<u8>,
}

/// Re-opens the volume, re-parses the MFT record `record_no` and returns
/// its CURRENT state. Used by preview and recovery so that records reused
/// since scan time fail loudly instead of recovering wrong bytes.
pub fn reparse_record(volume_path: &str, record_no: u64) -> Result<FreshRecord> {
    let mut handle = File::open(volume_path)
        .map_err(|e| Error::Scan(format!("cannot open volume '{}': {}", volume_path, e)))?;
    let ntfs =
        Ntfs::new(&mut handle).map_err(|e| Error::Scan(format!("NTFS parse error: {:?}", e)))?;
    reparse_record_on(&ntfs, &mut handle, record_no)
}

/// Same as [`reparse_record`] over an already-open handle/ntfs pair
/// (the recovery job and preview paths keep one handle per volume).
pub fn reparse_record_on<T: Read + Seek>(
    ntfs: &Ntfs,
    handle: &mut T,
    record_no: u64,
) -> Result<FreshRecord> {
    let file = ntfs
        .file(handle, record_no)
        .map_err(|e| Error::Scan(format!("MFT record {} unreadable: {:?}", record_no, e)))?;
    let still_deleted = !file.flags().contains(ntfs::NtfsFileFlags::IN_USE);

    let mut fresh = FreshRecord {
        still_deleted,
        name: crate::recovery::collect::record_win32_name(&file, handle),
        data_size: 0,
        resident: false,
        compressed: false,
        encrypted: false,
        runs: Vec::new(),
        resident_data: Vec::new(),
    };

    if let Some(item) = file.data(handle, "") {
        let item = item.map_err(|e| Error::Scan(format!("$DATA error: {:?}", e)))?;
        if let Ok(attribute) = item.to_attribute() {
            fresh.compressed = attribute
                .flags()
                .contains(ntfs::NtfsAttributeFlags::COMPRESSED);
            fresh.encrypted = attribute
                .flags()
                .contains(ntfs::NtfsAttributeFlags::ENCRYPTED);
            fresh.data_size = attribute.value_length();
            match attribute.value(handle) {
                Ok(ntfs::attribute_value::NtfsAttributeValue::Resident(r)) => {
                    fresh.resident = true;
                    fresh.resident_data = r.data().to_vec();
                }
                Ok(ntfs::attribute_value::NtfsAttributeValue::NonResident(nr)) => {
                    let cluster_size = ntfs.cluster_size() as u64;
                    for run in nr.data_runs() {
                        let run = run.map_err(|e| Error::Scan(format!("data run: {:?}", e)))?;
                        let lcn = run
                            .data_position()
                            .value()
                            .map(|pos| pos.get() / cluster_size);
                        fresh.runs.push(DwRun {
                            lcn,
                            clusters: run.allocated_size() / cluster_size,
                        });
                    }
                }
                Ok(ntfs::attribute_value::NtfsAttributeValue::AttributeListNonResident(_)) => {
                    // Spanned attribute — cannot locate runs cheaply.
                }
                Err(_) => {}
            }
        }
    }
    Ok(fresh)
}

/// Reads up to `max_len` logical bytes of a deleted record starting at
/// `offset`, following its data runs against the volume handle.
///
/// * Sparse runs contribute zeros.
/// * `bitmap` (when provided) is re-checked; if any needed cluster is now
///   allocated, the read stops there — the remainder has been overwritten.
///   Returns what is intact plus the number of intact bytes.
pub fn read_deleted_range<T: Read + Seek>(
    handle: &mut T,
    fresh: &FreshRecord,
    bitmap: Option<&ClusterBitmap>,
    cluster_size: u64,
    offset: u64,
    max_len: u64,
) -> Result<(Vec<u8>, u64)> {
    if fresh.resident {
        let data = &fresh.resident_data;
        if offset >= data.len() as u64 {
            return Ok((Vec::new(), 0));
        }
        let start = offset as usize;
        let end = ((offset + max_len) as usize).min(data.len());
        let slice = &data[start..end];
        return Ok((slice.to_vec(), slice.len() as u64));
    }

    if cluster_size == 0 || fresh.runs.is_empty() {
        return Ok((Vec::new(), 0));
    }

    // Locate the run covering `offset`, then stream cluster-aligned reads.
    let mut out = Vec::with_capacity(max_len.min(16 * 1024 * 1024) as usize);
    let mut remaining_offset = offset;
    let mut remaining_len = max_len;

    for run in &fresh.runs {
        let run_bytes = run.clusters * cluster_size;
        if remaining_offset >= run_bytes {
            remaining_offset -= run_bytes;
            continue;
        }
        if remaining_len == 0 {
            break;
        }
        let skip_in_run = remaining_offset;
        remaining_offset = 0;

        let mut cluster_idx = skip_in_run / cluster_size;
        let mut in_cluster_off = skip_in_run % cluster_size;
        while cluster_idx < run.clusters && remaining_len > 0 {
            let Some(lcn) = run.lcn else {
                // Sparse: zeros for the whole cluster remainder.
                let zeros_len = (cluster_size - in_cluster_off).min(remaining_len) as usize;
                out.extend(std::iter::repeat(0u8).take(zeros_len));
                remaining_len -= zeros_len as u64;
                cluster_idx += 1;
                in_cluster_off = 0;
                continue;
            };
            // Cluster reuse check (fresh allocation beats scan-time grade).
            if let Some(bm) = bitmap {
                if bm.is_allocated(lcn + cluster_idx) {
                    // Overwritten — stop; report intact bytes so far.
                    return Ok((out.clone(), out.len() as u64));
                }
            }
            let abs = lcn
                .checked_mul(cluster_size)
                .and_then(|b| b.checked_add(cluster_idx * cluster_size + in_cluster_off))
                .ok_or_else(|| Error::Scan("data run offset overflow".into()))?;
            handle
                .seek(SeekFrom::Start(abs))
                .map_err(|e| Error::Scan(format!("volume seek: {}", e)))?;
            let want = (cluster_size - in_cluster_off).min(remaining_len) as usize;
            let mut buf = vec![0u8; want];
            let mut read_total = 0usize;
            while read_total < want {
                let n = handle
                    .read(&mut buf[read_total..])
                    .map_err(|e| Error::Scan(format!("volume read: {}", e)))?;
                if n == 0 {
                    break;
                }
                read_total += n;
            }
            out.extend_from_slice(&buf[..read_total]);
            remaining_len -= read_total as u64;
            if read_total < want {
                break;
            }
            cluster_idx += 1;
            in_cluster_off = 0;
        }
        if remaining_len == 0 {
            break;
        }
    }
    let intact = out.len() as u64;
    Ok((out, intact))
}

/// Convenience: full intact read of a deleted record (bounded by the
/// caller). See [`read_deleted_range`].
pub fn read_deleted_all<T: Read + Seek>(
    handle: &mut T,
    fresh: &FreshRecord,
    bitmap: Option<&ClusterBitmap>,
    cluster_size: u64,
    max_len: u64,
) -> Result<(Vec<u8>, u64)> {
    read_deleted_range(handle, fresh, bitmap, cluster_size, 0, max_len)
}

/// Guard helper used by jobs: `true` when the run was cancelled.
pub fn cancelled(flag: &std::sync::atomic::AtomicBool) -> bool {
    flag.load(Ordering::Acquire)
}

// ---------------------------------------------------------------------------
// Tests — pure grading + read-range math against synthetic runs
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(size: u64, resident: bool, runs: Vec<(Option<u64>, u64)>) -> DeletedEntry {
        DeletedEntry {
            record_no: 42,
            parent_record_no: 5,
            name: "x".into(),
            data_size: size,
            is_dir: false,
            modified_unix: 0,
            resident,
            runs: runs
                .into_iter()
                .map(|(lcn, clusters)| DwRun { lcn, clusters })
                .collect(),
            compressed: false,
            encrypted: false,
        }
    }

    fn bm(bits: &[u8]) -> ClusterBitmap {
        ClusterBitmap {
            data: bits.to_vec(),
            total_clusters: (bits.len() * 8) as u64,
        }
    }

    #[test]
    fn grade_compressed_and_encrypted_are_poor() {
        let mut e = entry(1000, false, vec![(Some(0), 1)]);
        e.compressed = true;
        assert_eq!(grade_deleted(&e, &bm(&[0x00])), Confidence::Poor);
        e.compressed = false;
        e.encrypted = true;
        assert_eq!(grade_deleted(&e, &bm(&[0x00])), Confidence::Poor);
    }

    #[test]
    fn grade_zero_byte_is_poor() {
        assert_eq!(
            grade_deleted(&entry(0, false, vec![]), &bm(&[0x00])),
            Confidence::Poor
        );
    }

    #[test]
    fn grade_resident_is_good() {
        assert_eq!(
            grade_deleted(&entry(512, true, vec![]), &bm(&[0xFF])),
            Confidence::Good
        );
    }

    #[test]
    fn grade_thresholds() {
        // All free → Excellent
        assert_eq!(
            grade_deleted(&entry(8192, false, vec![(Some(0), 2)]), &bm(&[0x00])),
            Confidence::Excellent
        );
        // 60% free → Good (clusters 0,1 free of 0..5 allocated pattern):
        // bits 00111 → clusters 0,1 free (2/5 = 40% → Fair)
        assert_eq!(
            grade_deleted(
                &entry(5 * 4096, false, vec![(Some(0), 5)]),
                &bm(&[0b0000_0111])
            ),
            Confidence::Fair
        );
        // All allocated → Poor
        assert_eq!(
            grade_deleted(&entry(4096, false, vec![(Some(0), 1)]), &bm(&[0xFF])),
            Confidence::Poor
        );
        // No runs, non-resident → Poor
        assert_eq!(
            grade_deleted(&entry(4096, false, vec![]), &bm(&[0x00])),
            Confidence::Poor
        );
    }

    #[test]
    fn grade_directory_is_unknown() {
        let e = DeletedEntry {
            record_no: 7,
            parent_record_no: 5,
            name: "d".into(),
            data_size: 0,
            is_dir: true,
            modified_unix: 0,
            resident: false,
            runs: vec![],
            compressed: false,
            encrypted: false,
        };
        assert_eq!(grade_deleted(&e, &bm(&[0x00])), Confidence::Unknown);
    }

    #[test]
    fn read_range_resident_slices() {
        let fresh = FreshRecord {
            still_deleted: true,
            name: None,
            data_size: 4,
            resident: true,
            compressed: false,
            encrypted: false,
            runs: vec![],
            resident_data: b"hello world".to_vec(),
        };
        let (bytes, n) = read_deleted_range(&mut io_empty(), &fresh, None, 4096, 6, 5).unwrap();
        assert_eq!(bytes, b"world");
        assert_eq!(n, 5);
    }

    #[test]
    fn read_range_offset_past_end_returns_empty() {
        let fresh = FreshRecord {
            still_deleted: true,
            name: None,
            data_size: 0,
            resident: true,
            compressed: false,
            encrypted: false,
            runs: vec![],
            resident_data: b"abc".to_vec(),
        };
        let (bytes, n) = read_deleted_range(&mut io_empty(), &fresh, None, 4096, 10, 4).unwrap();
        assert!(bytes.is_empty());
        assert_eq!(n, 0);
    }

    #[test]
    fn read_range_nonresident_stops_on_allocated_cluster() {
        // Run at LCN 4, 2 clusters. Bitmap marks LCN 5 allocated (byte 0,
        // bit 5) → first cluster reads, second stops.
        struct Mem {
            pos: u64,
        }
        impl Read for Mem {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                for (i, b) in buf.iter_mut().enumerate() {
                    *b = ((self.pos as usize + i) % 251) as u8;
                }
                self.pos += buf.len() as u64;
                Ok(buf.len())
            }
        }
        impl Seek for Mem {
            fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
                match pos {
                    SeekFrom::Start(p) => {
                        self.pos = p;
                        Ok(p)
                    }
                    _ => Ok(self.pos),
                }
            }
        }
        let mut handle = Mem { pos: 0 };
        let fresh = FreshRecord {
            still_deleted: true,
            name: None,
            data_size: 8192,
            resident: false,
            compressed: false,
            encrypted: false,
            runs: vec![DwRun {
                lcn: Some(4),
                clusters: 2,
            }],
            resident_data: vec![],
        };
        let bitmap = ClusterBitmap {
            data: vec![0b0010_0000],
            total_clusters: 8,
        };
        let (bytes, intact) =
            read_deleted_range(&mut handle, &fresh, Some(&bitmap), 4096, 0, 8192).unwrap();
        assert_eq!(intact, 4096, "first cluster intact, second is reallocated");
        assert_eq!(bytes.len(), 4096);
    }

    #[test]
    fn read_range_sparse_run_gives_zeros() {
        struct Nop;
        impl Read for Nop {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                panic!("sparse clusters must not hit the volume");
            }
        }
        impl Seek for Nop {
            fn seek(&mut self, _p: SeekFrom) -> std::io::Result<u64> {
                Ok(0)
            }
        }
        let fresh = FreshRecord {
            still_deleted: true,
            name: None,
            data_size: 8192,
            resident: false,
            compressed: false,
            encrypted: false,
            runs: vec![DwRun {
                lcn: None,
                clusters: 2,
            }],
            resident_data: vec![],
        };
        let (bytes, n) = read_deleted_range(&mut Nop, &fresh, None, 4096, 0, 8192).unwrap();
        assert_eq!(n, 8192);
        assert!(bytes.iter().all(|&b| b == 0));
    }

    fn io_empty() -> std::io::Empty {
        std::io::empty()
    }
}
