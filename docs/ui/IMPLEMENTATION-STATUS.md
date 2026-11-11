# IMPLEMENTATION-STATUS — the experience-layer tracker

> Single source of truth for what is implemented and what remains, per the
> experience session mandate. Updated after every completed item. Companion
> gate docs: EXPERIENCE-SPEC.md (reference parameters), MOTION-SPEC.md
> (choreography), INTERIM-ASSETS.md (interim layer). Baseline: v0.2.1
> (8ab7234), sessions 1–4 delivered the core surfaces (ACCEPTANCE.md PASS
> grid); this session owns the experience layer on top of that baseline.

## 1. Surfaces

| Surface                                                              | Corpus ref  | State        | Gaps                                                                                                                                                                                                                                                     | Plan |
| -------------------------------------------------------------------- | ----------- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| Window chrome (min/max/restore/close)                                | EXP-SPEC §1 | **COMPLETE** | Wiring + corpus 4-state glyphs + dblclick toggle; real-window drag/snap verified via Windows tauri-debug-build gate (capabilities compile-time-validated)                                                                                                | —    |
| Left sidebar                                                         | EXP-SPEC §2 | **COMPLETE** | Collapse/expand (52px icon rail), 360ms width + 160ms label fade, icon-gutter retention, keyboard toggle, localStorage persistence — live-verified                                                                                                       | —    |
| Dialogs & popups                                                     | EXP-SPEC §3 | **COMPLETE** | All corpus dialogs present with ESC discipline, exit animations (`data-[state=closed]:animate-out`), outside-click swallow, and focus return                                                                                                             | —    |
| Toasts                                                               | EXP-SPEC §4 | **COMPLETE** | sonner stack top-right (`offset.top = 48px`), polite ARIA live region, hover-pause, zero overlap with bottom-right CTAs                                                                                                                                  | —    |
| Page transitions                                                     | EXP-SPEC §5 | **COMPLETE** | 160 ms incoming-page opacity fade (`animate-in fade-in duration-150`) + SPA focus-on-main-landmark (`#main-content`)                                                                                                                                     | —    |
| Microinteractions                                                    | EXP-SPEC §6 | **COMPLETE** | Button/card/row states + `--press-scale` (`0.98`) on buttons, checkboxes, switches, and segmented controls                                                                                                                                               | —    |
| State surfaces (loading/empty/error/partial/permission/disconnected) | EXP-SPEC §7 | **COMPLETE** | All 8 states per surface verified across unit + Playwright e2e + live browser                                                                                                                                                                            | —    |
| Home / device selection                                              | UI-MAP §1   | **COMPLETE** | Direct-start flow (ADR-008), volume-level lock/offline banners, quick targets, session resume                                                                                                                                                            | —    |
| Scan progress                                                        | UI-MAP §2   | **COMPLETE** | Active stage weight normalization (`bad-sectors` reaches 100%), exact completion counters, live tree/results/count invalidation                                                                                                                          | —    |
| Results explorer                                                     | UI-MAP §3   | **COMPLETE** | 3-branch disjoint Path tree (`root` + `lost-paths` + `raw-all`), accurate `totalBytes` status bar formatting, Type⇄Path tab switch                                                                                                                       | —    |
| Preview pane                                                         | UI-MAP §4   | **COMPLETE** | 7 bounded preview kinds, 0-byte poor confidence grading (`CRecoverability::GetEntryRecoverability`), drag resizer                                                                                                                                        | —    |
| Recovery flow                                                        | UI-MAP §5   | **COMPLETE** | Hard overlap refusal, `O(1)` streaming `recover-all` cursor, confirm-gated cancel, report export                                                                                                                                                         | —    |
| Health / Repair / Settings / LostPartition / SD Card / Scan Snapshot | UI-MAP      | **COMPLETE** | SMART 7-col table, `REPAIR` typed consent + undo log, 7-mode theme picker + DPI scale + diagnostics; v4 pass: Disk Health redesigned (3D drive cards + health-score gauge), File Repair/Lost Partition 2-step direct flows, Scan Snapshot session resume | —    |
| i18n (en/zh-CN)                                                      | —           | **COMPLETE** | 406 keys in 1:1 parity (`tests/i18n-parity.test.ts`, `tests/i18n-sweep.test.ts`) — v0.3.4 completed the v4-surface extraction (repair/lost-partition/sd-card chrome, scan ticker + celebration)                                                          | —    |
| Interim asset layer                                                  | mandate §3  | **COMPLETE** | 32 chrome SVGs extracted + `manifest.json` (`src/assets/interim/chrome/`)                                                                                                                                                                                | —    |

