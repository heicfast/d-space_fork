# NTFS-RECOVER Feature Harvest — Agent F

- **Repo**: https://github.com/mjgil-rust/ntfs-recover (local depth-1 clone at `/home/z/my-project/refs/ntfs-recover`)
- **HEAD SHA**: 5072bbc729b293efc0b2070b38ad30c3b7e043cf
- **License**: BSD Zero Clause License (0BSD) — see §Licensing
- **Stack**: Rust 2021, edition 2021, no external NTFS crates; deps: memmap2 0.9, rayon 1.10, clap 4 (derive), indicatif 0.17, serde 1 + serde_json 1 + bincode 1, glob 0.3, anyhow 1, log 0.4 + env_logger 0.11 (`Cargo.toml` [dependencies])
- **Date**: 2026-11-12 — **Agent F** (research only; repo untouched)

## Overview

`rust-ntfs-recover` is a CLI data-recovery tool for NTFS volumes whose Master File Table is corrupted — specifically the case where **both `$MFT` and `$MFTMirr` are unusable**, so `chkdsk`/`ntfsfix`/TestDisk cannot mount or repair the volume (`README.md` §The Problem, `SPEC.md` §Problem Statement). It brute-force scans the raw device/image for `FILE`-signature MFT records, rebuilds a directory tree from `$FILE_NAME` parent references, extracts file content via decoded data runs, and can patch a reconstructed run list back into MFT record 0 to make the volume mountable again.

**Crate layout** (~4,900 lines of Rust across 16 `.rs` files; `README.md` §Architecture):

```
src/main.rs        CLI (clap derive) + phase orchestration (Commands enum)
src/scanner.rs     Phase 1 — raw disk scan, .rntfs scan-file v2/v3/v4 I/O, resume
src/rebuilder.rs   Phase 2 — record→FileEntry parsing, directory-tree reconstruction
src/extractor.rs   Phase 3 — file extraction (resident copy / data-run reads)
src/repairer.rs    Phase 4 — MFT record 0 reconstruction + in-place disk write
src/types.rs       ScannedRecord, FileEntry, DirectoryTree, ScanIndexEntry, MftFragment, ScanResult
src/parser/        boot_sector, mft_record, fixup, attributes, filename, data_runs,
                   standard_info, attribute_list, error (all `&[u8]`-slice, bounds-checked, no unsafe)
```

**Entry point**: `main()` in `src/main.rs` (clap `Cli` struct; subcommands `Scan`, `List`, `Extract`, `Repair`). Global args: `device`, `-s/--scan-file` (default `./scan.rntfs`), `--cluster-size` override, `--threads` (default `available_parallelism()` via `num_cpus()`, `src/main.rs:151-155`), `-v/--verbose` (env_logger level debug vs info).

**CLI surface** (`src/main.rs` `enum Commands`):

- `scan <DEVICE_OR_IMAGE>` — full-disk MFT-record hunt, writes resumable `.rntfs` index.
- `list [device] [-p pattern]* [--min-size][--max-size][--include-deleted]` — loads scan, rebuilds tree, prints `flags size path` lines (`extractor.rs list_files`, lines 213-226).
- `extract <device> [-o DIR=./recovered] [-p glob]* [--min-size][--max-size][--list-only][--no-dirs][--include-deleted][--dry-run]` — one-shot: auto-scans first if no scan file exists (`main.rs:234-245`).
- `repair <device> [--dry-run][--backup PATH][--fix-mirror][--reconstruct]` — needs prior scan; interactive `Type 'yes'` confirmation before any write (`main.rs:358-374`).

**Output formats**: `.rntfs` binary scan cache (header + fragment table + bincode batches, detailed in §Output Formats); recovered files in an output directory tree; optional tree JSON via `rebuilder.rs save_tree` (present but never invoked by `main.rs` — dead export); terminal reports (list lines, repair plan, fragment-map comparison, hex dump in dry-run).

**Tests/fixtures**: in-tree `#[cfg(test)]` unit tests in `parser/boot_sector.rs`, `parser/mft_record.rs`, `parser/fixup.rs`, `parser/data_runs.rs`, `parser/filename.rs`, `extractor.rs` (size parsing, filename sanitization); integration fixture script `tests/create_test_image.sh` builds a 32 MB mkfs.ntfs image with resident + non-resident files (needs root to mount). No Rust integration-test files under `tests/` other than the shell script.

## The MFT-Corruption Recovery Path (deep dive)

