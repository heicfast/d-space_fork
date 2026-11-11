# ADR-008: The lost-data flow — direct start, automatic quick → advanced

## Status

Accepted (session 3, T6). Supersedes the ScanConfigPage design from sessions 1–2.

## Context

The reference product's lost-data flow, verified across three independent
corpus sources:

1. **Decompiled transition** (`scanClickedSlot`, artifact 04 §4): a scan
   click on a device tile prepares `CScanMgr`, flips the stack directly to
   `CDetailPage`, and starts the scan on the same stack switch. Scan and
   results are the same page.
2. **Runtime ground truth** (GT-scan): clicking "Search for Lost Data" lands
   directly in the scan-phase view; the quick pass (~21 s) completes and the
   advanced pass begins automatically — captured at "Advanced Scanning, 9%"
   with 1.11 M files and no user action in between.
3. **Engine wiring** (artifact 25 §3.1): `sigQuickScanFinished` ×2 →
   `onQuickScanCompleted` — the two-phase gate; runtime log shows
   `m_iScanType=3` then `m_iScanType=4` back-to-back in one continuous run.

No scan-configuration surface exists anywhere in the reference's 2,402 tr
call sites, 195 telemetry events, 722 connect() edges, or 1,710 QSS targets.
Engine-side scan setup (architecture/04 §3) is automatic: FS-type probe →
quick searcher; advanced = full searcher set + RAW carver (sector size
default 512). The only quick→deep boundary surface, `CQuickScanCompleteWnd`,
is a Free-tier upsell/coach (not a gate — advanced proceeds regardless) and
falls under the documented out-of-scope monetization decision (UI-MAP §0).

## Decision

1. **No configuration step.** Delete the app's ScanConfigPage and its route.
   Every scan entry point — the Home CTA, device-card scan buttons, volume
   links, the "Specify a location" quick targets (Recycle Bin, Desktop,
   Select Folder), and the Lost Partition Rescue page — starts the scan
   directly and navigates to the live scan surface.
2. **One automatic chain.** `ScanParams` reduces to `{ targetId, pathScope? }`.
   The connector always runs volume-analysis → quick-index → deep-walk →
   carve → tree-build. When a volume cannot be quick-indexed
   (`quickScanAvailable: false`), the quick pass collapses to a probe and the
   advanced pass carries the run — engine truthfulness, not a user choice.
3. **Reference status formats.** The scan surface shows the corpus lines:
   "Quick/Advanced scanning X%" with "Files found: N (size)", "Remaining
   time: …", "Reading sector: X/Y", and the full-width "Searching: <path>"
   ticker above the bottom control cluster. A `ScanPhase` mapping groups
   stages into the reference's visible phases.
4. **Affordances kept exactly as the reference exposes them**: pause, stop
   (confirm dialog with stat tiles), browse-while-scanning, Recover All
   mid-scan, the completion celebration (stats + "Go to Recover", no Esc).
5. **Contract consequences**: scan mode/categories/sectorSize/bad-sector
   policy/retries leave the contract (invented UI surface); preview-kind
   inference moves behind the connector (engine knowledge); recover-all
   becomes an explicit `RecoverySelection` (`nodes` | `all` + filter)
   driving a real lazy export loop; `SessionMeta` drops `mode`;
   `deleteSession` (not reference-evidenced) is removed; the filter payload
   gains `hideEmptyFiles` (corpus toggle) and loses `confidence` (not a
   reference filter surface).

## Consequences

- The scan-configuration feature, its form stack (react-hook-form, zod,
  @hookform/resolvers, @radix-ui/react-select), and its i18n section are
  deleted; categories copy moves to the shared top level.
- Folder scope survives — chosen on the landing page ("Select Folder" opens
  the picker first, then starts), matching the corpus's quick-target row.
- Automated coverage pins the corrected sequence: the connector tests assert
  the full automatic stage order, the quick→advanced phase mapping, the
  collapsed-quick path, ticker stage-gating, and direct location-target
  scans; the e2e golden path asserts the scan starts immediately after the
  device click with the corpus status line visible.
- Two live-verification bugs were found and fixed while proving the flow:
  device-id scan targets resolved the wrong NodeSpace (the first device's),
  and recover-all ETA scaled per-item (absurd at 340 k files).
