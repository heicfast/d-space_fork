# Visual Parameters Specification — Corpus & Adopted Token Evidence

> **Sources**:
>
> 1. Adopted theme token layer (`src/theme/adopted/tokens.css` & `src/theme/adopted/themes.ts` per ADR-007)
> 2. Semantic bridge (`src/styles/tokens.css`)
> 3. Corpus QSS & binary visual census (`artifacts/UIUX/01-visual-inventory.md`, `02-color-palette.md`, `03-typography.md`, `04-layout-spacing-grid.md`, `05-components-catalog.md`, `06-icons-assets-manifest.md`, `07-animations-micro-interactions.md`, `21-qss-styling-deep-dive.md`, `35-redesign-reference-atlas.md`, `screenshots/01..57`).

---

## 1. Color System & Theme Palettes

| Semantic Token           | Adopted Variable  | Light (`light`)           | Dark (`dark`)              | High Contrast (`.hc`)    | Corpus / QSS Source                                             |
| ------------------------ | ----------------- | ------------------------- | -------------------------- | ------------------------ | --------------------------------------------------------------- |
| `--background`           | `--bg-window`     | `#F6F8FB`                 | `#0F141B`                  | `#FFFFFF` / `#000000`    | `02-color-palette.md` §1 (`#F5F7FA`–`#F8FAFC` window canvas)    |
| `--surface`              | `--bg-card`       | `#FFFFFF`                 | `#161D27`                  | `#FFFFFF` / `#000000`    | `02-color-palette.md` §1 (`#FFFFFF` card surface)               |
| `--surface-elevated`     | `--bg-elevated`   | `#FFFFFF`                 | `#1D2633`                  | `#FFFFFF` / `#000000`    | Dialog & popover surface (`35-redesign-reference-atlas.md` §12) |
| `--surface-muted`        | `--bg-subtle`     | `#EEF2F7`                 | `#131922`                  | `#F2F4F7` / `#121212`    | Secondary strip & table header fill                             |
| `--surface-hover`        | `--bg-hover`      | `#E7EDF5`                 | `#1F2937`                  | `#E5E7EB` / `#1F2937`    | Row/tile hover fill (`21-qss-styling-deep-dive.md`)             |
| `--surface-selected`     | `--accent-soft`   | `rgba(37, 99, 235, 0.10)` | `rgba(59, 130, 246, 0.16)` | `rgba(0, 56, 204, 0.16)` | Selected tree/table row highlight (`05-components-catalog.md`)  |
| `--foreground`           | `--fg-primary`    | `#0F172A`                 | `#F1F5F9`                  | `#000000` / `#FFFFFF`    | Primary body copy (≥ 14:1 contrast on surface)                  |
| `--foreground-secondary` | `--fg-secondary`  | `#475569`                 | `#94A3B8`                  | `#111827` / `#E5E7EB`    | Metadata, secondary labels (≥ 5.5:1 contrast)                   |
| `--foreground-disabled`  | `--fg-muted`      | `#64748B`                 | `#94A3B8`                  | `#374151` / `#D1D5DB`    | Disabled / placeholder text (≥ 4.5:1 contrast)                  |
| `--primary`              | `--accent`        | `#2563EB`                 | `#3B82F6`                  | `#0038CC` / `#66B2FF`    | Primary CTA & active indicator (`02-color-palette.md`)          |
| `--primary-hover`        | `--accent-hover`  | `#1D4ED8`                 | `#60A5FA`                  | `#002699` / `#99CCFF`    | Primary CTA hover                                               |
| `--border`               | `--border-subtle` | `#E2E8F0`                 | `#243040`                  | `#1F2937` / `#E5E7EB`    | 1px structural hairlines (`04-layout-spacing-grid.md`)          |
| `--border-strong`        | `--border-strong` | `#CBD5E1`                 | `#334155`                  | `#000000` / `#FFFFFF`    | Form control borders & active card outlines                     |
| `--success`              | `--ok`            | `#16A34A`                 | `#22C55E`                  | `#0A6B2D` / `#4ADE80`    | Healthy status, Excellent/Good confidence, completed state      |
| `--warning`              | `--warn`          | `#D97706`                 | `#F59E0B`                  | `#92400E` / `#FBBF24`    | Caution status, Fair confidence, partial/paused state           |
| `--danger`               | `--err`           | `#DC2626`                 | `#EF4444`                  | `#991B1B` / `#F87171`    | Destructive actions, Poor confidence, overlap refusal           |

Additional adopted palettes (`ember`, `tide`, `blossom`) are defined in `src/theme/adopted/tokens.css` and snapshot-pinned in `tests/theme-snapshot.test.ts`.

---

## 2. Typography Scale & Hierarchy

