# EXPERIENCE-SPEC — The reference's experience layer, parameter by parameter

> Gate document for the experience session. Every claim carries its corpus
> source. Where the corpus is ambiguous, the choice is marked `ESTIMATED`.
> Precedence per session SOP: screenshots + decompiled-resource evidence win
> over VLM JSONs; architecture docs + decompiled evidence win over VLM JSONs
> unless screenshots clearly show otherwise. All resolutions recorded in §9.
>
> Sources referenced:
>
> - **ATL35** `re-workspace/artifacts/UIUX/35-redesign-reference-atlas.md`
> - **ART16** `…/16-shell-internals-ai-layer.md` (DrwMainWindow/title bar)
> - **ART05** `…/05-styling-theming-layout.md` (design system, window sizes)
> - **ART21** `…/21-qss-property-census.md` (interaction-state census)
> - **ART07/08/09/11** phase deep dives (home/scan/results/recovery)
> - **UFL** `architecture/07-user-flow-uiux.md`; **UIA** `architecture/05-uiux-architecture.md`
> - **VLM/\<area\>** `docs/evidence/vlm/shots/` (63/63 extractions)
> - **RCC** `re-workspace/reversed/ui-resources/drw_light/Resource/` (extracted assets)

## 1. Window & custom chrome

| Parameter                     | Value                                                                                                                                                                                                                                 | Source                                                                                     |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Frame                         | **Frameless custom chrome** (`decorations:false` in our shell)                                                                                                                                                                        | ATL35 §1; ART16                                                                            |
| Normal size                   | 1240×840                                                                                                                                                                                                                              | ART05; `QSize(0x4d8,0x348)` verified §10 UIA                                               |
| Minimum size                  | 800×600                                                                                                                                                                                                                               | ART05                                                                                      |
| Launch state                  | Opens **maximized** (`window_show_state=StatusMaxed` default, QSettings-persisted)                                                                                                                                                    | ART16 §6; UIA §2                                                                           |
| Title bar height              | **~35 px** (icon strip at y 13–28; click centers y≈20)                                                                                                                                                                                | ATL35 §1–2 pixel scan                                                                      |
| Title bar layout (left→right) | app icon + title text + version label … feature icons (AI robot, buy, activate, save-scan, hamburger) … **min · max/restore · close**                                                                                                 | ATL35 §2                                                                                   |
| Chrome button size            | 24×24 SVG glyphs; hover adds rounded-4 background plate                                                                                                                                                                               | RCC `btn_*.svg` (24px viewBox)                                                             |
| Hit target                    | ≥24×24 glyph + hover plate (our implementation: ≥32×32 hit area, `ESTIMATED` for a11y floor)                                                                                                                                          | RCC; a11y minimum                                                                          |
| Button order                  | minimize → maximize/restore → close (Windows convention; corpus census shows min at ~x959, max ~x994, close ~x1015)                                                                                                                   | ATL35 §2                                                                                   |
| Drag region                   | Title bar background drags the window; double-click title bar toggles maximize/restore (standard Windows behavior for the custom frame)                                                                                               | `ESTIMATED` — Qt frameless default; corpus shows snapped/maximized windows across captures |
| State coverage                | 4 states per button: nor / hov / pre / dis — **4-state SVG sets exist in the corpus for min, max, restore, close** (light: `titlebar_btn_min_*`, `btn_max_*`, `btn_restore_*`, `btn_close_*`; dark: same names under `ResourceDark/`) | RCC inventory                                                                              |
| Light-state colors            | nor glyph `#616777` transparent bg · hov bg `#D3DCE7` rx4 + glyph `#0560FD` · pre bg `#D3DCE7` + glyph `#1A1A1A` · dis glyph `#A0A6B1`                                                                                                | RCC SVG fills (extracted)                                                                  |
| Dark-state colors             | nor glyph `#B0B5C0` · hov bg `#323D4C` + glyph `#F1F5FF` · pre bg `#323D4C` + glyph `#5094FF`                                                                                                                                         | RCC `ResourceDark/btn_close_*.svg`                                                         |
| High-DPI                      | 100/125/150/175/200% (reference ships 100/150/200 zoom menu; Segoe UI scaling)                                                                                                                                                        | ATL35 §1 (`windowsSize/set_dpi`)                                                           |
| Snap / Aero                   | Drag to screen edges snaps (OS-level; frameless Qt windows participate)                                                                                                                                                               | `ESTIMATED` — OS behavior                                                                  |

