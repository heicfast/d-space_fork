# MFT-RESCUE Feature Harvest — Agent G

| Field        | Value                                                                                                                                                                          |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Repo         | https://github.com/hasithahathella/mft-rescue (local depth-1 clone `/home/z/my-project/refs/mft-rescue`)                                                                       |
| HEAD         | `9b0e5ecc9d24ae9918f83e3a81e4a8616f610a22` (single commit, verified `git rev-parse HEAD`)                                                                                      |
| License      | MIT, "Copyright (c) 2026 Hasitha Haththella" (LICENSE, verbatim standard MIT text, 21 lines)                                                                                   |
| Stack        | Python 3.10+ stdlib-only (argparse, ctypes, ctypes.wintypes, json, logging, os, re, struct, sys, time, dataclasses, pathlib, typing) — **no Rust/Tauri/GUI**; Windows CLI tool |
| Date / Agent | 2026-11-12 / Agent G (MFT-analysis & rescue features)                                                                                                                          |
| Size         | 8 tracked files, 1382 lines total; largest: recovery/ntfs_recover.py (655 ln)                                                                                                  |

## Overview

- What it actually **is**: a small read-only two-module Python CLI for Windows:
  1. `recovery/image_disk.py` — ddrescue-style forensic imager (clone failing source → `.img`).
  2. `recovery/ntfs_recover.py` — NTFS rescue for volumes whose boot sector/volume metadata is destroyed but whose MFT survived: brute-force **whole-volume scan** for `FILE` records, directory-tree rebuild from parent refs, file re-extraction with original names/paths. Package docstring: "NTFS recovery via raw MFT parsing — read-only" (`recovery/ntfs_recover.py:1-24`).
- It is **not** a general MFT forensic analyzer; no $LogFile/$UsnJrnl/slack analysis exists (see absence list). Single linear scan pass; no multi-pass salvage strategy.
- Entry points: `python -m recovery.image_disk`, `python -m recovery.ntfs_recover` (`main(argv)` + `__main__` guards: `recovery/image_disk.py:293-307`, `recovery/ntfs_recover.py:626-655`). No pyproject/setup.py/CI; modules run directly. Version `"0.1.0"` (`recovery/__init__.py:2`).
- Tests: 11 unit tests in `tests/test_ntfs.py` (synthetic in-memory MFT records, no disk access; `python -m unittest discover -s tests -v`, `README.md:156-164`). Classes: FixupTests (2), RunlistTests (3), SanitizeTests (2), ParseRecordTests (3), SafetyTests (1).

## Implemented Capabilities (verified in code)

1. **USA (Update Sequence Array) fixup** — `_apply_fixup(rec, sector_size=512)` (`recovery/ntfs_recover.py:66-88`):
   - Reads USA offset `<H` at 0x04, USA count `<H` at 0x06; sectors = `usa_count-1`.
   - Rejects: `len(rec)<0x30`, `usa_off==0`, `usa_count<2`, `usa_off+usa_count*2>len(rec)`, `sectors*sector_size>len(rec)`, sector-tail ≠ USN → `return False` ("corrupted record", comment line 85).
   - Restores each sector's last 2 bytes from the saved USA slot; mutates record in place.
2. **MFT record header parse** — `parse_record` prologue (`recovery/ntfs_recover.py:180-194`):
   - Rejects short buffer (`len < expected_size`) or sig ≠ `b"FILE"` → `None`.
   - Fields: flags `<H`@0x16 (`FLAG_RECORD_IN_USE=0x0001`, `FLAG_RECORD_IS_DIRECTORY=0x0002`, lines 50-51); attrs offset `<H`@0x14; record number `<I`@0x2C; sequence `<H`@0x10; base-record ref `<Q`@0x20 masked `& 0x0000_FFFF_FFFF_FFFF` (48-bit).
