# SECTORFORGE — Feature Map (Agent B)

| Field         | Value                                                                                                                                                           |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Repo          | https://github.com/terraseraph/sectorforge (depth-1 clone at `/home/z/my-project/refs/sectorforge`)                                                             |
| HEAD          | `b292a299c18013706e81d670657ced6c43f200ca` (verified via `git rev-parse HEAD`)                                                                                  |
| Tracked files | 197 (verified via `git ls-files \| wc -l`)                                                                                                                      |
| License       | **MIT**, `LICENSE` lines 1–21, "Copyright (c) 2026 terra seraph" — read in full                                                                                 |
| Stack         | Rust workspace: `crates/sf-core` (engine) + `crates/sf-cli` (CLI; clap 4, anyhow, serde_json, sha2; thiserror; windows-sys / libc for raw IO)                   |
| GUI           | Tauri 2 (`gui/src-tauri`, Rust) + React 19 / TypeScript strict / Vite 6 / zustand 5 / @tanstack/react-virtual 3 / Vitest + Testing Library (`gui/package.json`) |
| Version       | workspace `version = "0.3.0"` (`Cargo.toml` line 7); JSON contract v2 (`crates/sf-cli/src/lib.rs` `CONTRACT_VERSION: u32 = 2`, line 22)                         |
| Date / agent  | 2026-11-12, Agent B (research only; nothing modified, no network, no git state changes)                                                                         |

Citation convention: paths relative to repo root; anchors are function/struct/const/line-ranges; identifiers copy-exact (Grep-verified where noted).

### Executive summary (one screen)

- **Product**: read-only, safety-first drive rescue: image → scan partitions → list/extract files (NTFS, FAT12/16/32, exFAT, ext2/3/4; deleted included) or carve by signature (94 type ids). Windows + Linux, portable, no installer.
- **The named feature exists**: deleted/live classified at scan time (MFT in-use bit / FAT 0xE5 / exFAT type-bit-7 / ext dir-slack); exposed post-scan via `files --deleted-only` (`crates/sf-cli/src/recover.rs` `run_files` lines 236–243) and the GUI "Deleted only" filter (`FileBrowser.tsx` lines 154–160) plus the "Only list deleted files (quicker on very large volumes)" pre-run option (`FilesTab.tsx` lines 54–56); original hierarchy preserved via parent-reference chains with sequence-aware validation (`ntfs/mod.rs` `parent`/`ancestry`) and rebuilt on extraction under `--out` with `_orphaned/` and ` (id N)` collision suffixes (`ntfs/extract.rs`).
- **Stack match with the target (d-space_fork)**: Tauri 2 + Rust + React with a strict NDJSON child-process contract — directly comparable architecture; nearly every subsystem (imaging, mapfile, scan, files, extract, carve, hash, survey) ports concept-for-concept.
- **License**: MIT — compatible with a GPL-3.0-or-later re-implementation, whether studied or ported (with notice preservation on ported portions).
- **Highest-value ports**: the honesty plumbing (SafeReader + map → partial), the ancestry/path algorithm with the deleted-parent seq+1 rule, the same-drive refusal guard, the carver registry (94 ids / 51 kinds with structural end detection), the contract-smoke double-sided test method, and the column-oriented million-row file store.

## Overview & Architecture

**Product** (README.md lines 13–20):

- Windows/Linux portable data-recovery app for failing drives: two-pass ddrescue-style imaging → partition discovery → filesystem file listing incl. deleted + extraction; or header-signature carving when no filesystem survives.
- Ideas credited to ddrescue, TestDisk/PhotoRec, DMDE, R-Studio, UFS Explorer (README.md lines 18–19, 238–243). "SectorForge refuses to write anything onto the drive it's reading from" (README.md lines 104–105).

**Layering** (README.md "Layout" lines 224–236, confirmed in code):

- `crates/sf-core` — engine: raw device access, resumable mapfile, imager, partition detection, drive listing + same-drive check, NTFS/FAT/exFAT/ext readers, carver (`crates/sf-core/src/lib.rs` module list, lines 5–18).
- `crates/sf-cli` — thin CLI wrapper speaking the NDJSON contract v2 (`crates/sf-cli/src/lib.rs` header lines 1–3; `Command` enum lines 51–165).
- `gui/src-tauri` — Tauri 2 shell; **never links sf-core on Linux**; runs `sf-cli --json` as a child process and parses stdout (`gui/src-tauri/src/cli.rs` header lines 1–2; `docs/cli-contract.md` lines 5–11).
- Windows single-exe variant: the engine is embedded and the GUI runs a copy of _itself_ as the child — `SectorForge.exe sf-cli <args>` (`gui/src-tauri/src/main.rs` lines 4–9; `EMBEDDED_ARG = "sf-cli"` cli.rs line 214; `ENGINE_EMBEDDED: bool = cfg!(windows)` line 218).
- `gui/src` — React frontend; Zustand stores + a column-oriented `FileStore` held _outside_ React state for million-row listings (`gui/src/store/recover.ts` lines 92–98).

**Process model**:

- One CLI child per job; job kinds `image|scan|carve|files|extract|hash|survey` (`gui/src/lib/contract.ts` line 44).
- One job at a time in the UI (`gui/src/store/actions.ts` `begin()` lines 21–29: "Wait for the current {job} job to finish, or stop it first.").
- Cancellation = killing the child (`docs/cli-contract.md` lines 29–32; `gui/src-tauri/src/cli.rs` `stop_job` lines 468–473). Reader threads drop buffered output when the run-id no longer matches (cli.rs lines 442–445), which also ends a pkexec'd child we may not signal.
- The startup survey runs beside any user job (`gui/src/store/events.ts` lines 26–27).

**Threading model**:

- Engine itself is single-threaded for imaging/scan/carve (sequential passes).
- `run_survey`: `std::thread::scope`, one thread per source (`crates/sf-cli/src/lib.rs` lines 511–535).
- Tauri side: reader thread for child stdout + separate stderr-drain thread keeping a 16 KiB tail (`gui/src-tauri/src/cli.rs` `run_job` lines 389–466; `STDERR_KEEP` line 385).
- `exit_with_parent` watcher thread for the elevated relaunch (`gui/src-tauri/src/lib.rs` lines 560–573).
- GUI: file-row redraw throttled to 200 ms (`gui/src/store/events.ts` `FILES_REDRAW_MS` line 11); `file` events batched to the UI at 2000 rows / 80 ms (`gui/src-tauri/src/lib.rs` `BATCH_MAX`/`BATCH_EVERY` lines 50–51).

**Error model** (summary; full section below):

- Engine returns plain-English `String` errors from filesystem readers (e.g. `Ntfs::open` → "the NTFS boot sector can't be read", `crates/sf-core/src/ntfs/volume.rs` lines 84–90).
- CLI wraps with anyhow; emits `{"event":"error","message":…}` + exit 1; clap usage errors → stderr, exit 2, no JSON (`crates/sf-cli/src/lib.rs` `run()` lines 730–740; contract lines 27–28).
- GUI: one current error alert per screen + fallback "sf-cli stopped with exit code N." when a non-zero exit had no error event (`gui/src/store/events.ts` `onExit` lines 214–222).
- Recovery reads never abort a file: unreadable bytes become zeros and are counted and reported ("partial" honestly — `crates/sf-core/src/reader.rs` header lines 1–8).

**Persistence**:

