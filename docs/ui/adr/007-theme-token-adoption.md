# ADR-007: Theme token adoption from the two designated source files

## Status

Accepted (session 3, T5).

## Context

Session 3's mandate separates the app's visual identity from the reference's:
layout, structure, behavior, and flow follow the research corpus (the
reference product), while the visual identity — colors, gradients,
typography, radii, elevation, motion — must come from exactly two adopted
source files:

- `src/theme/adopted/themes.ts` (verbatim copy of the source's themes.ts)
- `src/theme/adopted/tokens.css` (verbatim copy of the source's tokens.css)

Before this ADR the app used a hand-rolled token set derived from the
reference's QSS census (brand blue #0560fd family), which matched the
reference's look rather than the project's own visual identity.

## Decision

1. **Verbatim adoption.** The two files live under `src/theme/adopted/` and
   are treated as vendored, immutable sources (excluded from knip like a
   dependency). The adopted system is a five-palette, two-layer attribute
   theme: `data-theme="<light|dark|ember|tide|blossom>"` (the palette) plus
   `data-scheme="<light|dark>"` (the family), written together by the
   adopted module's `applyThemeAttributes()`.
2. **Semantic mapping layer.** `src/styles/tokens.css` no longer contains
   palette values; it maps the app's semantic token names onto adopted
   names (surface ← panel, muted ← chip, accent-fill ← ink, success ← free,
   warning ← seg-compressed, danger ← used, elevation ← shadow-1/2/pop,
   motion/radius/spacing ladders pass through under matching names).
   Tailwind's `@theme inline` mapping in `index.css` is unchanged in shape.
3. **Dark family via attributes, not classes.** The `dark:` variant now
   resolves from `[data-scheme="dark"]` (the adopted two-layer system)
   instead of a `.dark` class.
4. **Accessibility-driven mapping choices** (sources are immutable; the
   mapping adjusts — all ratios measured, documented in VISUAL-SPEC §A11):
   - Text-grade accent: `--ink` (#ff6b4a) is 2.80:1 on white; the semantic
     `--accent` (text/links/icons/small fills) maps to a darkened derivative
     (#cc3d15, 4.95:1) in light-family themes; dark-family inks already pass
     (6.4:1+) and map through; Blossom's rose gets the same treatment.
   - Brand CTA exception: the primary button keeps the adopted ink gradient
     - glow with `--on-ink` text — the source's own signature treatment
       (analogous to Apple's system buttons); documented, and the HC layer
       provides the accessible alternative.
   - Status text grades are darkened derivatives of `--free` / `--used` /
     `--seg-compressed`; muted backgrounds keep source values.
   - Focus ring: solid text-grade ink (the source's 0.8-alpha coral ring
     measures 2.9:1; WCAG 2.2 focus indicators need 3:1).
5. **High contrast is derived, not adopted.** The sources define no HC
   palette; the app keeps its `.hc` layer (flat surfaces, saturated
   semantics, hard 1px borders, no gradients/shadows, 3px focus ring),
   re-derived from the adopted ink hue and documented.
6. **Structural geometry stays corpus-derived.** Title-bar height (36px ≈
   the corpus pixel-scan ~35px), sidebar width, and row height are layout
   structure of the reference, not theme identity, and remain
   reference-evidenced.
7. **Drift protection.** `tests/theme-snapshot.test.ts` pins the adopted
   token surface (block structure + every value) against a committed
   fixture generated from the fetched sources, verifies the five-palette
   registry, and fails if the semantic layer references an adopted token
   name that does not exist.

## Consequences

- Five selectable palettes (plus System and High contrast) in the title-bar
  quick menu and Settings; `ThemeProvider` resolves `system` to the
  canonical light/dark pair via `matchMedia`.
- A subtle CSS hazard was found and fixed during adoption: a header comment
  containing the literal sequence `*/` (in `dur-*/ease-*`) prematurely
  closed the comment and silently dropped the entire first token rule from
  the build. Comments in token files now avoid namespace lists that can
  produce `*/`.
- Same-name forwarding (`--x: var(--x)`) between layers is a self-reference
  that resolves to the fallback; the mapping layer only bridges
  distinct-name tokens and documents the pass-through names.
- Future palette work happens in the mapping layer, never in `adopted/`.
