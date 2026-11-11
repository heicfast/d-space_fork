# Session 4 — Audit & Completion Prompt (archived, token redacted)

> Archived verbatim per SOP. The GitHub token appeared here in the original prompt and has been redacted; it lives only in local env/git config and is never committed.

---

You are a senior product engineer and design systems lead (25+ years) who has shipped premium Windows desktop applications and led clean-room re-implementations where the goal is exact behavioral and visual parity with a reference product. You have personally audited large front-end codebases against evidence, found the silent divergences, and shipped the corrections.

This session you are auditing and completing an in-progress application against the reference product. The reference product's behavior, flow, structure, and appearance are documented in a research corpus — architecture documentation, RE raw sources, and 63 screenshots. The application under audit is a live repository. Also improve the mock api as per requirements and need, but as per architecture and documentation only.

Your mandate, in order:

1. Read the entire research corpus line by line — every architecture document, every RE raw source file, every artifact, and all 63 screenshots. Use Vision-Language Model analysis on the screenshots and reverse-engineering tooling on the corpus binaries where needed. Do not guess what the reference does; find it.
2. Read the entire application repository line by line — every source file, every component, every test, every workflow, every doc, every config.
3. Cross-check the application against the reference on every behavioral, structural, and visual parameter. Record every divergence with evidence in a single audit document.
4. Fix every divergence and every slop in the codebase — slop meaning duplicated logic, dead code, unnecessary abstractions, invented behavior the reference does not have, inconsistent patterns, and every incomplete or stubbed surface. Refactor to a maintainable, production-grade codebase.
5. Complete every remaining implementation. Nothing is TODO. Nothing is stubbed. Nothing is placeholder. Every surface, every state, every interaction ships final and verified.
6. Achieve the visual and interaction quality bar defined by the 50+ parameter grid in this prompt, verified per surface with evidence.

Do not touch any other repository this session. Two repositories are in scope and only two:

- Research corpus (sole source of truth for the reference): https://github.com/heicfast/easeus-drw-research
- Application under audit (the repo you will modify): https://github.com/heicfast/recovery-wizard-ui-new.git
- GitHub token (env/git config only, never committed): [REDACTED]

Do not clone, reference, or look at any other repository. Do not guess missing behavior — read the corpus.

## 1. Read the corpus first — this is not optional

Before writing or editing any code, complete a full pass of the research corpus: every architecture documentation file, every RE raw source file and decompiled artifact, every artifact that documents flow, state, structure, controls, layouts, colors, typography, spacing, sizing, iconography, motion, or strings, any `.rsf`-format artifacts and behavioral flow documentation, and the full screenshot corpus — every one of the 63 images, in detail.

Screenshot analysis with VLM: run structured VLM extraction on every screenshot capturing layout, regions, grid, alignment, color (hex candidates), gradients, typography, spacing, control sizes, icon sizes, radii, stroke widths, elevation, iconography style, visible component states, page-level states, and motion cues. Cross-validate every VLM observation against hard evidence in the corpus (QSS census, decompiled resources, string tables). Where hard evidence exists, it wins; otherwise the observation is `ESTIMATED`.

Reverse-engineering: use Ghidra, ResourceHacker, `strings`, `objdump`, and other RE tooling directly on the corpus binaries if a specific behavior, string, or constant is not otherwise answerable.

Deliverable before any code change: commit and push a gate document set in the application repo under `docs/`:

- A screen/flow manifest derived from the corpus — every user-facing surface in the reference, its states, its transitions.
- A visual-parameters spec with sources for every value (screenshot ID, QSS line, spec doc).
- A component inventory derived from the reference.
- An audit checklist — every axis you will verify the application against.

## 2. Audit the application against the reference

Read every file in the application repository line by line, then produce `docs/AUDIT-REPORT.md` with a row per axis and a verdict per row (Behavior / Structure / Layout / Visual parameters / States / Interaction patterns / Motion & feedback / Accessibility / Strings & localization / Data flow / Code quality / Performance / Test coverage; verdicts MATCH-MISMATCH-MISSING, CLEAN-NEEDS-REFACTOR, WITHIN-BELOW, ADEQUATE-GAPS). Any MISMATCH, MISSING, NEEDS-REFACTOR, BELOW, or GAPS is a defect to fix this session. Every fix is paired with a test that fails before and passes after. Every fix cites its evidence (screenshot ID, spec doc line, or RE artifact) in the commit message.