## 2. Left sidebar (navigation rail)

| Parameter         | Value                                                                                                                                                                                                                        | Source                     |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| Component         | `DrwFlyoutWindow`/`DrwFlyoutMenu`, synced with the wizard stack via `currentChanged(int)`                                                                                                                                    | ART16 §3                   |
| Width             | ~275 px at 1024-wide runner (VLM); app semantic token `--sidebar-width: 208px` at our 1240 default                                                                                                                           | VLM/02-landing; tokens.css |
| Section headers   | Uppercase-ish gray labels: "Data Recovery" / "Advanced Features" / "File Backup"                                                                                                                                             | VLM/02-landing; ATL35 §5   |
| Rows              | 6 entries (Hardware Disk · SD Card · More Recovery Tools · File Repair · Lost Partition Rescue · ChronoSnap Backup), ~53 px pitch on the runner                                                                              | ATL35 §5 pixel map         |
| Selected state    | Light-blue row bg + blue text/icon; a **green checkmark badge** marks the active row                                                                                                                                         | VLM/02-landing             |
| Hover state       | Tint ladder bg (QSS blue tints)                                                                                                                                                                                              | ART21 §4                   |
| Collapse behavior | The rail is fixed-width in all 63 captures; **no collapsed/rail icon mode is evidenced anywhere in the corpus**                                                                                                              | 63-shot library survey     |
| Our requirement   | Session mandate §2.1 requires a collapsible sidebar with smooth transition, state preservation, and keyboard access — implemented as an app-side affordance, marked as a **documented app extension** (not a corpus feature) | session SOP                |

## 3. Dialogs & popups

Corpus inventory (recovery-core subset in scope for this app; commercial
funnel surfaces excluded per UI-MAP §0):

| Dialog                                             | Corpus evidence       | Entry / exit choreography                                                            | ESC                  | Click-outside | Size/anchor                       |
| -------------------------------------------------- | --------------------- | ------------------------------------------------------------------------------------ | -------------------- | ------------- | --------------------------------- |
| Stop-scan confirm (`CScanCommonPopDlg`)            | ATL35 §6              | Qt modal: instant appear, dimmed background, Freeze protocol                         | no (choice required) | swallowed     | centered, ~540 message-box family |
| Scan-complete celebration                          | ATL35 §7; artifact 30 | **Standalone 540×380 OS window**; blocks toolbar until dismissed; only CTA dismisses | **rejected**         | swallowed     | centered                          |
| Device-lost (non-dismissable)                      | ART08                 | modal, no dismissal path but reconnect                                               | rejected             | swallowed     | centered                          |
| Destination picker (`ChooseDestinationPathDialog`) | ATL35 §10             | modal, Local/Cloud tabs, Save disabled until selection                               | no                   | swallowed     | ~560×480 (QSS census family)      |
| Insufficient-space gate                            | ATL35 §10             | modal over destination                                                               | no                   | swallowed     | centered                          |
| Activate dialog (`CEnterLicenceDlg`)               | ATL35 §4b             | modal; close only via X at (770,150)                                                 | **rejected**         | swallowed     | centered                          |
| Solution wizard (3-step)                           | ATL35 §3              | modal; Next ×3 walk                                                                  | rejected             | swallowed     | right-anchored modal              |
| Hamburger menu (`Drw_More_Menu`)                   | ATL35 §4              | Dropdown panel right-aligned under button, 13 items + 3 separators, items ≈42 px     | **closes**           | closes        | panel x748–972                    |
| Context menu (`CShowContextMenu`)                  | ATL35 §8              | Anchors **below-right of click**; geometry varies with row; 6 items ≈43.5 px         | closes               | closes        | dynamic                           |
| Filter dropdown menus                              | ATL35 §9              | Popover under filter bar                                                             | closes               | closes        | under bar                         |
| Preview (`PreviewMainDialog`)                      | ATL35 §11             | modal `exec()` under Freeze protocol                                                 | closes               | swallowed     | large window                      |

**General rules (all corpus modals):** the underlying window is dimmed and
**all clicks are swallowed** (no click-through) [ATL35 §12 rule 10]; the
Freeze/Unfreeze protocol disables the main UI while any modal runs [ART20].
Message-box family geometry: 540×240 fixed [ART05].

