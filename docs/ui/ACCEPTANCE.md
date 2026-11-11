# ACCEPTANCE — Session 4 verdicts (55-parameter quality grid)

> **Historical record (session 4).** The verification-stack numbers below
> are the session-4 snapshot (110/110 vitest, bundle 98.0/283.1 KB gzip,
> jscpd 0 clones, CONFIG surface pre-ADR-008) and are preserved as written.
> Current state at v0.3.4: 145/145 vitest, 12/12 Playwright e2e, bundle
> 124.6/309.2 KB gzip, i18n 406 keys, jscpd 1 clone (0.03%, gate-passing),
> and the scan-configuration surface no longer exists (ADR-008).

Per-surface verdicts against the mandate's quality grid, with evidence.
Scale: **PASS** (verified with evidence) · **PARTIAL** (works, verification
Windows-gated — rationale + plan noted) · **FAIL** (none open).

Verification stack used this session:

- Local (this repo's sandbox): typecheck, eslint, prettier, arch-lint,
  ast-grep (2 rules), knip (0 findings), jscpd (0 clones), vitest 110/110,
  Playwright e2e 12/12, production build + bundle budget (98.0/200 KB main,
  283.1/500 KB total),
  live browser walkthrough of the production build (zero console/page
  errors), axe-core 4.10 audits (zero violations — see T13), keyboard
  Tab-order walkthrough with computed focus-ring inspection
  (docs/evidence/a11y/), full-corpus VLM extraction
  (docs/evidence/vlm/shots/, 63/63).
- Windows CI (windows-latest): the full CI workflow (typecheck→lint→format→
  arch-lint→ast-grep→knip→jscpd→tests→build→bundle→cargo check→clippy),
  UI-Smoke (Playwright golden path + Tauri debug build), Release (NSIS).
- Corpus evidence: screenshots (63), QSS census, string tables, decompiled
  wiring — citations in AUDIT-REPORT.md and worklog.md.

## 1. Home / device selection — PASS

Direct-start flow corrected per corpus (ADR-008: device → scan, no
configuration step; evidence: artifact 04 §4 scanClickedSlot, GT-scan §3).
Device cards: nested-interactive resolved (radio on the card body, actions
are siblings — axe). Quick targets are real scans (Recycle Bin/Desktop are
virtual targets in the fixtures). States: loading skeletons, empty, error,
permission-denied banner, disconnected banner, healthy/caution chips —
all rendered with adopted tokens; contrast verified (axe, 6 palettes).

## 2. Scan progress (live) — PASS

Corpus bottom cluster (atlas §6, GT-scan §2/§7): gauge + "Quick/Advanced
scanning X%" status line + "Files found: N (size)" + remaining time +
"Reading sector: X/Y" (512 B sectors, corpus convention), full-width
"Searching: <path>" ticker, pause/stop with confirm dialog (stat tiles +
risk framing), celebration modal (no-Esc per corpus §7, stats + "Go to
Recover"), device-lost non-dismissable reconnect modal. Real elapsed time
from engine timestamps. Browse-while-scanning with progressive fill.

## 3. Results explorer — PASS

3-pane layout (tree / virtualized list / preview with drag resizer),
toolbar (nav cluster, breadcrumb-heading, view toggle with named icon
radios, filter, search), context menu (all 6 items implemented — Preview
previews the active row, Smart select picks best-per-name), select-all
tri-state over the FULL filtered set (all-mode with exclusions), icons view
(tile renderer with thumbnails), search-results breadcrumb with removable
chip, partial-state banner on stopped scans (with honest search-again
action preserving pathScope). Grid ARIA complete (columnheader/rowgroup/
gridcell, aria-sort). axe: 0 violations.

## 4. Filter bar — PASS

Corpus toggles (Existing files / Hide 0 KB) + Custom type-suffix entry
("*.doc; *.xlsx" parsing), corpus size buckets (<512 KB … >1 GB), date
presets + custom range, shared Checkbox/RadioGroup primitives, correct
aria-labels on the size inputs. Filter latency <200 ms asserted in tests
(grid #36) on the 340k-node space.

## 5. Preview pane — PASS

Connector-owned preview kinds (no UI-side engine knowledge), bounded
payloads, hex in a worker (deduped formatter), tri-state loading/failure/
content with connector reasonKeys, resizer with full separator semantics
(aria-valuenow/min/max + i18n label), confidence badge never re-graded in
the UI (grid #14).

## 6. Recovery (destination → queue → report) — PASS

Hard overlap refusal wired to the true source device (audit §2.6 fixed),
neededBytes from scan-state averages, recover-all is a REAL lazy job
(batch ticks + cursor; 340k files complete with exact counts), cancel is
confirm-gated (corpus CScanCommonPopDlg family), queue show-more, report
with processedTotal + truncation notice, export toast truthful. States:
validating, running, completed, completed-partial, cancelled, failed.

## 7. Sessions — PASS

Save/auto-save (corpus: auto_<Volume>_<timestamp>.rsf naming), list,
resume from Home, corrupt-session honest error, reasonKey semantics fixed
(session-not-found ≠ corrupt). Deliberate divergence (documented, UI-MAP
§0): the reference's retention/license gates are commercial funnel
surfaces and are not re-implemented.

## 8. Health — PASS

SMART table (7 columns, corpus DiskHealth artifact 19), good/caution/bad
statuses (failing-disk scenario added so 'bad' is reachable — audit §4
closed), wear displayed as a percentage, caution card + guidance.

## 9. Repair — PASS

Typed-consent gate (REPAIR), findings with i18n descriptions (missing-key
bug fixed this session), undo log + rollback with interpolated counts
(concatenation bug fixed), transactional framing per the corpus repair
model.

## 10. Settings / About — PASS

Full 7-mode theme picker (5 adopted palettes + system + derived HC),
corpus-backed Scale control (100/150/200% — the reference's DPI menu),
density, language (en/zh-CN), diagnostics ring buffer with export, About
dialog with truthful connector-sourced rows (all keys present — the
missing-about-namespace bug is fixed).

## 11. Shell / navigation — PASS

Sidebar IA per corpus (Data Recovery / Advanced sections), results-flow
variant with live-scan indicator (pulse on dot, never on text), title-bar
controls all named + i18n, theme menu covers all modes, no duplicate
links, no dead buttons (About is a real dialog).

## 12. Localization — PASS

en + zh-CN surface-identical AND exactly matching the referenced surface
(tests/i18n-sweep.test.ts: missing = 0, unused = 0; parity test for
en⇄zh). Plural pairs handled as single i18next entries. CJK font stacks
per locale (corpus QSS [style]). zh-CN spot-checked live (screenshot
05-results-zh.png). Long-string resilience: tabular numerals + truncation
with title affordances across data surfaces.

## 13. Theme & visual identity — PASS (ADR-007)

Adopted sources verbatim (src/theme/adopted/), drift-failing snapshot
test, 5 palettes + derived HC live-verified with screenshots, contrast
re-measured per family with axe (text-grade accents re-derived where
tints failed), category tints never color-alone. The brand CTA gradient
exception is documented.

## Grid parameters: measured this session

| #     | Parameter                                     | Verdict | Evidence                                                                                                                                                                                                                                                                                                         |
| ----- | --------------------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1–4   | Hierarchy, grid, alignment, spacing           | PASS    | atlas-derived layout; 4px-based utility spacing; VLM cross-check                                                                                                                                                                                                                                                 |
| 5     | Density scalability                           | PASS    | comfortable/compact row metrics via [data-density]                                                                                                                                                                                                                                                               |
| 6–8   | Type scale, line-height, fallbacks            | PASS    | corpus 14px base; Segoe UI + CJK stacks; zh-CN rendered live                                                                                                                                                                                                                                                     |
| 9     | Tabular numerals                              | PASS    | .tabular on every numeric context (base layer)                                                                                                                                                                                                                                                                   |
| 10    | Truncation                                    | PASS    | truncate + title affordances; no mid-glyph clip (zh spot check)                                                                                                                                                                                                                                                  |
| 11–14 | Color semantics + contrast + non-color status | PASS    | axe 0 violations × 8 states; confidence = icon+label+color                                                                                                                                                                                                                                                       |
| 15–17 | Gradients, dark parity, HC                    | PASS    | adopted ink-grad (HC flattens); every surface re-audited in dark + HC                                                                                                                                                                                                                                            |
| 18–20 | Elevation, radii, strokes                     | PASS    | adopted shadow/radius scales; single border system                                                                                                                                                                                                                                                               |
| 21–23 | Icon consistency/states                       | PASS    | Lucide single-weight set (interim, documented); control states via tokens                                                                                                                                                                                                                                        |
| 24–27 | Focus, keyboard, SR names, reduced motion     | PASS    | Tab walkthrough (focus ring on every stop); axe names; reduced-motion collapse in base CSS                                                                                                                                                                                                                       |
| 28–29 | Motion, microinteractions                     | PASS    | adopted motion ladder; press/hover/selection feedback; pulse never on text                                                                                                                                                                                                                                       |
| 30    | Loading skeletons                             | PASS    | adopted skeleton/shimmer system; geometry-matched                                                                                                                                                                                                                                                                |
| 31–33 | Empty/error/partial states                    | PASS    | designed EmptyState per surface; reasonKey-driven errors; partial banner wired                                                                                                                                                                                                                                   |
| 34    | Data density                                  | PASS    | 340k-node session virtualized (tests + live run)                                                                                                                                                                                                                                                                 |
| 35–38 | Scroll/filter/preview perf, main thread       | PASS    | tests assert filter <200 ms; hex in worker; dev watchdog; budget gates                                                                                                                                                                                                                                           |
| 39    | Cold start                                    | PARTIAL | Linux build+preview loads <1 s; Windows-hardware Tauri cold start measured in UI-Smoke (Playwright drive) — full number re-measured on the release Windows runner                                                                                                                                                |
| 40    | Theme switch                                  | PASS    | attribute flip, no remount jank (live walkthrough)                                                                                                                                                                                                                                                               |
| 41–44 | DPI 100–200%                                  | PARTIAL | Scale control verified live (100/150/200 zoom, persisted); 125/175% + native DPI raster crispness are Windows-hardware checks — the Windows UI-Smoke runs at runner scaling; per-step screenshots captured; remaining: a dedicated DPI matrix capture on physical Windows hardware when the engine session lands |
| 45–47 | Localization en/zh, long strings              | PASS    | see §12; en + zh live-verified                                                                                                                                                                                                                                                                                   |
| 48    | RTL                                           | PARTIAL | logical properties + flex-first layout (no left/right positioning in panes); full RTL flip not shipped (not a reference surface — corpus ships LTR only); tracked as deliberate non-goal                                                                                                                         |
| 49    | Intl formatting                               | PASS    | all numbers/dates/bytes via Intl (format.ts); 1024-based binary convention unified (corpus GiB-scale values)                                                                                                                                                                                                     |
| 50–51 | Brand coherence, trust                        | PASS    | one adopted identity; destructive actions confirm-gated (stop scan, cancel recovery, repair consent)                                                                                                                                                                                                             |
| 52    | Accessibility audit                           | PASS    | axe 0 violations × 8 states + manual keyboard walkthrough recorded                                                                                                                                                                                                                                               |
| 53–54 | Bundle budget, console cleanliness            | PASS    | 89.4/200 + 273.5/500 KB; zero console/page errors in live runs; e2e fails on console errors                                                                                                                                                                                                                      |
| 55    | No dead surfaces                              | PASS    | every control wired or truthfully disabled with a reason (Open location in mock: disabled + reason); i18n surface = referenced surface exactly                                                                                                                                                                   |

## Open PARTIAL items (precise rationale + plan)

1. **#39 cold start (Windows)** — Tauri NSIS cold-start timing needs
   physical Windows; the UI-Smoke workflow already gates the runnable app
   (Playwright golden path, console-error fail). Plan: record the number on
   the release runner during the engine-integration session (ROADMAP).
2. **#41–44 DPI matrix (125/175%)** — fractional-scale rendering is a
   Windows-display property; layout integrity at 100/150/200 verified via
   the Scale control; per-DPI screenshot capture scheduled with the same
   hardware gate.
3. **#48 RTL** — deliberate non-goal (corpus is LTR-only); primitives are
   direction-agnostic where flex/logical properties govern.
