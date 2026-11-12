# TARGET-STATE — DiskWizard (heicfast/d-space_fork) Current-State Map

| Field                 | Value                                                                                                                                                                                                              |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Repo                  | https://github.com/heicfast/d-space_fork (local: `/home/z/my-project/d-space`)                                                                                                                                     |
| Claimed HEAD          | `3907f7144ad8578079186b949ef5657b5e2c2d03` (pinned by the session, worklog/worklog.md T1 table)                                                                                                                    |
| **Actual clone HEAD** | `d183a3fd40a2ca235a7464e8a502fc82b3786ece` — one commit _ahead_ of the pin: `docs(worklog): T1 — feature-harvest session start…` (worklog-only; the pinned tree is the parent)                                     |
| Tracked files         | **313** (`git ls-files`), vs **311** claimed in the session prompt (the +2 are `worklog/worklog.md`, `worklog/session-feature-harvest-prompt.md` added by the T1 commit)                                           |
| License               | GPL-3.0-or-later (LICENSE; every source file carries an SPDX header — verified in all Rust files read)                                                                                                             |
| Product               | DiskWizard v0.5.0-beta (`package.json` version 0.5.0; `src-tauri/tauri.conf.json` version 0.5.0)                                                                                                                   |
| Stack                 | Tauri 2 (Rust engine `src-tauri/`, crate `diskwizard_lib`), React 19.3.1 + TypeScript + Vite, TanStack Query/Virtual, Zustand, Radix, i18next (EN/zh-CN), SQLite (rusqlite), BLAKE3, Windows 10 1809+/11 x64 first |
| Date / Agent          | 2026-11-12 · Agent A (feature-harvest session, Phase 0, research only — nothing modified)                                                                                                                          |

---

## Architecture

### Layering (dependency direction)

```
React 19 UI (src/features, src/components, src/app)
   │  reads only
src/queries (TanStack Query hooks + useSyncExternalStore event stores)
   │  imports only
src/connector/contract.ts  ← implements ← src/connector/tauri/tauri-connector.ts
   │  invoke() / listen()  (IPC: 52 dw_* commands, 2 event streams)
src-tauri/src/recovery/commands.rs (thin Tauri command layer)
   │
src-tauri/src/recovery/  ── scan orchestration (scan.rs), deleted engine (collect.rs,
   deleted.rs), results queries (results.rs), jobs (jobs.rs), sessions (sessions.rs),
   partitions (partitions.rs), health (health.rs), repair (repair.rs), preview (preview.rs)
   │ depends on
src-tauri/src/scan/ (walk, tree, find_first, mft probe, privilege, cloud)
src-tauri/src/fsfat/ + src-tauri/src/fsext/ (SectorForge-adapted engines)
src-tauri/src/volume/ (enumerate, preflight, hardware)
src-tauri/src/{staging, index, duplicate, snapshot, locked_file, safe_delete, v2}
src-tauri/src/db/ (SQLite + 4 migrations)
```

Frontend rules enforced in CI: ast-grep `no-raw-invoke.yml` (no `invoke()` outside `src/connector/**`) and `no-connector-import-outside-queries.yml`; `scripts/arch-lint.mjs` pins the layering. The tree never leaves the engine: the UI queries windowed slices (server-side filters/sorting/pagination, README.md:65).

### Process & lifecycle

- Single Tauri desktop process; `src-tauri/src/main.rs:6` → `diskwizard_lib::run()`. Windows release builds hide the console (`windows_subsystem = "windows"`).
- **UAC/admin**: `src-tauri/manifest.xml` requests `requireAdministrator` (raw `\\.\X:` access, MFT engine, deleted recovery, VSS, repair). Elevation is _probed_ via `scan::privilege::is_elevated()` (`src-tauri/src/scan/privilege.rs:11`, token `TokenElevation` query) — raw-engine scans and USN watchers are gated on it; pre-flight (`volume/preflight.rs`) never needs admin.
- `run()` (`src-tauri/src/lib.rs:347-483`): `init_tracing` (EnvFilter `DISKWIZARD_LOG`/`DSPACE_LOG`, default `info,diskwizard_lib=debug`) → `install_panic_hook` (logs every panic to tracing; `lib.rs:323-344`) → SQLite open with in-memory fallback + migrations → **startup WAL recovery** (`staging::recover_wal`) + snapshot retention purge → Tauri builder with managed state: `DbState` (`Arc<Mutex<Connection>>`), `Arc<ScanRegistry>`, `Arc<JobRegistry>`, `Arc<RepairUndoLog>`, `UsnWatchers` → plugins: dialog, process, updater, single-instance (focus existing window) → setup: `build_tray`, `spawn_tray_monitor` (30-min default interval, battery-doubled, ≥90% full-disk alerts on `tray-monitor-alert`), `spawn_usn_watchers` (per indexed volume) → 52 `dw_*` commands.

### Threading / concurrency model

- **Scans and recovery jobs run on dedicated detached OS threads** (`std::thread::Builder::spawn`, named `dw-scan-<id>` / `dw-job-<id>`; `recovery/scan.rs:165`, `recovery/jobs.rs:408`). The async commands return the id immediately (the CHANGELOG v0.5 notes the old design joined the thread and froze the invoke).
- Panics on those threads are caught with `std::panic::catch_unwind(AssertUnwindSafe(...))` and mapped to `Failed` scan/job with the panic message (`scan.rs:170-180`, `jobs.rs:411-421`); a global panic hook logs first (`lib.rs:323`).
- **Pause/cancel**: per-run `RunFlags { cancel: AtomicBool, pause: AtomicBool }` (`recovery/collect.rs:92-116`); pause sleeps in 50 ms slices, checks every 5000 MFT records (NTFS/ext/FAT) or every 32 dirs (fallback walk, `scan.rs:798`).
- IO commands use `tokio::task::spawn_blocking`; DB access is a global `Arc<Mutex<Connection>>`; the scan thread opens its _own_ connection for user rules (WAL allows concurrent readers, `scan.rs:407-414`).
- **No rayon in the recovery engine** (rayon appears in the duplicates engine per `duplicate/mod.rs` comments and Cargo deps; the MFT walker is single-threaded — `scan/walk.rs` header states v0.1 single-threaded, rayon planned).
- Registries bounded at 8 entries each, evicted by minted **sequence number** (never lexicographic), never evicting a running scan/job (`recovery/mod.rs:443-492`, `645-669`).

### Connector contract (the UI⇄engine boundary)

- Single source of truth: `src/connector/contract.ts` (889 lines, 10 domains + mock `ScenarioControl`); human spec `docs/CONNECTOR-CONTRACT.md` incl. a 2026-11-12 addendum mapping every domain to real commands.
- **`RecoveryConnector` domains**: `devices`, `scan`, `results`, `preview`, `sessions`, `recovery`, `health`, `repair`, `system`, `cleanup` (contract.ts:856-868).
- **Command surface: 52 `dw_*` commands** registered in `lib.rs:418-480` and implemented in `src-tauri/src/recovery/commands.rs` (verified by counting `#[tauri::command]` = 52 and `fn dw_` = 52):
  - Devices: `dw_devices_list`, `dw_devices_refresh`
  - Scan: `dw_scan_start` (args `targetId`, `pathScope`; one-active-scan-per-target returns the running id), `dw_scan_state`, `dw_scan_pause`, `dw_scan_resume`, `dw_scan_cancel`
  - Results: `dw_results_tree` (`parentId`, `mode` path|type|raw), `dw_results_query` (filter/sort/pagination, limit clamp 1..1000), `dw_results_count`, `dw_node_detail`
  - Preview: `dw_preview`, `dw_thumbnail`
  - Sessions: `dw_session_save`, `dw_sessions_list`, `dw_session_load` (`fileName`)
  - Recovery: `dw_recover_start` (`selection` enum `{Nodes|All}` + `options`), `dw_recover_cancel`, `dw_recover_state`, `dw_recover_items`, `dw_recover_report`
  - Health/repair/partitions: `dw_disk_health`, `dw_repair_dry_run`, `dw_repair_apply`, `dw_repair_undo_log`, `dw_repair_undo`, `dw_disk_layouts`
  - System: `dw_app_info`, `dw_mount_points`, `dw_validate_destination`, `dw_special_folders`, `dw_pick_directory`, `dw_open_path`
  - Cleanup (D-Space features): `dw_stage_path`, `dw_list_staging`, `dw_undo_staging`, `dw_undo_resolve`, `dw_permanent_delete`, `dw_list_expired`, `dw_cleanup_expired`, `dw_rules_list`, `dw_rule_add`, `dw_rule_delete`, `dw_rule_toggle`, `dw_duplicates_find`, `dw_snapshot_capture`, `dw_snapshots_list`, `dw_snapshot_delta`, `dw_snapshots_cleanup`, `dw_index_search`, `dw_get_setting`, `dw_set_setting`
  - Note: `index_build` is **not** a command anymore (internal only; the USN watcher/sync is driven at startup + after scans — `lib.rs:223-303`).