The scenario: `$MFT`'s own `$DATA` run list (and `$MFTMirr`'s copy) is corrupted, so no top-down traversal is possible. The tool's answer is a bottom-up, signature-driven reconstruction in four phases:

- Phase 1 (`scanner.rs`) — find candidate records anywhere on disk, validate, fixup, index, persist (resumable).
- Phase 2 (`rebuilder.rs`) — parse each record's attributes, dedup, resolve parent chains into paths.
- Phase 3 (`extractor.rs`) — read content via resident bytes or data runs; write output tree.
- Phase 4 (`repairer.rs`) — rebuild the MFT fragment map from scan offsets and patch record 0 in place.

Step-by-step, with citations:

**1. Geometry without trusting the MFT.**

- `scanner.rs read_boot_sector` (lines 180-198) reads 512 bytes at offset 0 and calls `parser::boot_sector::parse_boot_sector`.
- Bad OEM ID → warn + **fall back to defaults**: sector 512, cluster 4096, record 1024, `mft_start_offset = 0` (`scan_device` lines 219-222; constants lines 17-18).
- No backup-boot-sector fallback exists (see §Edge Cases — absence finding).

**2. Salvage any fragment map that still parses.**

- `scanner.rs parse_mft_fragments` (lines 61-156): seek `mft_start_offset`, read one record, check `has_file_signature`, fixup via `parse_mft_record`, then `iterate_attributes`.
- Finds the unnamed non-resident `$DATA` (`ATTR_DATA` = 0x80, `name_length == 0`, `non_resident`) and `decode_data_runs` its run bytes (`attr.data_run_bytes(&buf)`).
- Each run → `MftFragment { logical_start, cluster_start, cluster_count }`, `logical_start += cluster_count * (cluster_size/record_size)`; **sparse runs advance logical numbering only** (lines 103-105).
- Corruption guard: run with `cluster_offset > 0x1_0000_0000_0000` stops fragment parsing (lines 107-115).
- No FILE signature / decode failure ⇒ empty fragment list; scanning proceeds regardless (lines 81-84, warn at 124).

**3. Signature sweep at every record-size boundary.**

- `scan_device` loops 64 MB chunks (`CHUNK_SIZE`, line 16) via sequential `File::read` (not mmap — see §Coverage/anomalies).
- `scan_chunk` (lines 314-358) walks each chunk in `record_size` steps: `has_file_signature(slice) && validate_mft_record(slice)`.
- Sweep is **record-size-aligned (default 1024)**, not a generic sector sweep; parallelism = `chunk.par_chunks(1 MB)` under rayon (lines 276-279, thread pool at 244).

**4. Candidate validation.**

- `parser/mft_record.rs validate_mft_record` (lines 115-134): (a) ≥48 bytes + `FILE` magic; (b) allocated-size sanity — hard-reject only `> 65536` (0 and >4096 merely suspicious); (c) `first_attribute_offset` must be `≥ 0x30` and `< allocated`.
- Deliberately permissive ("best-effort, not strict validation", `README.md` §Why a custom parser).

**5. Fixup application + header extraction.**

- `parse_mft_record` (mft_record.rs 99-112) parses the full header and calls `fixup.rs apply_fixup_array` — Result is **discarded** (`let _ = ...`): a fixup mismatch does NOT reject the record.
- `apply_fixup_array` (fixup.rs 15-60): each sector-end word must equal the USA signature; originals restored in place; short buffer ⇒ break early.

**6. Record numbering (chaining).** `scan_chunk` lines 332-339, three-tier fallback:

1. `resolve_mft_record_number(disk_offset, fragments, ...)` (lines 158-176) — disk offset → cluster → covering fragment → logical number; **only if a fragment map covers that cluster** (`cluster >= frag.cluster_start && cluster < frag.cluster_start + frag.cluster_count`).
2. Else the header's self-describing `mft_record_number` at 0x2C (only if non-zero).
3. Else positional guess `disk_offset / record_size`.

- When $MFT runs are garbage, the record header's own number is the primary identity; the fragment map only refines it.

**7. Deduplication into a tree.** `rebuilder.rs rebuild_tree` (lines 21-155):

- Base records (`base_record == 0`, from `MftRecordHeader::base_record_number()` = low 48 bits of the 0x20 reference, mft_record.rs 33-36) → `parse_record_to_entry`.
- Extension records shelved separately (lines 31-35) and merged into empty-data base records later (lines 78-86, `extract_data_runs_from_record` 259-282).
- Per MFT number with multiple hits: prefer **in-use over deleted, then highest sequence number** (lines 66-74).

**8. Path resolution via parents, with cycle/orphan guards.**

