# VISUAL-SPEC

Resolved design tokens and per-screen visual specifications for the Recovery Wizard UI.
Every parameter carries a source; where VLM estimates and hard evidence (QSS `style.css`,
`config/themdark.ini`, `config/config.ini`) disagree, **hard evidence wins** and the
resolution is recorded. Unconfirmable values are marked ESTIMATED.

Hard-evidence abbreviations:

- **TD** = `og-codebase/config/themdark.ini` (118-slot dual palette; slots cited)
- **QSS** = `og-codebase/style.css` (20,802-line census: 434 radius rules, font census)
- **CFG** = `og-codebase/config/config.ini [windowsSize]`
- **ATLAS** = `artifacts/UIUX/35-redesign-reference-atlas.md` (pixel-verified geometry)
- **VLM** = 63-screenshot structured extraction (this session, `scripts/vlm-out/`)

## 1. Color tokens (light / dark)

| Token                                   | Light                                            | Dark                                                    | Source & resolution                                                                                                  |
| --------------------------------------- | ------------------------------------------------ | ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `--background` (app canvas)             | `#eff5ff→#deeaff` vertical gradient              | `#1b1d1f→#18191b`                                       | TD slot 1 (both themes); VLM #f5f7fa-family = same family sampled loosely; use slot-1 gradients on the window canvas |
| `--surface` (cards, panes)              | `#ffffff`                                        | `#202428`                                               | TD slot 2                                                                                                            |
| `--surface-muted` (strips, wells)       | `#f6f9fd`                                        | `#32383e`                                               | TD slot 21 / slot 15                                                                                                 |
| `--surface-tint` (selected/hover fills) | `#e5eefb` / `#e6edf6`                            | `rgba(141,189,255,0.08–0.2)`                            | TD slots 5/20; dark slots 5/16/23/24                                                                                 |
| `--text-primary`                        | `#1a1a1a`                                        | `#f1f5ff`                                               | TD slot 3; VLM sees #1f2329-family (consistent)                                                                      |
| `--text-secondary`                      | `#616777`                                        | `#b0b5c0`                                               | TD slot 17 / dark slot 17                                                                                            |
| `--text-disabled`                       | `#a0a6b1`                                        | `#7b808a`                                               | TD slot 28 / dark slot 4                                                                                             |
| `--accent` (brand blue)                 | `#0560fd`                                        | `#5094ff`                                               | TD slot 6 (both); VLM brackets confirm family                                                                        |
| `--accent-hover`                        | `#0561fc` + ring `rgba(5,96,253,.2)`             | `#68b4ff`                                               | TD slots 31/24; dark 64                                                                                              |
| `--accent-pressed`                      | `#0448be`                                        | `#3874ff`                                               | TD slot 65 / dark 65                                                                                                 |
| `--cta-gradient`                        | `#0c99ff→#0560fd` (0→0.95, horizontal)           | `#059afd→#0ccdff`                                       | TD slot 10 (both)                                                                                                    |
| `--focus-ring`                          | `rgba(5,96,253,0.9)`                             | `rgba(80,148,255,0.9)`                                  | TD slot 34 / dark 34                                                                                                 |
| `--success`                             | `#30b373`                                        | `#349a37`                                               | TD slot 59 / dark 59                                                                                                 |
| `--warning`                             | `#ffc10e` (amber CTA gradient `#FFCE76→#FFB025`) | `#ff7342` (orange accent) / `#ff5a5a`                   | TD slots 11/14; dark slots 11/36                                                                                     |
| `--danger`                              | `#f72c2c`                                        | `#ff5a5a`                                               | TD slot 36 / dark 36                                                                                                 |
| `--border`                              | `#e7e7e7` (outer), `#dee1ed` (inner)             | `#434851` / `#404c63`                                   | TD slots 9/12; dark 9/40                                                                                             |
| `--border-strong`                       | `#c1ccdf`                                        | `#596680`                                               | 4-step neutral ladder (TD/QSS census)                                                                                |
| `--shadow`                              | `rgba(4,5,43,0.5)` at low blur                   | `rgba(28,28,28,0.5)` / dark slot 18 `rgba(31,31,31,.5)` | TD slot 18                                                                                                           |
| `--overlay-scrim`                       | `rgba(0,0,0,0.4)`                                | `rgba(0,0,0,0.4)`                                       | TD slot 4                                                                                                            |
| `--info-surface`                        | `#ecf4ff`                                        | `#252b33`                                               | TD slot 32                                                                                                           |

