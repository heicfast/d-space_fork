# UI-MAP

Page inventory, transitions, controls, states, and dialogs for the Recovery Wizard UI.
Scope note: this re-implementation covers the **recovery workflow core** (device
selection → scan → results → recovery, plus SD Card, Disk Health, File Repair,
Lost Partition Rescue, Scan Snapshot, and Settings). Corpus surfaces
that are commercial funnel (retention gates, license activation, cross-sell product
pages, AI assistant) are deliberately excluded — see §0.

## 0. Documented divergences from the reference corpus

| Reference surface                                                  | Decision                  | Rationale                                                                                                                                                                           |
| ------------------------------------------------------------------ | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Retention/license/upsell gates                                     | **Excluded**              | Commercial funnel specific to the reference's monetization model; not part of a truthful recovery-tool IA. The connector never exposes them, so the UI cannot honestly render them. |
| AI assistant / AI recommend                                        | **Excluded this session** | Would be a UI-fabricated capability — the Mock Connector contract exposes no ranking model. Roadmap item if the engine ever provides one.                                           |
| Cloud/NAS/WeChat/iCloud targets                                    | **Excluded this session** | Reference chains require network backends; connector exposes local devices + removable media only. Cloud/NAS marked as unavailable-with-reason in the UI (truthful degradation).    |
| Reference sidebar product pages (backup product, crashed PC, etc.) | **Excluded**              | Separate products, not recovery wizard flow. (Our own `/backup` page — Scan Snapshot — resumes saved scan sessions; it is not the reference's backup product.)                      |
| Auto-Recover after scan                                            | **Excluded**              | Requires engine orchestration the contract does not expose.                                                                                                                         |

Everything else follows the corpus IA: 4-zone wizard stack, 3-pane results, scan lives
inside the results page, celebration modal, destination dialog semantics, session
save/load, contextual CTAs.

## 1. Route graph

```
/                      Home (device selection + quick targets)
/sd-card               SD Card / removable-media device selection
/scan/:deviceId/live   Scan Progress + live results (the SAME 3-pane surface;
                       corpus CDetailPage — scan runs inside the results page)
/results/:scanId       Results Explorer (3-pane)
/recover/:scanId       Recovery (destination → queue → report)
/health                Disk Health (SMART telemetry; /tools is a legacy alias)
/repair                File Repair (2-step direct flow; Advanced Repair toggles
                       the structural RawFixer workbench)
/lost-partition        Lost Partition Rescue (2-step direct flow)
/backup                Scan Snapshot (saved recovery checkpoints / sessions)
/settings              Settings (opened from the three-lines header Menu)
```

Router: React Router v7 data mode, `createHashRouter` (ADR-001). There is NO
configuration step between device selection and the scan (ADR-008 — corpus
`scanClickedSlot` flips directly to the scan surface and starts). Every scan
entry (CTA, card buttons, volume links, Recycle Bin / Desktop / Select Folder)
starts the scan directly. Guards: `/scan/:id/live` and `/results/:scanId`
require a live/loaded session via connector `getScanState`/`getSessionMeta` —
otherwise redirect `/` with a fallback prompt. The scan ALWAYS runs
quick → advanced automatically (corpus two-phase model).

Transitions: cross-fade 160ms; scan→results is explicit via celebration modal CTA
(auto-dismiss optional); disconnect error → inline reconnect prompt, not a route change.

## 2. Pages, controls, and states

### 2.1 Home (Device Selection)

Zones: title bar (brand mark; Buy-Now pill and activation-key trigger; standalone
language dropdown; three-lines Menu — Settings, Theme toggle, About; window
controls) / left sidebar (sections: Data Recovery rows — Hardware Disk, SD Card,
Disk Health; Advanced Features — File Repair, Lost Partition Rescue; File Backup —
Scan Snapshot; collapse toggle at the rail foot) / content (page title,
internal-drives group, removable group, quick targets row: Recycle Bin /
Desktop / Choose Folder, primary CTA bottom-right).

Controls: device cards (select on click, blue border + checkmark), capacity bar +
used/total, health hint chip (Good/Caution via connector), lost-partition badge,
refresh devices, session import (file picker → connector loadSession), recent
sessions list (resume → results), per-card Scan button.

States:

- loading: skeleton cards, device enumeration spinner (corpus "early" capture)
- empty (no devices): coaching illustration + refresh CTA + guidance
- error: connector error with reason + retry
- permission-denied: banner asking for elevation, per-device lock icon + reason
- disconnected: card marked offline mid-listing, rescan offered
- success: populated grid, selection, CTA enabled on selection

### 2.2 Scan Progress (live inside results layout — starts DIRECTLY, ADR-008)

There is no scan-configuration surface. Clicking any scan entry starts the
scan immediately; the quick pass runs first and the advanced pass (full
searchers + RAW carving) begins automatically when it completes — the corpus
two-phase model, with no user action in between. When a volume cannot be
quick-indexed the quick pass collapses to a probe and the advanced pass
carries the run (engine truthfulness, not a user choice).

Zones: the 3-pane results layout live during the scan (browse-while-scanning,
corpus GT-scan §3.2); the full-width "Searching: <path>" ticker above the
bottom cluster; bottom status cluster (atlas §6): circular overall gauge
(weighted monotonic), the reference status line ("Quick/Advanced scanning
X%" · "Files found: N (size)" · unreadable-areas badge), the second line
("Remaining time: …" · "Reading sector: X/Y" · used time when complete);
controls pause / resume / stop (confirm dialog with stat tiles); the
"Recover All…" CTA lives in the results bottom bar (enabled only when the
contract marks extents safe; otherwise disabled with a tooltip).

States: running (gauge + strip animate; tree fills progressively), paused (frozen
gauge + red strip), resuming, stopping, completed (celebration modal), cancelled
(partial results offered: "browse what was found" vs home), error-injection
scenarios: bad-sectors (error map + count), device-disconnected (pause + reconnect
prompt, no crash), permission-lost.

### 2.3 Results Explorer

Zones: toolbar (Home/Back/Up cluster, breadcrumb address, view toggle
detail⇄icons, filter button, search box with type scope) / filter bar (toggle row:
Path, File Type, Date Modified, File Size dropdowns + Show toggles + Reset) /
3-pane: left tree (tabs: Path | Type; 5 special branches for raw/deleted groups),
center virtualized list (columns: checkbox, Name, Date Modified, Type, Size;
confidence badge per row sourced from connector; thumbnail async), right preview
pane / bottom status bar (selection summary + contextual CTA Recover N / Recover
All).

Controls: multi-select (click, ctrl+click, shift+range, select-all, invert),
context menu (Recover / Preview / Open location / Select unrecovered / Smart
select best / Clear), sort headers, save session, load session, icon/detail views.

States: loading (skeleton rows), empty-filter ("no matches" + reset CTA),
empty-results (coaching), partial (scan incomplete banner + resume link), error
(connector), disconnected (device gone → results still browsable from session;
recovery requires source reconnection — truthful message), session-corrupt on load
(clear failure, no partial load), success.

### 2.4 Preview (pane)

Kinds: image (bounded decode, zoom/fit/1:1), hex (first 64 KiB + offset nav),
text (size-capped, encoding badge), PDF (page bitmaps from connector, pager),
audio/video (metadata + first-frame thumbnail), office (metadata + extracted
text). Failure = diagnostic reason from connector error code (never generic).

States: loading (bounded worker), ready, unsupported (reason), failed (reason),
partial (truncated notice when bounded).

### 2.5 Recovery (Destination → Queue → Report)

Zones: queue list (per-item state icons, collision-naming policy selector, folder
reconstruction + empty-folder preservation toggles), destination section (volume
rows with free-space bars + same-source warning, folder picker, manual path input
with live validation), progress panel (per-file + overall, cancel, hash-verify
indicator), report dialog (success/partial/failed with reasons, export).

States: validating destination, overlap-refused (hard gate — reason from
`validateDestination`; UI cannot override), insufficient-space (preflight from
connector: needed vs free; Continue-anyway only when connector allows), running,
cancelled, completed-partial (failures listed with reasons), completed-success,
error (destination unavailable).

### 2.6 Health (Disk Health — redesigned v4 surface)

Zones: physical-disk selector cards (3D `Drive3dIcon` with system-drive Windows
badge and a state-dependent BitLocker open/closed lock at the bottom-right,
capacity bar, free/total), circular refresh button, hero health-score card
(circular gauge + status Good/Caution/Bad), KPI tiles (temperature, power-on
hours, class SSD/HDD, TRIM state for SSD), SMART attribute table
(ID / Attribute / Current / Worst / Threshold / Raw / Status), Scan-this-drive
CTA (starts a scan on the selected disk), coaching for
caution.

States: loading, error, not-smart-available (reason), success.

### 2.7 Repair (File Repair — 2-step direct flow, no Start landing screen)

Zones: Step 1 dropzone (header + centered dashed "Add Files" card with the
drag-file illustration), Step 2 queue (top bar: Home + Advanced Repair buttons;
queued file cards with category/size/path and per-item status; sticky bottom
bar: save-path input, folder browse, add, clear, and the Repair All primary
CTA). The structural volume-repair workbench (RawFixer surface: dry-run
findings list, typed/checkbox consent gate with consequence text, apply
progress, undo-log viewer with sector snapshots) opens behind the
Advanced Repair toggle.

States: dropzone, queue (per-file queued / repairing / repaired), advanced
workbench (dry-run loading, dry-run findings (none → nothing-to-do state),
consent required, applying, applied (with undo available), failed (reason), undo
log empty).

### 2.8 Settings & About

Zones: appearance (theme dropdown — system/light/dark/ember/tide/blossom/
high-contrast per ADR-007; UI scale 100/150/200%; density comfortable/compact;
language en/zh-CN), diagnostics (event-log ring buffer + copy), about summary
(version, connector mode badge mock/tauri, engine status fields from connector,
open-source licenses). Settings opens from the three-lines header Menu; the
About menuitem opens `AboutDialog` (title-bar modal displaying connector-sourced
Version, Data source, Recovery engine, Disk driver, and Platform — ESC closes
and focus returns to the Menu trigger; outside click swallowed).

States: persisted via connector-backed settings where the contract exposes them;
localStorage for pure UI prefs (documented split).

## 3. Dialog inventory

| Dialog            | Trigger                 | Semantics                                                                                                                      |
| ----------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Stop-scan confirm | stop button             | stats + Stop/Continue (destructive-first per corpus but ours: Continue default-safe)                                           |
| Celebration       | scan completes          | 520×375 card, 2-column stats grid (Files found \| Size), green-check recoverable callout, primary CTA "Go to Recover" (no ESC) |
| Destination       | recover CTA             | per §2.5                                                                                                                       |
| Space gate        | preflight fail          | needed-vs-free, Continue/Reselect (Continue only if connector allows)                                                          |
| Context menu      | right-click row         | 6 actions per §2.4                                                                                                             |
| Preview modal     | preview action          | large preview surface                                                                                                          |
| Session picker    | import session          | file list from connector                                                                                                       |
| Repair consent    | apply repair            | checkbox + consequence text, disabled until acknowledged                                                                       |
| About             | header Menu → About     | connector-sourced version/engine fields; ESC closes, focus returns to trigger                                                  |
| Activation        | Buy-Now pill / key icon | license key entry + activate CTA; buy link via toast                                                                           |
| Error dialog      | unrecoverable errors    | reason + retry/contact actions                                                                                                 |

> Note (v3 code truth): there is NO close-during-scan confirm dialog — the
> custom-chrome window closes immediately even while a scan runs (no Tauri
> close-requested handler exists). If a guard is ever required, it must be
> added deliberately.

## 4. Cross-cutting UX rules (from corpus interaction rules, adapted)

1. Contextual bottom CTA reflects selection (Recover All ⇄ Recover N).
2. Progressive disclosure: results fill during scan; pause/stop always available.
3. Modal scrims dim main window; no click-through.
4. Single-ESC closes dismissible menus/dialogs only; the completion and
   reconnect modals suppress ESC (non-dismissible by corpus rule); the
   window itself has no close-during-scan guard.
5. Monotonic overall progress; per-stage progress may reset.
6. Every numeric uses tabular figures; every timestamp localized via Intl.
7. Focus visible on every control; full keyboard path documented in TESTING.
8. No dead controls: unavailable = disabled + truthful reason tooltip.