- `resolve_paths` + `build_path_cached` (rebuilder.rs 284-357): memoized DFS from each entry through `$FILE_NAME.parent_record_number` (filename.rs 63, low 48 bits) up to root record 5 (empty path, line 291).
- Guards: `visited: HashSet<u64>` breaks cycles → `__CYCLE__/<mft_num>` prefix (line 321); missing parent → `__ORPHANED__/<mft_num>` (line 326); self-parent returns bare name (lines 338-341).
- Deleted records are **kept during resolution** (deleted directories still resolve paths, comment lines 17-20) and pruned afterward unless `--include-deleted` (lines 119-134).
- `identify_orphans` (lines 360-375) fills `tree.orphaned` from path markers.

**9. Content extraction.** `extractor.rs extract_files` (lines 25-123):

- Filter (size/glob/deleted), sort by first data-run cluster to minimize seeks (lines 48-53).
- `extract_single_file` (lines 126-210): resident `$DATA` copied verbatim from stored record bytes; non-resident runs read at `cluster_offset*cluster_size .. +cluster_count*cluster_size`, capped at `file_size` (from `$DATA real_size` or resident length, `remaining` bookkeeping lines 160-204); sparse runs → zeros (lines 168-179); 1 MB buffer; EOF mid-run only warns (lines 194-197).

**10. MFT repair (making the volume mountable).**

- `main.rs` `Commands::Repair` + `--reconstruct` → `repairer.rs reconstruct_fragments` (lines 179-290):
  - Dedup by record number (highest sequence wins; extension records skipped).
  - **Group by `delta = disk_offset − mft_record_number × record_size`** — constant within one physical MFT extent (lines 227-231).
  - Each delta group → one fragment spanning min..max record numbers (`make_fragment`, lines 293-327: cluster-align logical bounds, compute `cluster_start`, reject negative/unaligned results).
  - Drop fragments `< 4 clusters` as stale artifacts (lines 247-255); sort by `logical_start`; cap to record 0's `$DATA end_vcn` (main.rs 295-336, `max_record = (end_vcn + 1) * recs_per_cluster − 1`); `merge_adjacent_fragments` (lines 330-369) coalesces contiguous and trims overlaps.
- `print_fragment_comparison` (lines 372-445) diffs reconstructed vs original map.
- `patch_record_data_runs` (lines 467-588) **patches in place**: read record 0; verify FILE magic; strip fixup (restore sector-end originals, warn-not-fail on mismatch, 493-507); walk attributes to unnamed non-resident `$DATA`; bail if new runs exceed existing run space (549-556); zero run area + copy new runs; re-apply fresh USA with signature `old+1` (wrap 0xFFFE→0x0001, 570-585).
- Backups: `mft_record0_backup_<unix_secs>.bin` (or `--backup`) before writing; `--fix-mirror` also backs up + writes the patched record at `$MFTMirr`'s cluster (lines 154-163).
- Post-repair docs direct the user to `ntfsfix` then mount (`README.md` 93-97). Design rationale (patch-vs-rebuild, ntfs-3g sign-extension bug) is `DEBUGGING.md`.

## Algorithm Inventory (A-xx)

- **A-01 Boot-sector/BPB parsing** — `parser/boot_sector.rs parse_boot_sector` (lines 20-76).
  - Input: 512-byte slice; validates `b"NTFS    "` at 0x03 (else `InvalidSignature`).
  - Output `BootSector`: `bytes_per_sector`(0x0B), `sectors_per_cluster`(0x0D), `cluster_size` = product, `total_sectors`(0x28), `mft_start_cluster`(0x30), `mft_mirror_start_cluster`(0x38), `mft_record_size`, `index_record_size`, `volume_serial`(0x48).
  - Signed-size encoding honored: value>0 → clusters×cluster_size; value<0 → `2^(-value)` (lines 48-61). Edge: buffer <512 → `BufferOverflow`.
  - **No backup boot sector (sector 6 / end-of-volume) fallback — absence confirmed**; caller falls back to hardcoded defaults instead (`scanner.rs:219-222`).