3. **Attribute walking** — inline loop in `parse_record` (`recovery/ntfs_recover.py:198-290`):
   - Stops at `ATTR_END=0xFFFFFFFF`, `total_len<24`, or `off+total_len>len(rec)` (and `off+64>len` for non-resident).
   - Resident: content size `<I`@off+16, content offset `<H`@off+20. Non-resident: runlist offset `<H`@off+32, alloc `<Q`@off+40, real `<Q`@off+48, init `<Q`@off+56, attr flags `<H`@off+12. Attribute names UTF-16-LE, `errors="replace"`.
   - Per-attribute body parsing wrapped in `try/except Exception: pass` (line 287-288) — corrupt content silently skipped.
   - Handles only types 0x10/0x30/0x80 (constants at lines 43-48).
4. **Resident + non-resident $DATA with runlists** — `DataAttr` (`recovery/ntfs_recover.py:135-145`); `_decode_runlist()` (`recovery/ntfs_recover.py:91-118`):
   - Header nibbles: len size = `&0x0F`, offset size = `>>4 & 0x0F`; terminator header byte 0; `off_size==0` → sparse run `(run_len, None)`; LCN deltas **signed**, accumulated (`prev_lcn += offset`).
   - Truncated runlist → `break` (silent), not an error.
5. **FILE_NAME (0x30) parsing & name/path recovery** — `recovery/ntfs_recover.py:228-250`:
   - Parent MFT ref `<Q`@content+0 masked to 48-bit; 4×FILETIME `<QQQQ`@+8 (created, modified, change-discarded, accessed); alloc/real `<QQ`@0x28; name length @0x40; namespace @0x41; name UTF-16-LE @0x42, `errors="replace"`; requires `len(content)>=0x42`; empty name → attr dropped (`if fname:`).
   - Namespace preference `_pick_name` (`recovery/ntfs_recover.py:306-317`): order `(NS_WIN32_DOS=3, NS_WIN32=1, NS_POSIX=0, NS_DOS=2)`; `by_ns.setdefault` = first attr per namespace wins; fallback `rec.names[0]`.
6. **Parent-reference directory reconstruction** — `_build_paths(records)` (`recovery/ntfs_recover.py:320-351`):
   - Memoized DFS via `parent_ref`; root hardcoded as record 5 (`paths[5]=""`).
   - Orphan taxonomy: cycle → `__orphans__/cycle_<n>` (not memoized); missing parent → `__orphans__/missing_<n>`; no name attr → `__orphans__/noname_<n>`; self-parent or parent==0 → `__orphans__/<name>`.
7. **Whole-volume aligned MFT scan + extension merge** — `scan_mft()` (`recovery/ntfs_recover.py:420-481`):
   - 16 MiB buffers; only `record_size`-aligned offsets checked for `FILE` sig; read error → warn + skip 1 MiB (line 437); short read → zero-pad.
   - Base records (`base_ref==0`) → `records[rec_num]`; dedup rule line 454: first occurrence kept unless later copy has `in_use` set.
   - Extension records grouped by `base_ref`; post-scan merge: `base.data_streams.extend(ext.data_streams)` + `base.names.extend(ext.names)` (lines 471-477) — multi-record (large) files reassembled. `$ATTRIBUTE_LIST` (constant line 44) **never parsed**; discovery is purely by `base_ref`.
8. **MFT record-size auto-detection** — `_detect_record_size()` (`recovery/ntfs_recover.py:392-417`):
   - Samples first 64 MiB; up to 64 `FILE`-sig hits whose allocated-size field `<I`@+0x1C ∈ {1024,2048,4096}; needs ≥4 hits; majority-vote of inter-hit diffs restricted to those sizes; fallback 1024.
   - **Cluster size is NOT auto-detected** — CLI default 4096 (`recovery/ntfs_recover.py:631`).
