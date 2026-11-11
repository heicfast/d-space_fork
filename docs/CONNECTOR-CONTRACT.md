# CONNECTOR-CONTRACT

The typed command/event surface between the UI and the recovery backend.
Single source of truth: `src/connector/contract.ts`. This document is the
human-readable companion — where the contract came from, what each domain
guarantees, and exactly how the real bridge replaces the simulator.

## Contract principles

1. **The UI renders and requests; the connector provides and grades.**
   Confidence, health status, error reasons, and destination validation are
   backend decisions. The UI never re-grades, re-interprets, or overrides.
2. **Errors carry machine codes + i18n reason keys.** The UI maps
   `ConnectorErrorCode` and `reasonKey` to localized copy — never string-matches
   message text.
3. **Events are subscription-shaped.** `onScanProgress` / `onRecoveryProgress`
   return unsubscribe functions; the UI consumes them via
   `useSyncExternalStore` external stores (`src/queries/scan.ts`,
   `src/queries/recovery.ts`).
4. **Bounded payloads.** Preview payloads are bounded by the backend (hex
   first 64 KiB, text capped, PDF page bitmaps, AV first-frame). The UI never
   requests unbounded data.

## Domains

| Domain   | Commands                                                                                                                                                                                                                                           | Events                          |
| -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| Devices  | `listDevices`, `getDeviceInfo(id)` (id may be a device or volume id), `refreshDevices`                                                                                                                                                             | —                               |
| Scan     | `startScan(params) → {scanId}` (params include optional `pathScope`), `pauseScan`, `resumeScan`, `cancelScan`, `getScanState`                                                                                                                      | `onScanProgress(scanId, cb)`    |
| Results  | `getResultTree(scanId, parentId, mode)`, `queryResults(scanId, filter, sort, page)`, `getNodeDetail`, `countResults`                                                                                                                               | —                               |
| Preview  | `requestPreview(scanId, nodeId)` → bounded payload union (the CONNECTOR infers the preview kind from its own file knowledge — never the UI), `getThumbnail`                                                                                        | —                               |
| Sessions | `saveSession(scanId, path)`, `listSessions`, `loadSession(path) → {scanId}`, `getSessionMeta`                                                                                                                                                      | —                               |
| Recovery | `startRecovery(scanId, selection, destination, options) → {jobId}` (`selection` = `RecoverySelection`: explicit `nodeIds`, or `all` + the captured `ResultsFilter`), `cancelRecovery`, `getRecoveryState`, `getRecoveryItems`, `getRecoveryReport` | `onRecoveryProgress(jobId, cb)` |
| Health   | `getHealth(deviceId)` → SMART table, class, TRIM, wear                                                                                                                                                                                             | —                               |
| Repair   | `dryRunRepair(deviceId)`, `applyRepair(deviceId, {consentToken})`, `getUndoLog`, `undoRepairs`                                                                                                                                                     | —                               |
| System   | `getDriverStatus`, `getAppInfo`, `validateDestination(path, sourceDeviceId, neededBytes)`, `pickDirectory(hint?)`, `listMountPoints`, `openPath`                                                                                                   | —                               |

### Key payload shapes

- `ScanProgressEvent`: weighted-monotonic `overallProgress` (never regresses,
  and normalizes active stage weights when `quick-index` is omitted on a volume
  with `quickScanAvailable: false`), per-stage `stageProgress` (may reset
  between stages), live counters (`filesFound` and `dataFoundBytes` reach 100%
  of the volume/scope total upon completion), bad-sector map, ETA,
  `recoverableDuringScan`, `deviceConnected`.
- `ResultsPage`: `{ nodes, total, totalBytes, offset, pageSize, elapsedMs }` —
  carries both the filtered file count (`total`) and the filtered byte total
  (`totalBytes`) so the status bar renders accurate filtered byte sums.
- `ResultsFilter`: pushed server-side — tree scope, search, categories, date
  range, size range, extensions, origins, and the `hideEmptyFiles` toggle
  (ADR-008 removed the `confidence` filter — not a reference filter surface).
  The mock resolves it over typed-array scans; the real engine will resolve
  it in Rust.
- `ScanParams.pathScope`: optional folder restriction. Volume-root-relative
  path; an absolute path on the target volume is also accepted (the connector
  strips the drive-letter prefix). `null`/omitted = whole volume. An
  unresolvable folder is a hard `scan-scope-invalid` error — the UI cannot
  override the connector verdict. Scoped scans list only files inside the
  subtree; the path-mode tree presents the scanned folder itself as root.
- `pickDirectory(hint?)`: the optional hint roots the native dialog at a
  volume (`{ volumeId }`) — used by the Home "Select Folder" quick target
  (picker runs first, then the scan starts directly on the chosen scope).
  Destination picking (recovery) passes `{ excludeDeviceId: sourceTargetId }`
  so the dialog avoids suggesting the recovery source device; the connector
  still owns validation of the result.
- `RecoveryOptions`: collision policy (`rename`/`overwrite`/`skip`), folder
  reconstruction, empty-folder preservation, file repair, hash-on-write verify.
- `DestinationValidation`: `{ allowed, reason?, reasonKey?, freeBytes?,
neededBytes? }` — **the overlap refusal is hard**: the UI cannot proceed
  when `allowed === false` with reason `overlap`.

### Semantic notes for the real adapter (from the corpus research)

- Scan modes map to the reference's staged model: quick = FS index pass;
  deep = full structural walk; deep-carve adds the signature scanner.
- `.rsf`-style sessions: save serializes the found-file tree + scan metadata;
  load returns a resumable `scanId`; a corrupt session must fail with
  `session-corrupt` (never a partial load).
- Path-mode tree partitions files into three disjoint top-level branches
  (`root` for existing + normal deleted files with intact paths, `lost-paths`
  for `origin === 'lost-path'`, and `raw-all` for `origin === 'raw'` with
  `path: null`).
- Zero-byte files (`sizeBytes === 0`) are always graded `confidence: 'poor'`
  per `CRecoverability::GetEntryRecoverability` (`size < 1 → Poor`).
- Repair is transactional: every `applyRepair` must be undo-able via the
  undo log; `consentToken` must be the user's explicit acknowledgment
  (`REPAIR` typed confirmation in the UI).

## Swapping in the real bridge

1. Implement `RecoveryConnector` in `src/connector/tauri/tauri-connector.ts`
   by delegating to `invoke()` calls on thin Rust commands in
   `src-tauri/src/commands/` (reveal/path helpers already exist).
2. Map engine progress callbacks to the two event subscriptions (Tauri
   `emit`/`listen`).
3. Set `VITE_CONNECTOR=tauri` at build time — the factory in
   `src/connector/index.ts` does the switch. **No component or view changes.**
4. The current stub throws `not-implemented` on every method with a pointer
   to this document — a misconfigured build fails loudly, never silently.

## Mock simulator guarantees (verified by tests)

- Determinism: fixed seed per scenario; identical runs produce identical
  device lists, result sets, previews, and reports.
- Timing: progress ticks 100–250 ms; pause/resume < 600 ms; cancel ≤ 300 ms
  (all asserted).
- Volumes: 2 TB NTFS with 1.2 M nodes (procedural typed-array index), 64 GB
  exFAT SD, 4 TB Btrfs, lost-partition scenario.
- Error injection: `bad-sectors`, `disconnect-mid-scan` (one-shot, recoverable
  by resume), `out-of-space`, `permission-denied`, `corrupt-session`.
- Preview payloads are deterministic SVG/byte/text generations; failures
  carry diagnostic reason keys.