- **Event streams**: `dw-scan-progress` (payload `ScanProgressEventWire`, `recovery/mod.rs:552-618`), `dw-recovery-progress` (`jobs.rs:454-460`); plus UI-only `tray-monitor-alert` and `tray-scan-request` (`lib.rs:40,126`). Subscriptions return unsubscribe functions; frontend consumes via `useSyncExternalStore` stores in `src/queries/scan.ts` / `src/queries/recovery.ts`.
- **Adapter** `src/connector/tauri/tauri-connector.ts` (1363 lines): camelCase pass-through for new wire structs, field-by-field mapping for legacy snake_case payloads (staging/rules/duplicates/snapshots/index), unix→ISO8601 conversion, image dimension parsing (PNG/GIF/BMP/JPEG SOF walk, lines 336-382), error mapping via a **regex table `CODE_BY_MESSAGE`** (lines 85-108) over engine English messages → `ConnectorErrorCode` (note: this _is_ string-matching, in the adapter, contradicting the contract's "never string-match" principle — pragmatic because the engine's `Error` carries only `kind`+`message`).
- Factory `src/connector/index.ts`: `VITE_CONNECTOR=mock|tauri`; mock is test/dev opt-in only; release workflow forces `VITE_CONNECTOR=tauri` (`.github/workflows/release.yml:66`). ast-grep rules (`rules/no-raw-invoke.yml`, `no-connector-import-outside-queries.yml`) enforce the boundary in CI.

### Error model

- Rust: single `Error` enum, 10 kinds (`src-tauri/src/error.rs:10-41`): `Io, Db, Volume, Scan, Staging, Snapshot, Duplicate, LockedFile, Index, NotImplemented(&'static str)`; serialized as `{ kind, message }` (lines 45-56).
- Frontend: `ConnectorErrorCode` union of 23 codes (`contract.ts:56-80`) + `reasonKey` i18n suffixes; `toConnectorError` in the adapter maps engine messages/kinds.
- Per-item recovery failure codes (machine strings, `recovery/jobs.rs`): `record-reused`, `clusters-overwritten`, `unsupported-encoding`, `source-missing`, `source-unreadable`, `destination-invalid`, `destination-unwritable`, `collision-skip`, `verify-failed`.
- Preview codes: `preview-unsupported`, `preview-too-large`, `preview-damaged`, `preview-missing-data` (`recovery/preview.rs:72-75`).

### Logging

`tracing`/`tracing-subscriber` fmt with target+level; panic hook logs message+location+thread before the default handler (`lib.rs:323-344`). Frontend logger in `src/lib/logger.ts` (test-verified `tests/logger.test.ts`).

---

## Feature Inventory

Legend: files are relative to repo root. "Wire" = serialized to the UI.

**F-01 — NTFS raw MFT engine (existing + deleted, one pass).** `src-tauri/src/recovery/collect.rs`: `collect_mft_all` / `collect_mft_all_reader` walk records 16..cap (cap = estimated ∧ 50M, `MAX_RECORD_HARD_LIMIT`, line 27); `IN_USE` flag splits `existing: Vec<RawMftEntry>` vs `deleted: Vec<DeletedEntry>`; `DeletedEntry` (lines 44-70) keeps `parent_record_no, name, data_size, is_dir, modified_unix, resident, runs: Vec<DwRun{lcn,clusters}>, compressed, encrypted, slot_key`. `extract_data_runs` caps at 4096 runs/record; attribute-list-spanning → no runs (graded Poor). `$Bitmap` read into `ClusterBitmap` (lines 178-254); `is_allocated` treats out-of-range as allocated (conservative); `free_fraction` counts sparse runs as free (test-pinned incl. out-of-range conservatism, lines 448-516).

**F-02 — FAT12/16/32 + exFAT deleted-file engine.** `src-tauri/src/fsfat/mod.rs` (907 lines; **adapted from SectorForge, MIT © 2026 terra seraph — notice retained in header lines 5-9**): `collect_fat` BFS-walks the root chain; deleted entries detected by `0xE5` / cleared in-use bit with LFN reconstruction (`fsfat/dir.rs`, also SectorForge-adapted); exFAT entry-set checksums + `NoFatChain`; FAT classification by cluster count (TestDisk rule: <4085 FAT12, <65525 FAT16, else FAT32, `mod.rs` parse_fat_boot + `recovery/partitions.rs:336-339`); deleted FAT runs are **contiguous-assumed from the first cluster** (classic undelete model, honestly graded); deleted directories harvested from ≤64 free clusters after the first (`DELETED_DIR_CLUSTERS`); exFAT allocation bitmap is allocation authority (`FatAllocMap`, lines 136-158); `FatGeometry` persisted for read-back; `reparse_slot` re-checks the directory slot byte at its absolute offset (`slot_key`). Synthetic ids: root 5, first record 16.

**F-03 — ext2/3/4 engine.** `src-tauri/src/fsext/mod.rs` (1057 lines; **SectorForge MIT notice retained, lines 5-9**): `collect_ext` walks from root inode 2; **deleted names harvested from directory slack** (deleted directory entries whose inode link count is 0); rename leftovers (links≠0) skipped; ext4 extents + ext2 indirect blocks + inline data; per-group block bitmaps grade reuse (`ExtAllocMap`); wiped-pointer ext3/4 deletions surface as Poor with "try carving" honesty (header lines 11-20); `slot_key` = inode number, re-checked at read-back via `reparse_inode` link count.

**F-04 — Directory-walk fallback (ReFS/network/unknown-FS).** `src-tauri/src/scan/find_first.rs` `scan_find_first_with_progress`: FindFirstFile recursion, MAX_DEPTH 50, symlinks not followed, BFS, hardlink dedup ≥64 KiB via NTFS file index, compressed/sparse physical size via `GetCompressedFileSizeW`; **existing files only — no deleted discovery on fallback**. UNC roots accepted (`ScanTarget::Unc`); routing in `recovery/scan.rs:290-368` (NTFS→MFT, FAT/exFAT→FAT, ext→ext, else fallback; all raw engines require elevation).

**F-05 — Confidence grading (allocation-based, honest).** `src-tauri/src/recovery/deleted.rs::grade_deleted` (lines 86-115): compressed/encrypted → Poor; 0-byte files → Poor; dirs → Unknown; resident → Good; thresholds ≥95% clusters free → Excellent, ≥60% Good, >0% Fair, 0% Poor. Wire enum `Confidence` snake_case. Read-back re-parse (`reparse_record_on`) never trusts scan-time state; `read_deleted_range` stops at the first reallocated cluster and returns intact bytes (lines 210-307).

**F-06 — Scan orchestration & stage machine.** `src-tauri/src/recovery/scan.rs::run_scan` (thread, catch_unwind, auto-session-save on Completed/Cancelled, USN watcher warm-up). Stages: `volume-analysis` (4%) → `quick-index` (raw engine, 71%) or `deep-walk` (fallback) → `tree-build` (25%) with sub-progress 0.3/0.6/0.8/1.0; weights sum pinned by test. `resolve_target` accepts `C`/`C:`/`vol:C`/`dev:C`, UNC, `lp:<disk>:<offset>`; `apply_path_scope` normalizes absolute/relative scopes, wrong-volume prefix rejected; `resolve_scope_node` (lines 846-881) resolves scope to a tree node, unknown → hard error (contract `scan-scope-invalid`); scoped scans present the scanned folder as tree root.

**F-07 — Server-side results engine.** `src-tauri/src/recovery/results.rs::query_results`: candidate materialization honoring tree scope (`subtree_files` DFS over existing+deleted, visited-set, `MAX_CANDIDATES` 2M); filters: search (name contains), categories (8-category map `category_of` lines 69-90: graphics/video/audio/document/email/archive/bookmarks/other with full extension tables), date range (unix), size range, extensions, **origins** (`existing|deleted|lost-path`), `hideEmptyFiles`; 5 sort keys (`name|modified|type|size|confidence`, confidence ranked Excellent=4…Unknown=0, lines 496-504); pagination with `total` + `totalBytes` + `elapsedMs`. Tree pages capped at 500 (`TREE_PAGE_LIMIT`). `node_detail` (lines 1004-1042) exposes `confidenceReason` (per-grade English strings, e.g. Excellent = "Data clusters are still free — very likely intact"), `clusterCount` (size div_ceil cluster_size), `badSectorsOverlap: false` (no bad-sector map exists). Zero-byte files are always graded `poor` (grade_deleted) matching the contract note `size < 1 → Poor` (CONNECTOR-CONTRACT.md:88-89).