9. **Recovery driver with filters** — `recover()` (`recovery/ntfs_recover.py:496-623`), 3 passes (scan → paths → write):
   - `_matches_filters` (484-493): `--ext` set vs lowercased extension (`_ext_of`, 354-358); `--name` regex compiled `re.I`, `search()`ed against basename.
   - `--max-size` default 2 GiB: `stream.real_size > max_size` → `skipped_size` counter, file skipped (572-574).
   - `--dry-run`: lists max 50 candidates `"  [dry] %s  (%d bytes)"`, no exhaustive count (536-552).
   - Stream choice line 571: `next((d for d in rec.data_streams if d.name == ""), rec.data_streams[0])` — unnamed default $DATA preferred → ADS effectively skipped (README.md:143).
10. **Data assembly** — `_read_runs()` (`recovery/ntfs_recover.py:368-389`):
    - Sequential run reads capped at `real_size`; sparse run → zeros; short read → zero-pad; per-run `OSError` → zeros.
    - Resident: `resident_bytes[:real_size]` (586). `init_size` parsed but **unused** — bytes between init_size and real_size read as stale cluster data instead of zeros (forensic inaccuracy).
11. **Timestamp extraction & restore** — `recovery/ntfs_recover.py`:
    - SI times `<QQQQ`@content+0 → `si_created, si_modified, _, si_accessed` (224-227; change-time discarded); FILE_NAME times into `FileNameAttr` (230-248).
    - `_ft_to_unix` (361-365): `(ft - 116444736000000000) / 10_000_000`; `ft<=0 → None`.
    - Restore (597-606): `os.utime(target, (at or mt, mt))`; mtime = SI-modified else FILE_NAME-modified fallback; atime = SI-accessed else mtime; creation time parsed but never used; utime OSError silently swallowed.
12. **Windows raw-device I/O layer** — `RawWinDevice` (`recovery/image_disk.py:88-157`):
    - `CreateFileW(GENERIC_READ, FILE_SHARE_READ|FILE_SHARE_WRITE, OPEN_EXISTING, FILE_FLAG_SEQUENTIAL_SCAN)`; size via `IOCTL_DISK_GET_LENGTH_INFO` (0x7405C); `SetFilePointerEx`/`ReadFile`/`CloseHandle` via ctypes (protos lines 42-84; `FILE_FLAG_NO_BUFFERING` defined but **unused**).
    - Non-raw path → plain `open(path,"rb")`; works off-Windows in file mode.
13. **ddrescue-style imager** — `image()` (`recovery/image_disk.py:198-290`):
    - Default 1 MiB chunk; pre-creates full-size sparse file; on chunk read error falls back to 512-byte sector retry (`SECTOR=512`), failed sectors zero-filled + appended to `bad_ranges` as `[s_off, s_off+SECTOR]` (248-269).
    - Progress checkpoint every ≥5 s or at end to `<out>.progress.json` via atomic tmp+`os.replace` (`_save_progress`, 192-195); `--resume` reloads `{"offset","bad"}` (corrupt JSON → `{}` silently, 183-189).
    - Rate/percent line `"%.2f%%  %.2f / %.2f GiB  %.1f MiB/s  bad=%d"` (275-278); always returns 0 even with bad bytes (287).
14. **Same-volume output refusal (safety)** — `image_disk.py:162-180` `_refuse_unsafe_output` (file source → resolved `.drive` compare; raw single-letter source → letter compare) and `ntfs_recover.py:501-505` (lighter re-implementation, runs **before** mkdir/logging; verified by `tests/test_ntfs.py:171-186`).
15. **Filename sanitization** — `_sanitize_name()` (`recovery/ntfs_recover.py:61-63`): `[<>:"/\\|?*\x00-\x1f]` → `_`; `rstrip(". ")`; empty → `"_"`. Tests `tests/test_ntfs.py:136-141`.
16. **Deleted-record handling (implicit only)** — `in_use` flag parsed (line 187) and used solely in scan dedup (line 454: a later in_use duplicate replaces an earlier copy). Scanner indexes records regardless of `in_use`, so **deleted files are recovered incidentally** and look identical to live files in output — **no deleted-status detection, flagging, or reporting anywhere**.
17. **Extension-record (MFT multi-record file) support** — capability 7: base+extension `$DATA` runs and names merged into base record; not attribute-instance-aware.

