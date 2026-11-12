<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# DiskWizard Changelog

Spec v1.4 frozen (D-Space lineage). The implementation delta lives in
`docs/DISCOVERY_LOG.md`.

## [v0.4.0-beta] — 2026-11-12

**DiskWizard.** The Recovery Wizard React UI becomes the product
frontend (replacing the Vue app), wired end-to-end to the real engine —
the mock simulated backend is eradicated from every production surface,
and the D-Space feature set returns inside the new design language.

### Product

- Rebrand to **DiskWizard** (com.heicfast.diskwizard): English-first UI
  (manual language switch to 简体中文), custom window chrome, original
  emblem, all corpus-derived branding (incl. the EaseUS-named app
  string) removed; activation/licensing surfaces deleted
- `requireAdministrator` manifest — the UAC prompt at launch replaces
  every scan dying with ACCESS_DENIED (raw volume access, MFT engine,
  deleted-file recovery, VSS and repair all need elevation)
- New sidebar: Data Recovery · Advanced Features · **Disk Cleanup** ·
  File Backup

### The real recovery engine (`src-tauri/src/recovery/`)

- **Unified MFT walk** — one pass collects live AND deleted records;
  deleted entries keep data-run maps; $Bitmap cluster-allocation
  reader; pause/resume/cancel with partial results
- **Confidence grading** — cluster-reuse model (excellent/good/fair/
  poor); fresh record re-parse + allocation re-check at read and
  recovery time (stale scan state never recovers wrong bytes)
- **Server-side results** — filters (category/date/size/extension/
  origin/search), 5 sort keys, pagination with totals, path tree with
  the Lost Path branch, 8-category type tree; cleanup scores ride on
  node attributes
- **Verified recovery jobs** — collision policy, folder rebuild,
  Blake3 verify-on-write, per-file progress events, honest failure
  codes (record-reused, clusters-overwritten, unsupported-encoding)
- **Bounded previews** — image (≤8 MiB, header-parsed dimensions),
  text (UTF-16LE/UTF-8 detect), hex (≤64 KiB); deleted files read
  through their data runs
- **Sessions** — scans persist to compact `.dws` binaries (SQLite
  registry, migration 0004); completed and cancelled scans auto-save
- **Health** — SMART via ATA pass-through (fixture-tested sector
  parser), physical-drive mapping, honest health-unavailable
- **Volume repair** — NTFS boot/backup + $MFT/$MFTMirr dry-run
  findings; consent-gated apply with undo log (non-system volumes)
- **Lost partitions** — GPT/MBR layout parsing, unallocated-gap boot
  probing, and the whole MFT engine running over lost volumes via an
  offset reader

### UI (React 19, connector-driven)

- Real IPC bridge over 46 `dw_*` commands; 27 wiring tests pin every
  command name, argument casing and result mapping; connector default
  is the real engine (mock is an explicit test/dev opt-in)
- Size Map squarified treemap view; cleanup-score pills; staging from
  the results context menu
- Cleanup Bin (restore, conflict resolution, typed-phrase permanent
  delete with DoD wipe, expired batch with threshold abort), Duplicate
  Finder, Size Snapshots with two-select deltas, user rules in Settings
- Volume Repair page (mock per-file dropzone deleted), Lost Partition
  Rescue from real disk layouts, Home quick targets via the native
  folder picker and real special folders
- Auto-updater dialog (Ed25519-signed passive install + relaunch),
  close-while-busy guard, tray scan hand-off and full-disk alert toasts

### Engine housekeeping

- English error/tray strings; DiskWizard data folders
  (`%LOCALAPPDATA%\DiskWizard\{db,staging,sessions}`); single-instance
  plugin; superseded commands removed (internals + tests retained)
- CI: merged pipeline — Rust fmt/clippy/tests (default + vss) and the
  full frontend gate stack on every push; blocking e2e smoke +
  tauri-debug link; release workflow gains a version-match gate and
  builds with the REAL connector only

### Post-merge stabilization (same release)

- **7 Windows test failures fixed** (surfaced by CI — the engine's
  tests had only ever been linked, never executed, in the merge
  environment): db migration count (12 tables after 0004), SMART
  fixture byte typo (8760 = 0x2238), GPT name fixture (U+0041 not
  U+4100), case-sensitive `category_of`, and a genuinely lenient
  device-id parser — free-form text like "garbage" was silently read
  as drive G; strict shared `recovery::device_letter` now rejects
  anything but `vol:X` / `dev:X` / `X` / `X:` (health + repair)