**ESC discipline (dynamic-proven)** [ATL35 §12]:

1. Custom dialogs **reject ESC** (celebration, activate, solution wizard).
2. **Menus close on single ESC.**
3. **Double-ESC from results = exit attempt** → retention gate (excluded surface; our app: single ESC closes topmost dismissible layer — documented divergence, commercial funnel removed).

## 4. Toasts & notifications

| Aspect             | Corpus                                                                                                                         | Source                                   |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------- |
| Instance           | "AI Recommend available." toast on results entry                                                                               | ATL35 §30 artifact; VLM/07-recovery      |
| Position           | top-right area below title bar (VLM reading of results captures)                                                               | VLM/07-recovery `recovery-entry-results` |
| Our implementation | sonner bottom-right stack — `ESTIMATED` position (corpus has exactly one toast surface; commercial) — verified non-overlapping | app implementation                       |
| Enter/exit         | Not discernible in stills; Qt toast convention = fade                                                                          | `ESTIMATED`                              |

## 5. Page & view transitions

| Transition           | Corpus behavior                                                                                                      | Source                                                |
| -------------------- | -------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| Wizard navigation    | `QStackedWidget.setCurrentIndex` — **instant swap, no transition** (the stack is not animated; verified ctor decode) | UIA §3; ART04                                         |
| Scan → results       | Same surface (scan runs INSIDE the results page) — only the bottom cluster state changes                             | UIA §3; ADR-008                                       |
| Type ⇄ Path tab      | **ASYNC**: tab highlights immediately, tree rebuilds on a worker (mid-transition captured)                           | artifact 33 v2 addendum; VLM/05-results midtransition |
| Filter apply         | Background thread (`CSetFilterThread`) — list never blocks                                                           | ART31                                                 |
| Chrome fades         | `QGraphicsOpacityEffect` fade-in/out on title bar/nav transitions                                                    | ART16 §1                                              |
| Classify tree expand | Async expand-on-scroll, 1 s debounce, TimeCritical worker                                                            | ART20                                                 |

## 6. Microinteractions

| Interaction                | Corpus treatment                                                                                                           | Source                  |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| Hover (buttons/rows/chips) | **Instant QSS state swap** — no CSS-transition concept in Qt QSS; 224 hover rules                                          | ART21 §3                |
| Pressed                    | Instant swap to pressed state; 195 rules                                                                                   | ART21 §3                |
| Disabled                   | Instant swap; 147 rules                                                                                                    | ART21 §3                |
| Hover tint ladder (light)  | `#f2f7ff → #e9edf5 → #e0eaff → #c4d7ff → #CCD8ED` (5-step blue)                                                            | ART21 §4                |
| Interaction blue           | `#2978ff` — link/pressed tones, distinct from brand `#0560fd`                                                              | ART21 §4                |
| Row selection              | Single-click: checkbox fills blue, details pane updates, bottom CTA becomes contextual ("Recover All ..." ⇄ "Recover ...") | ATL35 §8, §12 rule 8    |
| Double-click row           | Opens live preview panel                                                                                                   | ATL35 §8                |
| View toggle                | Two-state toolbar button; icon reflects current state                                                                      | ATL35 §12 rule 7        |
| Search                     | Breadcrumb becomes "Search results for X" + removable ✕ chip                                                               | ATL35 §8                |
| Progress gauge             | Circular blue ring, **animates during scan; freezes at pause %**                                                           | ATL35 §6; VLM/03 paused |
| Ticker                     | Full-width light-blue "Searching: …" strip (marquee-style path ticker)                                                     | ATL35 §6                |
| Loading spinner            | 50-frame PNG sequence (`Refresh/loading/loading_00000..00049`)                                                             | RCC inventory           |
| Splash progress            | Thin line at the very bottom edge of the splash window, fills L→R                                                          | VLM/01-splash           |

## 7. State surfaces (loading / empty / error / permission / disconnected)

- Loading: splash "Reading the disk information, please wait..." + bottom progress line [VLM/01-splash]; spinner frames [RCC].
- Empty: "&Lost data not found" / "&Some data not found" tree states; "NoDeviceGuide*" home frames [UFL §1, §4].
- Error: destination `state=error` validation theming [ART21 §3]; PreviewFailedWnd unreadable state [UFL §4].
- Permission/disconnected: NAS network-error overlay family [ART08]; device-lost non-dismissable modal [ART08].

