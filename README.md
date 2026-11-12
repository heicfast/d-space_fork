# DiskWizard

**Windows data recovery wizard and safe disk cleanup.** DiskWizard finds lost and deleted files with a raw NTFS engine, recovers them with verified copies, visualizes disk usage, and cleans up with a guaranteed undo — fully local, no telemetry, no cloud.

![CI](https://github.com/heicfast/d-space_fork/actions/workflows/ci.yml/badge.svg)
![Release](https://github.com/heicfast/d-space_fork/actions/workflows/release.yml/badge.svg)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11%20x64-0078D4)
![Stack](https://img.shields.io/badge/Stack-Tauri%202%20%C2%B7%20Rust%20%C2%B7%20React%2019%20%C2%B7%20TypeScript-24C8DB)

---

## What DiskWizard does

| Area                | Capabilities                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Data recovery**   | Whole-volume scans (raw MFT for NTFS; dedicated engines for FAT12/16/32, exFAT and ext2/3/4 — USB sticks, SD cards, Linux partitions; directory walk for ReFS/network shares), deleted-file discovery with allocation-based confidence grading (excellent / good / fair / poor), lost-path and deleted-folder-structure reconstruction, preview (image / text / hex), verified streaming recovery to any destination with collision policies |
| **Lost partitions** | GPT/MBR layout parsing (CRC-validated, backup-header fallback), unallocated-space boot-signature probing for NTFS/FAT/exFAT/ext, and in-place scanning of lost volumes through an offset reader                                                                                                                                                                                                                                              |
| **Disk health**     | SMART attribute tables with thresholds (ATA pass-through for SATA, native NVMe health log, predict-failure fallback for USB bridges), temperature / power-on hours / wear / TRIM, verdicts (good / caution / bad)                                                                                                                                                                                                                            |
| **Volume repair**   | Read-only structure checks (NTFS boot vs backup boot, $MFT vs $MFTMirr), consent-gated fixes with an undo log — non-system volumes only                                                                                                                                                                                                                                                                                                      |
| **Space cleanup**   | Size Map treemap, 0–100 safe-cleanup scores (63 built-in rules + user rules), staging with a 24-hour restore window, typed-phrase permanent delete with DoD 5220.22-M wipe, duplicate finder (BLAKE3), size snapshots with deltas                                                                                                                                                                                                            |
| **Everyday**        | Quick Search (Everything-style name finder over the live USN index), system tray with full-disk alerts, English/简体中文 UI, auto-updates with Ed25519-signed artifacts                                                                                                                                                                                                                                                                      |

Everything runs on your machine. The engine reads volumes through raw handles (`\\.\X:`), which is why the app requests administrator privileges at launch — deleted-file recovery, the MFT engine, VSS and volume repair all need elevation.

## Install

Download the latest release: **[MSI](https://github.com/heicfast/d-space_fork/releases/latest)** (WiX) or **`-setup.exe`** (NSIS). Windows 10 (1809+) / 11, x64.

> Builds are not yet code-signed (EV certificate is on the v1.0 roadmap) — SmartScreen may warn on first run. Verify the SHA-256 checksums attached to each release before running.

The auto-updater checks GitHub releases and installs signed, passive updates (`latest.json` feed, Ed25519/minisign signatures).

## The five pillars (inherited from D-Space, engine-complete)

1. **Speed** — raw NTFS master-file-table scans: the metadata for a 1 TB volume in seconds (admin required); raw FAT/exFAT engines for memory cards and thumb drives; automatic directory-walk fallback for ReFS/network shares.
2. **Intelligence** — a 0–100 safe-cleanup score per file (63 built-in rules: dev caches, browser caches, temp files, protected system/cloud/VM items) plus your own name/extension rules, evaluated before every scan's scoring pass.
3. **Visual** — the Size Map treemap with drill-down, three result views (details / icons / map), server-side filtering, virtualized lists over millions of entries.
4. **Time** — size snapshots with four-bucket deltas (added / removed / grew / shrunk) so you can see what changed between two points in time.
5. **Recovery guarantee** — nothing is ever deleted directly: files move to a staging area (atomic rename on the same volume, verified copy across volumes), restorable for 24 hours, with a typed-phrase gate and forensic ledger before anything permanent happens.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│ React 19 + TypeScript UI (connector-isolated, EN/zh-CN)      │
│    src/ — features, queries, stores, connector contract      │
├────────────── src/connector/tauri/tauri-connector.ts ────────┤
│            10+1 domains · typed errors · event streams       │
├─────────────────────────── IPC (46 dw_* commands) ───────────┤
│ Rust engine (src-tauri)                                      │
│   recovery/  scan orchestration · deleted-file engine ·      │
│              results queries · previews · recovery jobs ·    │
│              sessions · SMART · repair · partition layouts   │
│   fsfat/    FAT12/16/32 + exFAT deleted-file engine         │
│   fsext/    ext2/3/4 deleted-file engine (slack names)      │
│   scan/      MFT walk · directory walk · tree building       │
│   staging/   safe delete · undo · WAL · DoD wipe             │
│   index/     USN journal baseline/delta/watcher (quick search)│
│   duplicate/ 4-stage BLAKE3 pipeline · snapshot deltas · …   │
└──────────────────────────────────────────────────────────────┘
```

- **The UI never talks to the engine directly** — every interaction goes through the connector contract (`src/connector/contract.ts`, human-readable spec in [`docs/CONNECTOR-CONTRACT.md`](docs/CONNECTOR-CONTRACT.md)). The deterministic mock connector exists only for tests and browser development (`VITE_CONNECTOR=mock`); production builds always use the real bridge.
- **The tree never leaves the engine.** The UI queries windowed slices (filters, sorting and pagination run server-side in Rust); scans stream weighted-monotonic progress events with pause/resume/cancel.
- **Scans persist.** Completed (and cancelled) scans auto-save as compact `.dws` sessions — reopen them from the Scan Snapshot page without rescanning.

## Building from source

Prerequisites: Rust (stable), Node.js ≥ 24, pnpm 12.

```bash
pnpm install
pnpm tauri dev        # run the desktop app (UAC prompt: engine needs admin)
pnpm test             # 197 unit tests (mock connector)
pnpm e2e              # 12 Playwright smoke tests
pnpm build            # typecheck-safe production build (real connector)

cd src-tauri
cargo test --lib              # 170+ engine tests (Windows recommended)
cargo clippy --lib -- -D warnings
```

The complete gate set that CI enforces: `typecheck · lint · format · arch-lint · ast-grep rules · knip · jscpd · coverage thresholds (70/60/70/70) · build · bundle budget (200/500 KB gzip) · cargo fmt · cargo clippy (default + vss feature) · cargo test (default + vss) · Playwright e2e · tauri debug link`.

## Documentation

- [`docs/CONNECTOR-CONTRACT.md`](docs/CONNECTOR-CONTRACT.md) — the UI⇄engine contract (principles, domains, wire shapes)
- [`docs/NEXT_SESSION.md`](docs/NEXT_SESSION.md) — roadmap state and the next session's queue
- [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) — assets, actors, surfaces
- [`docs/RELEASE_CHECKLIST.md`](docs/RELEASE_CHECKLIST.md) — signing, updater keys, pre-release gates
- [`docs/DISCOVERY_LOG.md`](docs/DISCOVERY_LOG.md) — spec-divergence ledger
- [`docs/ui/`](docs/ui/) — the UI's architecture decisions (ADRs), design system, screen maps and session history
- `D-Space-Mimari-v1.4.docx` — the frozen original architecture specification (Turkish)

## History and licensing

DiskWizard continues the D-Space engine (v0.3.0-beta) with the Recovery Wizard UI (v0.3.4, clean-room React implementation) merged on top as its frontend. Both lines live on in this repository's history; see [`CHANGELOG.md`](CHANGELOG.md).

Licensed under **GPL-3.0-or-later**. Every source file carries an SPDX header.