High-contrast theme (shipped by the platform, not the reference): tokens map to
Windows HC light/dark — near-black/white canvas, 1px fully-saturated borders, no
gradients, focus ring = 2px solid accent. Marked ESTIMATED-derived (reference has
Theme submenu = system/light/dark only; HC parity is our accessibility floor, grid #17).

## 2. Typography

Base family stack (CFG/QSS `[style]`): `Segoe UI, Microsoft YaHei, Meiryo UI, Malgun
Gothic, sans-serif` — mapped to CSS with per-locale fallback via `:lang()`. Display
face: none shipped (reference uses DIN for marketing surfaces only; we keep Segoe UI
SemiBold — ESTIMATED divergence, documented).

| Role                     | Size / weight / line-height                              | Source                                   |
| ------------------------ | -------------------------------------------------------- | ---------------------------------------- |
| Hero / celebration title | 28px / 600 / 1.2                                         | VLM census (26–28/600); QSS 14px base ×2 |
| Page title               | 20px / 600 / 1.3                                         | VLM; ATLAS landing title band            |
| Section header           | 16px / 600 / 1.4                                         | QSS font census (16px = 92 uses)         |
| Body / list primary      | 14px / 400–500 / 1.45                                    | QSS base (278 uses) + VLM                |
| Table row / secondary    | 13px / 400 / 1.4                                         | QSS (186 uses) + VLM census peak         |
| Caption / meta / badge   | 12px / 400–600 / 1.3                                     | QSS (56 uses)                            |
| Numeric columns          | 13px tabular-nums (`font-variant-numeric: tabular-nums`) | grid #9 — Segoe UI has tabular figures   |

## 3. Geometry & spacing

| Token                | Value                                                          | Source                                                     |
| -------------------- | -------------------------------------------------------------- | ---------------------------------------------------------- |
| Window normal        | 1240×840, opens maximized                                      | CFG `[windowsSize]`                                        |
| Window minimum       | 800×600                                                        | CFG                                                        |
| Title bar height     | 36px (icons at y 13–28, click y≈20)                            | ATLAS §2                                                   |
| Dialog standard      | 540×240 (message); 540×380 (celebration)                       | CFG/QSS + ATLAS §7                                         |
| Radius scale         | 4 / 6 / 8 / 12 / 16 / 999 (pill)                               | QSS census (89×4, 73×8, 51×6, 25×12, 6×16) + VLM (20 pill) |
| Control heights      | sm 24, md 32, lg 36–40 (CTA), XL 44 (tile CTA)                 | QSS min-height ladder (16/19/24/32) + VLM (32/36/40/44)    |
| Icon sizes           | 16/20 title bar, 24 inline, 32 toolbar/feature, 48+ device art | VLM census + QSS icon standards 48/56/72/80                |
| 4px grid             | all spacing multiples of 4                                     | VLM grid notes; ATLAS pixel coords                         |
| Home content margins | 40/45/0/48 pattern → 40px outer, 45 section gaps               | QSS `#DrwHomePage` layoutContMargins                       |
| Scrollbars           | 12px (home), 6px overlay (results lists)                       | QSS scrollbar specs                                        |
| Tooltips             | 14px text, radius 8, min-height 25, max-width 400, 0.9 opacity | QSS tooltip spec                                           |

## 4. Iconography

- Style: dual-tone outline+fill, rounded corners, 2px effective stroke at 24px
  (VLM: filled/outline mixed, corner=rounded). Semantic families: device states
  (disk/USB/SD/camera, existing/lost), filetype set (~105 types at 24/48),
  navigation (home/back/up), actions (scan/pause/stop/recover/filter/search).
- Density modes: comfortable (24px rows of icons), compact (16px) — ESTIMATED from
  QSS density hooks; verified against VLM spacing notes.
- Interim base: **Lucide** (per v1 policy) re-tinted to token colors; replacement
  plan in `INTERIM-ASSETS.md`.

## 5. Elevation & motion

- Elevation ladder: flat (panes) → card `shadow rgba(4,5,43,.08)` → dropdown/dialog
  `rgba(4,5,43,.5)` family with scrim `rgba(0,0,0,.4)` (TD/QSS). Dark mirrors with
  slot-18 values.
- Motion tokens (VLM motion hints + Qt default easing; ESTIMATED where not pinned):
  - `--dur-fast: 120ms`, `--dur-base: 200ms`, `--dur-slow: 320ms`
  - `--ease-standard: cubic-bezier(0.2, 0, 0, 1)` (decelerate-in)
  - Choreography: cross-fade page swaps (160ms), list rows fade+slide-in (120ms),
    dialogs scale 0.96→1 + fade (200ms), progress transitions linear.
  - `prefers-reduced-motion`: all non-essential motion collapses to opacity fades
    ≤100ms; progress indicators keep functioning (grid #27).

## 6. Per-screen acceptance envelope (key screens)

| Screen             | Required layout envelope (from ATLAS/VLM)                                                                                                                                                                   |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Splash             | centered brand + progress line on gradient canvas; 640×480 min                                                                                                                                              |
| Home               | left sidebar (3 sections, 6+ rows, 200px) + content: page title, device tile grid (2-col, tiles with icon+name+capacity bar), quick-target row, primary CTA bottom-right                                    |
| Scan               | 3-pane results layout active during scan; bottom-left circular gauge cluster + stage/status lines; bottom-right Recover CTA; full-width status strip                                                        |
| Complete           | modal 540×380: title, stats grid, primary CTA                                                                                                                                                               |
| Results            | toolbar (nav cluster, breadcrumb, view toggle, filter, search) + filter bar (toggle row) + 3-pane (left tree 200px / center virtualized list / right preview 300px) + bottom status bar with contextual CTA |
| Destination dialog | tabs (Local/Cloud), destination rows with capacity stats + warning marks, folder picker link, file-repair toggle, Save (disabled until valid selection), overwrite-safety coaching, space-gate variant      |
| Health             | disk selector, health card, SMART table (7 columns: ID/Attribute/Current/Worst/Threshold/Raw/Status)                                                                                                        |

## 7. Acceptance envelope rules

1. Every token has a source column — no orphan values.
2. Screens must satisfy the 50+ parameter grid in `VISUAL-ACCEPTANCE.md`; DPI targets
   100/125/150/175/200 via vector assets + 4px-grid layout.
3. Dark parity is a hard gate: no light-only artifact ships (grid #16).
4. Contrast: text AA (AAA on critical warnings), non-text 3:1 (grid #12–13).
5. Any parameter later measured out of envelope → fix or file a tracked deviation
   with rationale.