- The ddrescue-format mapfile is the only durable engine state: checkpointed ≥ every 5 s and at each pass end (`CHECKPOINT_INTERVAL` sf-cli lib.rs line 23; `Imager::with_checkpoint` imager.rs lines 72–99; atomic `.tmp`+`fsync`+rename with Windows retry, mapfile.rs `save` lines 212–238).
- GUI session state is in-memory zustand only; across "Restart as Administrator", source/dest/map/screen survive via `--elevated --source … --screen …` argv round-trip (`gui/src-tauri/src/lib.rs` `Restore`/`restore_args`/`parse_restore` lines 400–440).
- Large extraction selections spill to a temp ids file, removed on exit (`gui/src-tauri/src/lib.rs` `start_extract` lines 246–279). No database anywhere.

**Entry points**: `sf-cli` binary (`crates/sf-cli/src/main.rs`), `SectorForge.exe [sf-cli …]` (`gui/src-tauri/src/main.rs`), React root `gui/src/main.tsx` (`boot()`: startBridge → restoreAfterElevation → render → checkCli/loadDevices/pollStatus).

## Feature Inventory (F-01 … F-42)

### Imaging / rescue

**F-01 Two-pass ddrescue-style cloning.**
`crates/sf-core/src/imager.rs` `run_copy_pass` lines 138–180 (1 MiB chunks, failed range parked `NonTrimmed`, jump `skip_size`)
and `split_and_retry` lines 243–284 (binary-split to sector granularity, `max_retries` per sector, default 3).
`ImagerConfig` lines 15–40. Test `copy_then_retry_isolates_bad_sectors_and_rescues_the_rest` (lines 329–358).

**F-02 Resumable, ddrescue-compatible mapfile.**
`crates/sf-core/src/mapfile.rs`: `Status` chars `? * / - +` (lines 28–49), `mark` splits/merges blocks (lines 126–164),
ddrescue text round-trip `to_ddrescue_string`/`from_ddrescue_str` (lines 191–311).
README.md lines 110–111: the map is ddrescue's format so `ddrescueview` can open it.
Resume refusals in `run_image` (`crates/sf-cli/src/lib.rs` lines 402–426):
wrong-size map ("mapfile {} is for a {}-byte device, but the source is {} bytes — wrong map for this source?")
and missing/different dest ("the map says {} was already copied, but the destination {} {}. Resuming would leave those parts of the image empty.").

**F-03 Reverse first pass.**
`--reverse` (sf-cli lib.rs lines 78–80); `run_reverse_copy_pass` imager.rs lines 184–222; dest preallocated sparse when possible ("Without this, the first write (at the end) waits while Windows zero-fills the whole image", lib.rs lines 448–452).

**F-04 Retry-only mode.**
`--retry-only` skips pass 1 (sf-cli lib.rs lines 75–77, 463). GUI: "Retry bad areas only (skip the fast first pass)" checkbox + map-viewer action (ImageScreen.tsx line 88–90; MapViewScreen.tsx line 97).

**F-05 Tunables + presets.**
CLI `--chunk-size/--skip-size/--retries` (lib.rs lines 66–74). GUI presets Healthy / Some bad sectors / Badly damaged / Thorough (`gui/src/store/image.ts` `PRESETS` lines 53–78).

**F-06 Direct hardware access, cache-bypassing, optional pass-through.**
Windows `FILE_FLAG_NO_BUFFERING`; SCSI READ(16)/ATA READ DMA EXT pass-through with per-command `--timeout` (default 10 s); Linux `O_DIRECT` (pass-through refused for now: "SCSI/ATA pass-through isn't available on Linux yet; use --access os", sf-cli lib.rs lines 255–273). Global `--access os|scsi|ata` (lines 37–45; contract lines 33–39).

**F-07 Same-drive output refusal (v2).**
`crates/sf-core/src/drives/mod.rs` `ensure_outputs_off_source` lines 80–104: "refusing to start: {what} {} is on the source drive ({d}). Writing to a failing drive is how data gets destroyed. Choose a location on a different drive."; also refuses when it can't tell which drive an output is on. Applied to `image --dest/--map`, `carve --out`, `extract --out` (`crates/sf-cli/src/recover.rs` `guard_outputs` lines 57–63).

**F-08 System-drive guard (GUI).**
`gui/src/store/app.ts` `systemDriveBlocked` lines 55–60 (confirmation keyed to the exact lower-cased source); `ImageScreen.tsx` `SystemDriveGuard` lines 142–150 ("I understand, image it anyway" pattern; Start disabled until ticked).

**F-09 Progress + phase events.**
`progress` throttled to 200 ms (~5/s) via `Throttle` (sf-cli lib.rs lines 202–218, 350–376); `phase` events for numberless steps — e.g. "Opening the NTFS volume and finding its file table ($MFT)" (recover.rs line 188). Killing the process loses ≤ 5 s of progress (README.md lines 198–199).

**F-10 Pause/resume UX.**
Pause kills the child; activity set to "Paused. The map was saved; press Resume to continue." (`gui/src/store/actions.ts` `pauseImage` lines 165–169; `events.ts` `onExit` image case lines 227–230).

### Devices / partitions

**F-11 Device listing without elevation.**
`run_devices` (recover.rs lines 24–55): path/model/serial/size/sector_size/bus/removable/system + `elevated` flag; human mode prints "[removable]"/"[SYSTEM]" tags. Contract lines 133–152.

**F-12 Startup survey (parallel per-drive quick scan).**
`run_survey` (sf-cli lib.rs lines 511–535) — one thread per source, slow drive delays only its own `survey_result`/`survey_error`; exit 0 unless bad args. Contract lines 283–298. GUI runs it for every attached drive when elevated, skipping otherwise to avoid a startup password prompt (`gui/src/store/actions.ts` `surveyDrives` lines 71–84).

**F-13 Partition table parsing.**
MBR incl. extended/logical EBR chains with a 256-link loop guard (`crates/sf-core/src/partitions.rs` `read_logical_partitions` lines 284–302); GPT with CRC32-checked header and **backup-header fallback** (lines 163–186, 304–333); "superfloppy" detection — a filesystem directly at sector 0 yields `table_kind none`, `source signature_scan`, `Confidence::Confident` (lines 226–241).

**F-14 Deep signature scan.**
`signature_scan` lines 365–407: every sector checked for NTFS/FAT12/16/32/exFAT/ext2/3/4 boot sectors (`detect_filesystem_at` lines 69–94, `DETECT_WINDOW = 1088` line 63); jumps past each found volume by its own size field so its contents and backup boot sector aren't mistaken for more partitions (`skip_until`, lines 397).

**F-15 Confidence classification.**
`Confidence::{Confident, Rebuilt, LowConfidence}` (partitions.rs lines 20–29; contract lines 112–120): valid table + matching boot sector, or sector-0 volume; signature-scan only; table entry with unrecognized boot sector, respectively.

### Filesystem recovery

**F-16 NTFS listing incl. deleted, straight from the MFT.**
`Index::build` sweeps MFT records in batches of 1024 (`BATCH`, ntfs/mod.rs line 26); fixup validation rejects torn records (`ntfs/record.rs` `apply_fixups` lines 19–45); $MFT bootstrap with mirror fallback (`ntfs/volume.rs` `Ntfs::open` lines 84–116); `$ATTRIBUTE_LIST` resolution validates that extension records point back at the base, so a reused extension record isn't misattributed after deletion (`resolve_attr_list` lines 234–273); namespace-ranked names Win32 > POSIX > DOS (`namespace_rank` record.rs lines 175–181). Deleted ⇔ MFT flags bit 0 clear (`parse_record` line 102: `in_use: flags & 1 != 0`).