**F-08 — Path tree with Lost Path branch + type tree.** `results.rs::result_tree`/`path_tree`/`type_tree`: path-tree children = existing children + deleted children merged inline (`ScanView::children_of`, lines 266-283); root level adds a synthetic **"Lost Path"** node (`LOST_PATH_ROOT = u64::MAX-1`, `recovery/mod.rs:37`) listing deleted ids whose parent chain is dead (lost list built in `finalize_scan`, `scan.rs:452-463` for all three raw engines); type tree = 8 categories → extension drill-down (`c:<cat>`, `ext:<cat>:<ext>` ids). **`mode == "raw"` is aliased to `"type"`** (`results.rs:739`) — the raw/carve branch is contract-reserved, not implemented (mock has a real `raw-all` branch: `src/connector/mock/node-space.ts:79`).

**F-09 — Deleted-directory aggregates.** `scan.rs::deleted_dir_size` (lines 737-762): recursive sum with an in-progress sentinel (`u64::MAX`) for cycle safety (corrupt-MFT cyclic chains — the v0.5 crash fix); results tree shows deleted dirs with recursive sizes via `deleted_dir_sizes`.

**F-10 — Previews (bounded, honest).** `src-tauri/src/recovery/preview.rs`: kind from extension (`preview_kind_for`, lines 41-50); image ≤8 MiB base64 + mime; text ≤64 KiB with UTF-16LE/UTF-8 detection + truncated flag; hex ≤64 KiB; thumbnails ≤2 MiB graphics only; **PDF/AV/Office return `preview-unsupported`** (header lines 10-12); deleted files read through fresh data runs with allocation re-check.

**F-11 — Recovery jobs (verified streaming copy).** `src-tauri/src/recovery/jobs.rs`: `build_job_items` (selection `Nodes` explicit ids or `All` filter → `query_results` with limit `MAX_JOB_ITEMS` 500k) → destination overlap pre-check (source volume letter / PhysicalDrive string, lines 216-231) → recursive directory expansion (`collect_item`) → keep-empty-folders marker pass → `run_job` on a detached thread with catch_unwind. `write_item`: `create_dir_all` parents → write to `<dest>.dw_tmp` → streaming source (4 MiB `COPY_CHUNK`, incremental blake3) → `sync_all` → atomic `rename` (fallback copy+delete on Windows) → optional verify re-read (full blake3 compare; mismatch deletes the file). Final status: Completed / CompletedPartial / Cancelled / Failed. Report lists capped at 500 per bucket with exact `processedTotal` and `truncatedItemLists`. `SourceCtx::prepare` (lines 628-706) parses the NTFS boot + `$Bitmap` (or FAT geometry + `FatAllocMap`, or ext geometry + `ExtAllocMap`) **once per job** — the v0.5 fix for per-file re-parsing. Read-back dispatch is engine-aware (`stream_deleted`, lines 924-1107): NTFS re-parses the MFT record by number (`reparse_record_on`), FAT re-checks the directory slot byte at the recorded absolute offset (`fsfat::reparse_slot`), ext re-parses the inode and re-checks its link count (`fsext::reparse_inode`); a reused record fails `record-reused` before any bytes are written.

**F-12 — Collision policy.** `jobs.rs::recover_one`/`unique_path` (lines 729-767): `skip` → `collision-skip`; `rename` → `name (i).ext` for i in 1..10000; `overwrite` → proceeds.

**F-13 — file_repair (intact-prefix publishing).** `jobs.rs::stream_deleted` (lines 917-1107): reads stop at the first reallocated cluster; without `file_repair` the item fails `clusters-overwritten`; with it the intact prefix is published (item keeps the original size so the UI can flag partial). Read-back is engine-aware per `BackendGeometry` (NTFS record re-parse / FAT slot byte / ext inode link count; `SourceCtx::prepare` prepares NTFS boot+$Bitmap or FAT/ext geometry ONCE per job).

**F-14 — Sessions (DWSESS02).** `src-tauri/src/recovery/sessions.rs`: compact little-endian length-prefixed binary under `%LOCALAPPDATA%\DiskWizard\sessions\<id>.dws` + SQLite row (migration 0004). Encodes: magic `DWSESS02`, meta (target enum 0/1/2, path_scope, cluster_size), **backend geometry tag (0 NTFS / 1 FAT / 2 ext — engine-version gating)**, all tree nodes (id/parent/name/sizes/is_dir/mtime/score+rule+reason), all deleted entries (runs, slot_key, confidence byte), dir sizes, lost list, runtime counters, completed flag. Children maps are derived, not stored. Deterministic encoding is test-pinned (`encoding_is_deterministic`, sessions.rs:920-925). Load: only known sessions (no arbitrary file read), fresh scan id, `strategy = "session"`, cluster_size + backend restored (the v0.5 fix for empty deleted previews). Older `DWSESS*` magic → error "older DiskWizard version; please re-scan" (lines 333-340). Defensive decode caps: node/deleted counts ≤60M, runs ≤4096, ext groups ≤4096, FAT copies ≤4, strings ≤64 MiB. Retention `KEEP_SESSIONS = 10`; auto-save on Completed **and** Cancelled (partial-but-honest data); `dw_session_load` requires the file to be in the SQLite list.

**F-15 — Partitions & lost-partition rescue.** `src-tauri/src/recovery/partitions.rs`: pure parsers `parse_gpt_header` (LBA 1 or backup, CRC-32 IEEE validated — classic vector test `0xCBF43926`), `parse_gpt_entries` (array CRC), `parse_gpt_entry` (UTF-16LE names), `parse_mbr` (4 entries, 0x55AA), `detect_fs_boot` (NTFS/MSDOS-FAT classified by cluster count/exFAT) and `detect_fs_at` (boot sector, then ext superblock magic `0xEF53` at +1024). `read_disk_layout`: GPT primary (LBA 1) → **backup header fallback (last sector, CRC-strict)** → MBR; lone 0xEE protective → honest "gpt" style without pretending; unallocated gaps ≥1 MiB probed over the **first 128 sectors** (`probe_gap`) for lost filesystems; `LostPartitionWire.id = lp:<disk>:<startByte>`; `resolve_lost_target` re-probes the boot sector. `OffsetReader<R>` (lines 47-67) wraps `\\.\PhysicalDriveN` at a base byte offset so the whole NTFS engine works over a lost volume. **No partition-table write-back/reconstruction exists** (see Gaps).

**F-16 — SMART / disk health.** `src-tauri/src/recovery/health.rs` (1058 lines): three graded paths — NVMe health log page 0x02 via `STORAGE_PROTOCOL_SPECIFIC_DATA` (synthesized attribute table with spare/percentage-used/media errors/unsafe shutdowns), ATA `SMART READ DATA` + `READ ATTRIBUTE THRESHOLDS` via corrected `ATA_PASS_THROUGH_EX` (compile-time layout assert), `IOCTL_STORAGE_PREDICT_FAILURE` fallback for USB bridges; temperature via `StorageTemperatureProperty`, TRIM via `StorageDeviceTrimProperty`; below-threshold = bad verdict; honest `health-unavailable` otherwise. Pure parsers fixture-tested host-independently (16 tests).

**F-17 — Volume repair (transactional, consent-gated).** `src-tauri/src/recovery/repair.rs`: dry-run findings = NTFS boot vs backup boot (both candidate offsets probed) and $MFT vs $MFTMirr (first 4 records); apply requires typed consent token `"REPAIR"` (`REPAIR_CONSENT_TOKEN`, line 22), non-system volumes only, writes guarded by an in-memory `RepairUndoLog` (original bytes stored, `undoRepairs` rolls back). Wire kinds currently emitted: `boot-sector-mismatch`, `mft-record-error` (the contract enum lists 6 kinds — 4 are unused by the engine).

