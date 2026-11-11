# ARCHITECTURE

Feature-sliced React 19 application inside a thin Tauri 2 shell, with a
hard connector boundary between UI and (mock) backend.

## Layers

```
src-tauri/            Rust shell: window lifecycle, dialog/opener plugins,
                      3 thin commands (reveal, exists, environment) for the
                      future real adapter. No domain logic.
src/connector/        THE boundary. contract.ts (types) + mock/ + tauri/
                      stub + factory. Only this folder and src/queries/ may
                      import the factory or implementations.
src/queries/          TanStack Query hooks over the connector; the ONLY layer
                      components/features may call. Event streams are exposed
                      as useSyncExternalStore external stores.
src/stores/           Zustand slices: scan session, selection (Set-backed),
                      results filter/sort, UI layout prefs.
src/components/       Design-system primitives (shadcn/ui v4 base, restyled):
                      alert, badge, brand icons, button, checkbox,
                      confidence badge, dialog, empty state (incl. stat tile
                      + capacity bar), file icon, input, label, menus
                      (dropdown/context/popover), progress (linear + gauge),
                      radio group, results tree, skeleton, stepper
                      (segmented control), switch, table, toaster, tooltip,
                      virtualized file list, window controls.
src/features/         One folder per wizard surface: device-selection,
                      scan-progress, results-explorer, lost-partition,
                      preview, recovery-queue, health, repair, backup,
                      settings-about, dev-panel (DEV-gated).
src/app/              providers, theme (5 adopted palettes + derived HC
                      + density), router (hash), shell layout, error
                      boundary.
src/lib/              utils (cn), format (Intl), structured logger.
src/i18n/             en + zh-CN resources (original copy).
src/workers/          hex preview module worker (+ jsdom fallback).
```

## Enforced boundaries

- **Only `src/queries/**` and `src/connector/**` import the connector
  factory/implementations.** Type-only imports of
  `@/connector/contract` are legal everywhere (shared vocabulary).
- **No `invoke(` outside `src/connector/**`** (ast-grep rule + arch-lint).
- **`src/components/**` never imports features.**
- **Features don't import each other**, except the two documented
  compositions: scan-progress → results-explorer (the scan renders inside
  the results surface, per the reference state machine) and
  results-explorer → preview (the preview pane is the explorer's right pane).
- Enforcement: `scripts/arch-lint.mjs` + `rules/*.yml` (ast-grep) +
  `import-x/no-restricted-paths` zones in eslint. All three run in CI.

## Data flow

- **Server state** (devices, trees, pages, health, sessions, jobs) → TanStack
  Query with query keys in `src/queries/client.ts`.
- **Live streams** (scan/recovery progress) → connector event subscriptions
  wrapped in module-level external stores; components subscribe with
  `useSyncExternalStore`; the stores ALSO invalidate the query cache on
  status transitions (that's how completion flows into results pages).
- **UI state** → Zustand (selection uses a Set for 100k+ selections).
- **Pure UI prefs** (theme, density, locale) → localStorage via the theme
  provider and i18n module.

## Error handling

- `ErrorBoundary` + designed `ErrorScreen` (reason + retry + home actions).
- Connector errors carry codes + i18n keys; every surface maps them to
  human copy — no raw exception text, no generic "cannot preview".
- The tauri stub throws loudly on every method (truthful failure).

## State machines worth knowing

- **Scan**: idle → starting → running ⇄ paused → (completed | cancelled |
  failed). Completion auto-saves a session (mock) and offers the celebration
  modal; cancelled scans keep browsable partial results with a truthful
  "search incomplete" banner.
- **Recovery job**: validating → running → (verifying →) completed |
  completed-partial | cancelled | failed; per-item states pending/running/
  done/failed/skipped/verifying.
- **Destination**: unvalidated → validating → allowed | refused(overlap |
  invalid | out-of-space). Refusal is hard at the UI layer.

## ADRs

See `docs/adr/` — router choice, React compiler, preview rendering boundary,
procedural mock index, dependency resolution at scaffold time, the
design-token basis, and the lost-data direct-start flow.