## 8. Experience parameter set (§4 of the session mandate) — corpus mapping

| #   | Parameter                             | Corpus anchor                                                                                |
| --- | ------------------------------------- | -------------------------------------------------------------------------------------------- |
| 1   | Window-size range 800×600 → maximized | ART05                                                                                        |
| 2   | Aspect resilience                     | 1024×735 runner + 1600×1000 Wine + 1240×840 normal all evidenced                             | ART24           |
| 3   | DPI 100–200%                          | ATL35 §1 zoom menu                                                                           |
| 4   | Sidebar collapse                      | **Not a corpus feature** — app extension per mandate                                         |
| 5   | Dialog open/close                     | Instant appear + dimmed Freeze; celebration blocks until CTA                                 | ART20; ATL35 §7 |
| 6   | Toast choreography                    | One toast evidenced; details `ESTIMATED`                                                     | VLM/07          |
| 7   | Chrome states                         | 4-state SVGs ×4 buttons ×2 themes                                                            | RCC             |
| 8   | Task clarity                          | Primary CTA always bottom-right ("Search for Lost Data" → "Recover All ..." → "Recover ...") | ATL35 §5/§6/§8  |
| 9   | Error recovery                        | Stat tiles + coaching in stop dialog; space-gate Continue/Reselect                           | ATL35 §6/§10    |
| 10  | Destructive gating                    | Stop requires confirm with risk framing; overwrite warnings in destination                   | ATL35 §6/§10    |
| 11  | Consistency                           | Same dialog family geometry (540×240), same button styles                                    | ART05           |
| 12  | First-run clarity                     | "Select a location to search for lost data" heading + grouped targets                        | VLM/02-landing  |
| 13  | Progressive disclosure                | Filter bar toggle; advanced scan upsell after quick                                          | ATL35 §8        |
| 14  | Loading feedback                      | Gauge, ticker, counters, skeletons (spinner frames)                                          | ATL35 §6        |
| 15  | Perceived performance                 | Browse-while-scanning; progressive tree fill                                                 | ART08           |
| 16  | Keyboard flow                         | `setAccessibleName` on all chrome elements (a11y built-in)                                   | ART16 §1        |
| 17  | Focus during transitions              | Qt modal focus handling (implicit)                                                           | ART20           |
| 18  | Scroll preservation                   | Splitter geometry persisted (QSettings); tree scroll preserved                               | UIA §3          |
| 19  | Copy quality                          | 1,537 verbatim English strings (adopted subset)                                              | ART02           |
| 20  | Non-color status                      | Icons + text everywhere (TRIM chips, warning icons, gauges)                                  | ART07/08        |

## 9. Conflict resolutions

1. **VLM landing sidebar width "275px" vs ART05 metrics** — VLM reads the
   1024-runner scale; the app token (208px at 1240) is proportionally
   equivalent. Resolution: keep the semantic token; no change.
2. **VLM "standard Windows 11 chrome" on some shots** — the runner captured
   the OS frame around the maximized app; the app itself is frameless
   (verified by icon-strip y-coordinates inside the client area). Screenshots
   - RCC 4-state SVGs win: custom chrome.
3. **Toast position (VLM top-right) vs current app bottom-right** — single
   data point, commercial context; sonner's default stack retained. Marked
   `ESTIMATED`, revisited if a corpus-critical toast appears.
4. **Sidebar collapse** — absent from corpus; implemented as documented app
   extension because the session mandate explicitly requires it (§2.1).
5. **QSS transition property** — none exists in the 2,805-rule census
   (Qt QSS does not support CSS transitions): hover/press/disabled are
   instant swaps in the reference. Our implementation keeps near-instant
   (≤100 ms) color fades for perceived smoothness — marked `ESTIMATED`
   refinement, documented in MOTION-SPEC §2.

## 10. Scope note

Commercial-funnel surfaces (retention gates, activation, buy/upsell, AI
assistant panel, product-link menu items, referral) remain excluded per
UI-MAP §0 — they are not honest for this app's connector-backed reality.
Their interaction _grammar_ (ESC discipline, Freeze, dimmed backdrop,
swallowed clicks) IS adopted where the in-scope surfaces need it.
