<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# DiskWizard Implementation Discovery Log

Living discovery ledger for the master architecture
(`D-Space-Mimari-v1.4.docx` chapter 28). The spec is **FROZEN** —
findings made while writing code land in this file; the master
document is never revised.

Template (chapter 28.1):

```
## #NNN — <Title>
Date: YYYY-MM-DD · Related chapter: <X.Y> · Type: gotcha | revised | deferred | cancelled
**Finding:** ...
**Impact:** ...
**Decision:** ...
**Reference:** <commit hash or file path>
```

---

## #001 — VSS `bBackupBootableSystemState` parameter wrong in the spec example

Date: 2026-05-15 · Related chapter: 34.2 · Type: revised

**Finding:** The master spec chapter 34.2 example code reads
`set_backup_state(false, true, VSS_BT_FULL, false)`
(`bBackupBootableSystemState=TRUE`, `backupType=VSS_BT_FULL`).
Microsoft Learn
([vsbackup/SetBackupState](https://learn.microsoft.com/en-us/windows/win32/api/vsbackup/nf-vsbackup-ivssbackupcomponents-setbackupstate))
recommends **`bBackupBootableSystemState=FALSE`** and
**`backupType=VSS_BT_COPY`** for ad-hoc volume snapshots — we do not
want system state included and must not stain writer backup history /
archive bits.

**Impact:** Copying the spec example verbatim in a future VSS
implementation would add needless system-state snapshot cost and
writer event-log noise.

**Decision:** The VSS sprint uses
`SetBackupState(false, false, VSS_BT_COPY, false)`. The chapter 34.2
example is illustrative only — not a reference.

---

## #002 — `windows` crate 0.61.3 does not provide `IVssBackupComponents`

Date: 2026-05-15 · Related chapter: 34.2, 34.5.4 · Type: resolved
(previously: deferred)

**Finding:** The `windows::Win32::Storage::Vss` module carries all the
provider/admin/component/snapshot-management interfaces, but the
**requester-side `IVssBackupComponents` interface and the
`CreateVssBackupComponents` factory are entirely missing**. Reason:
those APIs are defined in the `vsbackup.h` C++ header and the
automatic metadata source of windows-rs (`Windows.Win32.winmd`) does
not include them.

Verification: inside
`.../windows-0.61.3/src/Windows/Win32/Storage/Vss/mod.rs`,
`IVssAdmin`, `IVssAsync`, `IVssComponent`, `IVssEnumObject`,
`IVssSnapshotMgmt` etc. exist — searching `IVssBackupComponents`
returns nothing.

**Impact:** Chapters 34.5.4 (hash-time snapshot pool) and 34.5.5
(user drill-down) cannot be implemented directly. Three options:

1. **Manual COM vtable** (`IVssBackupComponents_Vtbl` struct + 15+
   method FFI declarations + IUnknown ref counting). ~400 lines of
   FFI, COM threading subtleties, ref-count traps, BSTR lifecycle.
2. **Third-party crate.** The crates.io review for a
   production-ready VSS requester crate was postponed to v1.5.
3. **WMI `Win32_ShadowCopy`** snapshot creation. Requires the
   ClientAccessible (persistent) context — breaks the chapter 34.5.6
   non-persistent guarantee and brings back leaked-snapshot risk.
   Rejected.

**Decision:** The chapter-34 v0.2 VSS pool sprint was **deferred**.
Chapter 34 v0.1 (share-violation probe + RestartManager owner
detection) shipped in the alpha as sufficient scope. The final VSS
approach (option 1 or 2) is revisited in the v1.5/v2.0 spec-revision
round.

**Reference:** commit `06368db` (chapter 34 v0.1); the deleted draft
`src-tauri/src/locked_file/vss.rs` (rolled back).

### 2026-05-15 update — option A chosen, sprint 2H.3 complete

The `winapi 0.3.9` crate provides `IVssBackupComponentsVtbl` and the
`CreateVssBackupComponents` factory through its `vsbackup` feature —
no manual COM vtable needed. Reference:
`docs.rs/winapi/0.3.9 → vsbackup::IVssBackupComponentsVtbl`.

Gated behind the `vss` Cargo feature (default OFF). Zero impact on
the default build, opt-in. `Cargo.toml` declares the optional
dependency only for `cfg(windows)` targets. New files:

- `src-tauri/src/locked_file/vss.rs` — low-level COM bridge (~430
  lines): BSTR helpers, COM MTA init, async wait, the
  RawSnapshot create/destroy chain, and a `SnapshotProvider` trait
  for test injection.
- `src-tauri/src/locked_file/vss_pool.rs` — high-level pool (~360
  lines): single worker thread, per-volume dedupe, reference counting
  and lease renewal (chapter 34.5.6), reaper (idle 5 min → destroy).
- `src-tauri/src/duplicate/scan.rs` — the
  `hash_file_with_retry` chain: `ERROR_SHARING_VIOLATION`
  (`raw_os_error == 32`) → VSS pool reader.

**Reference:** worktree `agent-a14d6e5c7ecb9b336`, sprint 2H.3.

---

## #003 — Active development order

Date: 2026-05-15 · Related chapter: roadmap · Type: gotcha

**Finding:** Phase-1 implementation does not follow the spec's
37-chapter order verbatim; it is organized by pillar:

1. ✅ Chapters 4.3/4.4/5/14/33.2/12.2/12.3 — speed + infrastructure +
   recovery-guarantee core
2. ✅ Chapter 6 v1 (rule engine), chapter 8 (time machine), 9.1
   (sunburst), 9.6 (lazy viewport), 7 v0.1 (Blake3 duplicates), 34
   v0.1 (lock probe)
3. Next — chapter 9.1 remaining 3 views (treemap/bubble/timeline),
   12.4 permanent delete, 20/21 tests + CI/installer, 6.2 missing
   rules.

**Decision:** The discovery log fills up at the end of every sprint.
`DISCOVERY_LOG.md` references the master spec; the spec stays frozen
while this file stays alive.

---

## #004 — VSS context: FILE_SHARE_BACKUP vs BACKUP

Date: 2026-05-15 · Related chapter: 34.5.4 · Type: revised

**Finding:** The master spec chapter 34.5.4 example either skips the
`SetContext` call (implicit `VSS_CTX_BACKUP=0`) or recommends
`VSS_CTX_BACKUP`. With `VSS_CTX_BACKUP`:

- Writer involvement is required — `GatherWriterMetadata` +
  `PrepareForBackup` + `DoSnapshotSet` spin up writers, and
  "VSS writer didn't respond" errors are common on user machines.
- Persistent + non-auto-release by default — leaked-snapshot risk
  returns.
- SQL Server / Exchange writers pause IO during quiesce — a needless
  performance penalty for hash-time reads.

DiskWizard only needs `FILE_READ_DATA`; writer freeze/thaw is
unnecessary (a consistency point shorter than a second suffices).

**Decision:** `SetContext(VSS_CTX_FILE_SHARE_BACKUP = 0x10)`:

- `VSS_VOLSNAP_ATTR_NO_WRITERS` — no writer involvement.
- `VSS_VOLSNAP_ATTR_AUTORECOVER` off (read-only; file-system-level
  consistency is enough).
- Non-persistent + auto-release — the service-side snapshot
  disappears automatically on
  `IVssBackupComponents::Release`. Zero leaked-snapshot risk.

**Impact:** The hash-time path can open a snapshot in ~500–800 ms
instead of 1–3 s, with no SQL Server / Outlook writer log noise.

**Reference:** `src-tauri/src/locked_file/vss.rs` (the `SetContext`
call + chain commentary); sticky note in the chapter 34.5.4 comments.

---

## #005 — USN journal index (the "Everything" model) — chapter 5.6 (new subsection)

Date: 2026-05-16 · Related chapter: 5.6 (new) · Type: revised (scope
extension)

**Finding:** Master spec v1.4 defines only the MFT walker +
`FindFirstFile` fallback for file scanning (chapters 5.1–5.2A).
There is no instant substring search — Ctrl+F is marked a "v0.2
placeholder" (chapter 15.2 shortcuts). For "Everything"-style
sub-second search, a **persistent index** + **incremental delta
sync** mechanism was added as chapter 5.6.

The NTFS USN (Update Sequence Number) journal is already maintained
by the volume itself (every file/dir creation, deletion and rename
writes a 32–128 byte record). `FSCTL_ENUM_USN_DATA` (`0x000900B3`)
enumerates a baseline; `FSCTL_READ_USN_JOURNAL` (`0x000900BB`) reads
deltas. A single IO call fills an 8 KB–1 MB buffer; on a Win11 NVMe
drive a 1 TB volume baselines in roughly 200–400 ms.

**Impact:**

- Persistent index (`diskwizard.sqlite` → `usn_index` +
  `usn_watermark` tables) loads in < 200 ms at startup.
- Incremental sync by polling (5 s) via
  `FSCTL_READ_USN_JOURNAL` next_usn watermark.
- Substring search < 50 ms (SQLite LIKE + `idx_usn_name`).
- Requires admin (`\\.\C:` raw volume + USN journal reads).
- `windows-rs` 0.61 does not ship the FSCTL constants under their
  symbolic names — hand-rolled
  `const FSCTL_ENUM_USN_DATA: u32 = 0x0009_00B3;` (stable since
  Win2K).
- USN_RECORD_V2 binary parsing: 60-byte fixed header + UTF-16 LE
  name. V3 (128-bit file_ref) exists for Win10+ ReFS but NTFS
  defaults to V2.
- Wraparound: when the USN journal is deleted and recreated the
  `journal_id` changes. A fresh mismatch against the watermark's old
  `journal_id` → full re-enumeration required (the old USN window is
  invalid; events may be missing).

**Decision:** Sprint 3.8 introduced the `src-tauri/src/index/`
module:

- `usn.rs` — USN_RECORD_V2 parser + FSCTL constants +
  ERROR_JOURNAL_* handling.
- `persist.rs` — SQLite save/load + LIKE search + `WITH RECURSIVE`
  parent-path chains.
- `delta.rs` — `apply_delta` with reason-mask filtering +
  `detect_wraparound`.
- `watcher.rs` — `std::thread` background polling (5 s), shutdown
  channel. Without admin the watcher **does not start**
  (`IndexStatus { mode: "needs_admin" }`).
- Tauri commands: `index_build` / `index_status` / `index_search`.

Migration `0003_usn_index.sql`, forward-only over the existing DB.
The v0.1 stub `index_build` returned only status; the real baseline
walker + background watcher with actual `DeviceIoControl` calls
landed in the next minor sprint (binary stream parsing + race tests
deserved separate scope).

**Reference:**

- `src-tauri/src/index/{mod,usn,persist,delta,watcher}.rs`
- `src-tauri/src/db/migrations/0003_usn_index.sql`
- That sprint's commit — worktree `agent-a1d7ae08a791e3114`

## #006 — v0.3.0 review wave: ~90 findings + remaining v0.3 scope

Date: 2026-11-07 · Related chapter: all · Type: gotcha (batch)

**Finding:** Five independent line-by-line reviews (scan/volume/
locked_file, data layer, command layer, frontend, CI/release)
produced ~90 findings. The most critical: (1)
`bundle.createUpdaterArtifacts` was missing — the updater `.sig` was
never produced; (2) the hardcoded `prerelease: true` 404'd
`releases/latest`; (3) `index_build` re-enumerated the full MFT on
every launch under the global DB mutex → minutes-long UI freezes;
(4) UNC inputs silently became drive letters via
`chars().find(alphabetic)`; (5) cross-volume undo always failed on
`fs::rename`; (6) `nodeFullPath` produced a doubled folder after
drillInto; (7) the e2e mock read `onboarding_seen` while the app
wrote `onboarding_done` — 4/6 scenarios were broken.

**Impact:** In v0.2.0-beta the auto-updater effectively did not work;
production risks like the startup freeze and the cancel race existed.

**Decision:** All findings resolved (list in the CHANGELOG v0.3.0-beta
section). The USN watcher was pulled forward from the v0.3.1 plan
into production (`sync_volume` incremental core + `UsnWatchers`
registry). The updater Ed25519 keypair was regenerated for the fork
channel; the private key lives in GitHub Secrets.

**Reference:** the `heicfast/d-space_fork` v0.3.0-beta tag.

## #007 — DiskWizard merge: engine test first-execution findings + strict device ids

Date: 2026-11-12 · Related chapter: all · Type: gotcha (batch)

**Finding:** The v0.4.0 merge shipped a large new engine whose tests
had been **compiled and linked** in a cross environment but never
**executed** (no Windows runtime in the build sandbox). First
execution on windows-latest CI surfaced 7 failures: a stale table
count (11 vs 12 after the sessions migration), two fixture typos
(SMART raw hours 0x28 vs 0x38; GPT name U+4100 vs U+0041), a
case-sensitive `category_of`, and — the only genuine engine bug —
`volume_letter_of` accepting any free-form text whose first character
was a letter ("garbage" → drive G). Separately, the tauri CLI
refused to build: the merged frontend's npm packages (@tauri-apps/api
2.12.4, plugin-updater 2.14.1) were minor-version ahead of the Rust
crates (2.11.1 / 2.10.1).

**Impact:** CI red on main immediately after the merge push; a
mock-connector debug build would also have failed the same check in
the Release workflow.

**Decision:** Strict shared `recovery::device_letter` (accepts only
`vol:X` / `dev:X` / `X` / `X:`) used by health + repair; fixtures
corrected; `category_of` lowercases its input; out-of-bitmap
`free_fraction` semantics locked with a dedicated test (unverifiable
clusters are NOT free). Rust crates bumped to tauri 2.12.5 /
tauri-plugin-updater 2.14.1 to match npm. Lesson recorded: **linked
is not run** — any engine change must execute its tests on
windows-latest before release tagging (the CI loop is the only place
that can run them).

**Reference:** commits 0aef2a3 (fix) and 48ce42e (English string
pass) on `heicfast/d-space_fork` main.
