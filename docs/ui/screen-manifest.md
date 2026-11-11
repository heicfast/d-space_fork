# Screen & Flow Manifest — Corpus Ground Truth

> **Corpus source of truth**: `heicfast/easeus-drw-research` (`docs/07-user-flow-uiux.md`, `artifacts/UIUX/01..35`, `artifacts/UIUX/dynamic-ground-truth--*.md`, `artifacts/DRWUI-exe--main-ui-architecture.md`, `artifacts/DRWUI-subsystems--assistant-grading-preview.md`, `artifacts/CORRECTIONS.md`, `screenshots/01..57`).
> **Application mapping**: Cross-referenced with `docs/UI-MAP.md` and `docs/adr/008-lost-data-flow.md`.

---

## 1. Top-Level Shell (`FramelessDialog` → `DrwMainWindow`)

| Region               | Corpus Widget / Class                                     | Geometry (1040×680 base / 1240×840 desktop)                                                     | App Component & Route                                      |
| -------------------- | --------------------------------------------------------- | ----------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Custom Title Bar     | `TitleBarWidget`                                          | Height `36px` (`--titlebar-h`), full width, draggable (`data-tauri-drag-region`)                | `WizardShell` `<header>` (`src/app/shell.tsx`)             |
| Window Controls      | `btn_minimize`, `btn_maximize`/`btn_restore`, `btn_close` | 24×24 SVGs inside 32×32 hit boxes; 4-state (`nor`/`hov`/`pre`/`dis`); hidden in browser runtime | `WindowControls` (`src/components/ui/window-controls.tsx`) |
| Left Navigation Rail | `LeftTabWidget`                                           | Width `208px` (`--sidebar-w`) expanded, `52px` (`--sidebar-w-collapsed`) collapsed              | `<aside><nav id="sidebar-nav">` (`src/app/shell.tsx`)      |
| Main Content Stack   | `QStackedWidget` (`DrwDevicePage` ⇄ `DrwScanPage`)        | `flex-1 min-w-0 min-h-0`, route-fade opacity transition (`120ms`)                               | `<main id="main-content">` (`src/app/shell.tsx`)           |

---

## 2. User-Facing Surfaces & State Matrices

### S1. Home / Device Selection (`/` and `/?media=removable`)

- **Corpus Reference**: `DrwDevicePage` / `DrwComputerPage` (`screenshots/01_main_window_home.png`, `02_main_window_hovered.png`, `03_main_window_scrolled.png`, `49_Select_Folder_popup_overlay.png`, `artifacts/UIUX/dynamic-ground-truth--landing-page.md`, `35-redesign-reference-atlas.md` §5).
- **Regions**:
  1. Header row: Page title (`home.title`), subtitle (`home.subtitle`), and manual refresh trigger (`home.refreshDevices`).
  2. Alert banners (when reported by device or volume): Permission-denied warning banner (`home.permissionDenied`), Disconnected error banner (`home.disconnected`).
  3. Drive grid (`role="radiogroup"`): Internal drives (`/`) or Removable media (`/?media=removable`), 2-column responsive card grid (`DeviceCard`) with 48×48 icon container, label, model, lost-partition badge, capacity bar (`CapacityBar`), health status indicator (`Healthy` / `Needs attention`), per-volume quick-scan pills, and secondary scan button.
  4. Quick location row (`Specify a location` / `home.quickTargets`): 3 tiles — `Recycle Bin` (`vol-recycle-bin`), `Desktop` (`vol-desktop`), `Choose a folder…` (invokes native directory picker then scans chosen path scope).
  5. Recent searches (`home.recentSessions`): Saved `.rsf`-equivalent scan sessions with timestamp, file count, and `Open results` action.
  6. Sticky bottom CTA bar: Selected target summary + primary `Search for lost data` button (`size="lg"`).
- **Transitions**:
  - Clicking `Search for lost data`, a volume pill, `Recycle Bin`, `Desktop`, or confirming `Choose a folder…` invokes `connector.scan.startScan({ targetId, pathScope })` and navigates **directly** to `/scan/:volumeId/live?scanId=<id>` (ADR-008; matches `scanClickedSlot` in `DRWUI.exe`).