**F-18 — Staging / undo / cleanup guarantee (Pillar 5).** `src-tauri/src/staging/`: no direct deletion — atomic same-volume rename into a staging folder (`ops.rs`), cross-volume two-phase copy with `.dspace_tmp` + WAL + blake3 verify (`cross_volume.rs`, `wal.rs`; startup `recover_wal`); 24 h default undo window (`STAGING_TTL_SECS`); conflict resolution `overwrite|rename|keep_both|cancel` with full-blake3 idempotency check ≤64 MB (`UndoOutcome`); lazy expiry (no background deletion), rate-limited cleanup with `AUTO_THRESHOLD` abort (`expiry.rs`); permanent delete behind typed-phrase double-confirm + forensic ledger (`permanent.rs`) + **DoD 5220.22-M 3-pass wipe** (0x00 / 0xFF / xorshift64 random, 64 KiB chunks, sync per pass — `wipe.rs`).

**F-19 — Safe-delete scoring (Pillar 3).** `src-tauri/src/safe_delete/`: 63 built-in rules (`rules.rs`, dev/system/cloud/VM caches; tiers 0-30 red / 31-60 yellow / 61-85 green / 86-100 blue), user rules with priority (`user_rules.rs`, migration 0002, runtime merge in `scan/tree.rs::build_tree_with_user_rules` lines 328-346), `RulesScorer` + `ml_scores` cache; **`TractMlScorer` is `unimplemented!()`** behind the `ml-tflite` feature (`scorer.rs:203-209`).

**F-20 — Duplicates (Pillar: BLAKE3).** `src-tauri/src/duplicate/scan.rs::find_duplicates` over the scan tree: size-bucket → head-hash prefilter → full blake3; groups carry `modified_unix` parallel to `paths` (engine-graded "keep newest"); `max_groups` cap 500 with total `group_count`; rayon parallel (v0.2) — 10 tests (vss-gated file).

**F-21 — Size snapshots & deltas (Pillar 4).** `src-tauri/src/snapshot/`: capture from a scan tree (SQLite `snapshots`/`snapshot_entries`), 4-bucket deltas added/removed/grew/shrunk (`delta.rs::compute_delta`), 90-day default retention with startup purge (`DEFAULT_RETAIN_DAYS`).

**F-22 — USN index & Quick Search.** `src-tauri/src/index/`: baseline `FSCTL_ENUM_USN_DATA` + `USN_RECORD_V2` parser (`baseline.rs`, `usn.rs`), watermark-based incremental delta (`delta.rs`, `persist.rs`), wraparound detection → full rebuild, 60 s poll watcher per volume (`watcher.rs::spawn_watcher`, wired at startup + after scans via `lib.rs::ensure_usn_watcher`); `index_search` top-50 substring + `WITH RECURSIVE` full path. UI: Quick Search page (`src/features/quick-search/quick-search-page.tsx`, route `/quick-search`).

**F-23 — Locked files & VSS.** `src-tauri/src/locked_file/`: `probe_lock_state` (CreateFileW share-violation), `find_lock_owners` (Restart Manager PID+app); VSS pool (`vss.rs`, `vss_pool.rs`) behind the **non-default `vss` Cargo feature** (Windows only) — lease renewal + dead-context eviction + bounded 60 s waits.

**F-24 — Devices/volumes surface.** `recovery/mod.rs::list_devices_wire` (lines 111-222): one device per mounted volume, `GetLogicalDrives` enumeration (`volume/enumerate.rs`), pre-flight statuses incl. **BitLockerLocked/Suspended/Encrypted** mapped to `locked`/`unlocked` wire states; kind inference (USB-bus removable → `usb`, else `sd`); `quick_scan_available`/`can_recover_deleted` = raw-engine FS (NTFS/FAT12/16/32/exFAT) ∧ elevated. Hardware profile via `IOCTL_STORAGE_QUERY_PROPERTY` (`volume/hardware.rs`: bus, media via seek-penalty, vendor/product/serial, estimated read speed).

**F-25 — Cloud placeholder detection.** `src-tauri/src/scan/cloud.rs`: `GetFileAttributesW` flags `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`/`RECALL_ON_OPEN`/`REPARSE_POINT` → placeholder states; on-demand probe only.

**F-26 — Tray & full-disk monitor.** `lib.rs::build_tray` (open / scan C: / quit) + `spawn_tray_monitor` (settings-driven, battery-aware throttling, `tray-monitor-alert` events).

**F-27 — Auto-updater.** `tauri-plugin-updater` with `createUpdaterArtifacts: true`; endpoint `https://github.com/heicfast/d-space_fork/releases/latest/download/latest.json`; minisign pubkey embedded in `src-tauri/tauri.conf.json`; passive install + relaunch (`src/features/settings-about/update-dialog.tsx`); release workflow signs with `TAURI_SIGNING_PRIVATE_KEY` secret and publishes betas as channel-Latest (alpha/rc stay prerelease).

**F-28 — i18n.** EN (default) + 简体中文 (`src/i18n/en.ts`, `zh-CN.ts`), parity-tested (`tests/i18n-parity.test.ts`), sweep scripts (`scripts/i18n-sweep.mjs`).

**F-29 — Mock connector (dev/test only).** `src/connector/mock/mock-connector.ts` (1729 lines) + `fixtures.ts`/`node-space.ts`/`mock-cleanup.ts`/`rng.ts`: 11 scenarios (`standard, large-ntfs, sd-card, btrfs-disk, lost-partition, permission-denied, disconnect-mid-scan, bad-sectors, failing-disk, out-of-space, corrupt-session` — `contract.ts:878-889`), procedural 2 TB NTFS/1.2 M-node index, error injection, mock-only dev panel (`src/features/dev-panel/dev-panel.tsx`).

**F-30 — v2 trait reservations (stubs).** `src-tauri/src/v2/mod.rs`: 5 traits (MlSafeDeleteScorer, NetworkShareScanner, CrossPlatformVolumeReader, Plugin, CloudBackupIntegration) — **"no trait has a concrete implementation — `unimplemented!()` or feature-gated"** (header line 9).

---

## Named Feature — Current Contract (deep dive)

**Feature under grade:** "reconstructed files with directory structure preserved, containing ONLY deleted files, as a post-scan option". Repo claim (docs/NEXT_SESSION.md:51-53): "deleted-ancestor chain reconstruction (original folder structure preserved), keep_empty_folders honored, file_repair = intact-prefix publishing".

### How deleted vs live is represented

- Live: `ScanTree { nodes: HashMap<u64, Node>, children, ... }` built by `scan/tree.rs::build_tree_with_user_rules` from `RawMftEntry` (record numbers as ids; ROOT_RECORD = 5; orphan/self-cycle parents rewired to root — "Gemini review B7"; walk-up aggregate sizes with `MAX_PARENT_HOPS` 256 + visited-set cycle guards; user rules score with priority over the 63 built-ins). `Node` fields: id/parent/name/data_size/aggregate_size/is_dir/score/score_rule/score_reason/modified_unix.
- Deleted: `RecoveryScan.deleted: RwLock<Arc<HashMap<u64, DeletedEntry>>>` + `deleted_children: HashMap<u64, Vec<u64>>` (**covers BOTH existing and deleted parents**) + `lost_path: Vec<u64>` (dead chains) + `deleted_dir_sizes` + `confidence` — built in `recovery/scan.rs::finalize_scan` for all three raw engines (identical logic blocks at lines 450-517 (MFT), 536-595 (FAT), 611-670 (ext); FAT/ext additionally store `BackendGeometry` for read-back dispatch). Fallback scans produce **no deleted data at all** — `FinalizeInput::Fallback` only builds the tree (scan.rs:672-695).
- Deleted map is published once behind an `Arc` (finalize consumes the collected scan; per-query `ScanView` clones entries out of the lock — results.rs:209-233).

### How the deleted-only tree is built (today)

There is **no dedicated deleted-only tree**. The observable post-scan model is:

1. **Path tree mixes deleted inline with existing**: `ScanView::children_of(parent)` (results.rs:266-283) = existing children ++ deleted children of that parent. A deleted folder appears as a directory node inside the live tree wherever its parent survives; `deleted_tree_node` marks it `special: "deleted"` (icon "directory", recursive size from `deleted_dir_sizes`).
2. **Broken chains go to the "Lost Path" branch**: any deleted entry whose `parent_record_no` is neither in the live tree nor the deleted map is listed under the synthetic root-level "Lost Path" group node (`path_tree`, results.rs:763-810; `parse_node_id("lost-paths") → LOST_PATH_ROOT`).
3. **A deleted-only _filter_ exists in the engine but not the UI**: `ResultsFilter.origins: ResultOrigin[]` filters candidates (results.rs:593-597); the frontend `filter-store.ts` can only produce `origins: ['existing']` (the "Existing" corpus toggle, line 213: `origins: f.existingOnly ? ['existing'] : null`) — **there is no "deleted only" toggle** anywhere in the UI (verified by grep over `src/`; the mock connector tests exercise `origins: ['lost-path']` directly).
4. **`Smart select best files`** (results-explorer.tsx:191-213) prefers non-existing origins per name — the closest UI gesture toward deleted-priority, but per-window only, not a tree view.