**F-17 Path reconstruction + orphan detection.**
See the Named Feature Deep Dive (§ below) — `Index::parent` / `ancestry` / `path` (`crates/sf-core/src/ntfs/mod.rs` lines 126–174).

**F-18 Recoverability classification.**
`Recoverable::{Full, Partial, None}` — (unreadable + reused) bytes vs located bytes; compressed/encrypted files are `Partial`; FAT deleted files judged on one contiguous run; ext deleted with wiped pointers are `None` (`ntfs/mod.rs` `assess` lines 198–221; contract lines 218–225).

**F-19 Deleted-cluster-reuse detection via $Bitmap.**
`Ntfs::damage` (volume.rs lines 348–366) counts clusters whose bitmap bits are set; unreadable bitmap bits read as **allocated** so a deleted file is never called "full" on unreadable evidence (`cluster_bitmap` lines 321–344, `count_set_bits` lines 395–405).

**F-20 FAT12/16/32 + exFAT listing incl. deleted.**
Deleted entries by first byte `0xE5` (`fat/dir.rs` `parse_fat` line 102, `DELETED: u8 = 0xE5` line 8); VFAT long names with checksum validation, deletion-tolerant (lines 51–81); deleted directories walked as free clusters — max `DELETED_DIR_CLUSTERS = 64`, strict parse that requires the first cluster to still begin with `"."` and stops at the first implausible entry (`fat/mod.rs` lines 364–393; dir.rs lines 86–89); exFAT file entry sets with set-checksum validated with the deletion bit masked in (dir.rs `set_checksum` lines 132–138, `parse_exfat` lines 148–191); unreadable FAT sectors repaired from the backup FAT copy, else read as end-of-chain (`load_table` mod.rs lines 182–241).

**F-21 ext2/3/4 listing.**
Live files through extent trees (`walk_extents` — depth ≤ 5, ≤ 65536 nodes) or ext2 indirect block maps (`walk_indirect`); deleted names recovered from directory slack (`parse_dir` lines 379–415, `deleted_entry` lines 417–429); deleted names whose inode is in use again (rename leftovers) are skipped (mod.rs `index` lines 476–479); fscrypt → note "This file is encrypted by ext4 (fscrypt); it was copied still encrypted." (lines 541–543).

**F-22 Deleted-only listing.**
See the Named Feature Deep Dive — CLI `--deleted-only`, GUI "Only list deleted files…" + "Deleted only" filter.

**F-23 Extraction by id, recreating folder structure.**
See the Named Feature Deep Dive — `crates/sf-core/src/ntfs/extract.rs`.

**F-24 Imaging-map honesty plumbing.**
`unreadable_from_map` (`crates/sf-core/src/reader.rs` lines 83–102) treats map ranges that aren't `finished` as unreadable _without touching the device_; overlapping files show `partial`; extracts get notes. GUI auto-applies the map when the recovery source is the Image screen's destination (`gui/src/lib/recoverlogic.ts` `mapForSource` lines 11–13; hint in RecoverScreen.tsx lines 110–114).

### Carving

**F-25 Signature carver: 94 type ids / 51 structural kinds.**
`TYPES` registry in `crates/sf-core/src/carve/formats.rs` lines 83–178 — **94 entries counted** (README says "About 95"); `Kind::ALL` = 51 structural families (lines 181–233) mapped to ids via `type_ids()` (lines 236–291), e.g. `Kind::Zip → ["zip","docx","xlsx","pptx","vsdx","xps","odt","ods","odp","odg","epub","jar","apk","3mf"]`, `Kind::Mp4 → ["mp4","mov","m4v","3gp","m4a","heic","avif","cr3"]`, `Kind::Tiff → ["tif","cr2","nef","arw","dng","orf","rw2"]`.

**F-26 Validated headers, not bare magic.**
`identify` (formats.rs lines 345–541) validates structure:
JPEG requires `FF D8 FF` + SOI marker ≥ 0xC0; ZIP checks the version byte < 100;
MP4 checks ftyp box size 16..512, multiple of 4, ASCII brand; ELF validates class/data/version + `e_type`/`e_phentsize`;
Ogg checks header-type and granulepos; every signature also ends the previous unknown-length file (comment lines 342–344).

**F-27 Structural end detection.**
`measure` (lines 544–595) dispatches per-kind end-finders:
JPEG walks marker segments and, after the first SOS, searches only for FF D9 (progressive/EXIF-safe, `jpg_end` lines 597–626);
PNG walks chunk lengths to IEND; ZIP walks local headers to the EOCD (a nested zip's EOCD is not mistaken for the outer end —
test `nested_zip_eocd_is_not_mistaken_for_the_outer_end`, carve/tests.rs line 787).
Formats with no recoverable end (gz, bz2, flac, amr, xcf, kdbx, dwg — comment lines 590–592) fall to a bounded guess.

**F-28 Bounded guesses + incomplete marking.**
Unknown end = next recognizable header or `unknown_end_size()` (16 MiB–256 MiB by kind, lines 314–324; `unknown_end` carve/mod.rs lines 159–169 scans sector-by-sector). `complete: false` when the end isn't found, the size runs off the source, or the range overlaps unreadable/map-bad bytes (`carve_one` lines 115–155).

**F-29 No carving inside a claimed file.**
Scanning resumes at the sector-rounded end of a hit so embedded thumbnails/zips aren't carved as separate files (carve/mod.rs lines 89–104, comment "Never carve into a region a previous file claimed: its insides (thumbnails, embedded zips) aren't separate files"; test `does_not_carve_inside_a_file`, tests.rs line 841).

**F-30 Carve options.**
`--types` (validated with the full known list in the error string, `validate_types` lines 48–56), `--start/--end` byte range, `--map`; GUI maps a partition to a range (`gui/src/lib/recoverlogic.ts` `carveRange` lines 16–20). Device-mode carving gives up on 64 KiB granularity after a failed read instead of per-sector retries (`CARVE_DEVICE_GRANULARITY` recover.rs lines 19–22; `SafeReader::with_granularity` reader.rs lines 127–131).

**F-31 Carve output layout.**
`<out>/<typeid>/f{:07}.<typeid>` named by the sector of the header (carve/mod.rs line 138: `format!("{type_id}/f{:07}.{type_id}", abs / 512)`) — deterministic re-run naming, no original names/folders by design (contract lines 174–190).

### GUI surface

