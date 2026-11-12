<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# DiskWizard — Next Session Roadmap

> **Last updated:** 2026-11-12 — v0.5.0-beta: production hardening +
> multi-filesystem engines (FAT12/16/32, exFAT, ext2/3/4), the
> lost-files crash fixed at five layers, SMART rewritten (NVMe +
> thresholds + USB-bridge fallback), GPT parsing actually working,
> deleted-directory-structure preservation, Quick Search UI. (v0.4's
> legacy developer-comment translation pass completed — the tree is
> fully English except the deliberate UTF-16 decode fixture.)
> **Current version:** `v0.5.0-beta` (0.5.0 — heicfast/d-space_fork
> channel)
> **Target:** `v1.0.0-stable`
> **Test count:** ~230 Rust (incl. 8 multi-FS engine + 6 health tests,
> +9 vss-gated) + 197 frontend + 12 e2e

Open this document first thing in the next session. Flow:

1. `git pull` + verify the latest state
2. Continue with the **next sprint** below
3. When finished, update the "NOW" section of this document

---

## 🟢 NOW: v0.5.0-beta — production hardening + multi-FS engines

**v0.5.0 scope (delivered):**

- **Crash eliminated** (search-for-lost-files): `panic=abort` removed,
  panic hook + catch_unwind on scan/job threads, non-blocking scan and
  recovery starts, cycle-safe deleted-dir sizing, streaming recovery
  writes (4 MiB chunks, incremental blake3), single NTFS parse + bitmap
  per job, Arc-shared deleted map, seq-ordered registry eviction,
  session cluster_size restored.
- **SMART works on real hardware for the first time**: correct
  ATA_PASS_THROUGH_EX + threshold sector, NVMe health log (protocol-
  specific query), predict-failure fallback (USB bridges), temperature
  and TRIM queries.
- **FAT12/16/32 + exFAT engine** (`src/fsfat`, adapted from
  SectorForge/MIT — notice retained): deleted entries with LFN, exFAT
  entry-set checksums, deleted-directory harvesting, allocation-bitmap
  authority, slot re-check at read-back.
- **ext2/3/4 engine** (`src/fsext`, adapted from SectorForge/MIT):
  slack-name recovery, extents + indirect blocks + inline data, group
  block-bitmap grading, inode link-count re-check at read-back.
- **GPT parsing fixed** (was reading LBA 0 — dead code): LBA 1 +
  backup header + CRC-32 of header and entries; FAT classified by
  cluster count; ext detected by superblock magic; lost-partition
  rescue covers NTFS/FAT/exFAT/ext.
- **Recovery fidelity**: deleted-ancestor chain reconstruction
  (original folder structure preserved), keep_empty_folders honored,
  file_repair = intact-prefix publishing.
- **UI**: failure surfacing (toast + banner), working Open-location,
  connector-authoritative overlap verdict, mtime-based newest-copy,
  real sector size + honest wear state, real SD-card data, Quick
  Search page over the USN index, BitLocker-locked volume states.
- Engine-aware sessions (DWSESS02; older sessions fail with a clear
  re-scan message).

**Next sprint (v0.5.x / v1.0 road):**

1. VHDX fixture generator (phase 4.3 — needs a Windows admin
   environment): synthetic NTFS + FAT32 + exFAT fixtures with known
   deleted files, hash-verified recovery in CI.
2. Raw carving engine (`raw-all` tree branch is contract-reserved):
   signature scanner with structural end detection — the
   SectorForge carver (95 formats, MIT) is the reference
   implementation to adapt.
3. NVMe pass-through reads + ReFS detection (honest unsupported
   message for ReFS deleted recovery).
4. ML scorer training path (phase 6.5, `ml-tflite` feature) — only if
   a training dataset materializes; the rules engine stays the
   default.
5. Remaining ~1,200 Turkish developer comments in the legacy core
   (runtime strings are all English).

## ✅ v0.4.0-beta — DiskWizard merge + stabilization complete

**v0.4.0 scope (delivered):**

- Recovery Wizard React UI merged as the product frontend; all
  mock-backend surfaces replaced with the real engine (46 `dw_*`
  commands, 27 wiring tests pinning the bridge).
- The recovery engine (`src-tauri/src/recovery/`): unified MFT walk
  over live + deleted records, confidence grading, server-side result
  queries, bounded previews, verified recovery jobs, persisted
  sessions, SMART health, consent-gated volume repair, lost-partition
  rescue over real disk layouts.
- D-Space feature set ported into the new UI: Size Map, cleanup
  scores, Cleanup Bin (staging/undo/permanent delete), duplicates,
  snapshots, user rules, updater dialog, tray hand-off.
- `requireAdministrator` manifest (UAC prompt at launch).
- Post-merge stabilization: 7 Windows test failures fixed (incl. the
  lenient device-id parser — a genuine engine bug), Tauri crate/npm
  version alignment (2.12.5 / 2.14.1), complete English
  runtime-string pass (30 files, ~280 strings), CHANGELOG and
  standards docs translated.
- CI: all gates green on windows-latest (fmt, clippy -D warnings
  default + vss, 206/215 Rust tests, full frontend gate stack,
  e2e smoke, tauri-debug link check).

**Next work for the coming session:**

1. Manual smoke test on a Windows machine: install (UAC prompt) →
   scan → deleted-file recovery preview → recovery job → staging →
   undo → update check. (The v0.4.0-beta release workflow output is
   verified: MSI + NSIS exe + latest.json + signatures + checksums
   all present and matching.)
2. Phase 4.3 — synthetic NTFS VHDX fixture generator (requires a
   Windows admin environment).
3. Phase 5.4 — EV certificate purchase + signtool + winget/Scoop
   (organizational action).