- **A-02 MFT record header parsing** — `parser/mft_record.rs parse_mft_record_header` (lines 50-95). Fields at fixed offsets (0x04 usa_offset, 0x06 usa_size, 0x08 lsn, 0x10 sequence, 0x12 hard_link_count, 0x14 first_attribute_offset, 0x16 flags, 0x18 real_size, 0x1C allocated, 0x20 base_record_reference, 0x28 next_attribute_id, 0x2C mft_record_number with `unwrap_or(0)` for short buffers). Helpers: `is_in_use` (flags&1), `is_directory` (flags&2), `base_record_number` (ref & 0x0000_FFFF_FFFF_FFFF), `is_base_record`.
- **A-03 Candidate validation** — `parser/mft_record.rs validate_mft_record` (lines 115-134). Contract: `true` ⇒ worth parsing. Rejects: <48 bytes, wrong magic, allocated >65536, `first_attr < 0x30 || ≥ allocated`. Accepts allocated 0 and 4096–65536 (documented as suspicious-but-possible).
- **A-04 Fixup (update sequence array) application** — `parser/fixup.rs apply_fixup_array` (lines 15-60). Input mutable record + usa_offset/usa_size + sector size. For each of `usa_size−1` sectors: check last-2-bytes equal USA signature (mismatch → `FixupMismatch`, loop **breaks** on short buffer), replace with USA-stored original. `usa_size<2` no-op. **Best-effort at call site**: `parse_mft_record` discards the Result (mft_record.rs line 104) — records with bad fixups still enter the index.
- **A-05 Attribute header iteration** — `parser/attributes.rs parse_attribute` (lines 90-131) + `iterate_attributes` (lines 165-202). Common header: type(0x00), length(0x04, reject `<16 || >0x10000` → `InvalidAttributeLength`), non_resident(0x08), name_length(0x09), name_offset(0x0A), flags(0x0C), attribute_id(0x0E). Resident extra: content_length(0x10), content_offset(0x14). Non-resident extra: start_vcn(0x10), end_vcn(0x18), data_runs_offset(0x20), compression_unit(0x22), allocated/real/initialized sizes (0x28/0x30/0x38). Iteration stops at `ATTR_END` (0xFFFFFFFF), zero/overlong length, or first parse error — **returns partial list** (README "Skip broken attributes and continue" is at record granularity; within a record, iteration stops at the first broken attribute).
- **A-06 Runlist decoding** — `parser/data_runs.rs decode_data_runs` (lines 25-100). Header byte: low nibble = length byte-count (reject 0 or >8 → `InvalidDataRun`), high nibble = offset byte-count (reject >8). `offset_bytes==0` ⇒ **sparse run** (cluster_offset 0, is_sparse true). Offsets are signed, relative, accumulated (`prev_offset + rel_offset`); **negative absolute offset ⇒ break, return partial runs** (lines 83-89 — "partial results are better than nothing"). Zero-length runs skipped (lines 55-61). Terminator 0x00.
- **A-07 Runlist encoding (repair-critical)** — `parser/data_runs.rs encode_data_runs` (lines 109-134) + `min_bytes_unsigned` (141-148) / `min_bytes_signed` (151-164). Emits minimal-byte runs, delta-encoded offsets, 0x00 terminator. **Key contract**: lengths are encoded with an extra sign bit (`(bits+1+7)/8`) because ntfs-3g's `ntfs_mapping_pairs_decompress` sign-extends the MSB — count 45,669 (`0xB265`) must be 3 bytes `65 B2 00`, else decoded as −19,867 (comment lines 136-140; full war story `DEBUGGING.md` §The Real Bug).
- **A-08 $FILE_NAME parsing** — `parser/filename.rs parse_filename_attr` (lines 53-109). Needs ≥0x42 bytes; parent_reference(0x00) split into `parent_record_number` (low 48) + `parent_sequence` (high 16); 4 timestamps (0x08/0x10/0x18/0x20, raw 100-ns-since-1601 u64s, never converted); allocated/real size (0x28/0x30); flags(0x38); filename_length(0x40), namespace(0x41); name = UTF-16LE decoded with `String::from_utf16_lossy` (lines 87-92). Namespace helpers `is_win32_name` (1 or 3), `is_dos_only` (2).
- **A-09 Filename choice among multiple $FILE_NAMEs (hard links)** — `rebuilder.rs parse_record_to_entry` lines 186-209: later attributes "dominate" if existing is DOS-only and new isn't, or new is Win32 and existing isn't; `file_size` taken from the first non-zero `$FILE_NAME.real_size` if `$DATA` hasn't set it. A hard-linked file (multiple FILE_NAMEs, possibly different parents) is recovered **once, under one name**; aliases are not enumerated (SPEC.md §Edge Cases says "note aliases" — not implemented; `hard_link_count` parsed but unused).
- **A-10 $STANDARD_INFORMATION parsing** — `parser/standard_info.rs parse_standard_info` (lines 13-29): created/modified/mft_modified/accessed at 0x00-0x20, `file_attributes` at 0x20; needs ≥0x30 bytes. Used only for created/modified into `FileEntry` (`rebuilder.rs:180-183`); timestamps stored raw, never rendered.
- **A-11 $ATTRIBUTE_LIST parsing** — `parser/attribute_list.rs parse_attribute_list` (lines 31-68): entries of ≥26 bytes with type/entry_length/name_length/name_offset/start_vcn/mft_reference/attribute_id; stops at 0xFFFFFFFF/0 type, undersized or overflowing entries. **Never called anywhere in `src/`** — the extension-record mechanism is instead driven by `base_record` references (rebuilder.rs lines 31-35, 78-86). Parsed but unused.
- **A-12 Deleted-record detection** — flags bit 0x01 absent ⇒ `is_deleted` (`rebuilder.rs:247`); deleted directories retained for path resolution, deleted files filtered unless `--include-deleted` (`rebuilder.rs:119-134`, `extractor.rs:38-40`); dedup prefers in-use copies (`rebuilder.rs:66-74`); repair-side dedup prefers highest sequence number regardless of in-use (`repairer.rs:195-215`).
- **A-13 Sparse/compressed/encrypted flagging** — `$DATA` header flags: 0x0001 compressed, 0x4000 encrypted, 0x8000 sparse (`rebuilder.rs:212-214`); surfaced as `DCES` letters in `list` output (`extractor.rs:216-223`) and honored for zero-fill on sparse runs only; **no LZNT1 decompression, no EFS decryption** (README Limitations).
- **A-14 Resident-data extraction** — `attributes.rs Attribute::resident_content` (lines 65-74, bounds-checked slice) → stored in `FileEntry.resident_data` (`rebuilder.rs:226-229`) → written verbatim at extract time (`extractor.rs:153-157`).
- **A-15 Non-resident extraction with seek-sort** — `extractor.rs` lines 47-53 (sort by `data_runs.first().cluster_offset`, `u64::MAX` for files without runs) and 158-204 (per-run reads capped by remaining size, 1 MB chunks, sparse zero-fill).
- **A-16 MFT fragment reconstruction** — `repairer.rs reconstruct_fragments` + `make_fragment` + `merge_adjacent_fragments` (lines 179-369).
  - Inputs: all `ScannedRecord`s + cluster/record size + optional `max_record`; output `Vec<MftFragment>` sorted, deduped, stale-filtered, overlap-trimmed, adjacency-merged.
  - Steps: (1) dedup by record number keeping highest sequence; (2) group by `delta = disk_offset − mft_num × record_size`; (3) per-group fragment min→max; (4) drop `<4 clusters`; (5) cap to `max_record`; (6) merge contiguous / trim overlaps.
  - `make_fragment` (293-327): cluster-aligns logical bounds, derives `cluster_start` from delta, returns `None` on negative offset or non-cluster-aligned start (warn).
  - `merge_adjacent_fragments` (330-369): merges when `logical_start == last_logical_end && cluster_start == last_cluster_end`; overlapping longer fragments get the overlap trimmed; contained fragments skipped.
