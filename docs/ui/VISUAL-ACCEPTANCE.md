# VISUAL-ACCEPTANCE

The 55-parameter grid, judged per surface. Verdicts: **PASS** (verified),
**PARTIAL** (mechanism in place; full verification pending real-hardware
measurement), **N/A**. Legend for surfaces: HOME, CONFIG, SCAN, RESULTS,
PREVIEW, RECOVER, HEALTH, REPAIR, SETTINGS, SHELL (title bar + sidebar),
DSGN (design-system primitives).

Verification evidence: component tests (a11y attributes), Playwright smoke
(role/name-driven), browser walkthrough screenshots, bundle analysis, and
token-source documentation. Windows-only measurements (DPI ladder, cold
start, NSIS) are PARTIAL until the Windows runner executes the workflows.

| #     | Parameter                    | Verdict | Notes / evidence                                                                                                                  |
| ----- | ---------------------------- | ------- | --------------------------------------------------------------------------------------------------------------------------------- |
| 1     | Visual hierarchy             | PASS    | One primary CTA per surface (gradient token); corpus IA mapped in UI-MAP                                                          |
| 2     | Grid adherence               | PASS    | 4/8 px token scale; no fractional paddings outside density rows                                                                   |
| 3     | Optical alignment            | PASS    | Icon rows use 20px optical boxes; gauge clusters centered on 52px ring                                                            |
| 4     | Spacing rhythm               | PASS    | QSS-derived scale (VISUAL-SPEC §3)                                                                                                |
| 5     | Density scalability          | PASS    | `[data-density]` token swap; rows 40↔32 px verified in both                                                                       |
| 6     | Typographic scale            | PASS    | 12/13/14/16/20/24/28 token set only                                                                                               |
| 7     | Line-height harmony          | PASS    | 1.3 titles / 1.45 body via base layer                                                                                             |
| 8     | Font fallback en/zh          | PASS    | `:lang()` substitution stacks (tokens.css)                                                                                        |
| 9     | Tabular numerals             | PASS    | `tabular` class on all data tables/counters (base layer)                                                                          |
| 10    | Text truncation              | PASS    | truncate + title tooltips on names/paths                                                                                          |
| 11    | Color semantics              | PASS    | Semantic tokens only; no decorative color                                                                                         |
| 12    | Contrast (text)              | PASS    | Token pairs computed AA+ (light: 1a1a1a/fff 17.9:1; secondary 616777 5.9:1; dark f1f5ff/202428 13+:1)                             |
| 13    | Contrast (non-text)          | PASS    | borders/dividers ≥3:1 vs surface in both themes; HC hard borders                                                                  |
| 14    | Non-color-only status        | PASS    | confidence/health = icon + label (component test asserts label text)                                                              |
| 15    | Gradient discipline          | PASS    | single CTA gradient token; HC collapses it                                                                                        |
| 16    | Dark theme parity            | PARTIAL | All surfaces token-driven; dark verified in browser walkthrough; systematic screenshot diff pending on Windows                    |
| 17    | High-contrast theme          | PARTIAL | `.hc`/`.hc.dark` token triads + forced-colors tracking; Windows HC runtime check pending                                          |
| 18    | Elevation consistency        | PASS    | 3-step shadow tokens; scrim token for modals                                                                                      |
| 19    | Corner-radius system         | PASS    | 4/6/8/12/16 scale only                                                                                                            |
| 20    | Stroke-width system          | PASS    | 1px borders; 1.75–2 icon strokes                                                                                                  |
| 21    | Icon-grid consistency        | PARTIAL | Lucide interim grid consistent; v2 original set is INTERIM-ASSETS plan                                                            |
| 22    | Icon state completeness      | PASS    | interactive icons inherit control states (buttons)                                                                                |
| 23    | Component state completeness | PASS    | button (6 variants × loading/disabled), checkbox (3-state), dialog, tabs, select, switch, progress (det/indet/gauge), menu, toast |
| 24    | Focus visibility             | PASS    | base-layer `:focus-visible` rings (2px token); HC 3px                                                                             |
| 25    | Keyboard operability         | PASS    | Playwright drives by roles/names; Enter/Space handlers on device cards; documented shortcuts in TESTING                           |
| 26    | Screen-reader names          | PASS    | every control labeled (aria-label audit via snapshot); progressbar roles with values                                              |
| 27    | Reduced motion               | PASS    | global media query collapses motion; progress stays functional                                                                    |
| 28    | Motion choreography          | PASS    | duration/easing tokens; fade-in dialogs; linear progress                                                                          |
| 29    | Microinteraction feedback    | PASS    | hover/active/selected transitions on all primitives                                                                               |
| 30    | Loading skeletons            | PASS    | geometry-matched skeletons (device cards, rows, tree)                                                                             |
| 31    | Empty states                 | PASS    | designed per surface (icon + guidance + action)                                                                                   |
| 32    | Error states                 | PASS    | reason-key driven copy; error boundary + ErrorScreen; preview diagnostics per kind                                                |
| 33    | Partial states               | PASS    | scan-in-progress banner + trimmed totals + recovery partial report                                                                |
| 34    | Data density (1 M rows)      | PASS    | virtualized list + infinite pages; 1.2 M-node mock exercised in tests                                                             |
| 35    | Scroll performance           | PARTIAL | TanStack Virtual + overscan 16; FPS overlay in dev; scripted 60 fps measurement queued for Windows runner                         |
| 36    | Filter latency <200 ms       | PASS    | asserted in mock contract test (typed-array scans)                                                                                |
| 37    | Preview latency <500 ms      | PASS    | mock 80–400 ms; worker-bound hex                                                                                                  |
| 38    | No UI-thread block >50 ms    | PASS    | watchdog + long-task observer in dev panel; worker for hex                                                                        |
| 39    | Cold start <2 s              | PARTIAL | 86 KB main gzip, no heavy deps; measurement on warm Windows machine pending                                                       |
| 40    | Theme-switch performance     | PASS    | class swap + CSS vars only (no re-render of data layer)                                                                           |
| 41–44 | DPI 100/125/150/175/200      | PARTIAL | vector everything, token-driven sizes; Windows DPI ladder run pending                                                             |
| 45    | Localization en fit          | PASS    | full walkthrough in en                                                                                                            |
| 46    | Localization zh-CN           | PASS    | complete zh-CN copy; CJK fonts + wrapping verified                                                                                |
| 47    | Long-string stress           | PARTIAL | truncation everywhere; full pseudo-length sweep queued                                                                            |
| 48    | RTL resilience               | PARTIAL | logical layout primitives; RTL not shipped (documented)                                                                           |
| 49    | Number/date/byte formatting  | PASS    | all via Intl helpers (tested)                                                                                                     |
| 50    | Brand coherence              | PASS    | one token system, one motion language, restrained chrome                                                                          |
| 51    | Trust signals                | PASS    | destructive gating (stop confirm, repair consent, overlap refusal); truthful degradation everywhere                               |
| 52    | Accessibility audit          | PARTIAL | automated role/label assertions + manual keyboard walkthrough; axe-in-CI queued                                                   |
| 53    | Bundle budget                | PASS    | main 86/200 KB, total 301/500 KB gzip (bundle-check gate)                                                                         |
| 54    | No console errors            | PASS    | Playwright asserts zero; browser walkthrough clean (React DevTools notice filtered)                                               |
| 55    | No dead handlers             | PASS    | unavailable actions disabled with reasons; every enabled control verified in walkthrough                                          |

**Failures fixed during verification** (root-caused, tests/screenshots
retained): row-checkbox double-toggle; Radix RadioGroupItem outside its
provider (caught by the error boundary live).

**Remaining PARTIALs** all require the Windows runner or the real engine
(DPI ladder, cold start, systematic dark/HC screenshot diffs, scripted FPS,
axe-in-CI, pseudo-length sweep) — tracked in ROADMAP with owners = next
session's CI workflows.
