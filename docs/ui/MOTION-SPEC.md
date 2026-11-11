# MOTION-SPEC — Choreography, durations, easings, reduced motion

> Gate document for the experience session. Corpus-sourced where evidenced;
> `ESTIMATED` where the corpus is silent and the premium standard matching the
> reference's character was chosen. Companion: EXPERIENCE-SPEC.md (parameters),
> ADR-007 (token architecture), `src/theme/adopted/tokens.css` (token values).

## 1. Motion philosophy — the corpus truth

The reference is a **Qt 5 Widgets** application (UIA §1). Two facts dominate
its motion character:

1. **QSS has no transition property.** The 2,805-rule stylesheet defines
   instant state swaps for `:hover` (224 rules), `:pressed` (195),
   `:disabled` (147) [ART21 §3]. Hover and press feedback in the reference
   is effectively **instant (0 ms)**.
2. **Real animation is sparse and purposeful.** The binary imports
   `QPropertyAnimation` + `QEasingCurve` (18 symbol hits) and uses
   `QGraphicsOpacityEffect` fades for chrome (title bar/nav) transitions
   [ART16 §1]. Continuous motion exists only for **progress surfaces**: the
   circular scan gauge, the "Searching:" path ticker, the 50-frame loading
   spinner, and the splash progress line.

Consequence for this app: motion is **restrained**. No slidey page
transitions, no bouncing dialogs, no parallax. Surfaces appear and
re-appear with quick fades; state feedback is near-instant; progress is
alive. Anything slower than the corpus tempo would read as wrong.

## 2. Duration & easing tokens (adopted layer, ADR-007)

| Token                               | Value                            | Corpus mapping                                                                                                                                 |
| ----------------------------------- | -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `--dur-micro`                       | 120 ms                           | menus, chips, hover swaps — covers the reference's instant swaps with a barely-perceptible fade (`ESTIMATED` refinement, EXPERIENCE-SPEC §9.5) |
| `--dur-fast`                        | 160 ms                           | entrances, scrim fades — the chrome `QGraphicsOpacityEffect` family                                                                            |
| `--dur-base`                        | 240 ms                           | layout settles, scan bar                                                                                                                       |
| `--dur-slow`                        | 360 ms                           | large surfaces, inspector/sidebar slide (`ESTIMATED` — sidebar collapse is an app extension)                                                   |
| `--ease-out`                        | `cubic-bezier(0.2,0.8,0.3,1)`    | entrances                                                                                                                                      |
| `--ease-in-out`                     | `cubic-bezier(0.5,0.05,0.4,1)`   | symmetric transitions                                                                                                                          |
| `--ease-overshoot`                  | `cubic-bezier(0.2,0.9,0.3,1.15)` | reserved; used only by the loading spinner pulse family                                                                                        |
| `--press-scale` / `--press-opacity` | 0.985 / 0.75                     | press feedback (`ESTIMATED` — reference swaps color only)                                                                                      |

## 3. Motion inventory

Status legend: **IMPLEMENTED** (verified) · **PLANNED** (this session's
work) · **ESTIMATED** (corpus-silent, premium standard chosen).

### 3.1 Window chrome

| Motion                                 | Spec                                                                 | Status           |
| -------------------------------------- | -------------------------------------------------------------------- | ---------------- |
| Chrome button hover                    | instant glyph/bg swap, no fade (corpus: instant QSS swap)            | PLANNED          |
| Chrome button press                    | instant swap to pressed plate                                        | PLANNED          |
| Window minimize/maximize/restore/close | OS-managed; no app-side animation (corpus: native Qt/Win32 behavior) | PLANNED (wiring) |
| Title-bar drag                         | no animation                                                         | PLANNED (wiring) |

### 3.2 Dialogs & popups

| Motion                 | Spec                                                                                                         | Status                                          |
| ---------------------- | ------------------------------------------------------------------------------------------------------------ | ----------------------------------------------- |
| Overlay scrim          | fade-in `--dur-fast` `--ease-out`, opacity `--overlay-scrim`                                                 | IMPLEMENTED (`animate-fade-in`)                 |
| Dialog enter           | fade-in `--dur-fast`; **no scale, no slide** (Qt modal: instant appear — fade is the `ESTIMATED` refinement) | IMPLEMENTED                                     |
| Dialog exit            | fade-out `--dur-fast`                                                                                        | PLANNED (Radix exit animation currently absent) |
| Dropdown/context menus | Radix animate-in 120 ms (`--dur-micro`), 1 px rise max                                                       | IMPLEMENTED (verify per menu)                   |
| Freeze behavior        | while modal open, underlying UI inert (aria-inert + pointer-events none via Radix)                           | IMPLEMENTED (Radix)                             |
| Celebration modal      | same as dialog enter; **no ESC, CTA-only dismissal** (corpus §7)                                             | IMPLEMENTED                                     |

### 3.3 Toasts

| Motion       | Spec                                                          | Status                       |
| ------------ | ------------------------------------------------------------- | ---------------------------- |
| Enter        | fade + slight rise, `--dur-fast`                              | IMPLEMENTED (sonner default) |
| Stack        | newest at bottom, gap 8 px, no reflow jump                    | IMPLEMENTED                  |
| Auto-dismiss | 4 s default; **pause on hover**; progress affordance on hover | IMPLEMENTED (sonner)         |
| Exit         | fade/slide-out `--dur-fast`                                   | IMPLEMENTED (sonner)         |