### Path reconstruction (recovery output layout)

`recovery/jobs.rs::rel_path_for` (lines 339-395) — the deleted-ancestor chain reconstruction:

- `rebuild_folders = false` → destination-root flat file name only.
- `rebuild_folders = true` (UI default): walk parents from the file to the **scope root** collecting names; **deleted ancestors resolve through the deleted map (`parent_record_no`), the first ancestor surviving in the existing tree switches back to tree parents**; hop cap 128; parts reversed and joined with `\`; the file name is appended (unless `drop_last` for a directory path). So a file deleted together with its folder recovers under `dest\<deleted-ancestor>…\<live-ancestor>…\<name>` — original structure preserved instead of flattening to the destination root.
- **Edge cases handled:** deleted parent → chain continues through the deleted map; live parent → tree parents resume; broken chain (parent in neither) → loop `break`s (the path is truncated at the last resolvable ancestor — the entry itself is in `lost_path`); shared parent (one deleted + one live sibling) → both sit under the same reconstructed folder; cycle safety → hop cap 128 (and `deleted_dir_size` uses a sentinel for sizing).
- **keep_empty_folders** (jobs.rs:247-266): during item collection every visited directory is remembered; folders that received no file become **marker items** `(u64::MAX, "", rel_path, 0, is_dir_marker=true)` so `recover_one` creates the directory and writes no file. Honored in the engine; **default false; NO UI toggle** (recovery-page.tsx:108).
- Directory selection without `rebuild_folders` is a no-op (`collect_item`, jobs.rs:307-311).

### Recovery options (exact names & defaults)

Wire `RecoveryOptionsWire` (jobs.rs:72-80, camelCase): `collision` (`rename|overwrite|skip`), `rebuild_folders`, `keep_empty_folders`, `file_repair`, `verify_on_write`. Contract twin `RecoveryOptions` (contract.ts:409-415). UI defaults (`src/features/recovery-queue/recovery-page.tsx:106-110`): `collision: 'rename', rebuildFolders: true, keepEmptyFolders: false, fileRepair: true, verifyOnWrite: true`. **The UI exposes only `collision` (radio group) and `fileRepair` (switch)** — `rebuildFolders`/`keepEmptyFolders`/`verifyOnWrite` are fixed defaults with no controls (verified by grep; i18n has no keys for them).

### Job behavior contract (observable)

- Selection: explicit `nodeIds` or `all` + captured `ResultsFilter` (recovery-page passes `resolveFilter()` or the selection-store `allFilter`); destination validated by `dw_validate_destination` (**overlap with the source volume is a hard refusal**; out-of-space surfaces needed vs free; `commands.rs::validate_destination` lines 538-608 also rejects non-absolute/nonexistent paths and uses the strict `device_letter` parser so `lp:` ids can't be misparsed).
- Item build (`build_job_items`, jobs.rs:173-273): tree snapshot → id resolution (`Nodes` parse; `All` = `query_results` size-desc with limit 500k) → overlap pre-check (source letter equality; `PhysicalDrive{N}` substring for lost partitions) → `collect_item` recursion with a `seen` set (dedup), directories expanded only when `rebuild_folders`, each visited folder remembered for the empty-folder pass; empty selection → `Error::Staging("nothing selected for recovery")`; oversized selection → explicit error.
- Streaming 4 MiB chunks, incremental blake3, `.dw_tmp` + atomic rename publish (Windows rename-race fallback: copy+delete), `sync_all` before publish, optional verify re-read (delete-on-mismatch, `verify-failed`), per-item failure codes as in F-11/F-13; progress events `dw-recovery-progress` (per item: currentFile, currentFileProgress, verifying flag, ETA, throughput); report `dw_recover_report` with capped lists (500/bucket) + exact `processedTotal` + `truncatedItemLists`.
- Existing-file items are re-read from disk (`stream_existing`, jobs.rs:882-915) — may have changed/vanished since scan (`source-missing`).
- Cancel: remaining items flip to `Skipped`; final status `Cancelled`.
- Report quirk: `verified: true` is **hardcoded** in `report_wire` (jobs.rs:1174) regardless of `verify_on_write`.

### Verdict on the named feature

**PARTIAL.** Component capabilities exist and are real (deleted representation, inline tree with Lost Path branch, engine-side `origins=['deleted']` filtering, deleted-ancestor chain reconstruction at recovery time, keep_empty_folders marker pass). Missing for the full claim: a **post-scan one-click "deleted-only reconstructed tree" view/option** (no UI filter, no dedicated tree mode — `raw` mode is aliased away), no `keep_empty_folders` UI surface, and the "tree containing ONLY deleted files" is achievable only via the unwired `origins` filter, not as a presented artifact.

### Named-feature observable-behavior matrix (for reference comparison)

| Behavior                                           | Current observable contract                                                                                                                                                 | Where                                           |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| Deleted representation                             | `DeletedEntry{record_no, parent_record_no, name, data_size, is_dir, modified_unix, resident, runs, compressed, encrypted, slot_key}`; separate map from the live `ScanTree` | collect.rs:44-70                                |
| Deleted child placement                            | deleted children indexed under BOTH live and deleted parents (`deleted_children`); inline merge in tree queries                                                             | scan.rs finalize ×3 engines; results.rs:266-283 |
| Dead parent chain                                  | entry listed under root-level synthetic "Lost Path" group (`LOST_PATH_ROOT = u64::MAX-1`); origin `lost-path`                                                               | results.rs:763-810                              |
| Deleted dir sizes                                  | recursive aggregate, cycle-safe sentinel                                                                                                                                    | scan.rs:737-762                                 |
| Post-scan "deleted only" option                    | **none in UI**; engine filter `origins:["deleted"]` works via `dw_results_query`/`dw_results_tree`-backed queries; UI store can only emit `['existing']` or `null`          | filter-store.ts:213                             |
| Output structure preservation                      | `rel_path_for`: deleted ancestors walked via deleted map until a live ancestor (or scope root/hop cap 128) resumes tree parents; `\`-joined rel path under destination      | jobs.rs:339-395                                 |
| Empty folders                                      | marker items create dirs only when `keep_empty_folders` (default **false**, no UI)                                                                                          | jobs.rs:247-266, 719-727                        |
| Flattening when off                                | `rebuild_folders=false` → bare file name at destination root                                                                                                                | jobs.rs:347-349                                 |
| Deleted-only _recovery_ of a whole deleted subtree | select the deleted dir node → `collect_item` expands deleted children recursively (needs `rebuild_folders`)                                                                 | jobs.rs:282-329                                 |
| Partial files                                      | `file_repair=true` publishes intact prefix (default true, UI switch); else `clusters-overwritten`                                                                           | jobs.rs:1100-1105                               |
| Confidence gates                                   | excellent ≥95% free / good ≥60% / fair >0% / poor 0% or compressed/encrypted/0-byte; dirs unknown                                                                           | deleted.rs:86-115                               |

---

## UI Surface Map

Routes (`src/app/router.tsx`, hash router in `WizardShell`): `/` Home (device selection + quick targets), `/sd-card` (SD-card flow), `/tools` + `/health` (SMART), `/backup`, `/quick-search`, `/scan/:deviceId/live` (progress w/ pause/resume/cancel + live results), `/results/:scanId` (Results Explorer), `/recover/:scanId` (recovery queue), `/repair`, `/settings`, `/lost-partition`, `/cleanup`, `/duplicates`, `/snapshots`, fallback. Sidebar per CHANGELOG v0.4: Data Recovery · Advanced Features · Disk Cleanup · File Backup (implemented in `src/app/shell.tsx`, sidebar tests in `tests/sidebar.test.tsx`).

- **Home** (`src/features/device-selection/home-page.tsx`): device list from `dw_devices_list`, quick targets via `dw_special_folders` + native picker (`pickDirectory({volumeId})`), scan start with optional path scope; SD-card page renders real device data (v0.5 fix).
- **Scan progress** (`scan-progress-page.tsx`): stage/status machine, weighted-monotonic overall progress, deleted-found headline counter, pause/resume/cancel, live ResultsExplorer (`liveMode`).
- **Results Explorer** (`src/features/results-explorer/results-explorer.tsx`, 690 lines): 3-pane (tree / virtualized list via `@tanstack/react-virtual` / preview), toolbar (nav cluster, breadcrumb, view toggle details|icons|map, Filter, search with debounce+commit), tree tabs **Path | Type only** (raw coerced to type, line 393; i18n key `results.tree.rawFound: 'Reconstructed'` exists but is never offered as a tab), 6-item context menu (incl. Open location via `dw_open_path` for existing files, Stage via `dw_stage_path`), "Smart select best files" (confidence-ranked, non-existing preferred, per loaded window), bottom CTA → `/recover/:scanId?all=1|ids=1`; sorts via `useSortStore` (name/type default asc, others desc).
- **Filter bar** (`filter-bar.tsx`, 276 lines): categories, date presets (yesterday/week/month/custom), size presets (tiny <512K … huge ≥1G, custom MB), extensions list, **"Existing" toggle** (existingOnly → `origins: ['existing']`), "Hide 0KB Files". **No "Deleted only" toggle exists.**
- **Recovery page** (`recovery-page.tsx`, 845 lines): destination list from `dw_mount_points` + native folder picker + manual path input, overlap/out-of-space inline alerts + insufficient-space modal gate, advanced section with collision radios (rename/overwrite/skip) + destination path, file-repair switch, Recover button gated on validated destination; progress/report views with per-item states (pending/running/done/failed/skipped/verifying) and error codes; `useStartRecovery` mutation + `useSyncExternalStore` progress store (`src/queries/recovery.ts`).
- **Other pages**: cleanup (Cleanup Bin staging/undo/permanent delete with typed phrase + DoD toggle), duplicates (BLAKE3 groups, keep-newest by `modifiedUnix`), snapshots (two-select 4-bucket deltas, 90-day retention), health (SMART table, verdict, TRIM/wear/temperature, `note` honest-limitation lines), repair (dry-run findings, typed REPAIR consent, undo log), lost-partition (from `dw_disk_layouts` → `lp:` targets), quick search (USN index, top-50), settings/about + Ed25519 updater dialog, dev panel (mock-only scenario/seed control).
- **Chrome/theme**: custom window controls (32 press-state SVGs, `src/assets/interim/chrome/`), light/dark tokens (`src/theme/adopted/`, snapshot-tested), toaster, error boundary, i18n EN/zh-CN with parity tests.

## Output & Persistence Formats

| Artifact          | Format / location                                                                                                                                                                                                                                                                                                           |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scan sessions     | `%LOCALAPPDATA%\DiskWizard\sessions\<scanid>.dws` — DWSESS02 LE binary (F-14); SQLite `sessions` table (migration `src-tauri/src/db/migrations/0004_sessions.sql`)                                                                                                                                                          |
| App DB            | `%LOCALAPPDATA%\DiskWizard\db\diskwizard.sqlite`, WAL, busy_timeout 5000, forward-only migrations 0001–0004 (12 tables: schema_meta, settings, snapshots, snapshot_entries, staging_items, staging_wal, permanent_deletes_forensic, ml_scores, user_rules, usn_index, usn_watermark, recovery_sessions — `db/mod.rs:13-15`) |
| Staging area      | `%LOCALAPPDATA%\DiskWizard\staging` (staging + WAL + forensic ledger)                                                                                                                                                                                                                                                       |
| Recovery output   | User-chosen destination; folder structure per `rel_path_for`; collisions `name (i).ext`; temp `<name>.dw_tmp`                                                                                                                                                                                                               |
| Updater feed      | GitHub Releases `latest.json` + `.sig` (Ed25519/minisign) + `.sha256` checksums                                                                                                                                                                                                                                             |
| Installers        | MSI (WiX) + NSIS `-setup.exe`, `createUpdaterArtifacts: true`                                                                                                                                                                                                                                                               |
| **Report export** | **None** — no CSV/JSON/text recovery report exists (UI-only report view)                                                                                                                                                                                                                                                    |

## Error Model

See Architecture. Notable honest-degradation behaviors: engine `Error::NotImplemented` for Windows-only paths (`commands.rs:698`, `volume/preflight.rs:198`, `index/baseline.rs:250`); preview `preview-unsupported` for PDF/AV/Office; `health-unavailable` when no SMART path succeeds; scan failure surfaces as toast + results-page banner via `ScanState.failure` mapped through the adapter's regex table.

## Tests, Fixtures & CI

**Rust (`src-tauri`) test counts by module** (rg `#[test]` per file):

