# ADR-004 — Procedural deterministic mock index (1.2 M nodes)

**Status:** Accepted

## Context

The mock must simulate a 2 TB NTFS volume with ~1.2 M result nodes — enough
to exercise virtualization, filtering, and paging — without materializing
1.2 M JS objects (hundreds of MB) in the WebView.

## Decision

A `NodeSpace` with:

- a **materialized directory skeleton** (a few thousand real nodes with
  precomputed subtree aggregates), and
- **typed-array file records** (`Uint32Array` dir index, `Uint8Array`
  category/confidence/origin, `Float64Array` size, `Uint16Array` date,
  `Uint8Array` extension) with names/attributes derived deterministically
  from the record index via a seeded `mulberry32` PRNG.

Filters scan the typed arrays directly (~10–40 ms for 1.2 M records);
pages materialize only the visible slice. Directory aggregates are computed
in two passes (own totals, then bottom-up subtree sums) so tree expansion
is O(children).

## Rationale

- Memory: ~25 MB of typed arrays vs ~500 MB of objects.
- Determinism: fixed seed per scenario — tests and screenshots reproduce.
- The mock is a simulator of the BACKEND, so server-side filter semantics
  (grid #36, <200 ms) are exercised for real.

## Consequences

- `queryResults`/`countResults` costs are linear scans — fine for a mock,
  and the contract allows the real engine to do better.
- Subtree aggregate correctness is unit-tested (a first implementation
  overwrote child sums; caught by the test, fixed with the two-pass pass).
