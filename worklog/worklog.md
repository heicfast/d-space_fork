# d-space worklog — chronological session handoff

> This file is the long-horizon handoff log for the feature-harvest session
> ("Feature Harvest, Gap Analysis & Production Implementation", started 2026-11-12).
> The full session SOP is archived at `worklog/session-feature-harvest-prompt.md`.
> Prior audit trails in this repository (`docs/ui/worklog/`, `docs/DISCOVERY_LOG.md`,
> `docs/NEXT_SESSION.md`, `CHANGELOG.md`) are preserved untouched; this root `worklog/`
> directory is the new session's area and extends — never overwrites — them.

---

## 2026-11-12 — T1: session start, repos cloned and pinned

**Session:** Feature harvest from six references into DiskWizard (heicfast/d-space_fork).

**Done this unit:**

- Session prompt archived with the token redacted (`worklog/session-feature-harvest-prompt.md`); read twice before action.
- All seven repositories cloned and pinned at session start:

| Repo | Role | HEAD (pinned) | Files |
| --- | --- | --- | --- |
| heicfast/d-space_fork | target | `3907f7144ad8578079186b949ef5657b5e2c2d03` | 311 |
| terraseraph/sectorforge | base ref A (same stack) | `b292a299c18013706e81d670657ced6c43f200ca` | 197 |
| htoklu/halim-recovery | base ref B (same stack) | `3b5d66d8537432f54b76489e717387f9da198635` | 62 |
| cgsecurity/testdisk | feature ref C (partition & carving) | `5f5d6056696d70d3a3ff1e1eaa5a5125cb42bb6a` | 793 |
| sleuthkit/sleuthkit | feature ref D (forensic engine) | `08167c8d6d5fc1bd4ee6d822e2fe042cabcdee6d` | 959 |
| mjgil-rust/ntfs-recover | feature ref E (NTFS deep recovery) | `5072bbc729b293efc0b2070b38ad30c3b7e043cf` | 25 |
| hasithahaththella/mft-rescue | feature ref F (MFT analysis) | `9b0e5ecc9d24ae9918f83e3a81e4a8616f610a22` | 8 |

  References are depth-1 clones (HEAD only) — citation provenance is the HEAD SHA above. The target is a full clone.

- Target license read: **GPL-3.0-or-later** (`LICENSE`). Reference licenses to be read and recorded by each Phase 0 agent in its context sheet.
- Target CI inventory: `.github/workflows/ci.yml` (Rust fmt/clippy -D warnings/tests + vss feature job, full frontend gate stack, all on windows-latest), `release.yml`, `ui-smoke.yml`. Worklog-only pushes do not touch compiled code paths; CI must stay green at every push.
- Token hygiene: the supplied GitHub token lives only in the session sandbox (git config for the target remote / local env file, mode 600, outside all repos). Never committed, never logged in any artifact. User rotates it after the session.

**Reading method (recorded honestly, per the no-hallucination rule):**

- sectorforge (197 files), halim-recovery (62 files), ntfs-recover (25 files), mft-rescue (8 files) and the target (311 files) are read **exhaustively** — every file accounted for.
- testdisk (793 files) and sleuthkit (959 files) are covered **subsystem-exhaustively**: full file enumeration and classification, plus deep reading of the subsystems this harvest actually draws on (testdisk: partition detection/reconstruction + PhotoRec carving engine; sleuthkit: the image/volume-system/filesystem abstraction and the NTFS/FAT-family parsers). Each sheet carries an explicit coverage statement (files read vs. enumerated). This is the honest maximum for these codebases within a session; it is recorded rather than glossed as "end to end".

**Context found in the target (affects the matrix):**

- The target is DiskWizard v0.5.0-beta (Tauri 2, Rust engine + React 19/TS UI, Windows-first, GPL-3.0-or-later), with prior sessions already delivering: raw NTFS MFT engine, FAT12/16/32 + exFAT + ext2/3/4 engines (adapted from SectorForge per `docs/NEXT_SESSION.md`), GPT/MBR parsing with lost-partition rescue, deleted-ancestor chain reconstruction with `keep_empty_folders`, sessions (DWSESS02), SMART health, staging/undo, USN Quick Search.
- The repo's own roadmap (`docs/NEXT_SESSION.md`) already calls for the VHDX fixture generator (matches T4) and the raw carving engine with SectorForge as reference (matches Wave 4) — this session's plan and the repo's trajectory are aligned.
- The named feature (deleted-only reconstructed tree as a post-scan option) is therefore expected to be **PARTIAL** in the target; Agent A must grade it precisely and Agent B must extract SectorForge's exact contract for the delta.