## Claimed-but-Absent Capabilities (critical absence list)

Verified by exhaustive read + grep (`LogFile|UsnJrnl|USN|journal|slack|INDEX_ALLOCATION|BITMAP|MFTMirr|boot|BPB|EFS|encrypt` — only docstring/README prose hits, no code):

1. **$LogFile journal replay — ABSENT.** No parsing, no replay semantics, no `$LogFile` reference anywhere. Nothing is replayed; no transaction/undo logic of any kind.
2. **$UsnJrnl analysis — ABSENT.** No USN journal parsing, no `USN_RECORD`, no deletion-cause reconstruction. (Only "USN" hits are Update Sequence Numbers in fixup tests — unrelated.)
3. **MFT slack-space carving — ABSENT.** Scan checks only `record_size`-aligned offsets (`recovery/ntfs_recover.py:445`); no unaligned `FILE`-sig search, no end-of-record slack, no stale-record recovery from slack, no record-reuse analysis.
4. **Parent-reference sequence-number validation — ABSENT.** `seq` parsed (line 191) but never compared against the parent ref's high 16 bits (masked off/discarded at lines 192/229). Stale parent refs to reused records silently followed — no sequence-mismatch orphaning.
5. **SI-vs-FILE_NAME timestamp divergence analysis — ABSENT.** Both sets parsed, never compared; no divergence report. FILE_NAME times only as utime fallback (598-600).
6. **Hard-link / multi-name recovery — ABSENT (lossy).** Multiple FILE_NAME attrs collected (`out.names` list) but `_pick_name` selects exactly one name/parent per record; hard-linked files recovered under a single name; sibling names discarded.
7. **Boot-sector/BPB parsing — ABSENT (by design; premise is destroyed boot sector).** No $MFT location from metadata; blind whole-volume scan → phantom `FILE` records in non-MFT data at aligned offsets get indexed (deduped only by rec_num rule).
8. **Directory-index parsing — ABSENT.** `ATTR_INDEX_ROOT=0x90` constant (line 47) never parsed; tree built exclusively from FILE_NAME parent refs.
9. **README-vs-code discrepancies (honesty notes):**
   - `README.md:153-154` "recovery logs every write with the originating MFT record number so the run is auditable" — **false**: successful writes are never logged individually; only failures, 5-s aggregates, dry-run entries (`recovery/ntfs_recover.py:610-618`).
   - `README.md:140-142` compressed `$DATA` "flagged in the log" — emitted at `log.debug` (line 589) while logging runs at `INFO` (line 509): **invisible by default**.
   - `README.md:134-139` implies geometry auto-detection — only record size is auto-detected; cluster size never is.

## Algorithm Inventory (A-xx contracts)

