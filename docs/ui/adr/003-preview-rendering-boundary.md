# ADR-003 — Preview rendering boundary

**Status:** Accepted

## Context

Previews must never load whole multi-GB files, never block the UI thread,
and every failure must show a diagnostic reason. Who renders what — engine
or UI — needed an explicit split.

## Decision

- **UI renders raw bounded payloads**: images (connector dataUrl, zoom
  client-side), hex (connector-bounded byte slice, formatted in a module
  **worker**), text (capped string + encoding label).
- **Engine renders document pages**: PDF page bitmaps and Office extracted
  text + metadata come from the connector as bounded artifacts. The UI does
  not ship a PDF/Office runtime.
- **AV**: metadata + first-frame thumbnail from the connector; no playback
  this session (roadmap: bounded playback behind a stability flag).
- Worker absence (jsdom tests) falls back to an identical inline formatter —
  same bounded input, same output; the worker remains the default in
  browser/WebView2.

## Rationale

- Keeps the bundle small (no pdf.js/Office runtimes; grid #53) and moves
  decode cost to where the bytes already live.
- The mock generates deterministic SVG "page bitmaps" and byte slices,
  proving the UI never assumes a specific renderer.
- ADR-003 documents why `requestPreview` returns a discriminated payload
  union rather than byte streams for every kind.

## Consequences

- The real engine must produce bounded page images for PDF and extracted
  text for Office — recorded in CONNECTOR-CONTRACT.md as a hard requirement.
- Preview latency stays under the 500 ms grid target (mock: 80–400 ms).