**F-32 Seven screens + console.**
`gui/src/store/app.ts` `Screen`: `home | image | partitions | recover | drives | mapview | verify` (`gui/src/App.tsx` `SCREENS` lines 15–23). Tool registry `gui/src/components/tools.ts` `TOOLS` lines 23–112 — each card carries group (Rescue/Inspect/Recover), "Use it when:", CTA. Console toggled with Ctrl+` (`components/Console.tsx` lines 12–22) shows every sf-cli command, JSON events, stderr, exits; Copy all / Clear / unseen-line counter (lines 53–115).

**F-33 File browser built for 1M+ rows.**
Column-oriented `FileStore` — no per-file object; shared folder-string pool (`gui/src/lib/filestore.ts` lines 68–127);
`@tanstack/react-virtual` list (`components/VirtualList.tsx`);
breadcrumbs "Volume root" + segments; "Orphaned files (N)" pseudo-folder link (FileBrowser.tsx lines 117–128);
filters — name search (debounced 200 ms, line 10), **Deleted only**, **Recoverable only**,
kind checkboxes Images/Documents/Audio/Video/Archives/Other (extension map filestore.ts lines 19–26);
flat vs folder view with "Back to folders" (FileBrowser.tsx lines 91–99);
sort heads name/type/modified/size with folders always first; select-all-shown;
selection summary "N selected (F folders, plus everything inside)" (lines 225–246).

**F-34 Per-row recovery badges.**
`RECOVERABLE` map (FileBrowser.tsx lines 12–16): "Fully recoverable" (green), "Partly recoverable" (amber), "Not recoverable" (red); plus amber "Deleted" badge (lines 217–220).

**F-35 Extract panel.**
Progress driven only by `extract_progress`; virtualized rows: path or "(not written)", Complete/Incomplete/Failed badges, plain-English note per row (`screens/recover/ExtractPanel.tsx` lines 7–43); finish summary counts extracted/failed/incomplete (events.ts lines 146–159).

**F-36 Health map.**
1000-cell / 50-col grid colored by worst status in slice, `role="img"` + summary label; live from `progress`, from polled `status` every 1 s otherwise (REQUIREMENTS.md lines 125–132; `actions.ts` `pollStatus`/`startImage` lines 99–163).

**F-37 Map viewer.**
Opens any mapfile without touching a drive; summary tiles Drive size/Rescued/Bad/Not yet read; contextual line ("Some areas are still damaged…" / "Part of the drive has not been read yet…" / "Every area was read."); actions "Resume imaging with this map" / "Retry bad areas" (`screens/MapViewScreen.tsx` lines 33–106).

**F-38 Verify screen.**
SHA-256 of a drive/image; session fingerprint list newest-first, Copy button, "Matches {name}: identical" green badge / "Differs from {name}, though the size is the same" red badge (`screens/VerifyScreen.tsx` lines 74–128).

**F-39 Elevation UX.**
Windows: unelevated start relaunches itself with the `runas` verb (UAC), waits, returns the elevated copy's exit code; state restored via `--elevated` argv; elevated copy exits with parent (PID watch) (`gui/src-tauri/src/lib.rs` `relaunch_elevated` lines 579–627, `restart_as_admin` lines 522–552, `exit_with_parent` lines 560–573). "Restart as Administrator" offered when `elevated === false` (REQUIREMENTS.md lines 93–99). Linux: pkexec wraps device-mode jobs when not elevated, with a root-readable private copy of the sidecar staged under `$XDG_RUNTIME_DIR/sectorforge` because root can't read the AppImage FUSE mount (`gui/src-tauri/src/cli.rs` `wants_pkexec`/`needs_private_copy`/`stage_for_root` lines 282–330); missing pkexec → instructions string (lines 347–349).

**F-40 Engine discovery + contract handshake.**
Lookup: `SF_CLI_PATH` > beside own exe > repo `target/{debug,release}` > PATH (`resolve` cli.rs lines 192–203, tested order line 711). `cli_info` runs `version --json`; mismatch produces a fix-it banner (`interpret_version` lines 260–279: "…which speaks contract {c}, but this app needs contract 2. Put the sf-cli that came in the same download next to SectorForge, or point the SF_CLI_PATH environment variable at the right file.").

**F-41 Large-selection plumbing.**

> 200 ids go through a temp `--ids-file` (Windows ~32k cmdline limit), auto-deleted (`gui/src-tauri/src/lib.rs` lines 242–279). CLI parses ids-file with per-line errors: "--ids-file line {n}: \"{l}\" is not a file id" (recover.rs `read_ids_file` lines 281–289).

**F-42 Dev bridge.**
Debug-only loopback HTTP server (POST /invoke, GET /events as SSE) so the real elevated app can drive the UI in a plain browser tab; Origin-checked, loopback-only (src-tauri/src/devbridge.rs lines 1–115; "Release builds don't contain this module" line 7).

## The Named Feature Deep Dive

**The named feature**: _"reconstructed files with their original directory structure preserved, containing only deleted files (not currently-existing files), as a post-scan option."_

**Verdict: the feature EXISTS, in three complementary layers** —
(1) scan-time deleted/live classification with a post-index listing filter (`--deleted-only`);
(2) a post-scan deleted-only _view_ in the file browser ("Deleted only" filter, flat list);
(3) extraction that reconstructs the original directory tree from deleted entries exactly as for live ones.
**Gap (stated plainly)**: there is no single "export deleted-only tree" button — the deleted-only _reconstruction_ is achieved by activating the Deleted-only filter, selecting (all-shown), and pressing "Extract selected". No code path does deleted-only extraction in one step.

### 1. When during the scan is deleted vs live identified?

Classification is **scan-time, during index building**, from on-disk evidence; the `--deleted-only` filter is applied **post-scan, at emission time, within the same command**:

- **NTFS**: MFT record flags bit 0 → `Record.in_use` (`crates/sf-core/src/ntfs/record.rs` `parse_record` line 102, `in_use: flags & 1 != 0`), copied into `IndexEntry { in_use, … }` while `Index::build` sweeps the MFT (`ntfs/mod.rs` lines 70–105). JSON emits the inverse: `"deleted": !e.in_use` (`crates/sf-cli/src/recover.rs` `run_files` line 250).
- **FAT**: directory entry first byte `0xE5` → `deleted` at parse time (`fat/dir.rs` `parse_fat` line 102, `DELETED: u8 = 0xE5` line 8); propagated as `in_use: !deleted` (fat/mod.rs `index` line 454); children of a deleted directory inherit deletion: `let deleted = r.deleted || parent_deleted;` (line 431).
- **exFAT**: entry-set in-use bit (bit 7 of the type byte) — `let deleted = in_use == 0;` (dir.rs `parse_exfat` line 173); checksum computed as if the bit were set (`set_checksum` lines 132–138).
- **ext**: live entries vs deleted names found in the slack after a live entry's name — `parse_dir` pushes `(ino, name, false)` live and `(ino, name, true)` slack (ext/mod.rs lines 392–409); `in_use: !deleted` (line 496).
- **The filter**: `run_files` builds the **entire** index first (`build_index`, recover.rs line 229 — same MFT walk either way), then walks `index.ids()` and skips live entries when the flag is set: `if deleted_only && e.in_use { continue; }` (recover.rs lines 236–243). It is an emission filter, not a distinct scan path.

### 2. Data-model representation

A **flag on the node**, not a separate list:
`IndexEntry { seq, in_use, is_dir, parent_ref, name, size, created, modified, recoverable }` (`crates/sf-core/src/ntfs/mod.rs` lines 35–46). `in_use == false` ⇔ deleted; both counts aggregate at `done`: `{"event":"done","files":N,"deleted":D}` (recover.rs lines 235–265). The GUI mirrors it as bit flag `F_DELETED = 2` in a packed `flags` column (`gui/src/lib/filestore.ts` line 6, `add` line 119–124) plus `deletedCount`.

### 3. How the deleted-only directory tree is constructed (original hierarchy preserved)

- **Parent references, not path strings, are primary**: every entry carries `parent_ref` (a full NTFS file reference = record number + sequence). FAT/exFAT/ext readers reuse the same field with the parent's index id (`fat/mod.rs` line 456 `parent_ref: dir_id`; ext line 498).
- **Sequence-aware validation** (`Index::parent`, ntfs/mod.rs lines 126–137): the parent must exist, be a directory, and its current sequence must equal the stored one — **except a deleted parent may be exactly one sequence ahead**, because deletion bumps the sequence: `let same = p.seq == pseq || (!p.in_use && p.seq == pseq.wrapping_add(1));`.
- **Recursive walk up the chain** (`Index::ancestry` lines 139–156) with two guards — chain length < 4096 and no repeated id (cycle protection) — producing `Ancestry::Rooted(Vec<u64>)` (ids from just below root down to the record) or `Ancestry::Orphaned(Vec<u64>)` when the chain breaks.
- **Path strings are derived** (`Index::path` lines 158–174): join rooted-chain names with `/`; `None` for orphans. This is how a deleted folder `/OldStuff` and its deleted child `/OldStuff/inner/memo.txt` keep full original paths — sample `docs/contract-samples/files_deleted_only.ndjson` lines 6–9; contract-smoke.ps1 line 239 ("file in deleted folder keeps its path").
- **GUI tree**: `FileStore.add(r)` splits the emitted `path` into `(dir, leaf)` (`splitPath` filestore.ts lines 45–50), interns the dir string into a shared `dirs[]` pool, and appends the entry index to `kids[dir]` — or `orphans[]` when `path == null` (lines 107–127). Folder view `list(dir)` is a cached folders-first, case-insensitive sort (lines 137–152). Deleted-only _view_ is `search(filter)` — a flat whole-volume pass skipping non-deleted rows: `if (f.deletedOnly && !(this.flags[i] & F_DELETED)) continue;` (lines 165–178).

### 4. How the option is presented post-scan (exact UI text and flow)

- **Post-scan filter (client-side, in the browser)**: after any listing, the FileBrowser "Filters" row shows a checkbox labeled exactly **"Deleted only"**, beside **"Recoverable only"** and a "Search by name" box (`gui/src/screens/recover/FileBrowser.tsx` lines 154–160; `setFlag("deletedOnly")` line 146). Any active filter switches the browser from folder view to a **flat whole-partition view**: crumbs show "Showing matches from the whole partition." with a **"Back to folders"** link (lines 91–99); each flat row shows an "in <folder>" link that clears filters and navigates to the file's folder (lines 201–212). Count line: "N matches (of M entries)" / "N items here (M entries in all, D deleted)" (lines 168–172).
- **Pre-listing option (server-side, a fresh sf-cli run)**: a checkbox labeled exactly **"Only list deleted files (quicker on very large volumes)"** above the "List files" button (`gui/src/screens/recover/FilesTab.tsx` lines 54–56; state `deletedOnlyRun`). `startListing()` forwards `deletedOnly: deletedOnlyRun` (`gui/src/store/actions.ts` line 246 → `start_files` appends `--deleted-only`, `gui/src-tauri/src/lib.rs` lines 215–225) and **starts the run in the flat deleted-only view**: `filter: deletedOnlyRun ? { ...NO_FILTER, deletedOnly: true } : NO_FILTER`, with the comment "A deleted-only run has no live folders to browse through, so start on the flat list of everything it found" (actions.ts lines 239–240).
- **The reconstruction step (extraction)**: tick rows or whole folders ("Selecting a folder extracts everything inside it.", FilesTab line 149), set "Extract to", press **"Extract selected"** (FilesTab lines 112–152). Extraction re-lists the source (the same `files` pass — contract lines 248–251: "Before any extract_progress, the CLI reads the whole MFT / directory tree") and recreates the original tree under the output dir (below).

### 5. Edge-case behavior

- **Parent folder no longer exists (record gone)**: `Index::parent` → `None` → `Ancestry::Orphaned` → `path: null` in the `file` event; the GUI groups those under the pseudo-folder **"Orphaned files (N)"** (FileBrowser.tsx lines 117–128; `ORPHANS = ":orphans:"` filestore.ts line 4 — "real folder keys always start with `/`, so this cannot collide"). On extract, orphans land under `--out/_orphaned/` (`extract.rs` `ORPHANS: &str = "_orphaned"` line 14; contract line 233).
- **Deleted file's parent also deleted**: the parent link still resolves — a deleted record's sequence is exactly stored+1 (`Index::parent`, above). Real-fixture proof: `/OldStuff/letter.txt` and `/OldStuff/inner/memo.txt` recorded deleted before deletion (make-ntfs-fixture.ps1 lines 92–93, 95–96) and listed with full paths (contract-smoke line 239; integration test asserts `out.join("OldStuff/inner/memo.txt").exists(), "deleted folder tree should be recreated"` — `crates/sf-core/tests/ntfs_fixture.rs` line 90).
- **Parent is currently-existing (record reused for something else)**: the sequence no longer matches (and the +1 rule fails) → chain breaks → orphaned. Hand-built test: record 23's parent (24) was reused for a new folder → id 23 gets `path == None` yet `recoverable == Full` (`ntfs/tests.rs` line 321).
- **Broken / cyclic parent chain**: guarded by `chain.len() < 4096 && !chain.contains(&p)` (`ancestry`, mod.rs line 149) — a loop can't hang path building.
- **Multiple deleted files sharing a parent**: no special case — each entry independently attaches to the same parent id / `kids[dir]`; extraction memoizes one output dir per folder id (`Writer.dirs: HashMap<u64, PathBuf>`, extract.rs lines 152–158, `dir_for` 216–238).
- **Name collisions** (e.g. a deleted and a live `a.txt` in the same folder): extraction never overwrites — `target.exists() || self.written.contains(&target)` → retry as `name (id N)` with the suffix **before** the extension (`with_id_suffix`, sanitize.rs lines 66–71: `report (id 42).docx`); if that also collides or hits a directory, the item fails: "an output file with the same name already exists" (extract.rs lines 178–184). Test: deleted `a (id 32).txt` beside live `a.txt` (`ntfs/tests.rs` lines 375–376); a re-run never clobbers the user's own file (test `rerunning_extract_never_overwrites_a_different_file`, lines 425–435).
- **Orphan roots**: get the id suffix so two orphaned "Documents" folders don't merge — `top_name` applies `with_id_suffix` when `chain.first() == Some(&id)` for `Ancestry::Orphaned` (extract.rs lines 199–207); output `_orphaned/orphan (id 23).txt` (test line 393). "An orphaned folder keeps its contents under it" (contract line 262).
- **Directory collision on disk** (a same-named _file_ where a folder must go): `dir_for` retries with the id-suffixed name (extract.rs lines 232–234).
- **Path traversal / hostile names**: every component passes `safe_component` — `\ / : * ? " < > |` and control chars → `_`; trailing dots/spaces → `_`; reserved Windows names (`CON`, `aux.txt`, `COM1`…) get a leading `_`; UTF-16 truncation to 240 units keeping the extension (`sanitize.rs` lines 11–62; tests lines 77–113). Extraction tests assert outputs `_CON` and `__` (`ntfs/tests.rs` lines 395–396).
- **Symlink/junction escape inside the output folder**: `Writer::make_dir` canonicalizes each created directory and refuses when it resolves outside `out` — "{} leads outside the output folder" (extract.rs lines 240–252).
- **NTFS metadata pollution**: records 0–15 (`$MFT` etc.) and anything inside `$Extend` are never listed — `FIRST_USER_RECORD = 16` (mod.rs line 25); `is_listed` rejects chains containing `EXTEND` (11) (lines 176–185). Extracting a folder pulls only listed children (`plan()` filter, extract.rs line 145; test `root_extracts_everything_but_metadata` lines 416–422 asserts no `$MFT` output).
- **Deleted FAT directories whose clusters ran on**: parsed strictly — first cluster must still begin with `"."`; parsing stops at the first implausible entry (fat/dir.rs lines 86–89, 106–110); at most 64 free clusters read (`DELETED_DIR_CLUSTERS` fat/mod.rs line 30).
- **Deleted exFAT directory whose first cluster was reallocated**: `dir_bytes` returns empty — "reused: whatever is there now isn't this directory" (fat/mod.rs lines 380–383).
- **Deleted FAT files are assumed contiguous**: `loc.contiguous |= deleted && self.kind != FatKind::ExFat` (fat/mod.rs line 435); documented limitation (README lines 187–188).
- **ext deleted names whose inode is reused**: skipped outright (rename leftovers) — ext/mod.rs lines 476–479; contract line 225.
- **Unknown id selected for extraction**: reported as a failed item — `Extracted { path: None, complete: false, note: "there is no file or folder with id {id} on this volume" }`, counted in `done.failed` (extract.rs line 162; contract lines 253–257; smoke lines 261–264).