| Tests | File                          | Tests | File                                |
| ----- | ----------------------------- | ----- | ----------------------------------- |
| 16    | recovery/health.rs            | 7     | snapshot/capture.rs                 |
| 13    | scan/tree.rs                  | 6     | scan/find_first.rs                  |
| 12    | index/baseline.rs             | 6     | fsfat/mod.rs                        |
| 11    | safe_delete/rules.rs          | 6     | fsfat/dir.rs                        |
| 11    | recovery/partitions.rs        | 5     | staging/ops.rs                      |
| 10    | duplicate/scan.rs (vss-gated) | 5     | snapshot/delta.rs                   |
| 9     | recovery/deleted.rs           | 5     | safe_delete/user_rules.rs           |
| 9     | recovery/collect.rs           | 5     | safe_delete/scorer.rs               |
| 9     | index/usn.rs                  | 5     | recovery/scan.rs                    |
| 8     | index/delta.rs                | 5     | recovery/results.rs                 |
| 7     | volume/hardware.rs            | 5     | recovery/repair.rs                  |
| 7     | staging/wipe.rs               | 5     | locked_file/vss.rs (vss-gated)      |
| 5     | fsext/mod.rs                  | 4     | staging/permanent.rs                |
| 4     | recovery/preview.rs           | 4     | locked_file/vss_pool.rs (vss-gated) |
| 4     | index/persist.rs              | 3     | volume/preflight.rs                 |
| 3     | v2/network_scanner.rs         | 3     | staging/wal.rs                      |
| 3     | scan/cloud.rs                 | 3     | recovery/sessions.rs                |
| 3     | locked_file/detect.rs         | 3     | index/mod.rs                        |
| 2     | volume/enumerate.rs           | 2     | staging/expiry.rs                   |
| 2     | staging/cross_volume.rs       | 2     | locked_file/mod.rs (vss-gated)      |
| 2     | index/watcher.rs              | 2     | db/mod.rs                           |
| 1     | scan/walk.rs                  | 1     | scan/mft.rs                         |

Total 243 `#[test]` attributes, 21 inside `vss`-feature-gated files → ~222 default / 243 with `--features vss`. Engine tests are pure/fixture-based (GPT CRC vectors incl. the classic `0xCBF43926`, ATA layout asserts, synthetic FAT/ext images built in-test, codec roundtrip for DWSESS02, allocation-bitmap vectors) — **no real-volume/VHDX fixtures exist** (`tests/fixtures/` on the frontend side holds only `adopted-tokens.snapshot.json`; the planned phase-4.3 VHDX generator is pending per docs/NEXT_SESSION.md:63-65).

**Frontend** — 25 `tests/*.test.ts(x)` files (+`setup.ts`), 192 `it()/test()` blocks, 57 `describe` blocks. Biggest: tauri-connector.test.ts 32 (the wiring suite pinning command names/casing/mappings), connector-mock.test.ts 27, mock-cleanup.test.ts 14, stores.test.ts 12, node-space.test.ts 11. **e2e** — 2 specs (`e2e/smoke.spec.ts` 6 + `e2e/responsive.spec.ts` 4) = **10 Playwright test blocks**, run against `vite preview` with the mock connector.

**CI (`.github/workflows/`)**

- `ci.yml` (push main + PR): job `rust` (windows-latest: `cargo fmt --check`, `clippy --lib -D warnings`, `cargo test --lib`), job `rust-vss` (clippy + tests with `--features vss`), job `frontend` (windows-latest: pnpm 12.15/Node 24 — typecheck, lint, format:check, arch:lint, ast-grep rules, knip, jscpd, `test:coverage` (thresholds 70/60/70/70 per README), build, bundle budget 200/500 KB, `cargo check`).
- `release.yml` (tags `v*.*.*`): version-match gate (tag semver core == tauri.conf.json version), tauri-action with `VITE_CONNECTOR=tauri` + signing secrets, betas publish as Latest (alpha/rc prerelease), `includeUpdaterJson: true`, sha256 checksums attached.
- `ui-smoke.yml` (push main + PR + dispatch): Playwright smoke vs mock-connector build + `pnpm tauri build --debug --no-bundle` link check; artifacts uploaded.