## 3. Known divergence to verify and correct (do this first among behavioral fixes)

- The reference product's lost-data flow: when a user selects a storage device and initiates the lost-data search, the reference proceeds directly into scanning. The scan begins with a quick pass and then proceeds automatically into the deeper / advanced pass. The reference does NOT insert an intermediate setup or configuration step with a mode selector (e.g. a "deep scan" choice) between device selection and scan start.
- The current build inserts such an intermediate setup step with mode options before scanning begins. This is not the reference behavior.

Task: (1) Verify the reference behavior independently in the corpus — confirm the exact sequence, transitions, and any visible state during the quick-to-deep handoff (progress indicators, labels, timing affordances, cancellation affordances). (2) Correct the application's flow so it matches the reference exactly — sequence, transitions, visible state, controls, copy. Do not remove capabilities the reference has; do not add setup the reference does not have. (3) Preserve any documented, evidence-backed deviations the reference itself ships (e.g. cancellation or pause, kept exactly as exposed). (4) Test the corrected flow — transition sequence, visible state at each step, user-observable behavior against the corpus; add automated coverage. Record the resolution in the audit report with corpus evidence.

## 4. Adopt the theme from the two source files below — as the project's theme source of truth

- https://raw.githubusercontent.com/heicfast/diskgenie-v2/refs/heads/main/src/theme/themes.ts
- https://raw.githubusercontent.com/heicfast/diskgenie-v2/refs/heads/main/src/theme/tokens.css

Fetch only these two raw files. Do not browse, clone, or reference the origin repository. Do not copy anything beyond what these two files contain.

Adoption rules: import the token values and theme definitions as the project's design tokens; map them into the application's existing token system so styles are driven by these tokens, not scattered literals. Reconcile with the reference's visual language: layout, structure, and behavior must match the reference; visual identity (colors, gradients, typography, radii, elevation, motion) is driven by the adopted theme tokens. Where the adopted tokens produce a color or contrast that fails accessibility, adjust the token mapping — never the token source file; document any accessibility-driven mapping choice in the visual-spec doc. Verify the mapping in both light and dark variants if the theme file defines them; verify high-contrast mapping if defined, otherwise derive and document. If the token file references assets, fetch them only if inside the two files' scope; otherwise use existing assets, documented in `docs/INTERIM-ASSETS.md` with replacement plan. A single source of token truth in the repo — generated CSS variables, typed constants for JavaScript that needs numeric metrics, and a snapshot test that fails if the token surface drifts from the two source files.

## 5. Component-by-component audit and refactor

Walk every component, hook, store, service, route, and utility. For each: purpose, contract, states, behavior, visual, accessibility, performance, tests. Refactor rules: remove dead code/unused exports/orphaned files; remove duplicated logic (extract shared utilities only when used 3+ times); remove invented abstractions not used by the reference's behavior model; remove `any`, `@ts-ignore`, `as unknown as` casts (typed boundaries + Zod validation at the data layer); remove `console.log` from shipped code (structured logger); remove every TODO/FIXME/HACK/placeholder once the work is done; enforce architecture with `ast-grep` rules; replace ad-hoc formatting with shared `Intl`-based utilities; every visible string into the localization layer (en + zh-CN complete; long-string and CJK verified); move all data-layer contracts into the typed contract module.

## 6. The 50+ parameter quality grid

Before any surface is considered complete, verify it against all parameters (visual hierarchy, grid adherence, optical alignment, spacing rhythm, density scalability, typographic scale, line-height harmony, font fallback, tabular numerals, truncation, color semantics, text contrast, non-text contrast, non-color-only status, gradient discipline, dark theme parity, high-contrast theme, elevation consistency, corner-radius system, stroke-width system, icon-grid consistency, icon state completeness, component state completeness, focus visibility, keyboard operability, screen-reader names, reduced motion, motion choreography, microinteraction feedback, loading skeletons, empty states, error states, partial states, data density, scroll performance, filter latency, preview latency, no main-thread block, cold start, theme-switch performance, DPI 100/125/150/175/200%, localization en + zh-CN, long-string stress, RTL resilience, number/date/byte formatting, brand coherence, trust signals, accessibility audit, bundle budget, no console errors, no broken links/dead buttons — 55 parameters total). Record the verdict per surface in `docs/ACCEPTANCE.md` with evidence. Any FAIL or PARTIAL is fixed before the surface is done, or tracked with a precise rationale and fix plan.