### 6. On-disk output layout

- Under `--out DIR` (created if missing — recover.rs line 299): the **original hierarchy is recreated** from the (possibly deleted) parent chain; `dir_for` recursively creates parents (`fs::create_dir_all`), mapping each volume directory id to `out/<sanitized names>`; files written via `BufWriter` (extract.rs lines 160–197). Deleted folders are created exactly like live ones — deletion affects notes, never layout.
- Orphans: `out/_orphaned/<name> (id N)<ext>`, orphaned folders keep contents under them.
- Collision handling as above; nothing existing is ever overwritten.
- Carving output is separate and flat: `out/<typeid>/f0071043.jpg` (F-31).

### 7. Every other edge case the code/tests handle for this feature

- Torn MFT records rejected via fixups (record.rs `apply_fixups`); $MFT record 0 rescued from the mirror (test `torn_mft_record_zero_falls_back_to_the_mirror`, ntfs/tests.rs lines 345–353).
- Compressed/encrypted files: listed and flagged `partial`; extracted **empty** with notes "This file is NTFS-compressed, which SectorForge can't decompress yet, so it was left empty." / "This file is EFS-encrypted; it can't be decrypted without the original Windows account's key, so it was left empty." (extract.rs lines 55–65; smoke line 270).
- Reused deleted clusters flagged, not hidden: `REUSED_NOTE = "Some of this deleted file's space has since been reused by other files, so parts of it contain other data."` (extract.rs lines 85–86; README lines 194–195).
- Unreadable bytes zero-filled and reported: "N bytes could not be read (bad sectors, not yet imaged, or missing allocation info) and are filled with zeros." (`unreadable_note`, extract.rs lines 81–83; test `unreadable_data_is_zero_filled_and_flagged` lines 402–414).
- A deleted-only run's `done` counts only deleted entries — `{"deleted":5,"event":"done","files":5}` (files_deleted_only.ndjson line 10); smoke asserts "--deleted-only lists only deleted entries" and ≥ 4 fixture entries (contract-smoke.ps1 lines 242–244).