- **A-01 USA fixup** — `_apply_fixup` (`ntfs_recover.py:66-88`). In: record bytes; Out: bool; mutates in place (restores sector tails). Rejections listed in capability 1.
- **A-02 MFT header parse** — `parse_record` prologue (180-194). `None` on short/wrong-sig.
- **A-03 attribute walker** — `parse_record` loop (198-290). Terminators: `ATTR_END`, `total_len<24`, overrun, `off+64>len` (non-resident). Silent per-attr `except: pass`.
- **A-04 resident attr extraction** — SI times; FILE_NAME struct (parent ref, 4 times, sizes, name, namespace; `len>=0x42`); $DATA resident + `compressed = flags_attr & 0x0001`.
- **A-05 non-resident attr header** — runlist offset / alloc / real / init (264-272); runlist bytes sliced `[off+runlist_off : off+total_len]`.
- **A-06 runlist decode** — `_decode_runlist` (91-118). Signed relative LCN deltas; sparse `lcn=None`; `break` on truncation.
- **A-07 record-size autodetect** — `_detect_record_size` (392-417). 64 MiB sample; alloc-size ∈ {1024,2048,4096} filter; ≥4 hits; mode of diffs; fallback 1024.
- **A-08 aligned whole-volume scan + merge** — `scan_mft` (420-481). 16 MiB buffers; aligned-only; "in_use wins" dedup; extension merge by `base_ref`. Stats line `"MFT scan complete. base records=%d, extensions=%d, raw FILE hits=%d"` (the "raw FILE hits" counter actually counts parsed base records incl. duplicates).
- **A-09 path reconstruction** — `_build_paths` (320-351). Root=5 hardcoded; memoized; orphan taxonomy (cycle/missing/noname/self-parent).
- **A-10 name selection** — `_pick_name` (306-317). Namespace ladder 3→1→0→2; first-per-namespace; fallback `names[0]`.
- **A-11 sanitization** — `_sanitize_name` (61-63).
- **A-12 filter + stream choice + size cap** — `recover` (561-574) + `_matches_filters` (484-493) + `_ext_of` (354-358).
- **A-13 data assembly** — `_read_runs` (368-389) / resident slice (586-592). Zero-fill on sparse/short/OSError; `init_size` unused.
- **A-14 FILETIME→Unix + utime restore** — `_ft_to_unix` (361-365); restore block (597-606).
- **A-15 Win32 raw-device IO** — `RawWinDevice` (`image_disk.py:88-157`) + protos (42-84).
- **A-16 ddrescue-style imaging** — `image` (`image_disk.py:198-290`). Sector-granularity salvage; `[start,end]` bad-range pairs; short chunk reads zero-padded but **not** logged as bad (silent zero regions, lines 242-245).
- **A-17 checkpoint/resume** — `_load_progress`/`_save_progress` (`image_disk.py:183-195`). JSON `{"offset":int,"bad":[[s,e]...],"size":int}`; atomic replace; corrupt checkpoint silently ignored.
- **A-18 output-volume refusal** — `image_disk.py:162-180`; `ntfs_recover.py:501-505`.
- **A-19 dry-run preview** — `recover` (536-552). Max 50 lines; no exhaustive count.
- **Dead code:** `_parse_attr_header` (`ntfs_recover.py:162-177`) defined, divergent contract, never called (grep-verified: definition only); `ATTR_ATTRIBUTE_LIST`/`ATTR_INDEX_ROOT` constants unused in logic.

## Edge Cases & Error Model

**Parse-level (silent rejections):** short record; bad signature; fixup failure → `parse_record` returns `None` → skipped by scan with no logging of rejected records; attribute-loop breaks on malformed length; per-attr `except: pass`; UTF-16 decode errors → U+FFFD replacement; empty FILE_NAME name → attr dropped.

**Path-level:** cycles → `__orphans__/cycle_N` (not memoized → repeated re-walk); root record 5 virtual even if never found; unbounded Python recursion on >~1000-deep parent chains → uncaught `RecursionError` [UNVERIFIED — inferred from recursion design, not exercised by tests].

**Scan-level:** mid-scan read error → `"Read error at %d: %s. Skipping 1 MiB."` (`ntfs_recover.py:437`); alignment assumes `record_size` divides 16 MiB (true for 1024/2048/4096; custom `--record-size` not dividing it desynchronizes records across buffers [UNVERIFIED — inferred]); phantom records indexed from arbitrary aligned offsets.

**Recovery-level user-facing messages (copy-exact):**

- `"Refusing: output {out} is on source drive {src_letter}:"` (`ntfs_recover.py:505`, SystemExit)
- `"Refusing: output {out} is on the same volume as source {src}"` / `"... is on source drive {src_letter}:. Pick a different volume."` (`image_disk.py:173,178-180`, SystemExit)
- `"CreateFileW({path}) failed (WinError {err}). Run as Administrator?"`, `"IOCTL_DISK_GET_LENGTH_INFO failed (WinError {err})"`, `"SetFilePointerEx({offset}) failed (WinError {err})"`, `"ReadFile failed (WinError {err})"` (OSError; `image_disk.py:108,126,138,149`)
- `"Raw device paths require Windows"` (RuntimeError; `image_disk.py:96`)
- `"mkdir failed for %s: %s"` / `"recover failed for %s: %s"` (warnings; `ntfs_recover.py:580,611`)
- `"Read error at offset %d (%s). Sector retry."` / `"Salvaged %d/%d bytes in problem region"` (`image_disk.py:250,269`)
- `"compressed stream (raw bytes): %s"` (debug-level → invisible at INFO; `ntfs_recover.py:589`)

