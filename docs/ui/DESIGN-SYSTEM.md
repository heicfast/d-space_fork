# DESIGN-SYSTEM

Living design-system document for the Recovery Wizard UI. Token values live in
`src/styles/tokens.css` (single source of truth consumed by Tailwind v4
`@theme inline`); this doc explains the system, usage rules, and evolution.

## 1. Principles

1. **One focal point per surface** — a single primary CTA (brand gradient) per view;
   everything else is secondary (outline/ghost) or tertiary (text/link).
2. **Trust through restraint** — destructive actions gated and colored danger;
   warnings amber; capability claims always backed by connector data.
3. **4px grid** — all spacing/size tokens are multiples of 4 (8 for macro rhythm).
4. **Tabular data discipline** — numerics always tabular; sizes/dates via Intl.
5. **Non-color-only status** — every semantic color pairs with an icon or label.
6. **Dark/HC parity is a gate, not a variant** — tokens are triads (light/dark/HC).

## 2. Token architecture

```
src/theme/adopted/
  themes.ts     — 5 adopted palettes (theme-light, theme-dark, theme-Nord,
                  theme-Solarized, theme-HighContrast) + 46 semantic variables
  tokens.css    — base palette CSS variables
src/styles/
  tokens.css    — imports adopted/tokens.css + generated-tokens.css; maps
                  --surface-*, --text-*, --accent-*, --status-* into semantic
                  roles (--background, --surface, --primary, etc.) + .hc triad
  index.css     — @import "tailwindcss"; @import "tw-animate-css";
                  @custom-variant dark (&:is(.dark *)); @theme inline
```

Component classes use Tailwind utilities + `@variant` only; no ad-hoc hexes
(enforced by `tests/theme-snapshot.test.ts`).

## 3. Color roles (summary — full table in visual-spec.md & VISUAL-SPEC.md)

- Canvas & Surfaces: `--surface-bg`, `--surface-card`, `--surface-elevated`, `--surface-hover`
- Brand & CTA: `--accent-primary`, `--accent-hover`, `--accent-muted`, `--cta-gradient`
- Semantic status: `--status-good`, `--status-warning`, `--status-danger`, `--status-info`
- Borders & Elevation: `--border-subtle`, `--border-strong`, `--shadow-card`, `--shadow-dialog`
- High Contrast (`.hc` / `.hc.dark`): gradients disabled (`--cta-gradient: var(--primary)`), 2px focus rings

## 4. Typography

Segoe UI first with Microsoft YaHei/Meiryo/Malgun fallbacks via `:lang()`.
Scale: 12 caption / 13 table+secondary / 14 body / 16 section / 20 page / 28 hero;
weights 400 body, 500 emphasis, 600 headings+buttons. Line-heights 1.3 (titles) →
1.45 (body). No font below 12px. All-numeric columns `tabular-nums`.

## 5. Component inventory (18 primitives + 5 shell/domain components)

button (primary gradient / secondary outline / ghost / danger / link; sizes sm–lg + icon) ·
input · label · checkbox · radio-group · switch · tooltip · popover ·
dropdown-menu · context-menu · dialog · alert · badge (incl. confidence) ·
progress (linear + circular gauge) · skeleton · separator · toast (sonner) ·
segmented-control · empty-state · stat-tile · window-controls · results-tree ·
virtual-file-list.

State coverage per component: default / hover / active / focus-visible / disabled /
loading / selected / (indeterminate where applicable) — verified by unit tests +
Playwright e2e.

## 6. Motion

`--dur-fast 120ms` micro, `--dur-base 200ms` panels, `--dur-slow 320ms` surfaces;
standard decelerating cubic-bezier. Progress and live counters animate linearly.
`prefers-reduced-motion` collapses to ≤100ms fades; never removes information.

## 7. Iconography

Lucide interim base (24px, 2px stroke, rounded joins) tinted via `currentColor`,
plus custom 3D target icons (`src/components/ui/brand-icons.tsx`:
Drive3dIcon with system-drive Windows badge and state-dependent BitLocker
open/closed lock, RecycleBin3dIcon, Desktop3dIcon, SelectFolder3dIcon) and
32 corpus-extracted window-chrome SVGs (`src/assets/interim/chrome/`).
Interim assets tracked in `docs/INTERIM-ASSETS.md`.

## 8. Layout system

- Shell: title bar 36px / collapsible sidebar 208px (`w-52`) expanded or 52px collapsed /
  content fluid; window 1240×840 target, 800×600 floor.
- Results: 224px (`w-56`) tree / `1fr` virtualized file list / resizable `220..520px`
  (default `300px`) preview pane with 4px splitter handle.
- Density: comfortable (`--row-height: 40px`, `--pane-pad: 16px`) / compact
  (`--row-height: 32px`, `--pane-pad: 12px`) via `[data-density]` attribute.

## 9. Change process

1. Proposal → `docs/visual-spec.md` & `docs/VISUAL-SPEC.md` row (with source) → `tokens.css` change.
2. Component changes require updated state matrix + a11y check.
3. Breaking token renames logged in this file's changelog section below.

### Changelog

- v0.1.0: initial token set from corpus hard evidence + VLM census.
- v0.2.0: adopted 5-palette theme pair (`src/theme/adopted/`, ADR-007).
- v0.3.0: collapsible sidebar (208px ⇄ 52px), window chrome 4-state SVGs, dialog/menu exit fades, press-scale tokens.
- v0.3.3 (v4 UI pass): Settings/Theme/About consolidated into the three-lines
  header Menu (language becomes its own header dropdown); More Recovery Tools
  replaced by the redesigned Disk Health page; ChronoSnap Backup renamed Scan
  Snapshot; File Repair & Lost Partition Rescue now 2-step direct flows (no
  Start landing screens); 3D drive/recycle-bin target icons introduced.
- v0.3.4 (gate/docs/i18n hygiene, zero behavior change): CI push triggers
  retargeted to the v3 default branch; knip stale-ignore removed; the
  VirtualFileList compiler skip made explicit ('use no memo' + targeted
  eslint disable, ADR-002); 11 unused reference-product screenshots removed
  from the installer payload; v4-surface strings (repair/lost-partition/
  sd-card chrome, scan ticker + celebration) moved into the governed en/zh-CN
  catalogs (406 keys); docs synced to code truth (UI-MAP, screen-manifest,
  IMPLEMENTATION-STATUS, component-inventory, ARCHITECTURE,
  CONNECTOR-CONTRACT post-ADR-008).