## Algorithms Worth Porting

1. **Two-pass skip-and-retry imager** (`crates/sf-core/src/imager.rs`). Contract: inputs `BlockDevice` + writable/seekable dest + `MapFile` + `ImagerConfig{initial_chunk, skip_size, max_retries, reverse}`; outputs map statuses + progress. Edge cases: forward run wraps to untried areas before `current_pos` (test line 412); reverse run resumes from `current_pos` as the lowest reached point; checkpoint flushes the dest **before** the map claims data ("Image data must reach the OS before the map claims it's rescued", lines 87–89); transient map-save failures tolerated mid-pass, fatal at pass end (lines 90–96).
2. **ddrescue-compatible MapFile** (`mapfile.rs`): sorted/merged block list; `mark` range carve-out; `find_next`/`find_prev` for forward/reverse sweeps; atomic `.tmp` save with Windows rename-retry. Interop with real ddrescue/ddrescueview.
3. **SafeReader fault-tolerant reads** (`reader.rs`): zero-fill + count; `RangeSet` merged half-open ranges with binary-search intersection (`partition_point`, lines 71–77); map holes skipped without device reads (wear saving); failed reads binary-split down to configurable granularity (sector = exact; 64 KiB for device-mode carving).
4. **NTFS MFT sweep + $ATTRIBUTE_LIST resolution** (`ntfs/volume.rs`, `record.rs`): batch reads (`for_each_record`, one sweep per 1024 records); fixup validation with explicit zero-sector rejection (record.rs lines 33–36); mirror fallback for $MFT; attribute-list entries validated by back-reference (`base_ref` + seq match, volume.rs lines 254–259).
5. **Ancestry/path reconstruction with sequence-aware parent validation and orphan classification**
   (`ntfs/mod.rs` `parent`/`ancestry`/`path`) — reusable for any id/parent-ref tree,
   incl. the "deleted parent is seq+1" rule and cycle guards. This is the heart of structure-preserving deleted recovery.
6. **Recoverability assessment** (`ntfs/mod.rs` `assess` + `volume.rs` `damage`; fat `damage`/`assess`; ext `assess`): (located, unreadable, reused) triple vs init_size; unreadable bitmap bits default to _allocated_ (pessimistic honesty).
7. **FAT multi-copy table repair** (`fat/mod.rs` `load_table` lines 182–241): per-sector fallback to the second FAT copy; unreadable → 0xFF (reads as end-of-chain/allocated).
8. **FAT deleted-directory free-cluster walk + exFAT NoFatChain/valid-length handling** (`DirSrc::Free`; `Loc{contiguous, valid}`).
9. **ext deleted-name slack scanning** (`parse_dir`/`deleted_entry`): validates inode range, rec_len alignment, name bytes (no NUL/slash), ftype byte 1..7; skips hashed-directory index entries masquerading as slack (lines 396–399).
10. **The carver** (`carve/mod.rs`, `probe.rs`, `formats.rs`):
    16 MiB scan chunks with 1 KiB header over-read; per-candidate `Probe` (1 MiB sliding window, bounded reads relative to file start)
    so parsers walk structures sector-agnostically; `identify` structural validation; `measure` end detection; claimed-region skip;
    bounded `unknown_end` next-header scan; `max_size()` false-positive caps per kind (formats.rs lines 293–310);
    `search_limit()` footer-search bounds (1 GiB for pdf/zip/ps/xz, lines 326–332).
11. **Deterministic carve naming**: files named by header sector (`f{:07}`), so re-carving the same range reproduces identical names.
12. **NDJSON child-process protocol + contract handshake + batched file events** (`sf-cli` `Out`/`Throttle`; tauri `cli.rs` typed `Event` enum `#[serde(tag = "event")]` with unknown-field tolerance and malformed-line logging "skipped malformed line: …" line 449; `lib.rs` batching 2000 rows / 80 ms).
13. **Job slots with run-ids** (`cli.rs` `Slot = Arc<Mutex<Option<(u64, Child)>>>`, `NEXT_RUN_ID`):
    a finished run's reader thread can never claim a newer run's process (lines 380–466) —
    the correct pattern for kill/resume UIs.
14. **Same-drive refusal via backing resolution** (`drives/mod.rs` `backing_of_source`/`backing_of_output`/`existing_ancestor`): maps a not-yet-existing output path to its nearest existing ancestor, then to physical disks; refuses when undecidable.