- **States**: Initial loading (`Skeleton` grid), Populated, Empty (`EmptyState` with refresh action), Error (`EmptyState` with retry + refresh), Permission-denied (device or volume level), Disconnected (device or volume level).

### S2. Lost Partition Rescue (`/lost-partition`)

- **Corpus Reference**: `DrwLostDiskPage` (`artifacts/UIUX/35-redesign-reference-atlas.md` §5, `docs/07-user-flow-uiux.md` §1.1).
- **v4 flow**: 2-step direct flow — no Start landing screen.
  1. Step 1 (`select-disk`): "Choose a disk to start searching for lost partition(s)" — disk cards (Disk 2 / Disk 0 / Disk 1) with segmented partition bars and BitLocker lock icons, a `Go to Hardware Disk` link, and the `Search for Lost Partition` CTA.
  2. Step 2 (`scan-completed`): "Smart Partition Scan Completed" — found-partitions table with the "Double click the partition to explore the content." banner, a `Preview for disk 2` bar, and a `Try Data Recovery` CTA.
- **States**: Loading (`Skeleton`), Populated, Empty (`EmptyState`), Error (`EmptyState`).

### S3. Live Scan & Browse-While-Scanning (`/scan/:volumeId/live?scanId=<id>`)

- **Corpus Reference**: `DrwScanPage` (`screenshots/34_scanning_in_progress.png`, `35_scan_completed_overview.png`, `45_Scan_Pause_Stop_Hover.png`, `artifacts/UIUX/dynamic-ground-truth--scan-phase-captured.md`, `30-interaction-driver-ground-truth.md`).
- **Architecture**: The reference does not use a separate scan-only screen; `DrwScanPage` hosts the 3-pane results explorer live while the bottom status bar tracks scan progress (`ADR-008`).
- **Automatic Stage Pipeline**:
  - `volume-analysis` (`Quick scanning`) → `quick-index` (`Quick scanning`, skipped when `volume.quickScanAvailable === false`) → `deep-walk` (`Advanced scanning`) → `carve` (`Advanced scanning`) → `tree-build` (`Advanced scanning`) → `done` (`Search complete`).
  - Transitions from Quick to Advanced automatically without user intervention (`CORRECTIONS.md` #1).
- **Regions**:
  1. Live `ResultsExplorer` (`liveMode={!completed}`, `recoverableDuringScan`).
  2. Full-width `Searching: <path>` ticker strip (`bg-[#BBD5FF]` light / `bg-primary-muted` dark, plain truncated path text) visible during `deep-walk` and `carve`.
  3. Bottom status bar: 52px circular `ProgressGauge`, primary status line (`Quick scanning X%` / `Advanced scanning X%` / `Search paused` / `Search complete` + `Files found: N (size)` + bad-sector badge when `errors > 0`), secondary telemetry line (`Remaining time: X` + `Reading sector: current/total` or `Time used: X`), and `Pause`/`Resume` + `Stop` controls.
- **Modals on this surface**:
  - **Stop Confirmation (`CScanCommonPopDlg`)**: Triggered by `Stop`; displays files found + time used stat tiles, `Continue` (primary) and `Stop` (danger). Confirming cancels scan and navigates to `/results/:scanId` with partial-scan banner.
  - **Device Disconnected Modal**: Non-dismissible modal when `deviceConnected === false && status === 'paused'`; `I reconnected it` resumes scan.
  - **Scan Completion Celebration Modal**: Standalone `520px` modal (`w-[520px]`, `onEscapeKeyDown` prevented per corpus §7), shows a 2-column stats grid (`Files found` | `Size`), the green-check recoverable callout banner, and `Go to Recover` primary XL button.

### S4. Results Explorer (`/results/:scanId`)

- **Corpus Reference**: `DrwScanPage` post-scan (`screenshots/35..44`, `51..56`, `artifacts/UIUX/30-interaction-driver-ground-truth.md`, `35-redesign-reference-atlas.md` §8–§11).
- **Regions**:
  1. Partial-scan warning banner (`PartialBanner`) when `scanState.status === 'cancelled'`, with `Continue searching` action.
  2. Top toolbar (`48px` / `h-12`): Navigation cluster (`Home`, `Back`, `Up one level`), Breadcrumb / Search heading bar (`role="heading" aria-level="1"` showing active location + item count or `Search results for "<query>"` with clear button), View mode segmented toggle (`Details view` ⇄ `Icon view`), `Filter` toggle button with active indicator badge, and `Search by name` input (commits on `Enter`).
  3. Collapsible `FilterBar`: `Type` popover (8 categories from `FileFilter.xml` + `Custom` extension suffix input `*.doc; *.xlsx`), `Date` popover (`Any time`, `Since yesterday`, `Last 7 days`, `Last 30 days`, `Custom range…`), `Size` popover (`Any size`, `Under 512 KB`, `512 KB – 1 MB`, `1 – 512 MB`, `512 MB – 1 GB`, `Over 1 GB`, `Custom range…`), `Existing files` checkbox, `Hide 0 KB files` checkbox, and `Reset` button.
  4. Left Tree Pane (`w-60`): Segmented mode switch (`By location` ⇄ `By type`) + `ResultsTree` (`role="tree"`). In `By location` (`path`) mode, top-level nodes are disjoint: `Existing files` (`root` — directory hierarchy), `Paths partially lost` (`lost-paths`), and `Recovered by signature` (`raw-all`). In `By type` mode, displays the 8 `FileFilter.xml` categories.
  5. Center Virtualized File List (`VirtualFileList`, `@tanstack/react-virtual`, `overscan: 16`): Supports 1M+ rows (`rowHeight = 42px` in details mode, `96px` in icon mode), tri-state Select All header checkbox over full filtered set (`CSelAllCheckBox`), sortable columns (`Name`, `Date modified`, `Type`, `Size`, `Condition`), 6-item right-click `ContextMenu` (`Recover…`, `Preview`, `Open location`, `Select not-yet-recovered`, `Pick best copy of each`, `Clear selection`).
  6. Right Preview Pane (`PreviewPane`, resizable `240px–560px` via `PreviewPaneResizer` `role="separator"`): Thumbnail, filename, size/date/extension metadata, `ConfidenceBadge` + origin badge (`Existing` / `Deleted` / `Carved`), bounded preview renderer (`image` with zoom/fit, `text` with encoding badge, `pdf` with page stepper, `av` with metadata/duration, `office` with extracted text, `hex` via off-main-thread Web Worker), and confidence diagnostic footer.
  7. Bottom Status Bar (`h-[68px]`): Live-results badge (when scanning), total/selected count + byte size summary, `Save this search` session button, and contextual primary CTA (`Recover all…` ⇄ `Recover N items…`).

### S5. Recovery Destination, Queue & Report (`/recover/:scanId`)

- **Corpus Reference**: Native destination picker + `CRecoverProgressDialog` + completion report (`artifacts/UIUX/35-redesign-reference-atlas.md` §10, `docs/07-user-flow-uiux.md` §5).
- **Regions**:
  1. Pre-start configuration: Mount point list (inline `Drive3dIcon` rows with capacity bars), manual path input + `Choose folder…` button, real-time connector destination validation (`overlap` hard refusal alert, `out-of-space` alert with needed bytes), and recovery options (`CollisionPolicy` radio group: rename/overwrite/skip; the `Enable file repair` switch — the remaining payload options `rebuildFolders` / `keepEmptyFolders` / `verifyOnWrite` ride along with defaults but have no visible controls).
  2. Active queue (`RecoveryQueue`): Overall progress bar + percentage + current file + item counter + ETA, scrollable bounded item list with per-item state badges (`Waiting`, `Copying`, `Verifying`, `Recovered`, `Failed`, `Skipped`), and confirm-gated `Cancel recovery` dialog.
  3. Completion report (`QueueReport`, `role="status"`): Status banner (`Recovery finished` / `Recovery finished with issues` / `Recovery failed`), 3 stat tiles (`Recovered`, `Failed`, `Skipped`), failed item diagnostic list, truncation notice for large sets, JSON report export (`Export report` + toast), and `Close` button.

### S6. Disk Health (`/health`; `/tools` is a legacy alias — redesigned in v4)

- **Corpus Reference**: Disk Health / S.M.A.R.T. surface (`docs/UI-MAP.md` §2).
- **Regions**: Physical-disk selector cards (3D `Drive3dIcon` with system-drive Windows badge and state-dependent BitLocker open/closed lock, capacity bar, free/total — `role="radiogroup"`), circular refresh button, hero health-score card (circular gauge, status `Good` / `Caution` / `Poor`, serial number, scan-this-drive CTA), 2×2 telemetry tiles (`Temperature`, `Powered on`, `Type`, `TRIM` / `Estimated life used`), and full S.M.A.R.T. attributes table (`ID`, `Attribute`, `Current`, `Worst`, `Threshold`, `Raw`, `Status`).

### S7. File Repair (`/repair`)

- **Corpus Reference**: `RawFixer.dll` surface (`docs/01-executive-overview.md`, `docs/UI-MAP.md` §2).
- **v4 flow**: 2-step direct flow — no Start landing screen.
  1. Step 1 (`dropzone`): "Repair Corrupted Videos, Photos and Documents" header + centered dashed `Add Files` card with the drag-file illustration.
  2. Step 2 (`queue`): top bar (`Home` + `Advanced Repair`), queued file cards (name, size, category, path, per-item status), and a sticky bottom bar (save-path input, folder browse, add, clear, `Repair All` primary CTA).
- **Advanced structural repair**: the RawFixer workbench (`VolumeRepairWorkbench`, behind the `Advanced Repair` toggle) keeps the physical disk selector, dry-run structural findings list (`boot-sector`, `backup-dbr`, `fat-table`, `mft-mirror`), two-factor consent gate (acknowledgement checkbox + typed `REPAIR` confirmation input) before enabling the repair danger CTA, and session `Undo log` with `Undo repairs` rollback button.

### S8. Settings (`/settings`) & About Dialog (`AboutDialog`)

- **Regions**:
  - `SettingsPage` (opened from the three-lines header Menu): Theme dropdown (`Match system`, `Light`, `Dark`, `Ember`, `Tide`, `Blossom`, `High contrast`), UI Scale segmented control (`100%`, `150%`, `200%`), Density segmented control (`Comfortable`, `Compact`), Language segmented control (`English`, `简体中文`), Diagnostics event log copy, and About summary table.
  - `AboutDialog` (header Menu → `About` menuitem): Title-bar modal displaying connector-sourced `Version`, `Data source`, `Recovery engine`, `Disk driver`, and `Platform`.

### S9. SD Card / Removable Media (`/sd-card`)

- **App-side v4 surface** (corpus lineage: reference landing removable-media flows, `35-redesign-reference-atlas.md` §5).
- **Regions**: "Connect Your Memory Card to Recover Lost Photos & Videos" header + subtitle, refresh trigger, not-detected banner (`Get Help`), `Show all external devices` toggle, SD/USB device cards (`SdCardPage`, `SdCardSlotIcon`) with per-card `Scan Card` CTA, and three feature badges (Fix SD card access / Photos that open / Video that play; `FeatureBadgeItem`).
- **Transitions**: The per-card CTA starts the scan directly (ADR-008) and routes to `/scan/:volumeId/live?scanId=<id>`.

### S10. Scan Snapshot (`/backup`)

- **App-side v4 surface** (renamed from "ChronoSnap Backup"; deliberately reinterpreted from the reference's sidebar backup product page — UI-MAP §0).
- **Regions**: `Scan Snapshot` title + subtitle, three feature bullets (Instant Session Snapshots / Incremental Protection / Zero Source Modification), `Start New Scan` CTA (routes to `/`), and the `Saved Recovery Checkpoints` card: session list (target label, timestamp, files found) with per-session `Open` action (loads the session and routes to `/results/:scanId`), with an empty state when no checkpoints exist.