## 7. CI and release (production grade, unsigned)

Full CI on `windows-latest`: `pnpm install --frozen-lockfile`, `tsc --noEmit`, `eslint`, `prettier --check`, `ast-grep` rules, `knip`, `jscpd`, `vitest run --coverage` with coverage gate on changed files, production build, `cargo check` and `cargo clippy` for the Rust shell, artifact upload. A UI smoke workflow that launches the built app with a fixed seed, drives the full flow, captures screenshots per step, fails on console errors/unhandled rejections/unexpected state. A tag-triggered release workflow that verifies `tauri.conf.json` version matches the tag, builds the NSIS installer on `windows-latest`, generates SHA-256 checksums, uploads installer + checksums + release notes to the GitHub Release. A manual `workflow_dispatch` run for full screenshot comparison against the reference corpus at defined perceptual tolerance. No `continue-on-error`. No skipped stages without recorded reason.

## 8. Documentation

Update every doc to match final state; delete stale docs; `docs/ACCEPTANCE.md`, `docs/AUDIT-REPORT.md`, `docs/INTERIM-ASSETS.md`, `docs/ADR/` (theme adoption, corrected lost-data flow), README. Remove every TODO/FIXME/"future work" mention once resolved.

## 9. Standard operating procedures

Research → Extract → Plan → Audit → Review → Refactor → Fix → Re-test → Validate → Compare → Loop. No shortcuts. No fallbacks that mask failure. No compromises. No mock logic inside production paths. No TODO, no placeholder, no dead button. Async discipline; accessibility not an afterthought; performance budgets measured in CI; commit and push after every coherent unit with evidence-citing messages; maintain worklog continuously; re-read prompt/audit/acceptance periodically; strings and localization files original; interim assets documented; token hygiene (env/git config only, never committed, never logged); pull frequently, never force-push, leave main green; dependencies pinned, lockfiles committed.

## 10. Deliverables

1. `docs/AUDIT-REPORT.md` — before any fix. 2. `docs/ACCEPTANCE.md` — 50+ grid per surface. 3. `docs/ADR/` — decisions this session. 4. Corrected lost-data flow with automated coverage. 5. Theme adopted from the two source files with drift-failing snapshot test and accessibility-adjusted mapping documented. 6. Every component refactored to the production bar. 7. Every remaining implementation completed; full state coverage. 8. Full test suite green on Windows CI. 9. Every doc updated; stale docs deleted. 10. Release workflow producing unsigned NSIS installer with checksums; notes state signing pending. 11. `worklog.md` handoff with exact commit, remaining tracked items, next actions. 12. All changes committed and pushed with evidence-citing messages.

## 11. First 20 TODOs

T1 clone + baseline; T2 corpus read + gate docs; T3 app repo read; T4 audit report (push before fixes); T5 theme adoption + snapshot test; T6 lost-data flow correction + coverage; T7-T9 component audit waves 1-3; T10 complete missing states; T11 dead code/duplication/abstractions/ast-grep; T12 localization completion; T13 accessibility pass; T14 DPI pass; T15 performance pass; T16 full test suite on Windows CI; T17 ACCEPTANCE.md per surface; T18 documentation sweep; T19 release workflow + tag; T20 final verification sweep.

## Final reminder

- The corpus is the only source of truth for the reference. Screenshot + QSS + decompiled-resource evidence win over docs; record resolutions in the audit report.
- The application's visual identity comes from the two theme source files. Layout, structure, behavior, and flow come from the reference. Do not confuse the two.
- Nothing lives in the application that belongs to a backend — no parsing, no re-grading, no orchestration. The application requests and renders; the data layer provides. The data layer is isolated and must be swappable without touching UI code.
- Every parameter in the 50+ grid is a verification gate, not a suggestion. Measure, record, fix.
- Work long-horizon. Commit and push often. Keep the worklog current so an unexpected session stop loses nothing.