- `free_fraction` out-of-bitmap semantics locked in with a dedicated
  test: unverifiable clusters count as NOT free (recovery confidence
  never overpromises)
- **Tauri crate/npm version alignment** — `tauri` 2.11.1 → 2.12.5 and
  `tauri-plugin-updater` 2.10.1 → 2.14.1 to match the frontend's
  `@tauri-apps/api` 2.12.4 / `@tauri-apps/plugin-updater` 2.14.1 (the
  tauri CLI refuses mismatched major/minor pairs)
- **Complete English runtime-string pass** — every error payload,
  tracing message, test assertion and all 63 built-in cleanup-rule
  explanations translated (30 files, ~280 strings). The UI, logs and
  rule panel are now fully English regardless of locale; only the
  deliberate UTF-16 decode fixture keeps non-ASCII data

### Numbers

197 frontend unit tests (+ wiring suite) · 12 e2e smoke · 206 Rust
engine tests default / 215 with vss (executing on Windows CI) ·
coverage 81.8/62.4/70.7/82.8 over 70/60/70/70 thresholds · bundle
143/327 KB gzip (budget 200/500)

## [v0.3.0-beta] — 2026-11-07

Third pre-release — **code-review wave + remaining v0.3 scope
completion**. Every finding from five independent line-by-line reviews
(Rust scan/volume/locked_file + data layer + command layer + Vue
frontend + CI/release) was resolved; the USN watcher went to
production.

### Critical fixes (release blockers)

- **`bundle.createUpdaterArtifacts: true`** added (`tauri.conf.json`) —
  without this flag the Tauri v2 bundler produces no updater `.sig`
  artifact; the auto-updater chain (latest.json + signature
  verification) was broken end to end.
- **`prerelease: true` constant removed** (release.yml) — GitHub's
  `releases/latest` only reflects stable releases; the hardcoded
  prerelease was 404-ing the updater endpoint. Prerelease status is
  now derived from alpha/beta/rc in the tag.
- **UNC input validation** — inputs like `\\fileserver\public` were
  silently mapped to drive `F:` by the old `chars().find(alphabetic)`
  pattern (risking a scan of the wrong volume's MFT shown as correct
  results). The strict `volume::parse_drive_letter` rejects UNC/path
  at every command boundary.

### Command layer (lib.rs)

- **`index_build` freeze fixed** — every launch re-enumerated the full
  MFT while holding the global DB mutex (the UI locked up for
  minutes). Now: incremental delta via `sync_volume` when a watermark
  exists, full baseline otherwise/on wraparound; the walk runs on a
  dedicated connection + `spawn_blocking`.
- **USN watcher wired to production** (v0.3.1 plan pulled forward) —
  background thread per volume at startup and after `index_build`;
  60 s polling applies `FSCTL_READ_USN_JOURNAL` deltas, automatic
  rebuild on wraparound. (`UsnWatchers` registry + `spawn_watcher`
  production wiring.)
- **Blocking commands moved to async** — `stage_path`, `undo_staging`,
  `undo_resolve_staging`, `cleanup_expired_staging_cmd`,
  `permanent_delete_cmd` (DoD wipe can take minutes),
  `capture_snapshot`, `index_search`, `index_status` now run on
  `spawn_blocking`; the main thread no longer stalls.
  (`DbState.conn` → `Arc<Mutex<Connection>>`.)
- **Cancel race fixed** — with a single global flag, cancel → quick
  rescan reset the old scan's flag and ran both walkers in parallel.
  Every run now carries its own `Arc<AtomicBool>`.
- **`scan_unc_path` cancellable + live progress** — SMB walks can take
  hours; the command now emits `scan-progress` events and can be
  stopped from the UI.
- **Migrations for the in-memory DB fallback** — a schema-less
  fallback killed every DB command with "no such table".
- Tray interval `saturating_mul(60)`; `is_on_battery` treats 255
  (unknown) conservatively; complete tokio feature set
  (`rt, sync, time, macros`).

### Staging / recovery guarantee (Pillar 5)