### 3.4 Pages & views

| Motion        | Spec                                                                                                                                                                                             | Status           |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------- |
| Route change  | **instant content swap** (corpus: QStackedWidget) — fade-in of the incoming page at `--dur-fast` max, no cross-slide                                                                             | PLANNED          |
| Scan→results  | same surface; state cluster updates in place (ADR-008)                                                                                                                                           | IMPLEMENTED      |
| Type⇄Path tab | tab highlight **instant**; tree rebuild async — a light skeleton/opacity treatment during rebuild, ≤120 ms fade when data lands (corpus: mid-transition capture shows instant tab, rebuilt tree) | PLANNED (verify) |
| Filter apply  | list dims slightly (opacity 0.6) while worker runs, restores on completion; never blocks                                                                                                         | PLANNED (verify) |

### 3.5 Sidebar

| Motion          | Spec                                                                                                                                                                                                                            | Status                  |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| Collapse/expand | width transition `--dur-slow` (360 ms) `--ease-in-out`; labels fade at `--dur-fast`; icons remain visible; `overflow:hidden` during transition; **no layout shift outside the sidebar**; main-content scroll position preserved | PLANNED (app extension) |
| Toggle control  | icon button in the sidebar header; keyboard focusable; state persisted (localStorage)                                                                                                                                           | PLANNED                 |
| Reduced motion  | width snaps instantly; label visibility toggles without fade                                                                                                                                                                    | PLANNED                 |

### 3.6 Progress surfaces (continuous motion — the reference's liveliest layer)

| Motion              | Spec                                                                                                                  | Status      |
| ------------------- | --------------------------------------------------------------------------------------------------------------------- | ----------- |
| Circular scan gauge | ring fills with progress; indeterminate shimmer while awaiting first %; **freezes at pause** (corpus: paused capture) | VERIFY      |
| "Searching:" ticker | full-width light-blue strip; path text scrolls (marquee, ~30 s/loop-equivalent — gentle); pauses with scan            | VERIFY      |
| Files-found counter | numeric updates without animation (tabular numerals prevent jitter)                                                   | IMPLEMENTED |
| Loading spinner     | CSS spinner (single-element equivalent of the corpus 50-frame sequence), 0.9–1.2 s loop                               | IMPLEMENTED |
| Progress stripes    | 1.1 s linear loop (determinate bars)                                                                                  | IMPLEMENTED |
| Status pulse        | sidebar scan dot, 2.4 s opacity pulse; text never animates (contrast)                                                 | IMPLEMENTED |

### 3.7 Microinteractions

| Motion                        | Spec                                                                                                                                | Status      |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ----------- |
| Button hover/press            | color/bg/box-shadow fade ≤120 ms (`transition-[background-color,border-color,color,box-shadow] duration-100` — current Button base) | IMPLEMENTED |
| Card selection (device tiles) | border + checkmark appear instantly (corpus: blue border + check on selection)                                                      | IMPLEMENTED |
| Row selection                 | checkbox fill instant; details pane updates without animation                                                                       | IMPLEMENTED |
| Context menu                  | anchors below-right of pointer; enter 120 ms fade + 1 px rise                                                                       | IMPLEMENTED |
| Chip toggle (filters)         | instant visual state; removable chips fade on remove                                                                                | VERIFY      |
| Drag (preview resizer)        | no animation — follows pointer 1:1                                                                                                  | IMPLEMENTED |
| Drag-and-drop                 | not evidenced in corpus recovery-core flows; not implemented                                                                        | N/A         |

## 4. Reduced motion (`prefers-reduced-motion`)

Global rule already in `src/styles/index.css`:

- All animation/transition durations collapse to 0.01 ms.
- `scroll-behavior: auto`.
- No information is conveyed by motion alone: the gauge still shows its
  numeric %, the ticker still shows its current path statically, paused
  still shows the paused strip. (Verified per surface during this session.)

## 5. Frame budget

Every motion above must compose within 60 fps on the Windows CI runner
class hardware. Watchdog: any main-thread task > 50 ms in debug builds is a
finding (session mandate §8). Continuous animations are compositor-only
(`transform`/`opacity`); the gauge uses SVG stroke-dashoffset (paint), kept
off the layout path.

## 6. Sources

- ART16 §1 — QGraphicsOpacityEffect chrome fades.
- ART21 §3–4 — interaction-state census; instant swaps; tint ladder.
- ATL35 §2/§5/§6/§7/§8/§12 — pixel maps, gauge, ticker, dialogs, ESC rules.
- ART20 — Freeze/Unfreeze protocol; async expand debounce.
- ART31 — filter thread (non-blocking list).
- VLM/01-splash, 03-scan-phase, 05-results — splash line, gauge freeze,
  midtransition evidence.
- `re-workspace/reversed/DRWUI.exe/strings_ascii.txt` — QPropertyAnimation/
  QEasingCurve imports (18 hits).