## Prior Sessions & Third-Party Provenance

- **Session trail**: CHANGELOG v0.1.0-alpha (2026-05-15) → v0.2.0-beta (05-17, VSS pool, USN index, updater) → v0.3.0-beta (11-07, five review waves ~90 findings, watcher to production) → v0.4.0-beta (11-12, DiskWizard merge: React UI + real engine + UAC manifest) → v0.5.0-beta (11-12, multi-FS engines, crash fixes, SMART rewrite, GPT fix, deleted-structure preservation, Quick Search). docs/DISCOVERY_LOG.md carries entries #001–#008 (spec divergences: VSS params, windows-rs gaps, USN index design, merge findings, updater prerelease 404). docs/ui/ holds the UI line's ADRs (001–008 incl. ADR-008 lost-data-flow: quick→advanced automatic, no mode selection), design system, screen manifest, session worklogs. Root `worklog/worklog.md` = THIS session's T1 (7 repos cloned and pinned: target + sectorforge + halim-recovery + testdisk + sleuthkit + ntfs-recover + mft-rescue).
- **SectorForge provenance (already adapted)**: `THIRD_PARTY_NOTICES.md` records — SectorForge, MIT, © 2026 terra seraph, adapted into `src-tauri/src/fsfat/` and `src-tauri/src/fsext/` (FAT12/16/32 + exFAT and ext2/3/4 engine logic: boot/FAT/inode parsing, directory-entry parsing, deleted-name recovery, allocation views), "adapted to DiskWizard's types and progress/pause/cancel model", MIT notice retained in module headers (verified: fsfat/mod.rs:5-9, fsext/mod.rs:5-9, fsfat/dir.rs:5-8). The NEXT_SESSION roadmap (lines 66-69) further names the SectorForge carver (95 formats, MIT) as the reference for the planned raw carving engine — **not yet adapted**.
- **Engine lineage**: D-Space engine v0.3.0-beta + clean-room Recovery Wizard React UI v0.3.4 merged as v0.4.0 (README.md:98, CHANGELOG). Spec: `D-Space-Mimari-v1.4.docx` (37 chapters, frozen, Turkish; binary — enumerated only). ~1,200 Turkish developer comments reportedly remain in the legacy core (NEXT_SESSION:75-76) — consistent with observed "Chapter X.Y" comment conventions throughout the legacy modules.

## Claimed vs Actual

| Claim                                                               | Source                                                         | Actual                                                                                                                                                        | Verdict                                                           |
| ------------------------------------------------------------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| "46 dw_* commands"                                                  | README.md:50, CHANGELOG v0.4/v0.5, CONNECTOR-CONTRACT addendum | **52** commands registered (lib.rs:418-480, commands.rs)                                                                                                      | **Understated/stale** — 6 more than claimed                       |
| "197 unit tests" frontend                                           | README.md:75                                                   | 192 it()/test() in 25 files                                                                                                                                   | Close but stale (192)                                             |
| "12 Playwright smoke tests"                                         | README.md:76                                                   | 10 test blocks (smoke 6 + responsive 4)                                                                                                                       | **Overstated**                                                    |
| "~230 Rust (incl. 8 multi-FS + 6 health, +9 vss)"                   | NEXT_SESSION:15-16                                             | 243 `#[test]` attrs (21 in vss-gated files)                                                                                                                   | Roughly right (the "~" holds); CHANGELOG's older 206/215 is stale |
| "raw-all tree branch is contract-reserved"                          | NEXT_SESSION:66, CONNECTOR-CONTRACT:86-87                      | Engine aliases raw→type (results.rs:739); UI coerces raw→type (results-explorer.tsx:393); mock implements raw-all                                             | Honest, correctly labeled reserved                                |
| "deleted-ancestor chain reconstruction, keep_empty_folders honored" | NEXT_SESSION:51-53                                             | Real (jobs.rs rel_path_for + marker pass) but keep_empty_folders has **no UI toggle** and no deleted-only post-scan view                                      | **PARTIAL** (see deep dive)                                       |
| `dw_pick_directory(hint?)` hint semantics                           | CONNECTOR-CONTRACT:61-65                                       | Command accepts `hint_volume_id`/`exclude_device_id` but **ignores them** (`let _ = …`, commands.rs:660)                                                      | Stub-ish deviation                                                |
| Connector "errors… never string-matches message text"               | CONNECTOR-CONTRACT:13-15                                       | Adapter maps via regex over English messages (tauri-connector.ts:85-108) because the engine error wire is `{kind,message}` only                               | Principle violated in the adapter by necessity                    |
| `report.verified`                                                   | contract.ts:462                                                | Hardcoded `true` (jobs.rs:1174) regardless of verify_on_write                                                                                                 | Dishonest field                                                   |
| `badSectorMap`                                                      | contract.ts:190                                                | Always empty (mod.rs:609-611, "deep-carve concept")                                                                                                           | Placeholder, documented in code                                   |
| Repair finding kinds (6)                                            | contract.ts:533-539                                            | Engine emits only `boot-sector-mismatch`, `mft-record-error`                                                                                                  | 4 enum values unused                                              |
| Preview union (pdf/av/office)                                       | contract.ts:352-372                                            | Engine returns `preview-unsupported` for those kinds                                                                                                          | Honest, documented                                                |
| TractMlScorer (ml-tflite)                                           | CHANGELOG v0.1 "infrastructure ready"                          | `unimplemented!()` (scorer.rs:203-209)                                                                                                                        | Gated stub                                                        |
| v2 traits                                                           | v2/mod.rs:9                                                    | No concrete implementations                                                                                                                                   | Stated openly                                                     |
| BitLocker detection "TODO in v0.1"                                  | preflight.rs:9                                                 | `VolumeStatus::BitLockerLocked` exists and maps to wire `locked`/`unlocked` (mod.rs:131-148) — detection evidently implemented despite the stale TODO comment | Comment stale, feature real                                       |
| "scan/mft.rs" as an engine                                          | (task framing)                                                 | scan/mft.rs is a 101-line **boot-sector probe** (`probe_ntfs`); the MFT deleted engine is recovery/collect.rs                                                 | Clarification                                                     |
| `MFT Service` layer 3                                               | scan/mod.rs:42-43                                              | v2 stub; strategy picker only checks elevation                                                                                                                | As documented                                                     |

## Gap List (capability level, vs a best-of-breed recovery product)

