# ROADMAP

## v0.3.4 — gate/docs/i18n hygiene — DONE (pending tag)

- CI push triggers fixed to the v3 default branch (push CI previously
  never fired on the renamed default branch).
- knip stale ignore removed (0 findings, 0 hints); VirtualFileList
  compiler skip made explicit per ADR-002 (lint now 0 warnings).
- Clean-room/packaging: 11 unused reference-product screenshots removed
  from `public/` (they shipped in every NSIS installer); AI-Studio
  scaffold remnants (`.env.example`, `metadata.json`) removed.
- i18n completion: all v4-surface chrome strings extracted into the
  governed en/zh-CN catalogs (406 keys, en text byte-identical).
- Docs truth pass: UI-MAP/screen-manifest/IMPLEMENTATION-STATUS/
  component-inventory/ARCHITECTURE synced to code; CONNECTOR-CONTRACT
  updated to post-ADR-008 signatures; historical session docs carry dated
  addenda instead of rewrites.
- No features, no dependency changes, no behavior changes.

## v4 UI pass (merged at v0.3.3) — DONE

- Settings / Theme / About consolidated into the three-lines header Menu
  (trigger label from `menu.label`); language moved to its own standalone
  header dropdown; the standalone header About button is gone.
- More Recovery Tools page deleted; its spot is the redesigned Disk Health
  page (`/health`): 3D drive cards, health-score gauge, SMART table.
- ChronoSnap Backup renamed **Scan Snapshot** (`/backup`).
- File Repair & Lost Partition Rescue now skip their Start landing screens —
  2-step direct flows (`/repair`, `/lost-partition`).
- 3D target icons introduced (`Drive3dIcon` with state-dependent BitLocker
  open/closed lock, `RecycleBin3dIcon`); Buy-Now pill + activation key icon
  remain in the custom title bar.

## Session 4 (audit & completion) — DONE

- Theme adoption from the designated source pair (ADR-007): verbatim adopted
  layer, generated semantic mapping with drift-failing tests, 5 palettes +
  derived HC, measured text-grade accents.
- Lost-data flow corrected to the corpus behavior (ADR-008): direct start,
  quick → advanced automatic, corpus status lines, real recover-all,
  connector-owned preview kinds.
- Slop purge + dedupe (jscpd 0 clones), dead deps removed, architecture
  enforcement kept green.
- Localization completed: en/zh surface-identical AND exactly the
  referenced surface (i18n-sweep governance test: missing = 0, unused = 0).
- Accessibility: live axe audit — 0 violations across every palette + HC +
  scan/results surfaces (evidence in docs/evidence/a11y/).
- Full 63/63 VLM extraction committed as evidence.
- Acceptance grid filled per surface (docs/ACCEPTANCE.md).

## v0.1.0-ui-mock — DONE (sessions 1–3)

- Research gate: screenshot manifest, visual spec, UI map, design system.
- Full Tauri 2 + React 19 + TS strict + Vite 8 scaffold, CI on windows.
- Mock Connector: 9 domains, deterministic fixtures (1.2 M-node index),
  timing budgets, error injection, dev panel.
- All 9 wizard surfaces with loading/empty/error/partial states.
- Virtualized results (tree + list), bounded preview workers, recovery queue
  with hard overlap refusal, SMART + transactional repair, sessions.
- 52 tests green, Playwright golden path, browser-verified with evidence.

## Next: engine integration session

1. **Wire the real connector** (`src/connector/tauri/`): implement the
   contract over `invoke()` + `emit`/`listen`; delete the throwing stub.
   Map the scan engine's staged progress to `ScanProgressEvent` (weighted,
   monotonic) and its IOCTL-backed enumeration to the Devices domain.
2. **Engine command surface** (Rust): thin commands mirroring the contract
   (start/pause/resume/cancel, tree/page queries with server-side filters,
   preview reads with bounds, session save/load to real `.rsf`-equivalents,
   recovery job orchestration, SMART passthrough, RawFixer dry-run/apply/undo).
3. **Real validation**: replace the JS-side `validateDestination` with
   engine volume checks (same-letter + raw volume open semantics).
4. **Measured budgets on real hardware**: re-run grid #34–44 (1 M rows,
   filter <200 ms, preview <500 ms, cold start <2 s, DPI ladders) against
   the engine, not the simulator.

## v0.2 candidates

- Icon set v2: purpose-drawn two-tone filetype/device icons (see
  INTERIM-ASSETS.md).
- AV bounded playback behind a stability flag.
- Additional locales beyond en/zh-CN (infrastructure already locale-safe).
- Cloud/NAS targets — only when the engine exposes them in the contract
  (UI stays truthful until then).
- RHF→stable-8 / React Router 8 / Vitest 5 upgrades (ADR-005 notes the
  mapping).
- Per-route automated axe audit in CI.

## Standing decisions to keep

- No monetization/retention surfaces (documented divergence, UI-MAP §0).
- Preview rendering boundary per ADR-003 (engine renders document pages).
- The UI never re-grades confidence or health — connector data is final.