4. Consider a v0.4.1-beta point release to ship the post-tag
   follow-ups (comment pass, runtime-string stragglers, hardware
   value fixes) documented in the CHANGELOG.

---

## 🟡 v0.4.0 → v0.5.0 road

| Sprint               | Scope                                                                                                                                                                          | Size       |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------- |
| **4.3** Test pyramid | Synthetic NTFS VHDX fixture generator + chapter 20.3 critical test cases (1M+ node treemap render, 100%-full-disk 4-tier fallback, migration rollback). Target 200+ Rust tests | ~500 lines |
| **6.5** ML Tier 2    | TFLite model training (path/ext/size/age → 0-100 score) + real TractMlScorer impl + ml_scores cache validation                                                                 | ~400 lines |
| **Perf tour**        | MFT walker rayon parallel parse + duplicate Criterion benchmark harness (100 GB < 60 s target, chapter 7.4)                                                                    | ~300 lines |
| **NVMe health**      | NVMe SMART/health-log read path (currently ATA pass-through only; NVMe degrades honestly to health-unavailable)                                                                | ~200 lines |
| **webdriver E2E**    | `cargo install tauri-driver` + `pnpm tauri build --debug` + move the webdriver project into the required check                                                                 | ~150 lines |

---

## 🌌 Post-v1.0 (v2.0 — out of scope, next year)

- **Chapter 5.2A layer 3** — MFT Service as a Windows service
- **Chapter 6.5 tier 3** — cloud ML inference (premium, opt-in)
- **Chapter 26.3** — Linux/macOS port (real `CrossPlatformVolumeReader`)
- **Chapter 26.4** — plugin system (WASM sandbox)
- **Chapter 26.5** — cloud backup endpoint (S3-compatible)
- **Chapter 27.4 v1.0+** — third-party pen-test + ISO 27001 prep

---

## 📋 Session-start ritual

```bash
# 1. Pull origin
git pull origin main

# 2. Check CI status
gh run list --limit 3 --workflow ci.yml

# 3. Check tags
git tag -l --sort=-creatordate | head -5

# 4. Open the sprint listed in "NOW" above

# 5. Sprint end:
#    - cargo test --lib + cargo clippy + cargo fmt --check
#    - cargo test --lib --features vss
#    - pnpm test + pnpm build + pnpm e2e
#    - commit + push
#    - update this document
```

---

## 🔑 Key references

- **Master spec:** `D-Space-Mimari-v1.4.docx` (37 chapters, FROZEN)
- **Connector contract:** `docs/CONNECTOR-CONTRACT.md`
- **Discovery Log:** `docs/DISCOVERY_LOG.md`
- **Threat Model:** `docs/THREAT_MODEL.md`
- **Release Checklist:** `docs/RELEASE_CHECKLIST.md`
- **CHANGELOG:** `CHANGELOG.md`
- **REPO_STANDARDS:** `REPO_STANDARDS.md`
- **Repo:** `https://github.com/heicfast/d-space_fork` (release channel)

## 🎯 v1.0 success criteria

- [x] 200+ Rust tests + 150+ frontend tests (now 215 Rust incl. vss +
      197 frontend + 12 e2e)
- [x] All 5 pillars at v1 scope
- [ ] EV-cert-signed MSI + NSIS (external purchase)
- [ ] SmartScreen reputation earned
- [ ] VirusTotal 0/70 false positives
- [x] Auto-updater working (fork channel + new Ed25519 key)
- [ ] Published on winget + Scoop
- [x] Detailed CHANGELOG per release
- [x] Discovery Log alive
- [x] Threat Model current

---

## 📜 Session history

### 2026-11-12 session (v0.4.0-beta — DiskWizard merge)

- Recovery Wizard React UI integrated as the DiskWizard frontend;
  mock backend eradicated from production; real IPC connector
  default; D-Space cleanup features re-implemented in the new design.
- Recovery engine built (collect/deleted/results/preview/jobs/
  sessions/health/repair/partitions/scan + commands).
- Admin manifest, English-first product, 197 frontend tests, 27
  wiring tests, 12 e2e; CI/UI-Smoke pipelines merged and green.
- Post-merge: 7 Windows test failures fixed, Tauri versions aligned,
  full English runtime-string pass, docs translated.
- Post-tag follow-ups: updater endpoint repair (betas publish as
  Latest), legacy developer-comment translation (~1,400 lines, 43
  files, comment-only verified by a Rust-aware lexer diff), 30
  runtime-string stragglers fixed, stale .github/REPO_STANDARDS.md
  removed; v0.4.0-beta release artifacts verified (checksums match,
  3-platform signed latest.json feed serves from releases/latest).

### 2026-11-07 session (v0.3.0-beta — review wave)

- Five independent line-by-line code reviews (~90 findings) — all
  resolved (details: CHANGELOG v0.3.0-beta).
- USN watcher to production; `index_build` incremental via watermark;
  release chain repaired (updater artifacts, conditional prerelease,
  new Ed25519 key); 4 broken e2e tests fixed; `rust-vss` CI job.

### 2026-05-17 session (v0.2.0-beta tag + phases 4/5 partial)

- v0.2.0-beta tagged, MSI version fix (0.2.0-beta → 0.2.0).
- Phase 4.1 duplicates v0.2 — head-hash prefilter + rayon parallel
  (+5 tests).
- Phase 4.2 snapshot retention — 90-day purge + auto-cleanup
  (+3 tests).
- Phase 4.3 partial — pipeline + skip-prefilter tests (+2).
- Phase 4.4 partial — UNC path support in the find_first walker
  (+4 tests).
- Phase 4.5 LOD + viz transitions; 4.6 ReFS bypass; 5.1 tray live
  monitor; 5.2 onboarding comparison table; 5.3 empty-state
  animation.
