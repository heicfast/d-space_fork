# ADR-001 — Router: React Router v7 data mode with hash history

**Status:** Accepted

## Context

The wizard is a linear state machine (home → scan config → live scan →
results → recovery) with side surfaces (health, repair, settings). The stack
spec allows React Router v7 (data mode) or TanStack Router, to be justified
here.

## Decision

React Router v7 (`react-router` 7.18.4), `createHashRouter`.

## Rationale

- The flow is **guarded and linear**, not search-param-heavy: guards read the
  session store and connector state, which loaders would only re-implement.
  Typed search params (TanStack Router's headline feature) would benefit a
  URL-driven results filter, but a desktop wizard persists filters in app
  state, not URLs.
- The app must run identically in three hosts: WebView2's custom protocol
  (`tauri://localhost`), plain browser dev/preview, and any static host.
  **Hash routing** behaves the same in all three with zero server rewrites —
  history routing requires per-host configuration and breaks `file://`
  edge cases.
- Team familiarity and ecosystem maturity; RR7's data-mode APIs
  (loaders/actions) remain available for the real-engine session if needed.
- TanStack Router v8 exists (8.5.0 at scaffold time); upgrading is a roadmap
  item, not a blocker — the surface we use (route table + guards + Navigate)
  maps 1:1.

## Consequences

- URLs read `#/results/scan-1` — acceptable in a desktop shell.
- Route guards live in components (`ResultsRoute`) rather than loaders; this
  keeps session truth in one place (connector-backed stores) instead of
  splitting it into loader data.
