# ADR-005 — Dependency resolution at scaffold time (2026-10-24)

**Status:** Accepted

## Context

The stack spec names versions as "latest stable, verified live against
npm." The live registry at scaffold time differed from the spec's named
majors in three places.

## Decision

| Spec said               | Registry says                                                                                 | Resolved                                                                                                                                  |
| ----------------------- | --------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Vite 7                  | **no 7.x exists** (6.4.5 → 8.x); `@vitejs/plugin-react@6` peers `^8.0.0` only                 | **Vite 8.3.5** + plugin-react 6.1.2                                                                                                       |
| TypeScript 5.x (strict) | latest is 7.0.2 (native rewrite); typescript-eslint 8.72 peers `<6.1`                         | **TypeScript 5.9.3** (spec-named, toolchain-verified)                                                                                     |
| React Router v7         | latest is 8.5.0                                                                               | **7.18.4** (spec-named; API fully known; RR8 upgrade is roadmap)                                                                          |
| Vitest v4               | latest is 5.0.5                                                                               | **4.1.12** (spec-named)                                                                                                                   |
| React Hook Form latest  | 8.0.0-**beta**                                                                                | 7.90.0 at scaffold; **removed entirely in the ADR-008 flow change** (the scan-configuration form was the reference-contradicting surface) |
| i18next / react-i18next | published <24 h before scaffold; pnpm's `minimumReleaseAge` supply-chain policy rejected them | repinned to 26.5.6 / 17.1.4                                                                                                               |

Everything else pinned to latest stable at scaffold time (React 19.3.1,
Tailwind 4.3.3, Zustand 5.0.16, TanStack Query 5.104.5, TanStack Virtual
3.14.22, Radix latest, sonner 2.0.8, Playwright 1.64.1, ESLint 10.13.0,
typescript-eslint 8.72.0, @tauri-apps 2.12.4). `pnpm-lock.yaml` committed;
`packageManager` pinned to pnpm 12.15.0.

## Rationale

- "Latest stable, verified live" is the governing rule; where a spec-named
  major exists and is known-good, the name wins (TS, RR, Vitest); where it
  does not exist (Vite 7), latest stable wins.
- No beta dependencies, nothing published inside the 24 h supply-chain
  policy window.

## Consequences

- React Compiler is wired through the oxc port (plugin-react 6's mechanism)
  rather than Babel — see ADR-002.
- `manualChunks` uses the function form (Rolldown requirement).
- The worker pipeline needs `esbuild` as an explicit devDependency.