## UX Surface Map

- **Screens**: `home | image | partitions | recover | drives | mapview | verify` (app.ts `Screen`); nav + status bar (aria-live) + console + in-app pickers/dialogs (App.tsx lines 25–55).
- **Home**: drive strip with per-drive survey results and read-error flags; 7 tool cards with per-tool busy StatusChips; "Good to know" facts incl. "Ctrl+` opens the console" (HomeScreen.tsx lines 8–76).
- **Image**: Source/Destination/Map fields with hints; system-drive red notice + confirmation checkbox; retry-only toggle; presets; Start/Resume/Pause ("This map is part done; Resume carries on where it stopped."); tiles Rescued%/Bad/Pending/Current pass; health map; "What happens when you press Start" 3-step explainer (ImageScreen.tsx lines 27–125).
- **Partitions**: deep-scan checkbox ("reads the entire drive, slow, finds partitions missing from the table" — REQUIREMENTS.md lines 143–144); results table + disk-layout bar + confidence badges; "No partitions were found." empty state (REQUIREMENTS.md lines 145–157; PartitionsScreen.tsx enumerated).
- **Recover**: source defaults to the Image destination until edited; map-in-use hint; live-drive warning Notice;
  tabs **"Browse files" / "Carve by file type"** (RecoverScreen.tsx lines 16–19, 110–123).
  Files tab: partition picker (unusable fs → "No readable filesystem: use carving", FilesTab.tsx line 52), deleted-only checkbox,
  List files/Stop, progress "Reading the file table: X of Y", done/stopped/error states
  ("Stopped early. N entries were read, so the list is incomplete." — FilesTab lines 96–103).
  Carve tab: type checkboxes + Select all/none, "Search in" (partition or whole source), Save to, Start carving/Stop,
  results list Type/Saved as/Size/Position on source/State (CarveTab.tsx).
- **Verify**: fingerprint list with match/differ badges + Copy (VerifyScreen.tsx lines 123–127).
- **Console**: timestamps + job tags + `$`/out/err/rows/exit line kinds; Copy all; unseen counter (Console.tsx).
- **Dialogs/pickers**: in-app file/folder pickers (FilePicker component, 202 lines, enumerated) with free-space checks; confirmReadDrive dialog "Read the drive directly?" (api.ts lines 82–89).
- **States**: every list/panel has loading ("Reading the file table..."), empty ("This folder is empty." / "Nothing matches these filters." — FileBrowser.tsx line 42), stopped and error states with counts.

## Output Formats

- **Imaging**: raw sector-for-sector image file (dest preallocated via `set_len`, optionally sparse) + ddrescue-format text mapfile (F-02; mapfile.rs lines 191–210: `# Mapfile. Created by SectorForge`, `0x… current_status`, then `0x%08X  0x%08X  <status-char>` blocks).
- **`files`**: NDJSON events per contract lines 192–227; human mode prints id, `D` flag, `<dir>`/recoverable, size, path or `<orphaned>/name` (recover.rs lines 254–262).
- **`extract`**: real files under `--out` recreating original paths, `_orphaned/…`, ` (id N)` collisions, sanitized names; `extracted` events carry out-relative paths (contract lines 229–265).
- **`carve`**: `out/<type>/f%07.<type>` files; `file` events carry out-relative `name` (contract lines 174–190).
- **`hash`**: a single `done.sha256` hex string (contract lines 267–281).
- **`status`**: one line with sorted contiguous `blocks` covering `[0, size)` (contract lines 81–93).

## Error Model

- **Transport**: `{"event":"error","message": plain English}` then exit 1; clap usage errors exit 2 with text on stderr only (contract lines 27–28; sf-cli lib.rs `run()` lines 730–740).
- **Exact user-facing strings (copy-exact, sampled)**:
  - "access denied opening {path}: direct hardware access needs Administrator (Windows) or root (Linux)" (sf-cli lib.rs lines 228–231).
  - "the map says {} was already copied, but the destination {} {}. Resuming would leave those parts of the image empty. Use the destination file from the original run, or delete the map to start over." (lib.rs lines 417–425).
  - "refusing to start: {what} {} is on the source drive ({d}). Writing to a failing drive is how data gets destroyed. Choose a location on a different drive." (drives/mod.rs lines 94–98).
  - "files supports NTFS, FAT12/16/32, exFAT and ext2/3/4 (no recognizable filesystem at LBA {n})" (recover.rs lines 178–179).
  - "the boot sector at LBA {n} can't be read (bad sector, or not yet imaged)" (recover.rs line 174); "--partition-lba {n} is past the end of the source ({size} bytes)" (line 168).
  - "map {} is for a {}-byte source, but this source is {} bytes. Wrong map for this source?" (reader.rs lines 85–88).
  - "unknown file type \"{t}\" (known types: …)" (carve/mod.rs line 52); "--ids-file line {n}: \"{l}\" is not a file id" (recover.rs line 287).
  - Extract notes: `unreadable_note` / `REUSED_NOTE` / compressed / EFS / "Its block pointers were wiped when it was deleted (ext3/ext4 do this), so its contents can't be located; try carving instead." (ext/mod.rs line 520).
  - Hash: "can't read {source} at byte {pos}: {e}. A drive with unreadable sectors has no single hash; image it first and hash the image" (lib.rs line 559).
- **Tauri command rejections** are plain-English strings (REQUIREMENTS.md line 30), e.g. "A {job} is already running." (cli.rs line 399); GUI fallback "sf-cli stopped with exit code N." + stderr (events.ts lines 220–222).
- **Policy**: per-file `recoverable` classes full/partial/none; carve `complete:false`; never silently returning zeros (reader.rs header; README "Honest results" lines 140–145).

## Tests & Fixtures

- **Engine unit tests are in-module** beside each algorithm (imager, mapfile, reader, sanitize, partitions, ntfs, fat, ext, carve `tests.rs`), driven by `FakeBlockDevice` — an in-memory BlockDevice with `mark_bad` ranges and a read counter (`crates/sf-core/src/block_device.rs` lines 34–95).
- **Hand-built hostile NTFS volume** (`ntfs/tests.rs` `build()` lines 209–296): fragmented MFT, torn record, reused parent record, reused clusters, deleted folder with deleted child, orphan, DOS alias, $ATTRIBUTE_LIST extension, sparse run, compressed flag, `CON`/`..` names, deleted duplicate name — asserted in `lists_paths_deletion_and_recoverability` and `extracts_files_folders_orphans_and_hostile_names` (lines 306–435).
- **Real Windows NTFS fixture**: `tests/fixtures/ntfs-small.vhd.gz` + `ntfs-small.manifest.json` (per-file SHA-256 recorded _before_ deletion); generated by an elevated PowerShell script formatting a 16 MiB VHD with Windows' own NTFS driver, creating hard links (to force `$ATTRIBUTE_LIST`), fragmented/sparse/compressed files, then deleting a photo and the whole `OldStuff` tree (`tests/fixtures/make-ntfs-fixture.ps1`). The integration test (`tests/ntfs_fixture.rs`) checks every recovered file byte-for-byte against the manifest and asserts the deleted folder tree is recreated (line 90).
- **Carve tests** (`carve/tests.rs`, 973 lines; 40+ synthetic format builders): `recovers_every_format_exactly`, nested-zip EOCD, missing-end → incomplete, map holes → incomplete-but-written, read errors → incomplete, no carving inside claimed files, types/range filters, `docx` filter skipping plain zips, unknown types rejected, header-only formats bounded by next header, every type id belongs to exactly one registry entry, random data ≠ file, near-miss headers rejected (test names, lines 697–942).
- **Loop-device integration** (Linux, `#[ignore]`d): `tests/linux_loopdev.rs` + `scripts/linux-loopdev-test.sh` — 512/4096-byte logical sectors, O_DIRECT, unaligned SafeReader reads, byte-for-byte full-image comparison.
- **Contract smoke**: `scripts/contract-smoke.ps1` builds synthetic disks (real JPEG/PNG/GIF/ZIP/DOCX/PDF bytes from Windows encoders), runs every v2 command and its error paths, validates NDJSON shape field-by-field and exit codes; `-Samples` writes real CLI stdout into `docs/contract-samples/` (lines 1–10, 196–276). The GUI parser test then requires **every line of every real sample to parse** (`gui/src-tauri/src/cli.rs` test `every_line_of_real_cli_output_parses`, lines 556–586) — a double-sided contract check.
- **GUI tests**: Vitest + Testing Library over `gui/fixtures/*.ndjson` (17 files, incl. `malformed.ndjson` with garbage + an unknown event, and `v2_*` samples); screens/store/components/lib tests (~10 files, enumerated); `scripts/gen-v2-fixtures.mjs` regenerates them. No-`dangerouslySetInnerHTML` rule with a hostile-name render test (REQUIREMENTS.md lines 228–232).
- **CI**: `.github/workflows/ci.yml`, `release.yml` (existence only; not read).