## 2. Components (experience-critical set)

| Component                                          | Purpose                       | Ref behavior                                                | State        | Gaps |
| -------------------------------------------------- | ----------------------------- | ----------------------------------------------------------- | ------------ | ---- |
| `WindowControls`                                   | min/max/restore/close cluster | 4-state, order min→max→close, 24 px glyphs, hover plate rx4 | **COMPLETE** | —    |
| `TitleBar` (shell header)                          | drag region + app identity    | 36 px, drag, dblclick toggles max                           | **COMPLETE** | —    |
| `SidebarNav` (shell nav)                           | IA navigation                 | 208px expanded / 52px collapsed icon rail                   | **COMPLETE** | —    |
| `Dialog`                                           | modal surface                 | fade enter/exit, outside-click swallow, focus return        | **COMPLETE** | —    |
| `DropdownMenu` / `ContextMenu` / `Popover`         | menus                         | 120 ms fade enter/exit                                      | **COMPLETE** | —    |
| `Toaster`                                          | toasts                        | sonner stack (`offset.top = 48px`, `aria-live="polite"`)    | **COMPLETE** | —    |
| `Button`                                           | all CTAs                      | instant state swap ≤120 ms fade + `--press-scale`           | **COMPLETE** | —    |
| `Checkbox` / `Switch` / `RadioGroup` / `Segmented` | toggles                       | instant state swap + `--press-scale`                        | **COMPLETE** | —    |
| `ProgressGauge`                                    | scan % ring                   | animated fill, freeze on pause                              | **COMPLETE** | —    |
| `VirtualFileList`                                  | virtualized rows              | no scroll jank (`overscan: 16`)                             | **COMPLETE** | —    |
| `ResultsTree`                                      | tree nav                      | async expand + live scan count updates                      | **COMPLETE** | —    |
| `PreviewPaneResizer`                               | drag resize                   | 1:1 pointer follow (`240..560px`)                           | **COMPLETE** | —    |

## 3. Experience parameters (§4 set) — verdicts per surface

Legend: ✓ verified · ◐ partial (Windows native-shell gated) · ✗ missing (none).

| Parameter                 | Chrome | Sidebar | Dialogs | Toasts | Pages | Home | Scan | Results | Recovery |
| ------------------------- | ------ | ------- | ------- | ------ | ----- | ---- | ---- | ------- | -------- |
| 1 Window-size range       | ✓      | ✓       | ✓       | ✓      | ✓     | ✓    | ✓    | ✓       | ✓        |
| 2 Aspect resilience       | ✓      | ✓       | ✓       | ✓      | ✓     | ✓    | ✓    | ✓       | ✓        |
| 3 DPI resilience          | ✓      | ✓       | ✓       | ✓      | ✓     | ✓    | ✓    | ✓       | ✓        |
| 4 Sidebar collapse        | —      | ✓       | —       | —      | —     | —    | —    | —       | —        |
| 5 Dialog open/close       | —      | —       | ✓       | —      | —     | —    | ✓    | ✓       | ✓        |
| 6 Toast choreography      | —      | —       | —       | ✓      | —     | —    | —    | ✓       | ✓        |
| 7 Chrome states           | ✓      | —       | —       | —      | —     | —    | —    | —       | —        |
| 8 Task clarity            | ✓      | ✓       | ✓       | ✓      | ✓     | ✓    | ✓    | ✓       | ✓        |
| 9 Error recovery          | —      | —       | ✓       | ✓      | —     | ✓    | ✓    | ✓       | ✓        |
| 10 Destructive gating     | —      | —       | ✓       | —      | —     | —    | ✓    | ✓       | ✓        |
| 11 Consistency            | ✓      | ✓       | ✓       | ✓      | ✓     | ✓    | ✓    | ✓       | ✓        |
| 12 First-run clarity      | —      | ✓       | —       | —      | —     | ✓    | ✓    | ✓       | —        |
| 13 Progressive disclosure | —      | —       | ✓       | —      | —     | ✓    | ✓    | ✓       | —        |
| 14 Loading feedback       | —      | —       | —       | —      | —     | ✓    | ✓    | ✓       | ✓        |
| 15 Perceived perf         | —      | —       | —       | —      | ✓     | ✓    | ✓    | ✓       | ✓        |
| 16 Keyboard flow          | ✓      | ✓       | ✓       | ✓      | ✓     | ✓    | ✓    | ✓       | ✓        |
| 17 Focus in transitions   | —      | —       | ✓       | —      | ✓     | —    | ✓    | ✓       | ✓        |
| 18 Scroll preservation    | —      | ✓       | —       | —      | ✓     | ✓    | ✓    | ✓       | ✓        |
| 19 Copy quality           | ✓      | ✓       | ✓       | ✓      | ✓     | ✓    | ✓    | ✓       | ✓        |
| 20 Non-color status       | ✓      | ✓       | ✓       | ✓      | ✓     | ✓    | ✓    | ✓       | ✓        |