1. **No raw carving engine** — signature scanning / structural end detection absent; `raw-all` branch and `carve` stage are contract-reserved only (NEXT_SESSION itself lists this as sprint 2, SectorForge carver — 95 formats, MIT — as reference).
2. **No partition-table reconstruction or write-back** — GPT/MBR parsing, backup-header fallback and unallocated-gap probing exist, but nothing rewrites/repairs a partition table (TestDisk-class rescue); lost volumes can only be _scanned in place_ via the offset reader; probe covers only the first 128 sectors of each gap.
3. **No deleted-only reconstructed tree as a post-scan option** — engine `origins` filter + deleted-ancestor chain exist, but no one-click view/option; the path tree mixes live+deleted (the session's named feature gap).
4. **keep_empty_folders / rebuild_folders / verify_on_write lack UI controls** — engine honors them; the recovery dialog exposes only collision + file repair.
5. **No backup/archive structure recovery** — no ZIP/7z/RAR repair, no VHDX/VMDK/VM-image, no Outlook PST internals; categories classify such files but recovery is raw bytes only.
6. **No journal replay / transaction rollback** — NTFS $LogFile and ext journals unused; MFT-mirr-broken volumes get a _repair finding_, not a deep reconstruction from $MFTMirr (repair restores $MFTMirr FROM $MFT, never the reverse).
7. **No session resume after a crash mid-scan** — sessions persist only on Completed/Cancelled; a killed process loses the in-flight scan (partial results are not checkpointed; `recoverableDuringScan` is simply `overall_progress >= 1.0`, mod.rs:614).
8. **No report export** — no CSV/JSON/XML/printable recovery report; results live only in the UI/DB; the RecoveryReport is in-memory only (job registry, not persisted).
9. **No real-volume test fixtures** — no VHDX generator / hash-verified recovery in CI (phase 4.3 explicitly pending); all engine tests are synthetic in-memory images.
10. **No NVMe raw passthrough reads; ReFS deleted recovery unsupported and ReFS is not even detected** (fallback walk only; NEXT_SESSION sprint 3 admits the honest-message work is future).
11. **NTFS-compressed/EFS-deleted files are unrecoverable raw** (graded Poor, fail `unsupported-encoding`) — no decompression path; FAT deleted runs are contiguous-assumed (fragmented deleted FAT files degrade ungraded).
12. **Result dedup / hard-link awareness in recovery results unverified** (the find_first walker dedups hard links ≥64 KiB; the MFT raw engine's per-record view may double-count hard-linked names in aggregates) [UNVERIFIED].
13. **bad-sector map, deep-carve progress concepts** are wire placeholders (always empty/false).
14. **Single-threaded MFT walk** (rayon parallel parse still on the roadmap — NEXT_SESSION "Perf tour").
15. **Recovery jobs and scans are in-memory only** — no job persistence across app restarts; the 8-entry registries are per-process.

## Coverage

- **Tracked files: 313.** Type mix: 64 .rs, 62 .tsx, 55 .ts, 40 .md, 32 .svg, 16 .png, 8 .json, 7 .yml, 4 .sql, 4 .mjs, 3 .css, 2 .yaml, + ico/icns/bmp/docx/lockfiles.
- **Read fully (all lines)**: 41 files — README.md, CHANGELOG.md (682 l), REPO_STANDARDS.md, THIRD_PARTY_NOTICES.md, docs/CONNECTOR-CONTRACT.md, docs/NEXT_SESSION.md, worklog/worklog.md, src/connector/contract.ts, src/queries/recovery.ts, src/stores/filter-store.ts, src/app/router.tsx, src-tauri/src/{lib.rs, main.rs, error.rs}, src-tauri/src/recovery/{mod, collect, deleted, results, jobs, scan, sessions, partitions, commands}.rs, src-tauri/src/scan/{mod, mft, privilege}.rs, src-tauri/src/v2/mod.rs, src-tauri/src/safe_delete/mod.rs, src-tauri/src/duplicate/mod.rs (54 l), .github/workflows/{ci, release, ui-smoke}.yml.
- **Read partially (headers + targeted sections, purpose + key functions verified)**: 32 files — recovery/{preview (180/431), health (120/1058), repair (130/503)}.rs, scan/tree.rs (400/1034), scan/{walk, find_first, cloud}.rs (headers), fsfat/mod.rs (260/907), fsfat/dir.rs (40/429), fsext/mod.rs (180/1057), staging/mod.rs (80) + wipe.rs (45), index/mod.rs (90/378), snapshot/mod.rs, locked_file/mod.rs (60/198), db/mod.rs (70/229), volume/{mod, enumerate, preflight, hardware}.rs (headers), src/connector/tauri/tauri-connector.ts (1040/1363), src/features/results-explorer/results-explorer.tsx (400/691), src/features/recovery-queue/recovery-page.tsx (~510/846), package.json, src-tauri/tauri.conf.json, src-tauri/manifest.xml, docs/DISCOVERY_LOG.md (head + entry index), persisted CHANGELOG tool outputs.
- **Enumerated / grepped only (existence + purpose recorded)**: remaining ~240 files — LICENSE, pnpm-lock.yaml, src-tauri/Cargo.lock, all icon/bitmap assets (32 svg + 16 png + ico/icns/bmp), D-Space-Mimari-v1.4.docx (binary), tests/ (25 files counted, excerpts grepped), e2e/ (2 specs counted), src/features/* other pages (15 pages — route + purpose verified via router/grep, bodies not read), src/components/ui/* (26 components), src/i18n/* (key greps), src/connector/mock/* (structure + scenarios verified via contract/grep, 2700+ lines not line-read), src/queries/* (8 files besides recovery.ts), src/stores/{selection, ui, scan-session}.ts, src/workers/_, src/hooks, src/lib/_, scripts/* (4), docs/ui/* (24 files — purposes from index references), docs/{THREAT_MODEL, RELEASE_CHECKLIST}.md (sizes; claims cross-referenced via CHANGELOG/NEXT_SESSION which quote them), .github/FUNDING.yml [UNVERIFIED content], sgconfig/knip/eslint/vite/tsconfig/playwright configs (enumerated; counts unaffected).
- **Anomalies**: (1) clone HEAD is one worklog commit ahead of the pinned SHA (worklog-only delta — code state identical to the pin); (2) 313 vs 311 tracked files (the 2 worklog files); (3) README test/command counts are stale (52 vs "46" commands; 192 vs "197" unit tests; 10 vs "12" e2e); (4) `verified: true` hardcoded in the job report; (5) `dw_pick_directory` silently ignores its hint arguments. No credentials were read; `.git/config` and remotes were never inspected.

---

## Appendix — dw_* command → engine function map (all 52)

| Command                                                    | Engine function (file)                                                                                                       |
| ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| dw_devices_list / dw_devices_refresh                       | `recovery::list_devices_wire` (recovery/mod.rs:111) via `volume::list_drives` + `probe_drive_hardware`                       |
| dw_scan_start                                              | `recovery::scan::resolve_target` + `apply_path_scope` + `run_scan` (thread spawn; one-active-per-target)                     |
| dw_scan_state / pause / resume / cancel                    | registry + `RunFlags` atomics (recovery/mod.rs, commands.rs:105-156)                                                         |
| dw_results_tree / query / count                            | `recovery::results::result_tree` / `query_results` / `count_results`                                                         |
| dw_node_detail                                             | `recovery::results::node_detail`                                                                                             |
| dw_preview / dw_thumbnail                                  | `recovery::preview::request_preview` / `request_thumbnail`                                                                   |
| dw_session_save / sessions_list / session_load             | `recovery::sessions::save_session` / `list_sessions` / `load_session`                                                        |
| dw_recover_start / cancel / state / items / report         | `recovery::jobs::build_job_items` + `new_job` + `run_job` / cancel flag / `snapshot_progress` / `items_wire` / `report_wire` |
| dw_disk_health                                             | `recovery::health::disk_health`                                                                                              |
| dw_repair_dry_run / apply / undo_log / undo                | `recovery::repair::dry_run` / `apply_repair` / `undo_log_wire` / `undo_repairs`                                              |
| dw_disk_layouts                                            | `recovery::partitions::list_disk_layouts`                                                                                    |
| dw_app_info                                                | const reply (name/version/connectorMode "tauri"/engineVersion `dw-engine <v> (MFT+USN+VSS, arch)`/platform)                  |
| dw_mount_points                                            | `volume::list_drives` filtered Ready/Encrypted                                                                               |
| dw_validate_destination                                    | `commands::validate_destination` (overlap hard / out-of-space / invalid)                                                     |
| dw_special_folders                                         | `dirs::` + `%SystemDrive%\$Recycle.Bin`                                                                                      |
| dw_pick_directory                                          | `tauri_plugin_dialog` blocking folder pick (hint args **ignored**)                                                           |
| dw_open_path                                               | `reveal_in_explorer` (explorer /select)                                                                                      |
| dw_stage_path / list_staging / undo_staging / undo_resolve | `staging::stage` / `list_pending` / `undo` / `undo_with_resolution`                                                          |
| dw_permanent_delete                                        | `staging::permanent_delete` (typed phrase + optional DoD)                                                                    |
| dw_list_expired / dw_cleanup_expired                       | `staging::list_expired` / `cleanup_expired`                                                                                  |
| dw_rules_list / add / delete / toggle                      | `safe_delete::list_rules` / `add_rule` / `delete_rule` / `toggle_rule`                                                       |
| dw_duplicates_find                                         | `duplicate::find_duplicates` over the scan tree (letter volumes only)                                                        |
| dw_snapshot_capture / list / delta / cleanup               | `snapshot::capture_snapshot` / `list_snapshots` / `compute_delta` / `purge_old_snapshots`                                    |
| dw_index_search                                            | `index::index_search` (USN top-50 substring)                                                                                 |
| dw_get_setting / dw_set_setting                            | `db::get_setting` / `set_setting`                                                                                            |

Event payloads: `dw-scan-progress` = `ScanProgressEventWire` (scanId, status, stage, stageProgress, overallProgress, bytesScanned/Total, filesFound, dataFoundBytes, deletedFound, errors, badSectorMap[], etaSeconds, throughput, recoverableDuringScan, deviceConnected, currentPath); `dw-recovery-progress` = `RecoveryProgressWire` (jobId, scanId, status, overallProgress, currentFile, currentFileProgress, completed/failed/skipped/totalCount, throughput, verifying, etaSeconds).

---