- **A-17 Record-0 in-place patching** — `repairer.rs patch_record_data_runs` (lines 467-588).
  - Contract: preserve every attribute except the unnamed `$DATA`'s run bytes; refresh USA signature (`old+1`, wrap 0xFFFE→0x0001); refuse (bail) if runs don't fit existing run space; warn (not fail) on pre-existing fixup mismatch.
  - Sequence: open device → seek `mft_start_offset` → read_exact record → verify `FILE` magic → strip fixup → walk attr chain (stop at 0xFFFFFFFF/0-length/oversized) → locate `attr_type==0x80 && non_resident==1 && name_length==0` → zero old run area → copy new runs → re-apply fixup → return bytes for `write_record` (823-834, `OpenOptions` write + seek + write_all + flush).
  - Fixup strip/apply loops hardcode `num_sectors = record_size / sector_size` (lines 476, 493-507, 576-585).
- **A-18 From-scratch record-0 builder** — `repairer.rs build_mft_record_0` (lines 590-796): writes FILE header + minimal $STANDARD_INFORMATION + $FILE_NAME "$MFT" (parent ref 5|(1<<48), flags 0x06, namespace 3) + non-resident $DATA with VCN range, USA at 0x30, 8-byte alignment, end marker, then fixup. **Never called** (grep: only definition; the failed "Attempt 1" of `DEBUGGING.md`, kept as dead code).
- **A-19 Scan-file persistence & resume** — `scanner.rs` lines 21-56 (format comment) + `create_scan_file`/`flush_batch`/`update_header_offset`/`try_resume`/`read_all_batches`/`load_scan` (lines 362-566).
  - v4 magic `RNTFS004` (v3/v2 accepted for read; anything else → legacy whole-file bincode `ScanResult`).
  - Header fields: sector/cluster/record sizes, `last_scanned_offset`, `device_size`, `total_records`, `mft_start_offset`.
  - Resume requires matching device size + geometry else fresh scan (bails: `"device size mismatch"`, `"params mismatch"`); flush every 1 GiB (`FLUSH_INTERVAL`) appending length-prefixed bincode batches; header progress patched in place (`update_header_offset` seeks to offsets 18/34).

