# Session Audit & Completion Prompt (token redacted)

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
- GitHub token (env/git config only, never committed): REDACTED

Do not clone, reference, or look at any other repository. Do not guess missing behavior — read the corpus.

---

## 1. Read the corpus first — this is not optional

Before writing or editing any code, complete a full pass of the research corpus:

**Read line by line:**

- Every architecture documentation file.
- Every RE raw source file and decompiled artifact in the corpus.
- Every artifact that documents flow, state, structure, controls, layouts, colors, typography, spacing, sizing, iconography, motion, or strings.
- Any `.rsf`-format artifacts and behavioral flow documentation.
- The full screenshot corpus — every one of the 63 images, in detail.

**Screenshot analysis with Vision-Language Model:**
Run structured VLM extraction on every screenshot, capturing layout, regions, grid, alignment, color (with hex candidates), gradients (stop colors and direction), typography, spacing, control sizes, icon sizes, radii, stroke widths, elevation, iconography style, visible component states, page-level states (initial, loading, empty, error, partial, permission-denied, disconnected, success), and any motion cues.

Cross-validate every VLM observation against hard evidence in the corpus (QSS census, decompiled resources, string tables). Where hard evidence exists, it wins. Where it does not, the observation is `ESTIMATED` and resolved during implementation or tracked precisely.

**Reverse-engineering:**
Use Ghidra, ResourceHacker, `strings`, `objdump`, and other RE tooling directly on the corpus binaries if a specific behavior, string, or constant is not otherwise answerable. Do not assert what the reference does without evidence.

**Deliverable before any code change:** commit and push a gate document set in the application repo under `docs/`:

- A screen/flow manifest derived from the corpus — every user-facing surface in the reference, its states, its transitions.
- A visual-parameters spec with sources for every value (screenshot ID, QSS line, spec doc).
- A component inventory derived from the reference.
- An audit checklist — every axis you will verify the application against.

Do not skip reading. If a file is large, chunk it and take structured notes. The user has stated in every session that skipping is not allowed.

---

## 2. Audit the application against the reference

Read every file in the application repository line by line, then produce `docs/AUDIT-REPORT.md` with a row per axis and a verdict per row:

| Axis                   | Verdicts                   | What you check                                                                                       |
| ---------------------- | -------------------------- | ---------------------------------------------------------------------------------------------------- |
| Behavior               | MATCH / MISMATCH / MISSING | Every interaction, transition, state, error path, ordering.                                          |
| Structure              | MATCH / MISMATCH / MISSING | Screen inventory, flow graph, information architecture, component composition.                       |
| Layout                 | MATCH / MISMATCH / MISSING | Grid, alignment, gutters, proportions, responsive behavior.                                          |
| Visual parameters      | MATCH / MISMATCH / MISSING | Colors, gradients, typography, spacing, sizing, radii, elevation, iconography.                       |
| States                 | MATCH / MISMATCH / MISSING | Initial, loading, empty, error, partial, permission-denied, disconnected, success — per surface.     |
| Interaction patterns   | MATCH / MISMATCH / MISSING | Selection, filtering, sorting, paging, search, context actions.                                      |
| Motion & feedback      | MATCH / MISMATCH / MISSING | Transitions, microinteractions, feedback, reduced-motion behavior.                                   |
| Accessibility          | MATCH / MISMATCH / MISSING | Keyboard, focus, screen-reader names, contrast, ARIA, high-contrast, reduced motion.                 |
| Strings & localization | MATCH / MISMATCH / MISSING | Every visible string; en + zh-CN parity; long-string and CJK resilience.                             |
| Data flow              | MATCH / MISMATCH / MISSING | How state flows through components; no invented layers or side-channels.                             |
| Code quality           | CLEAN / NEEDS-REFACTOR     | Duplication, dead code, over-abstraction, inconsistent patterns, `any`, `@ts-ignore`, `console.log`. |
| Performance            | WITHIN / BELOW             | Cold start, scroll at scale, filter latency, preview latency, main-thread blocking.                  |
| Test coverage          | ADEQUATE / GAPS            | Unit, component, UI smoke, accessibility — per surface and per interaction.                          |

Any `MISMATCH`, `MISSING`, `NEEDS-REFACTOR`, `BELOW`, or `GAPS` is a defect to fix this session. Every fix is paired with a test that fails before and passes after. Every fix cites its evidence (screenshot ID, spec doc line, or RE artifact) in the commit message.

---

## 3. Known divergence to verify and correct (do this first among behavioral fixes)

- **The reference product's lost-data flow.** When a user selects a storage device and initiates the lost-data search, the reference product proceeds directly into scanning. The scan begins with a quick pass and then proceeds automatically into the deeper / advanced pass. The reference product does **not** insert an intermediate setup or configuration step with a mode selector (for example, a "deep scan" choice) between device selection and the start of the scan.
- **The current build.** Verify and ensure the application's flow matches the reference exactly — sequence, transitions, visible state, controls exposed, and copy.

---

## 4. Adopt the theme from the two source files below — as the project's theme source of truth

- https://raw.githubusercontent.com/heicfast/diskgenie-v2/refs/heads/main/src/theme/themes.ts
- https://raw.githubusercontent.com/heicfast/diskgenie-v2/refs/heads/main/src/theme/tokens.css

Fetch only these two raw files. Do not browse, clone, or reference the origin repository. Do not copy anything beyond what these two files contain.

---

## 5. Component-by-component audit and refactor

Walk every component, hook, store, service, route, and utility in the application. Verify Purpose, Contract, States, Behavior, Visual, Accessibility, Performance, and Tests.

---

## 6. The 50+ parameter quality grid

Verify every surface against all 55 parameters in `docs/ACCEPTANCE.md` with evidence.

---

## 7. CI and release (production grade, unsigned)

- Full CI on `windows-latest`: `pnpm install --frozen-lockfile`, `tsc --noEmit`, `eslint`, `prettier --check`, `ast-grep` rules, `knip`, `jscpd`, `vitest run --coverage` with coverage gate on changed files, production build, `cargo check` and `cargo clippy` for the Rust shell, and an artifact upload.
- UI smoke workflow launching the built application with a fixed seed and driving the full flow.
- Tag-triggered release workflow verifying `tauri.conf.json` version matches the tag, building the NSIS installer on `windows-latest`, generating SHA-256 checksums, and uploading to the GitHub Release.
- Manual `workflow_dispatch` run for the full screenshot comparison against the reference corpus at the defined perceptual tolerance.
- No `continue-on-error`. No skipped stages without a recorded, reviewed reason.

---

## 8. Documentation

Update every doc in the repository to match the final state (`docs/ACCEPTANCE.md`, `docs/AUDIT-REPORT.md`, `docs/INTERIM-ASSETS.md`, `docs/ADR/` / `docs/adr/`, `README.md`, gate docs). Delete stale docs and remove resolved-divergence mentions.