**Next:** T2 — parallel agents A–G produce the seven Phase 0 context sheets in `worklog/` (pushed before any code change), then T3 consolidates `worklog/FEATURE-MATRIX.md` as the gate document. Nothing is implemented before the matrix is committed and pushed.

---

## 2026-11-12 — T2: Phase 0 context sheets (Agents A–G, parallel)

**Done this unit:** seven parallel read-agents produced the Phase 0 context sheets (2,232 lines total). No repository code was touched; only `worklog/` grew. Load-bearing citations were independently spot-verified with Grep by the session lead before this push (all passed; details below).

| Sheet | Lines | License found | Coverage reported by agent |
| --- | --- | --- | --- |
| TARGET-STATE.md | 359 | GPL-3.0-or-later (target) | 41 files read fully, 32 partially, ~240 enumerated (assets/locks/mock/docs) of 311 |
| SECTORFORGE-FEATURES.md | 389 | MIT | 68 fully, 4 partially, ~120 enumerated of 197 |
| HALIM-FEATURES.md | 307 | MIT | 60 fully of 62 (2 binaries existence-only) |
| TESTDISK-FEATURES.md | 384 | GPLv2-or-later | 30 fully + ~20 partially of 793 (subsystem-exhaustive; 353-format registry enumerated) |
| SLEUTHKIT-FEATURES.md | 400 | CPL-1.0 core / Apache-2.0 bindings (+ islands) | 22 fully + ~18 partially of 959 (subsystem-exhaustive) |
| NTFS-RECOVER-FEATURES.md | 241 | 0BSD | 24 of 25 files fully (Cargo.lock partial) |
| MFT-RESCUE-FEATURES.md | 152 | MIT | 8 of 8 fully |

**Licensing conclusion (recorded for the matrix):** target is GPL-3.0-or-later. SectorForge (MIT), Halim (MIT), ntfs-recover (0BSD), mft-rescue (MIT) are permissive — still re-implemented from understanding per SOP, never copied verbatim. TestDisk (GPLv2+) and Sleuth Kit (CPL-1.0 core) require strict behavioral re-implementation from documented understanding with citation; no code copying. Both constraints are compatible with the harvest SOP.

**Headline findings (full detail in the sheets):**

1. **The named feature EXISTS in SectorForge and is graded PARTIAL in the target.** SectorForge contract (verified): deleted/live classified at scan time (NTFS MFT flags bit 0 in `crates/sf-core/src/ntfs/record.rs`, FAT 0xE5, exFAT type-bit-7); post-scan emission filter `--deleted-only` skips `in_use` records (`crates/sf-cli/src/recover.rs`); GUI surfaces it twice — a post-scan "Deleted only" checkbox (`gui/src/screens/recover/FileBrowser.tsx`) and a pre-run "Only list deleted files (quicker on very large volumes)" option (`gui/src/screens/recover/FilesTab.tsx`); parent-reference tree with sequence-aware validation (deleted parent allowed at seq+1, cycle guard, `Rooted` vs `Orphaned` classification); extraction recreates the original hierarchy, orphans land in `_orphaned/<name> (id N)`, collisions get ` (id N)` before the extension — pinned by `crates/sf-core/src/ntfs/tests.rs` (`Docs/a (id 32).txt`, `_orphaned/orphan (id 23).txt`).
   Target delta (verified in `src-tauri/src/recovery/jobs.rs`, `src/connector/contract.ts`, `src/stores/filter-store.ts`): engine-side `origins:["deleted"]` filtering and `rel_path_for` deleted-ancestor path reconstruction + `keep_empty_folders` already exist, but **the UI offers no deleted-only post-scan view/option** (only an "Existing" origin toggle), `keep_empty_folders` defaults off with no UI control, and there is no single "export the deleted-only reconstructed tree" surface. This delta is the P0 named-feature work.