## Licensing

- `LICENSE` (21 lines, read in full): **MIT License, "Copyright (c) 2026 terra seraph"** — standard MIT grant (use/copy/modify/merge/publish/distribute/sublicense/sell) + "AS IS" warranty disclaimer. Confirmed by README badge (line 5: `License: MIT`) and `Cargo.toml` line 9 (`license = "MIT"`).
- **No NOTICE / THIRD_PARTY / THIRD_PARTY_NOTICES files exist** in the tracked tree (verified against `git ls-files` — only `LICENSE` at root).
- Third-party code visible is ordinary crates only (clap, serde_json, anyhow, sha2, thiserror, windows-sys, libc, tauri, tauri-plugin-dialog — per the three `Cargo.toml` files read) — all permissive-compatible. `Cargo.lock` files (root, `gui/src-tauri`) recorded for existence only.
- **Implications for a GPL-3.0-or-later target re-implementing from understanding**: MIT is GPL-compatible. Two clean strategies: (a) _study-only_ — re-implement algorithms/UX from this document's understanding; no license obligation attaches to independently written code; (b) _port code_ — keep SectorForge's MIT notice on the ported portions (e.g. mapfile parser, sanitize rules), which GPL-3.0-or-later fully satisfies (MIT may be included in GPL-3.0+ works with notice preservation). No field-of-use or additional restrictions exist anywhere in the repo.

## Coverage

197 tracked files total.

- **Read fully (68)**: `LICENSE`; `README.md`; `docs/cli-contract.md`; `docs/contract-samples/{files_deleted_only,extract,error_files_not_ntfs}.ndjson`; root/Cargo.toml + `crates/sf-core/Cargo.toml` + `crates/sf-cli/Cargo.toml` + `gui/src-tauri/Cargo.toml` + `gui/src-tauri/tauri.conf.json` + `gui/package.json`; Rust: `sf-cli/src/{main,lib,recover}.rs`; `sf-core/src/{lib,sanitize,reader,mapfile,imager,block_device}.rs`; `sf-core/src/drives/mod.rs`; `sf-core/src/ntfs/{mod,extract,record,volume,tests}.rs`; `sf-core/src/fat/{mod,dir}.rs`; `sf-core/src/ext/mod.rs`; `sf-core/src/carve/{mod,probe}.rs`; `sf-core/tests/{ntfs_fixture,linux_loopdev}.rs`; `tests/fixtures/make-ntfs-fixture.ps1`; Tauri: `gui/src-tauri/src/{main,lib,cli,devbridge}.rs`; GUI: `App.tsx`, `main.tsx`, `screens/{HomeScreen,MapViewScreen,VerifyScreen}.tsx`, `screens/recover/{RecoverScreen,FilesTab,FileBrowser,ExtractPanel,CarveTab}.tsx`, `lib/{filestore,contract,ipc,api,recoverlogic,format,devices}.ts`, `store/{recover,actions,events,bridge,app,scan,image}.ts`, `components/{VirtualList,Console,tools}.ts(x)`; `gui/REQUIREMENTS.md`; `gui/fixtures/v2_files.ndjson`; `scripts/linux-loopdev-test.sh`.
- **Read partially (4)**: `crates/sf-core/src/carve/formats.rs` (~650 of 1866 lines — registry, identify, measure dispatch, jpg/png/gif ends read; remaining per-format `*_end` bodies summarized from dispatch + tests); `crates/sf-core/src/partitions.rs` (500 of 700 — in-module tests not read); `scripts/contract-smoke.ps1` (~210 of 285 — image/status/survey sections skimmed via assertions); `gui/src/screens/ImageScreen.tsx` (150 of 291 — SystemDriveGuard tail/AdvancedSettings/Tiles not read).
- **Grep-verified only (4)**: `carve/tests.rs`, `fat/tests.rs`, `ext/tests.rs` (test names/assertions around claimed-file/EOCD behavior); `.github/workflows/*` (existence).
- **Enumerated only, existence + purpose recorded (~120)**: 13 `docs/media/*` images/GIF + 17 `gui/src-tauri/icons/*` binaries (visual assets); `Cargo.lock` ×3 + `gui/package-lock.json` (dependency graphs); `tests/fixtures/ntfs-small.vhd.gz` (gzipped real NTFS VHD) + `ntfs-small.manifest.json` (SHA-256 manifest — contents inferred from generator script + consumer test); 24 remaining `gui/fixtures/*.ndjson` + 12 remaining `docs/contract-samples/*.ndjson` (CLI transcripts consumed by parser tests; shapes verified indirectly via `cli.rs` tests); ~30 remaining GUI files (screens DrivesScreen/PartitionsScreen; components DrivePicker/DriveStrip/DriveInspector/PartitionMap/PartitionPicker/FilePicker/AppDialog/ui/Shell/HealthMap/StatusChip/toolStatus; stores verify/console/dialog/picker/hooks; libs map/paths; tests ×6; styles.css; index.html; vite.config.ts; tsconfig.json; scripts ×3; test setup/reset) — enumerated by name/size, behavior documented only insofar as `REQUIREMENTS.md` (read in full) specifies it; plus `build.rs`, `tauri.linux.conf.json`, `capabilities/default.json`, `vite-env.d.ts`, `.gitattributes`, `.gitignore`.
- **Absences stated plainly**: no NOTICE/THIRD_PARTY files; no database/session store (GUI state is memory-only; only the mapfile and elevation argv persist); no SCSI/ATA pass-through on Linux (explicit error string, sf-cli lib.rs line 265); no NTFS decompression or EFS decryption (flagged, files left empty); no carve reassembly of fragmented files (README line 193); no GUI-side parallel listing (single child process); and the named feature has **no single "export deleted-only tree" button** — it is the combination of `--deleted-only`/"Deleted only" with ordinary selection + extraction (Deep Dive §4).