- **Cross-volume undo fixed** — `fs::rename` fails across devices;
  undo now runs the copy → Blake3 verify → atomic rename →
  delete-source chain (mirroring staging's two-phase protocol).
- **Idempotent undo silent-data-loss risk fixed** — full Blake3
  comparison for files ≤ 64 MB; a 4 KB prefix match alone is only
  "probably identical" and falls to Conflict (the user decides).
- **stage() crash window** — a failed DB insert rolls back the rename.
- **UNC source staging** — UNC paths now take the two-phase copy route
  (the old code failed on raw rename).
- **DoD wipe hardened** — readonly attribute cleared +
  truncate-to-zero + random-name rename before deletion (NTFS
  directory-entry residue).
- WAL phase-2b failure is now an explicit `wal_abort`; empty staging
  bucket directories cleaned; expired list LIMIT 2000; rate-limit
  sleep only between items.

### Scan engine (Pillar 1)

- **Cancel/progress gate fixed** — the fallback walker's
  `entries.len() % 500` check could be skipped by variable growth (a
  single huge directory = no checks at all). Replaced with a
  deterministic `visited_dirs % 32` counter.
- **Live sunburst O(N²) fixed** — instead of a full-tree rebuild +
  full clone + 63 rules × N nodes every 10k entries: NO clone
  (`build_tree_light` borrows), NO rule scoring, geometric intervals
  (10k→40k→160k→640k→2.5M cap). Total cost ~O(N log N).
- **MFT walker** — huge upfront reservation clamped (1M records);
  warning log on hard-limit truncation; `GetCompressedFileSizeW`
  0xFFFFFFFF success/error disambiguation via `GetLastError`.
- **Tree building** — self-cycle MFT records rewire to the root
  (previously invisible); aggregate inflation across mutual parent
  cycles prevented with a visited set; `NameAsc` sorting via
  `sort_by_cached_key`; `DirEntry::metadata()` instead of
  `symlink_metadata` (one syscall per file saved on SMB scans).

### Locked files / VSS

- `CreateFileW` failures now disambiguated by the reliable `e.code()`
  (HRESULT) instead of `GetLastError()` (which the error path could
  stain).
- VSS `Wait(INFINITE)` → 60 s bounded wait (a stuck VSS service froze
  duplicate scans).
- VSS pool dead-snapshot-context eviction + single retry.
- `RmGetList` ERROR_MORE_DATA retry (3 attempts).

### Index (USN)

- `resolve_full_path` root self-join fixed (`C:\C:\C:\…` garbage).
- `load_watermark` DB errors no longer silently mean "no watermark".
- `purge_volume` before a full rebuild (wraparound ghost records).
- `index_status.last_sync_unix` per volume.
- USN_RECORD_V3 records no longer kill the whole baseline (skipped).

### Frontend (Vue 3)

- **`nodeFullPath` double-folder fix** — after drillInto the detail
  panel showed `C:\a\b\b`; stage/probe/lock targeted the wrong path.
- **Index search pseudo-node full path** — uses the backend-resolved
  `full_path`; no fabricated `C:\<name>`.
- **Stale selection fix** — changing the window clears the old
  selectedNode; out-of-order response protection on fast drill clicks
  (request sequencing).
- **i18n completion** — conflict dialog, lock labels, shortcuts modal,
  Duplicate/Snapshot/UserRules panels, volume/cloud/drive warning
  labels, expired-cleanup flow — all moved into the locale files (no
  more Turkish text in the EN locale). Locale-aware number/date
  formatting (hardcoded `tr-TR` removed); `index.html` lang attribute
  updates on language change.
- **3D cleanup** — center disk geometry/material dispose +
  `forceContextLoss` + `rising`/`pulse` animation state reset (GPU
  leak + writes to disposed meshes).
- **Shortcuts** — suppressed while ANY modal is open (previously they
  fired outside conflict/perm-delete modals and triggered background
  actions); Ctrl+1..5 also selects the 3D mode; dead `searchOpen`
  removed.
- **Update.close()** — dismissing frees the updater resource.
- **AUTO_THRESHOLD abort surfaced** — the user sees an explicit alert
  when expired cleanup exceeds the threshold.
- `@types/three` moved to devDependencies; vitest coverage block
  (uninstalled package) removed.

### E2E infrastructure (4 broken tests fixed)

- Mock `onboarding_seen` → `onboarding_done` (onboarding reopened on
  every load and blocked the theme/update tests).
- Onboarding spec: 4 slides + scoped `.slide-title` selectors
  (strict-mode collision).
- View-mode spec: real `.view-chip`/`.view-chip-active` markup +
  mocked scan data + Ctrl+1..5 keyboard test.
- Playwright webdriver project explicitly opt-in
  (`D_SPACE_E2E_WEBDRIVER=1`); CI e2e `--with-deps chromium` + html
  reporter + localhost host.

### CI

- New `rust-vss` job — `--features vss` clippy + the 9 VSS tests are
  now verified on every push instead of merely compiling in the
  default run.

### Known limits (on the v1.0 road)

- **Code is unsigned** — the SmartScreen "unknown publisher" warning
  remains (EV certificate is phase 5.4, an external purchase).
- **EV cert / winget / Scoop** — phase 5.4 (organizational action).
- **Synthetic NTFS VHDX fixture** (phase 4.3) — requires a Windows
  admin environment.
- **TFLite model** (Tier 2 ML) — v0.4; infrastructure ready, model
  not trained.

---

## [v0.2.0-beta] — 2026-05-17

Second public release — **beta channel**. Still unsigned (EV
certificate is the chapter 18.2 stable goal), but the auto-updater
plugin is enabled; future releases can ship signed.

### Delivered

**Visualization (Pillar 2 maturity):**

- Three-column workspace (sprint 3.1c) — left VolumeSidebar (drive
  list + usage bar + status pill) + center workspace + right detail
  panel; responsive `<960px` top-sticky.
- Live scan sunburst (sprint 3.7) — `ScanProgress` overlay split
  layout, left 360×360 SVG arc (pure hand-rolled, 2 rings), right
  numeric stats; the backend emits a `partial_tree` event every 10k
  entries.
- Light/dark/auto theme toggle (chapter 9.5).

**Fast search (Discovery #005 / chapter 5.6):**

- NTFS USN journal index layer (sprints 3.8 + 3.8.1) — `usn_index` +
  `usn_watermark` SQLite tables, real-time delta watcher,
  `Everything`-style substring search.
- Baseline walker (`FSCTL_ENUM_USN_DATA` + `USN_RECORD_V2` parser)
  walks the whole MFT with a 1 MB buffer; records the watermark
  journal_id + next_usn so later launches replay only the delta.
- `IndexSearchBar` sticky top box, Ctrl+F shortcut, 150 ms debounce,
  top-50 substring matches + optional `WITH RECURSIVE` full_path.
- Graceful without admin: `mode="needs_admin"` badge.

**VSS pool (Pillar 5 addition):**

- Sprint 2H.3 `IVssBackupComponents` snapshot pool — `winapi 0.3.9`
  COM bridge (Discovery #002 resolved), worker thread + per-volume
  dedupe + lease renewal + reaper.
- Off by default (`vss` Cargo feature), opt-in build.
- Chapter 34.5.4 hash-time + 34.5.6 reference-counting behavior.

**Auto-updater (chapter 21.4) — sprint 3.6:**

- `tauri-plugin-updater` + `tauri-plugin-process` enabled;
  `tauri.conf.json` plugins.updater points at the GitHub Releases
  latest.json endpoint; release workflow `includeUpdaterJson: true`.
- `UpdateNotification.vue` — "check for updates" button in the
  header; modal (version + notes + download progress) → relaunch.
- Error typing: signature/network/other (TR + EN i18n).
- **Ed25519 pubkey placeholder** — the maintainer must generate it
  with `pnpm tauri signer generate` before the first real beta
  release (RELEASE_CHECKLIST §2).

**Cloud placeholder detection (chapter 11.1):**

- Win32 `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` flag detection — shows
  a ☁ badge in the UI.

**E2E infrastructure (sprint 3.5):**

- Playwright dev dep + `playwright.config.ts` (web + webdriver
  projects).
- 5 smoke scenarios: onboarding, visual mode toggle, IndexSearchBar,
  theme cycle, update-check fallback.
- `e2e/fixtures/tauri-mock.ts` Tauri IPC stub — backend-independent
  UI flow verification against the Vite dev server.
- `.github/workflows/e2e.yml` ubuntu runner; required after v0.2.0.

**Disk hardware profile (2026-05-17 polish):**

- `IOCTL_STORAGE_QUERY_PROPERTY` per drive: bus type (NVMe / SATA /
  USB / SAS / SCM), media type (SSD / HDD), vendor, model, serial,
  typical read speed (MB/s, derived from bus type).
- VolumeSidebar shows a small badge per drive card (e.g. "NVMe SSD
  ~3500 MB/s") + vendor/model line.

**Scan stop/cancel (2026-05-17 polish):**

- `ScanCancel(Arc<AtomicBool>)` Tauri state. The `scan_cancel`
  command sets the flag, the walker checks it periodically and bails
  with a `scan-cancelled` error.
- "Stop scan" pill button in the ScanProgress overlay — dismisses
  silently on click.

**Hard link dedup (2026-05-17 fix):**

- Standard-mode `find_first` walker gained NTFS file-index based
  dedup (Win32 GetFileInformationByHandle). For files ≥ 64 KB the
  same physical data counts size 0 on second sight — totals no longer
  exceed physical disk usage. Previous bug: a 1.2 TB report on a 1 TB
  disk (WinSxS hard links).

**Theme + UX polish (2026-05-17):**

- Info popover pattern (`InfoButton.vue`): ⓘ next to a heading →
  click opens the paragraph. Permanently visible intro paragraphs
  removed.
- Three-column workspace (Snapshot | Duplicate | Detail) side by side
  > 1600px; two columns 960–1600px; single column <960.
- Live sunburst: Tableau 10 palette + SVG radial-gradient gloss +
  drop shadow → a light 3D feel. Hover scale 1.025 + brightness.
  Click → selected wedge halo + detail chip in the lower panel.
- Pill colors (pill-ok / pill-warn / pill-frozen, score tiers,
  status/tier/lock pills) with per-theme text overrides keep 5:1+
  contrast in both light and dark.
- Probe button dark teal → vibrant blue (#2563eb).
- Enter/leave/move animations in the pending-deletions queue.
- Live scan log: bold file name + parent folder on two lines,
  scrollbar, tooltip per path, dashed borders between rows.
- Locale leak fix: all hardcoded TR strings moved to i18n (staging
  queue naming, "forensic ledger", "drilldown", score tier labels).
  `toLocaleString` calls with hardcoded "tr-TR" became locale-aware.

**Tests:**

- 124 Rust default (+9 vss-gated) + 34 frontend = **158 tests
  passing** (v0.1.0-alpha: 79).
- Cargo clippy strict + cargo fmt + vue-tsc + vite build clean.

### Known limits (later sprints)

- **Code is unsigned** — SmartScreen "unknown publisher" warning
  remains (EV cert v1.0).
- **Updater pubkey placeholder** — the maintainer must generate it
  before a real release or signature validation fails.
- **No duplicate v0.2** — head-hash prefilter + rayon parallel
  (phase 4.1).
- **No snapshot retention** — 90-day cleanup + streaming delta loader
  (phase 4.2).
- **No network scanner** — UNC path scan + bandwidth-aware
  (phase 4.4).
- **No ML model** — TFLite infrastructure ready, model in v0.3.
- **No tray live monitor** — launcher/quit only (phase 5.1).
- **No EV cert** — SmartScreen reputation on the v1.0 road
  (phase 5.4).

---

## [v0.1.0-alpha] — 2026-05-15

First public alpha. **Unsigned** MSI/NSIS — Windows SmartScreen will
show a "publisher unknown" warning (chapter 18.2: EV cert in the v0.2
sprint).

### Delivered

**Pillar 1 — Speed (chapter 5):**

- Direct MFT reading (`ntfs` crate) — a 1 TB target in <5 s with
  admin rights
- FindFirstFile fallback — non-elevated mode, always works
- Three-tier privilege strategy (chapter 5.2A K1+K2)
- Volume pre-flight (chapter 33.2 layer 0): ReFS/Remote/Removable/
  CdRom warnings

**Pillar 2 — Visual elegance (chapter 9.1):**

- Sunburst donut (hand-rolled SVG)
- Treemap (squarified Bruls/Huijsen/van Wijk 2000)
- Bubble pack (Vogel spiral + 80-iteration force relaxation)
- Timeline (mtime axis + Y-relax, log scale beyond 1 year)
- Lazy viewport-aware queries (chapter 9.6): ~200 nodes per window

**Pillar 3 — Intelligence layer (chapter 6):**

- 63 built-in rules — dev caches / system protection / cloud sync /
  VM disks
- Score tier colors: 0–30 red, 31–60 yellow, 61–85 green, 86–100 blue
- User-defined rules — UI editor + DB persistence + runtime merge
  (chapter 6.4)
- `RulesScorer` concrete impl + SQLite `ml_scores` cache
  (chapter 6.5.2)
- TFLite infrastructure behind the `ml-tflite` feature gate (model in
  v0.3)

**Pillar 4 — Time dimension (chapter 8):**

- Snapshot capture (dir-only in v0.1)
- Delta computation (added/removed/grew/shrunk top-10)

**Pillar 5 — Recovery guarantee (chapter 12):**

- Same-volume staging + 24 h undo
- Cross-volume two-phase commit + WAL recovery (chapter 12.3 v1.4
  fix)
- Conflict-resolution dialog (chapter 12.2.4): Overwrite/Rename/
  KeepBoth/Cancel
- Permanent delete + forensic ledger (chapter 12.4) — double
  confirmation (typed file name)
- Lazy expiry cleanup (chapters 12.2.1–2.3) — rate-limited,
  AUTO_THRESHOLD=100

**Duplicate detector (chapter 7) v0.1:**

- Blake3 streaming hash
- Size-bucket → hash-bucket pipeline
- Reclaimable-bytes report

**Locked files (chapter 34) v0.1:**

- CreateFileW share-violation probe
- Windows Restart Manager owner detection (PID + app name)
- Scan-time hot-path isolation (chapter 34.5.1)
- VSS pool **deferred** — Discovery Log #002 (windows-rs 0.61 lacked
  `IVssBackupComponents`; albertony/vss port in v0.2)

**UI/UX (chapter 15):**

- Progressive disclosure, 3 levels (summary · expand · advanced)
- Keyboard-first: Ctrl+R, Ctrl+1-4, Backspace, Delete, Ctrl+Z,
  Ctrl+F, Ctrl+?
- Onboarding 3 slides + Fast/Standard mode choice (chapters 15.3 + 37)
- System tray icon + menu (chapter 13.1)
- Conflict / permanent-delete modals

**Multiple languages (chapter 19):**

- vue-i18n v10 + TR/EN locales + language toggle chip
- Onboarding fully translated, main flow translated

**Infrastructure:**

- SQLite (chapter 14): 9 tables, WAL mode, 2 migrations
- Settings KV API (onboarding/language/telemetry preferences)
- Local performance telemetry (last 20 samples, NO transmission)
- Telemetry opt-in flag (off by default, chapter 18.3)

**v2 trait reservations (chapter 26):**

- 5 trait stubs: MlSafeDeleteScorer · NetworkShareScanner ·
  CrossPlatformVolumeReader · Plugin · CloudBackupIntegration

**Build + distribution:**

- GitHub Actions CI: fmt + clippy `-D warnings` + cargo test + pnpm
  build + vitest
- Release workflow: tag → tauri-action → MSI + NSIS + GitHub Release

**Tests:**

- 69 Rust unit/integration + 10 frontend Vitest = **79 tests**

**Documentation:**

- Bilingual (TR/EN) README, D Brand template
- REPO_STANDARDS.md
- docs/DISCOVERY_LOG.md (#001-003)
- docs/THREAT_MODEL.md (chapter 27: 5 actors + 8 surfaces + T1-T5)
- docs/RELEASE_CHECKLIST.md (chapters 18.2 + 21.4 + 35)

### Known limits (v0.2 sprints)

- **Code is unsigned** — SmartScreen "unknown publisher" warning
  (EV cert v0.2)
- **No VSS pool** — locked files cannot retry hashing (Discovery Log
  #002)
- **No ML inference** — model training in v0.3
- **Auto-updater passive** — Ed25519 key + endpoint in v0.2
- **No three-column layout** — single column (v0.2 phase 3.1)
- **No light theme** — dark only (v0.2 phase 3.2)
- **No tray polling/auto-clean** — launcher/quit only (v0.2 phase
  3.4)
- **No Win32 reparse-tag detection** — cloud placeholders detected by
  pattern only
- **No Playwright e2e** — Vitest unit tests only (v0.2 phase 3.5)

### License

GPL-3.0-or-later. The spec document had recommended MIT
(chapter 22.4); overridden on 2026-05-14.