2. Target gap list (Agent A, top items): no carving engine (raw-all reserved), no partition-table reconstruction/write-back, no journal replay, no $MFTMirr→$MFT restore, no crash-resume (sessions save only on complete/cancel), no CSV/JSON/report export, `verified:true` hardcoded in the job report, `dw_pick_directory` ignores hint args.
3. TestDisk deep assets for Waves 3–4: 353-format signature registry with 256-bucket dispatch, blockwise carving with paranoid levels and brute-force fragment repair (`src/phbf.c`), GPT backup validation/rewrite, MBR/EBR chain reconstruction, deep-scan candidate ladder, `photorec.ses` checkpoint format.
4. Sleuth Kit deep assets: linear INDX scan for deleted names, deleted-name-from-directory-slack rule, fixup verify/swap, signed-delta runlist decoder, $OrphanFiles pass, gap-filling unallocated volumes, blkls carving input semantics.
5. ntfs-recover (0BSD) supplies the both-$MFT-and-$MFTMirr-broken path: full-disk FILE-magic sweep at record-aligned boundaries, 3-tier record numbering, delta-grouping fragment reconstruction, in-place record-0 patch (`--fix-mirror`).
6. mft-rescue honest finding: $LogFile replay, $UsnJrnl analysis, and MFT slack carving are **absent** from that repo (README overclaims); its real value is the imager (sector-retry salvage, JSON checkpoint/resume) and path/orphan taxonomy.
7. Halim Recovery value: health-scoring rubric (55/35/10 with forced-Red rule), FAT32 deleted-directory harvesting, 8 structural carving validators, safe-writer semantics, natural-language filter.

**Spot-verification by session lead (Grep, all passed):** SectorForge `--deleted-only` arg flow (`crates/sf-cli/src/lib.rs`, `gui/src-tauri/src/lib.rs`), "Deleted only"/"Only list deleted files" strings, `_orphaned/` + `(id N)` outputs in tests; target `rel_path_for` (`jobs.rs:339`), `keep_empty_folders` (`jobs.rs:77,250`), `origins` filter (`contract.ts:285`); ntfs-recover `scan_chunk` (`scanner.rs:314`); Sleuth Kit `seqend_off` (`tsk/fs/ntfs_dent.cpp`); TestDisk `psearchn.c`/`phbf.c`/`sessionp.c` present.

**Next:** T3 — Agent H consolidates the seven sheets into `worklog/FEATURE-MATRIX.md` (the gate document); session lead reviews, corrects, and pushes it. Implementation waves begin only after the matrix is pushed.

---

## 2026-11-12 — T3: FEATURE-MATRIX.md committed as the gate document

**Done this unit:** Agent H consolidated the seven Phase 0 sheets into `worklog/FEATURE-MATRIX.md` (385 lines). The session lead reviewed it in full (Named Feature Contract §1, matrix rows, stats, discrepancy resolution, wave plan, reverse-traceability appendix) before this push.

**Matrix summary:**

- **136 rows** across 11 categories (Partition 12 / FS 21 / Recovery 17 / Carving 12 / Data model 10 / UX 14 / Output 9 / Persistence 9 / Performance 9 / Error 9 / Edge 14).
- Target status: **PRESENT 60 / PARTIAL 18 / MISSING 58 / N-A 1** (EFS half of FS-21 — decryption needs account keys; stays Poor + honest note).
- Priority: **P0 25 / P1 60 / P2 41 / P3 10**.
- Every row carries reference citations carried forward from the sheets; the TARGET column was independently re-verified against the target's code by Agent H (✓V marks); two sheet-vs-code discrepancies found and resolved code-wins (no parent-sequence validation exists anywhere — recorded MISSING/P0; keep-empty default anchor drift).
- Licensing table recorded: TestDisk (GPLv2+) and Sleuth Kit (CPL-1.0 core) demand strict no-code-copy re-implementation; SectorForge/Halim/mft-rescue (MIT) and ntfs-recover (0BSD) permissive — but the session rule is behavioral re-implementation with citation for ALL references regardless.

**Named Feature Contract (§1 of the matrix — the Wave 5 spec):** SectorForge's exact observable contract is pinned with citations (scan-time classification per engine; `--deleted-only` emission filter; "Deleted only" checkbox + "Only list deleted files (quicker on very large volumes)" pre-run option; sequence-aware parent validation where a deleted parent may be exactly seq+1; Rooted/Orphaned ancestry with cycle guards; `_orphaned/<name> (id N)` outputs; ` (id N)` collision suffix before the extension; sanitization incl. reserved names and UTF-16 240-unit cap). The target's current contract is graded and the closing delta is a 10-item list (U-01 deleted-only toggle, U-02 pre-run option, deleted-only tree presentation, R-13 `_orphaned/` bucket, R-14 ` (id N)` collisions, R-15 sanitization, U-05/U-06/U-07 option toggles, one-click "Recover deleted tree", E-09 honest `report.verified`, acceptance tests mirroring SectorForge's pins).

**Gate lifted:** with this push the feature matrix is committed — implementation waves may begin. Wave order and row mapping are in the matrix §6; the P0 set is the implementation priority list.

**Next:** T4 — the seeded validation fixture (deterministic disk image + seed manifest + ground-truth expectations, versioned in-repo), then Wave 1 (pin PRESENT P0 contracts with tests) and Wave 2 onward per the matrix.