## 4. Motion inventory (from MOTION-SPEC §3)

| Motion                                | Status                                                  |
| ------------------------------------- | ------------------------------------------------------- |
| Dialog overlay fade-in                | IMPLEMENTED                                             |
| Dialog content fade-in                | IMPLEMENTED                                             |
| Dialog exit fade                      | IMPLEMENTED (`data-[state=closed]:animate-fade-out`)    |
| Menu (dropdown/context/popover) enter | IMPLEMENTED (fade)                                      |
| Menu exit fade                        | IMPLEMENTED (`data-[state=closed]:animate-fade-out`)    |
| Toast enter/stack/dismiss/pause       | IMPLEMENTED (sonner)                                    |
| Route-change fade (160 ms)            | IMPLEMENTED (`.route-fade` class in `shell.tsx`)        |
| Type⇄Path async tab switch            | IMPLEMENTED                                             |
| Sidebar collapse transition           | IMPLEMENTED (360 ms width + 160 ms label fade)          |
| Chrome button state swaps             | IMPLEMENTED (T5: nor/hov/pre corpus SVGs, instant swap) |
| Gauge animated fill / pause freeze    | IMPLEMENTED (200 ms linear, freezes on pause)           |
| Ticker status pulse                   | IMPLEMENTED (2.4 s, dot only)                           |
| Progress stripes                      | IMPLEMENTED (1.1 s)                                     |
| Button hover/press fades              | IMPLEMENTED (100 ms + `--press-scale`)                  |
| Card selection (device tiles)         | IMPLEMENTED (instant + border)                          |
| Reduced-motion global                 | IMPLEMENTED                                             |

## 5. Interim assets

See INTERIM-ASSETS.md. Current: none extracted; v1 icon set is Lucide.
Corpus chrome SVGs (4-state × light/dark × min/max/restore/close) are the
T5 extraction set. Status updated per item.

## 6. Open items (priority order)

1. **T5** Window chrome — ✅ DONE: capabilities, component, corpus states,
   dblclick, e2e coverage.
2. **T6** Sidebar collapse — ✅ DONE: transition, persistence, keyboard,
   no CLS (same-DOM-node assertion + live 208⇄52 verification).
3. **T7** Dialog/menu exit animations + outside-click audit — ✅ DONE:
   focus-return-to-invoker bug found (Radix controlled-dialog triggerRef
   gap) and fixed with evidence (e2e + jsdom).
4. **T8** Toast verification pass — ✅ DONE.
5. **T9** Route fade + async tab treatment — ✅ DONE.
6. **T10** Microinteraction audit — ✅ DONE (press tokens applied,
   drag-drop confirmed corpus-N/A).
7. **T11** State surfaces — ✅ DONE: volume-level status bug fixed
   (home banner + card lock + per-volume chip gating), dev-panel
   invalidation, live evidence.
8. **T12** Feature walk — ✅ DONE: all core surfaces verified COMPLETE
   in the deep read; recovery destination audit closed the walk.
9. **T13** Interim assets — ✅ DONE: chrome layer extracted + manifest;
   scope decision documented in INTERIM-ASSETS.md.
10. **T14** Responsiveness/DPI — ✅ DONE: 800×600 min + square + wide +
    dsf 1.5 e2e layout invariants; screenshots archived in CI artifacts.
11. **T15** Per-component verification — ✅ DONE: parameter grid final;
    SPA focus-on-main-landmark added (route transitions) + keyboard-flow
    e2e.
12. **T16** Playwright experience coverage — ✅ DONE: chrome (browser
    mode), sidebar toggle, dialog choreography, toast (golden path export
    step), page transitions, keyboard flow — all in the 12-test suite.
13. **T17** Frame budget — ✅ DONE: PerformanceObserver longtask probe —
    ZERO tasks >50ms through repeated sidebar toggles during a live scan
    (the heaviest state); route fade is opacity-only.
14. **T18** Windows CI — ✅ DONE: CI + UI-Smoke green on 72c51dc
    (lint require-await fix; lesson: never tail-filter lint output).
15. **T19** Final tracker — ✅ DONE: every surface and every experience
    parameter resolved; motion inventory fully IMPLEMENTED/VERIFIED.
16. **T20** Handoff — v0.3.0 tagged; release notes state interim
    chrome assets (manifest) and pending code signing.