- **Font Stacks** (`src/theme/adopted/tokens.css` & `03-typography.md`):
  - `--font-sans`: `'Segoe UI Variable Text', 'Segoe UI', -apple-system, BlinkMacSystemFont, 'PingFang SC', 'Microsoft YaHei UI', system-ui, sans-serif`
  - `--font-display`: `'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif`
  - `--font-mono`: `'Cascadia Mono', 'Consolas', 'SFMono-Regular', ui-monospace, monospace`
- **Type Ramp** (`src/styles/tokens.css`):
  - `--text-2xs`: `11px` / `16px` (`0.6875rem` / `1rem`) — compact tree counts, hex table cells, badges
  - `--text-xs`: `12px` / `16px` (`0.75rem` / `1rem`) — table headers, status secondary lines, metadata
  - `--text-sm`: `13px` / `18px` (`0.8125rem` / `1.125rem`) — desktop body copy, tree rows, file list rows, buttons
  - `--text-base`: `14px` / `20px` (`0.875rem` / `1.25rem`) — card titles, section headings, dialog body
  - `--text-lg`: `16px` / `22px` (`1rem` / `1.375rem`) — dialog titles, stat tile values
  - `--text-xl`: `20px` / `28px` (`1.25rem` / `1.75rem`) — primary page `<h1>` headings
  - `--text-2xl`: `24px` / `32px` (`1.5rem` / `2rem`) — display metrics
  - `--text-hero`: `28px` / `34px` (`1.75rem` / `2.125rem`) — Scan Complete celebration title
- **Numeric Discipline**:
  - `.tabular` (`font-variant-numeric: tabular-nums`) applied to all byte sizes, file counts, percentages, durations, sector counters, S.M.A.R.T. raw/current/worst/threshold columns, and hex offsets.

---

## 3. Layout, Spacing, Radii, Elevation & Control Sizing

- **Spacing Grid**: 4px base (`4, 6, 8, 12, 16, 20, 24, 32px`).
- **Shell Dimensions**:
  - Title bar height: `--titlebar-h: 36px` (`2.25rem`)
  - Sidebar width: `--sidebar-w: 208px` (`13rem`) expanded; `--sidebar-w-collapsed: 52px` (`3.25rem`) collapsed
  - Results left tree width: `224px` (`w-56`)
  - Results right preview width: `320px` default, resizable `240px–560px`
  - Results bottom CTA bar height: `56px` (`h-14`)
- **Row Density**:
  - Comfortable (`default`): `--row-h: 36px`, `--cell-py: 6px`
  - Compact (`[data-density='compact']`): `--row-h: 28px`, `--cell-py: 2px`
- **Button Sizing** (`src/components/ui/button.tsx`, `05-components-catalog.md`):
  - `sm`: `h-7` (`28px`), `px-2.5`, `text-xs`
  - `md`: `h-8` (`32px`), `px-3.5`, `text-sm`
  - `lg`: `h-10` (`40px`), `px-5`, `text-base` (primary bottom-right CTAs)
  - `xl`: `h-12` (`48px`), `px-7`, `text-base` (Scan Complete `Go to Recover` CTA)
  - `icon`: `size-8` (`32×32px`)
- **Corner Radii** (`src/theme/adopted/tokens.css`):
  - `--radius-xs`: `4px` (inline pills, menu items, icon buttons)
  - `--radius-sm`: `6px` (buttons, inputs, checkboxes)
  - `--radius-md`: `8px` (popovers, banners, inner panels)
  - `--radius-lg`: `12px` (device cards, section cards)
  - `--radius-xl`: `16px` (dialogs, celebration modal)
- **Elevation Levels**:
  - `--shadow-card`: subtle 1px border + low-blur resting elevation
  - `--shadow-overlay`: menu / popover / hovered card elevation
  - `--shadow-dialog`: modal dialog elevation with `--overlay-scrim` backdrop

---

## 4. Motion & Feedback Tokens

- **Durations** (`src/theme/adopted/tokens.css`, `docs/MOTION-SPEC.md`):
  - `--dur-instant`: `60ms`
  - `--dur-fast`: `120ms` (route fade, menu/popover fade)
  - `--dur-normal`: `200ms` (dialog enter/exit, sidebar width collapse, progress bar transform)
  - `--dur-slow`: `320ms`
- **Easings**:
  - `--ease-out`: `cubic-bezier(0.16, 1, 0.3, 1)`
  - `--ease-in`: `cubic-bezier(0.7, 0, 0.84, 0)`
  - `--ease-standard`: `cubic-bezier(0.2, 0, 0, 1)`
- **Press Microinteractions**:
  - `--press-scale`: `0.985` (buttons, checkboxes, radio items)
  - `--press-opacity`: `0.75` (segmented controls)
- **Reduced Motion**:
  - `@media (prefers-reduced-motion: reduce)` collapses all animation/transition durations to `0.001ms` and disables pulsing/stripes (`src/theme/adopted/tokens.css`).