**Safety-check quirks (findings):** `ntfs_recover.py:501-502` extracts `source[4].isalpha()` — source `\\.\PhysicalDrive1` yields `src_letter="P"` and **falsely refuses** output on `P:`, while the imager guards with `len(src_letter)==1` (`image_disk.py:176-177`) — the two checks are inconsistent. On POSIX, `_same_volume` compares `.drive` which is `''` for all paths → **any file-to-file imaging refused on Linux** (`image_disk.py:162-166`) [UNVERIFIED on real POSIX run — inferred]. Imager exit code always 0 even with bad sectors (`image_disk.py:287`). After extension merge, multiple unnamed `$DATA` streams: only the first recovered (line 571).

## Output Formats

1. **Log file** (both tools): `logging.basicConfig(format="%(asctime)s %(levelname)s %(message)s", handlers=[FileHandler(encoding="utf-8"), StreamHandler()])` (`image_disk.py:202-206`; `ntfs_recover.py:508-512`). Defaults: `<out>.log` (imager), `<out>/recovery.log` (recovery).
2. **Progress checkpoint JSON** (`<out>.progress.json`): `{"offset": <byte offset>, "bad": [[start,end],...], "size": <total bytes>}` (`image_disk.py:213-216,279`).
3. **Imaging bad-region records:** 512-byte `[s_off, s_off+SECTOR]` pairs; final line `"DONE. %d bytes imaged. %d bad bytes zero-filled across %d ranges."` (`image_disk.py:283-286`).
4. **Recovered tree:** sanitized original relative paths (`rel.replace("/", os.sep)`), mtime/atime restored, orphans under `__orphans__/...` variants (`ntfs_recover.py:320-351,576,586-606`).
5. **Dry-run listing:** `"  [dry] %s  (%d bytes)"` capped at 50 + `"... (use full run to recover)"` (`ntfs_recover.py:546-549`).
6. **Periodic status:** `"MFT scan %.2f%%  records=%d  pos=%.2f GiB"`; `"recovered=%d  failed=%d  skipped(size)=%d  written=%.2f GiB"` (5-s throttle; `ntfs_recover.py:464-467,616-617`).

## Licensing

MIT License, Copyright (c) 2026 Hasitha Haththella — full standard text in `LICENSE` (21 lines, unmodified, no extra terms). Permissive; algorithms harvestable into the Tauri/Rust product with attribution + license notice in redistribution.

## Coverage

- **8/8 tracked files read exhaustively in full** (`git ls-files`): `.gitignore` (32 ln), `LICENSE` (21 ln), `README.md` (175 ln), `recovery/__init__.py` (2 ln), `recovery/image_disk.py` (307 ln), `recovery/ntfs_recover.py` (655 ln), `tests/__init__.py` (0 ln, empty), `tests/test_ntfs.py` (190 ln).
- Repo is exactly as small as advertised: single-commit, ~1.4k-line v0.1.0 work-in-progress; no CI, no packaging, no hidden directories.
- Read-only session honored: no file modified, git state untouched (verified clean `git status --porcelain`), no network, no credential material accessed. Tests were **read**, not executed.
- **Anomaly for the harvest:** the MFT-analysis surface is narrower than the session prompt assumed — the genuinely reusable assets are the USA fixup, runlist decoder, FILE_NAME/path reconstruction with orphan taxonomy, whole-volume aligned scan, extension-record merging, and the ddrescue-style imager with its refusal/checkpoint safety model.