- **A-20 Delta-based logical record resolution** — `scanner.rs resolve_mft_record_number` (lines 158-176) and its repair-time dual (A-16's delta grouping). The invariant exploited: within one MFT extent, `disk_offset − record_number × record_size` is constant.

**Top extractable algorithms for the d-space port** (self-contained, no NTFS-crate dependency):

1. The 1024-boundary `FILE`-signature sweep with `validate_mft_record` cheap filter + rayon sub-chunk parallelism (`scanner.rs scan_chunk`).
2. Delta-grouping fragment reconstruction from scattered records (`repairer.rs reconstruct_fragments`) — the crown jewel; solves "no valid $MFT at all".
3. `encode_data_runs` with sign-bit-safe `min_bytes_unsigned` — required for ntfs-3g compatibility (`data_runs.rs 109-164`, `DEBUGGING.md`).
4. `decode_data_runs` partial-result semantics (negative-offset break, sparse runs, zero-length skip).
5. `apply_fixup_array` + its inverse in `patch_record_data_runs` (fixup strip/re-apply with signature increment).
6. Memoized parent-chain path resolution with `__CYCLE__`/`__ORPHANED__` guards (`rebuilder.rs build_path_cached`).
7. Record-number resolution fallback chain (fragment map → header number → positional) (`scanner.rs scan_chunk`).
8. Best-effort attribute iteration returning partial lists (`attributes.rs iterate_attributes`).

## Edge Cases & Error Model

**ParseError enum** (`parser/error.rs` lines 5-35; Display lines 37-76 — exact messages):

- `BufferOverflow{offset,needed,available}` → `"buffer overflow at offset {:#x}: need {} bytes, only {} available"`.
- `InvalidSignature{offset,expected}` → `"invalid signature at offset {:#x}: expected '{}'"` (used for boot OEM ID and FILE magic).
- `FixupMismatch{sector,expected,found}` → `"fixup mismatch at sector {}: expected {:#06x}, found {:#06x}"`.
- `InvalidAttributeLength{offset,length}` → `"invalid attribute length {} at offset {:#x}"`.
- `InvalidDataRun{offset,reason}` → `"invalid data run at offset {:#x}: {}"`.
- `Generic(String)`.
  All reads go through bounds-checked helpers `read_u8/i8/u16_le/u32_le/u64_le/i64_le/uint_le/int_le/read_bytes` (lines 82-195); `read_int_le` sign-extends from the n-byte MSB.

**anyhow user-facing failures** (strings copy-exact):

- `scanner.rs`: `"Device/image reports size 0"` (line 204); resume bail reasons `"no scan file"`, `"wrong magic"`, `"device size mismatch"`, `"params mismatch"`, `"header too small"`, `"fragment count too large: {}"` (line 377, cap 10,000), `"open scan file"`, `"create scan file"`, `"reopen scan file"`, `"deserialize legacy"`, `"file too small"`.
- `main.rs`: `"No scan file found. Run 'scan' first."` (list, line 194; repair, line 280); `"invalid glob pattern: '{}'"` (line 386).
- `repairer.rs`: `"No MFT fragments found in scan results. The scan must have found MFT record 0 with parseable data runs, or fragments must be reconstructed from the scan. Cannot repair without fragment map."` (lines 35-39); `"MFT start offset is 0 — boot sector was likely unreadable. Cannot determine where to write the repaired record."` (lines 42-46); `"Boot sector is required for repair — cannot determine disk geometry"` (line 50); `"Record at offset {:#x} does not have FILE signature"` (line 485); `"Could not find unnamed non-resident $DATA attribute in existing record"` (line 533-535); `"New data runs ({} bytes) don't fit in existing $DATA attribute ({} bytes available). Would need to restructure the record."` (lines 549-555); `"MFT record too small to fit data runs. … Use repair with --fit-record to auto-trim fragments."` (lines 728-736, unreachable dead path — `--fit-record` flag **does not exist** in the CLI and `fit_record` is always `false`).
- `extractor.rs`: `"failed to open device/image for reading"`, `"failed to create output directory"`, `"failed to create parent directories"`, `"failed to create output file"`, `"failed to write resident data"`; per-file failures warn `"Failed to extract '{}': {}"` and continue (lines 101-108) — extraction never aborts on one bad file.

**Behavioral edge cases:**

- Truncated images: scan chunk read may return short (`chunk.truncate(n)`, scanner.rs 268-270).
- Extraction EOF mid-run: warn `"Unexpected EOF reading data run"`, keep partial file (extractor.rs 194-197).
- Scan-file truncation: read as many valid batches as possible (`read_all_batches` breaks on EOF / length `>1_000_000_000` / deserialize error with warns, scanner.rs 493-518).
- Bad fixup: never fatal (A-04); repair side likewise warns (repairer.rs 499-503).
- "Endless"/corrupt runlists: nibble bounds (A-06); absurd-cluster guard `>0x1_0000_0000_0000` (scanner.rs 108); negative absolute offset ⇒ partial return (data_runs.rs 83-89); zero-length run skip.
- Parent-chain cycles: `visited` set → `__CYCLE__/<mft_num>` prefix (rebuilder.rs 320-321).
- Orphans: `__ORPHANED__/` prefix + `tree.orphaned` (rebuilder.rs 326, 360-375); stats count `rooted` by absence of both markers (rebuilder.rs 138-147).
- Duplicate records: in-use > deleted > highest sequence (rebuilder.rs 66-74); repair path: highest sequence only (repairer.rs 209-214).
- Boot sector unreadable: defaults 512/4096/1024, `mft_start_offset=0`; repair then refuses (mft_start==0 bail).
- MFT spanning multiple records (`$ATTRIBUTE_LIST` → record 15): reconstruction capped to record 0's `end_vcn` (main.rs 288-336) so record 15's intact runs are not clobbered; documented in `DEBUGGING.md` (4.6M vs 5.6M records).
- Interactive safety gate: repair demands literal `yes` on stdin unless `--dry-run` (main.rs 358-374).
- `extract` with a pre-existing scan file **does not re-validate the device** — it trusts the cache (main.rs 234-245); `list` never touches the device (`/dev/null` passed, main.rs 217).
- Timestamps: raw u64 NTFS ticks (100 ns since 1601-01-01) stored in `FileEntry` but never converted or displayed (types.rs 35-37; extractor list output has no dates).

## Output Formats

- **`.rntfs` scan file (v4)** — `scanner.rs` lines 21-43 header comment, exact layout: 64-byte header `[0..8]="RNTFS004"`, `[8..10] sector u16le`, `[10..14] cluster u32le`, `[14..18] mft_record_size u32le`, `[18..26] last_scanned_offset u64le`, `[26..34] device_size u64le`, `[34..38] total_records u32le`, `[38..46] mft_start_offset u64le`, `[46..64] reserved`; then `frag_count u32le` + 24-byte fragments (`logical_start/cluster_start/cluster_count` u64le each); then repeated `[u64le length][bincode Vec<ScannedRecord>]` batches. v3/v2 read-compatible (no fragment table); pre-v2 "legacy" = bare bincode `ScanResult` (scanner.rs 557-565).
- **Recovered files** — output dir (default `./recovered`), original folder structure via sanitized `full_path` (`sanitize_path`, extractor.rs 267-273: per-component, drops empty/`.`/`..`), or `--no-dirs` flatten. `sanitize_filename` (lines 259-264) replaces `/ \ : * ? " < > | \0` with `_`, trims trailing dots/spaces. Collisions: `ensure_unique_path` (lines 276-302) appends `_1.._9999` before extension, fallback `<stem>_overflow`.
- **Terminal outputs** — list: `"{} {:>12}  {}"` flags `DCES` + size + path, then `"\nTotal: {} files"` (extractor.rs 213-225); dry-run: `"[dry-run] {} ({} bytes)"`; repair: plan block, fragment map lines, hex dump of first 128 bytes, comparison table with `+` markers for new fragments (repairer.rs 91-145, 372-445).
- **Backups** — `mft_record0_backup_<unix_secs>.bin` (and `mftmirr_record0_backup_<unix_secs>.bin` with `--fix-mirror`), raw `record_size` bytes; restore via dd documented in README lines 109-115.
- **Tree JSON** — `save_tree` (rebuilder.rs 378-383) writes `serde_json::to_string_pretty(DirectoryTree)`; not wired to any command.

## Tests & Fixtures

- Unit tests (in-file `#[cfg(test)]`): boot sector valid parse (`boot_sector.rs` 78-118, builds a synthetic 512-byte BPB); MFT header parse + validation (`mft_record.rs` 136-176, `make_minimal_record` with USA at 0x30/size 3, seq 1, flags 1, record 42); fixup round-trip (`fixup.rs` 62-103); data-run decode/encode incl. sparse, negative relative offset, multi-run, large 2 TB-scale offsets (`data_runs.rs` 181-306); filename parse (`filename.rs` 111-140); `parse_size` + `sanitize_filename` (`extractor.rs` 330-349).
- Integration fixture: `tests/create_test_image.sh` — 32 MB image, `mkfs.ntfs -F -Q -s 512 -c 4096`, populated tree (Documents/Reports, Photos, Music), resident `readme.txt` (~23 B) vs non-resident 16 KB/64 KB/128 KB/256 KB/512 KB files; runbook in README §Testing (scan → list → extract). Requires root for loop mount; no corruption step is scripted — the "corrupt the MFT" idea from SPEC.md §Testing Strategy is not implemented as a fixture.
- No CI config, no `tests/*.rs` integration tests, no benchmarks in the repo.

## Licensing

`LICENSE` (verbatim, 11 lines): "BSD Zero Clause License" — "Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted." plus the standard AS-IS/no-warranties disclaimer of merchantability, fitness, and liability. This is **0BSD** — permissive, trivially compatible with incorporation into the target product (attribution not even legally required, though courteous). README §License says only "See repository for license details."

## Coverage

Repo has **25 tracked files** (`git ls-files | wc -l` = 25). Read fully with the Read tool (or full equivalent via persisted output for the oversized file):

1. `README.md` (260 lines)
2. `SPEC.md` (613)
3. `DEBUGGING.md` (182)
4. `sources.txt` (109)
5. `LICENSE` (11)
6. `Cargo.toml` (23)
7. `tests/create_test_image.sh` (54)
8. `src/main.rs` (388)
9. `src/types.rs` (107)
10. `src/scanner.rs` (566)
11. `src/rebuilder.rs` (391)
12. `src/extractor.rs` (349)
13. `src/repairer.rs` (858 — read completely in three passes)
14. `src/parser/mod.rs` (9)
15. `src/parser/boot_sector.rs` (118)
16. `src/parser/mft_record.rs` (176)
17. `src/parser/fixup.rs` (103)
18. `src/parser/error.rs` (195)
19. `src/parser/attributes.rs` (202)
20. `src/parser/data_runs.rs` (306)
21. `src/parser/filename.rs` (140)
22. `src/parser/standard_info.rs` (29)
23. `src/parser/attribute_list.rs` (68)
24. `Cargo.lock` — **partial**: not read line-by-line; package inventory extracted (76 packages via `name =` pattern scan) to verify dependency set (includes `memmap2`, unused in code — see anomalies).
25. That accounts for all 25 tracked files; there is no 25th content file beyond those listed (the count includes Cargo.lock as #24 and `.git` internals are out of scope/not read).

Not read: `.git/` directory contents (excluded per rules; no credentials touched; `git remote` never run).

**Anomalies / absence findings (plain):**

- `memmap2` declared in Cargo.toml and locked, but **zero uses** in `src/` (grep `memmap` = no matches) — scanning is plain `File::read` into 64 MB buffers. README's "Memory-mapped I/O" claim is stale.
- Dead code: `build_mft_record_0` (repairer.rs 590-796) and `fit_record` field never called/read; `save_tree`/`load_tree` (rebuilder.rs 378-391) exported but unused; `parse_attribute_list` + `attribute_list.rs` module never invoked; `ScanIndexEntry` (types.rs 72-79) never constructed; `hard_link_count`, `parent_sequence`, `lsn`, `index_record_size`, `volume_serial`, `total_sectors` parsed but unused downstream.
- `main.rs` note at line 294 ("end_vcn = 1,154,035 → records 0 through 4,616,143") embeds a real-drive constant in a comment only — code is generic.
- `list` subcommand ignores its `device` arg entirely (main.rs 185-217).
- No backup-boot-sector fallback; no $INDEX_ROOT/$INDEX_ALLOCATION parsing (deliberate, README Limitations); no LZNT1/EFS; no timestamp conversion to human-readable form anywhere.
- Trailing-dot trimming in `sanitize_filename` means a file literally named `foo.` becomes `foo` — collision suffixing then disambiguates.
