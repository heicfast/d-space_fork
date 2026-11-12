/**
 * Real IPC adapter — the DiskWizard engine bridge.
 *
 * Every domain method maps to a `dw_*` Rust command (see
 * src-tauri/src/recovery/commands.rs and docs/CONNECTOR-CONTRACT.md).
 * Mapping rules:
 *   * top-level command arguments are camelCase (Tauri v2 convention);
 *   * new recovery wire structs are camelCase and pass through directly;
 *   * legacy engine payloads (staging, rules, snapshots, duplicates,
 *     index) are snake_case and mapped field by field here;
 *   * unix-second timestamps convert to Iso8601 here;
 *   * engine errors `{ kind, message }` map to typed ConnectorError codes.
 *
 * Events: the engine emits `dw-scan-progress` / `dw-recovery-progress`
 * with the scan/job id in the payload; the subscription adapters filter
 * and forward, returning unsubscribe functions.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AppInfo,
  CleanupDomain,
  CleanupExpiredItem,
  CleanupPermanentResult,
  CleanupReport,
  CleanupRule,
  CleanupSnapshotMeta,
  CleanupStagedItem,
  CleanupUndoOutcome,
  CleanupUndoResolution,
  ConfidenceGrade,
  ConnectorError,
  ConnectorErrorCode,
  Device,
  DestinationValidation,
  DiskHealth,
  DiskLayout,
  DriverStatus,
  DuplicatesResult,
  FileCategoryId,
  HealthDomain,
  IndexHit,
  MountPoint,
  NodeDetail,
  PreviewPayload,
  RecoveryConnector,
  RecoveryDomain,
  RecoveryJobStatus,
  RecoveryOptions,
  RecoveryProgressEvent,
  RecoveryReport,
  RecoverySelection,
  RepairDomain,
  RepairDryRun,
  RepairUndoEntry,
  ResultOrigin,
  ResultNode,
  ResultsDomain,
  ResultsFilter,
  ResultsPage,
  ScanDomain,
  ScanParams,
  ScanProgressEvent,
  ScanState,
  ScanStatus,
  SessionMeta,
  SessionsDomain,
  SpecialFolders,
  SnapshotDelta,
  SortDirection,
  SortField,
  SystemDomain,
  TreeNode,
  TreePage,
  DevicesDomain,
  PreviewDomain,
} from '../contract';
import { isTauriRuntime } from '@/lib/runtime';

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

const CODE_BY_MESSAGE: readonly [RegExp, ConnectorErrorCode][] = [
  [/scan scope/i, 'scan-scope-invalid'],
  [/scope '.*' was not found/i, 'scan-scope-invalid'],
  [/administrator privileges are required/i, 'device-permission'],
  [/not accessible/i, 'device-gone'],
  [/is not mounted/i, 'device-gone'],
  [/no valid (FAT|exFAT|ext) |superblock|boot sector/i, 'scan-scope-invalid'],
  [/NTFS parse error|NTFS parse on/i, 'device-gone'],
  [/no SMART data path succeeded/i, 'health-unavailable'],
  [/has been reused since the scan/i, 'recovery-item-failed'],
  [/job thread panicked|scan thread panicked/i, 'unknown'],
  [/preview-too-large/i, 'preview-too-large'],
  [/preview-damaged/i, 'preview-damaged'],
  [/compressed or encrypted/i, 'preview-damaged'],
  [/clusters.*overwritten|clusters have been reused/i, 'preview-damaged'],
  [/destination overlaps|is on the source volume/i, 'destination-overlap'],
  [/out-of-space|exceeds the .* limit/i, 'out-of-space'],
  [/cannot create (folder|file)|write failed|unwritable/i, 'destination-unwritable'],
  [/unknown session/i, 'session-not-found'],
  [/not a DiskWizard session file/i, 'session-corrupt'],
  [/SMART query failed|empty SMART table/i, 'health-unavailable'],
  [/consent token missing/i, 'repair-consent-required'],
  [/repairs? (failed|cannot)/i, 'repair-failed'],
];

const ConnectorCodes: ReadonlySet<string> = new Set<string>([
  'device-gone',
  'device-permission',
  'scan-not-found',
  'scan-not-paused',
  'scan-not-running',
  'scan-scope-invalid',
  'session-corrupt',
  'session-not-found',
  'destination-overlap',
  'destination-invalid',
  'destination-unwritable',
  'out-of-space',
  'preview-unsupported',
  'preview-too-large',
  'preview-damaged',
  'preview-encrypted',
  'preview-timeout',
  'preview-missing-data',
  'recovery-item-failed',
  'repair-consent-required',
  'repair-failed',
  'health-unavailable',
  'not-implemented',
  'unknown',
]);

function connErr(
  message: string,
  opts: { code: ConnectorErrorCode; reasonKey?: string; detail?: Record<string, unknown> },
): ConnectorError {
  return Object.assign(new Error(message), {
    code: opts.code,
    reasonKey: opts.reasonKey,
    detail: opts.detail,
  });
}

function toConnectorError(
  raw: unknown,
  fallbackCode: ConnectorErrorCode = 'unknown',
): ConnectorError {
  const asError = raw as { code?: string };
  if (
    asError &&
    typeof asError === 'object' &&
    'code' in asError &&
    ConnectorCodes.has(asError.code ?? '')
  ) {
    return raw as ConnectorError;
  }
  const wire = raw as { kind?: string; message?: string } | string | undefined;
  const kind = typeof wire === 'object' && wire !== null ? (wire.kind ?? '') : '';
  const message =
    typeof wire === 'object' && wire !== null
      ? (wire.message ?? String(raw))
      : typeof wire === 'string'
        ? wire
        : String(raw);
  for (const [pattern, code] of CODE_BY_MESSAGE) {
    if (pattern.test(message)) {
      return connErr(message, { code, detail: { kind } });
    }
  }
  if (kind === 'volume') return connErr(message, { code: 'device-gone' });
  return connErr(message, { code: fallbackCode, detail: { kind } });
}

function requireTauri(): void {
  if (!isTauriRuntime()) {
    throw connErr(
      'The real engine connector requires the Tauri runtime. Run the app via `pnpm tauri dev`, or use VITE_CONNECTOR=mock for browser development.',
      { code: 'unknown' },
    );
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  requireTauri();
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    throw toConnectorError(raw);
  }
}

// ---------------------------------------------------------------------------
// Date + id helpers
// ---------------------------------------------------------------------------

function iso(unixSeconds: number | null | undefined): string {
  if (unixSeconds === null || unixSeconds === undefined) return new Date(0).toISOString();
  return new Date(unixSeconds * 1000).toISOString();
}

function toGrade(raw: string): ConfidenceGrade {
  switch (raw) {
    case 'excellent':
    case 'good':
    case 'fair':
    case 'poor':
    case 'unknown':
      return raw;
    default:
      return 'unknown';
  }
}

function toCategory(raw: string | null): FileCategoryId | null {
  if (raw === null) return null;
  const known: readonly FileCategoryId[] = [
    'graphics',
    'video',
    'audio',
    'document',
    'email',
    'archive',
    'bookmarks',
    'other',
  ];
  return (known as readonly string[]).includes(raw) ? (raw as FileCategoryId) : 'other';
}

function toOrigin(raw: string): ResultOrigin {
  switch (raw) {
    case 'existing':
    case 'deleted':
    case 'lost-path':
      return raw;
    default:
      return 'existing';
  }
}

// ---------------------------------------------------------------------------
// Engine wire shapes (only the fields the adapter reads)
// ---------------------------------------------------------------------------

interface EngineScanProgress {
  scanId: string;
  status: string;
  stage: string;
  stageProgress: number;
  overallProgress: number;
  bytesScanned: number;
  bytesTotal: number;
  filesFound: number;
  dataFoundBytes: number;
  deletedFound: number;
  errors: number;
  badSectorMap: { startByte: number; sizeBytes: number }[];
  etaSeconds: number | null;
  throughputBytesPerSecond: number;
  recoverableDuringScan: boolean;
  deviceConnected: boolean;
  currentPath: string | null;
}

interface EngineScanState {
  scanId: string;
  targetId: string;
  targetLabel: string;
  status: string;
  startedAtUnix: number;
  completedAtUnix: number | null;
  progress: EngineScanProgress;
  failure: string | null;
}

interface EngineDevice {
  id: string;
  kind: string;
  model: string;
  label: string;
  serial: string | null;
  totalBytes: number;
  usedBytes: number;
  freeBytes: number;
  status: string;
  volumes: {
    id: string;
    deviceId: string;
    label: string;
    letter: string | null;
    fs: string;
    totalBytes: number;
    usedBytes: number;
    freeBytes: number;
    sectorSize: number;
    status: string;
    quickScanAvailable: boolean;
    canRecoverDeleted: boolean;
    bitlocker: string | null;
  }[];
  canScanWholeDisk: boolean;
  isSourceOfActiveScan: boolean;
}

function toDevice(d: EngineDevice): Device {
  return {
    id: d.id,
    kind: d.kind as Device['kind'],
    model: d.model,
    label: d.label,
    serial: d.serial ?? '',
    capacity: { totalBytes: d.totalBytes, usedBytes: d.usedBytes, freeBytes: d.freeBytes },
    status: d.status as Device['status'],
    isSourceOfActiveScan: d.isSourceOfActiveScan,
    volumes: d.volumes.map((v) => ({
      id: v.id,
      deviceId: v.deviceId,
      label: v.label,
      letter: v.letter,
      fs: (v.fs === 'unknown'
        ? 'unknown'
        : v.fs.toLowerCase()) as Device['volumes'][number]['fs'],
      capacity: { totalBytes: v.totalBytes, usedBytes: v.usedBytes, freeBytes: v.freeBytes },
      sectorSize: v.sectorSize === 0 ? undefined : v.sectorSize,
      status: v.status as Device['volumes'][number]['status'],
      quickScanAvailable: v.quickScanAvailable,
      bitlocker: (v.bitlocker ?? undefined) as Device['volumes'][number]['bitlocker'],
    })),
    canScanWholeDisk: d.canScanWholeDisk,
  };
}

function imageDimensions(bytes: Uint8Array): { width: number | null; height: number | null } {
  // PNG: IHDR big-endian at 16..24.
  if (
    bytes.length >= 24 &&
    bytes[0] === 0x89 &&
    bytes[1] === 0x50 &&
    bytes[2] === 0x4e &&
    bytes[3] === 0x47
  ) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    return { width: view.getUint32(16), height: view.getUint32(20) };
  }
  // GIF: little-endian at 6..10.
  if (bytes.length >= 10 && bytes[0] === 0x47 && bytes[1] === 0x49 && bytes[2] === 0x46) {
    return { width: bytes[6] | (bytes[7] << 8), height: bytes[8] | (bytes[9] << 8) };
  }
  // BMP: little-endian at 18..26.
  if (bytes.length >= 26 && bytes[0] === 0x42 && bytes[1] === 0x4d) {
    return {
      width: bytes[18] | (bytes[19] << 8) | (bytes[20] << 16) | (bytes[21] << 24),
      height: bytes[22] | (bytes[23] << 8) | (bytes[24] << 16) | (bytes[25] << 24),
    };
  }
  // JPEG: walk the segment markers to an SOF frame.
  if (bytes.length >= 4 && bytes[0] === 0xff && bytes[1] === 0xd8) {
    let i = 2;
    while (i + 8 < bytes.length) {
      if (bytes[i] !== 0xff) break;
      const marker = bytes[i + 1];
      const len = (bytes[i + 2] << 8) | bytes[i + 3];
      if (
        (marker >= 0xc0 && marker <= 0xc3) ||
        (marker >= 0xc5 && marker <= 0xc7) ||
        (marker >= 0xc9 && marker <= 0xcb) ||
        (marker >= 0xcd && marker <= 0xcf)
      ) {
        return {
          height: (bytes[i + 5] << 8) | bytes[i + 6],
          width: (bytes[i + 7] << 8) | bytes[i + 8],
        };
      }
      if (marker === 0xd8 || (marker >= 0xd0 && marker <= 0xd9)) break;
      i += 2 + len;
    }
  }
  return { width: null, height: null };
}

interface EngineResultNode {
  id: string;
  scanId: string;
  kind: 'file' | 'directory';
  name: string;
  path: string | null;
  sizeBytes: number;
  modifiedUnix: number;
  extension: string;
  category: string | null;
  confidence: string;
  origin: string;
  childCount: number | null;
  thumbnailAvailable: boolean;
  parentId: string | null;
  attributes: Record<string, string> | null;
}

interface EngineTreeNode {
  id: string;
  parentId: string | null;
  label: string;
  icon: 'directory' | 'category' | 'group' | 'device';
  childCount: number;
  fileCount: number;
  sizeBytes: number;
  origin: string;
  special: string | null;
}

interface EngineResultsPage {
  nodes: EngineResultNode[];
  total: number;
  totalBytes: number;
  offset: number;
  pageSize: number;
  elapsedMs: number;
}

interface EngineNodeDetail {
  node: EngineResultNode;
  fullPhysicalPath: string | null;
  createdUnix: number;
  accessedUnix: number;
  confidenceReason: string;
  badSectorsOverlap: boolean;
  clusterCount: number;
}

interface EnginePreview {
  kind: 'image' | 'text' | 'hex';
  dataBase64: string;
  mime: string | null;
  text: string | null;
  encoding: string | null;
  truncated: boolean;
  totalSize: number;
  reasonKey: string | null;
}

interface EngineSessionMeta {
  id: string;
  fileName: string;
  savedAtUnix: number;
  targetLabel: string;
  filesFound: number;
  dataFoundBytes: number;
  deletedFound: number;
  completed: boolean;
}

interface EngineJobProgress {
  jobId: string;
  scanId: string;
  status: string;
  overallProgress: number;
  currentFile: string | null;
  currentFileProgress: number;
  completedCount: number;
  failedCount: number;
  skippedCount: number;
  totalCount: number;
  throughputBytesPerSecond: number;
  verifying: boolean;
  etaSeconds: number | null;
}

interface EngineJobItem {
  nodeId: string;
  name: string;
  sizeBytes: number;
  state: 'pending' | 'running' | 'done' | 'failed' | 'skipped' | 'verifying';
  errorCode: string | null;
  savedAs: string | null;
}

interface EngineJobReport {
  jobId: string;
  status: string;
  startedAtUnix: number;
  finishedAtUnix: number;
  succeeded: EngineJobItem[];
  failed: EngineJobItem[];
  skipped: EngineJobItem[];
  totalBytesWritten: number;
  verified: boolean;
  processedTotal: number;
  truncatedItemLists: boolean;
}

// Legacy snake_case payloads ------------------------------------------------

interface EngineStagedItem {
  id: number;
  original_path: string;
  staged_path: string;
  size_bytes: number;
  staged_at_unix: number;
  expires_at_unix: number;
  is_dir: boolean;
  fallback_tier: 'normal' | 'cross_volume';
}

interface EngineUserRule {
  id: number;
  pattern: string;
  pattern_type: 'name' | 'extension';
  score: number;
  explanation: string;
  enabled: boolean;
  created_at_unix: number;
  updated_at_unix: number;
}

interface EngineDuplicateGroup {
  hash_hex: string;
  size_bytes: number;
  paths: string[];
  modified_unix: number[];
}

interface EngineDuplicates {
  drive_letter: string;
  scanned_files: number;
  filtered_small: number;
  candidate_pairs: number;
  hash_errors: number;
  groups: EngineDuplicateGroup[];
  stats: { group_count: number; redundant_bytes: number; elapsed_ms: number };
}

interface EngineSnapshotMeta {
  id: number;
  volume_id: string;
  captured_at_unix: number;
  total_size_bytes: number;
  file_count: number;
  dir_count: number;
  entry_count: number;
}

interface EngineDelta {
  from_id: number;
  to_id: number;
  from_captured_at: number;
  to_captured_at: number;
  net_change_bytes: number;
  total_changed_paths: number;
  added: { path: string; size_bytes: number }[];
  removed: { path: string; size_bytes: number }[];
  grew: { path: string; from_size: number; to_size: number; delta_bytes: number }[];
  shrunk: { path: string; from_size: number; to_size: number; delta_bytes: number }[];
}

interface EngineIndexHit {
  volume_id: string;
  name: string;
  full_path: string | null;
  attrs: number;
}

// ---------------------------------------------------------------------------
// Mapping helpers
// ---------------------------------------------------------------------------

function toResultNode(n: EngineResultNode): ResultNode {
  return {
    id: n.id,
    scanId: n.scanId,
    kind: n.kind,
    name: n.name,
    path: n.path,
    sizeBytes: n.sizeBytes,
    modifiedAt: n.modifiedUnix > 0 ? iso(n.modifiedUnix) : null,
    extension: n.extension.length > 0 ? `.${n.extension}` : null,
    category: toCategory(n.category),
    confidence: toGrade(n.confidence),
    origin: toOrigin(n.origin),
    childCount: n.childCount,
    thumbnailAvailable: n.thumbnailAvailable,
    parentId: n.parentId,
    attributes: n.attributes ?? undefined,
  };
}

function toTreeNode(n: EngineTreeNode): TreeNode {
  return {
    id: n.id,
    parentId: n.parentId,
    label: n.label,
    icon: n.icon,
    childCount: n.childCount,
    fileCount: n.fileCount,
    sizeBytes: n.sizeBytes,
    origin: toOrigin(n.origin),
    special: (n.special as TreeNode['special']) ?? undefined,
  };
}

function toScanStatus(raw: string): ScanStatus {
  switch (raw) {
    case 'starting':
    case 'running':
    case 'paused':
    case 'pausing':
    case 'resuming':
    case 'stopping':
    case 'completed':
    case 'cancelled':
    case 'failed':
      return raw;
    default:
      return 'failed';
  }
}

function toScanProgressEvent(e: EngineScanProgress): ScanProgressEvent {
  return {
    scanId: e.scanId,
    status: toScanStatus(e.status),
    stage: e.stage as ScanProgressEvent['stage'],
    stageProgress: e.stageProgress,
    overallProgress: e.overallProgress,
    bytesScanned: e.bytesScanned,
    bytesTotal: e.bytesTotal,
    filesFound: e.filesFound,
    dataFoundBytes: e.dataFoundBytes,
    deletedFound: e.deletedFound,
    errors: e.errors,
    badSectorMap: e.badSectorMap,
    etaSeconds: e.etaSeconds,
    throughputBytesPerSecond: e.throughputBytesPerSecond,
    recoverableDuringScan: e.recoverableDuringScan,
    deviceConnected: e.deviceConnected,
    currentPath: e.currentPath,
  };
}

function toJobStatus(raw: string): RecoveryJobStatus {
  switch (raw) {
    case 'validating':
    case 'running':
    case 'completed':
    case 'completed_partial':
      return 'completed-partial';
    case 'cancelled':
    case 'failed':
      return raw;
    default:
      return 'failed';
  }
}

function minimalTargetFor(targetId: string): Device {
  // The UI never dereferences ScanState.target; a minimal synthetic device
  // keeps the contract shape without a devices round-trip.
  const letter = targetId
    .replace(/^(vol|dev):/i, '')
    .slice(0, 1)
    .toUpperCase();
  return {
    id: `dev:${letter}`,
    kind: 'internal',
    model: targetId,
    label: targetId,
    serial: '',
    capacity: { totalBytes: 0, usedBytes: 0, freeBytes: 0 },
    status: 'ok',
    volumes: [],
    canScanWholeDisk: true,
  };
}

function toScanState(s: EngineScanState, params: ScanParams): ScanState {
  return {
    scanId: s.scanId,
    params,
    target: minimalTargetFor(s.targetId),
    status: toScanStatus(s.status),
    startedAt: iso(s.startedAtUnix),
    completedAt: s.completedAtUnix === null ? null : iso(s.completedAtUnix),
    progress: toScanProgressEvent(s.progress),
    failure: s.failure !== null ? toConnectorError(s.failure, 'unknown') : undefined,
  };
}

function toSessionMeta(m: EngineSessionMeta): SessionMeta {
  return {
    id: m.id,
    fileName: m.fileName,
    savedAt: iso(m.savedAtUnix),
    targetLabel: m.targetLabel,
    filesFound: m.filesFound,
    dataFoundBytes: m.dataFoundBytes,
    completed: m.completed,
  };
}

function toStagedItem(i: EngineStagedItem): CleanupStagedItem {
  return {
    id: i.id,
    originalPath: i.original_path,
    stagedPath: i.staged_path,
    sizeBytes: i.size_bytes,
    stagedAt: iso(i.staged_at_unix),
    expiresAt: iso(i.expires_at_unix),
    isDir: i.is_dir,
    fallbackTier: i.fallback_tier,
  };
}

function toRule(r: EngineUserRule): CleanupRule {
  return {
    id: r.id,
    pattern: r.pattern,
    patternType: r.pattern_type,
    score: r.score,
    explanation: r.explanation,
    enabled: r.enabled,
    createdAt: iso(r.created_at_unix),
    updatedAt: iso(r.updated_at_unix),
  };
}

function toDuplicates(d: EngineDuplicates): DuplicatesResult {
  return {
    driveLetter: d.drive_letter,
    scannedFiles: d.scanned_files,
    filteredSmall: d.filtered_small,
    candidatePairs: d.candidate_pairs,
    hashErrors: d.hash_errors,
    groups: d.groups.map((g) => ({
      hashHex: g.hash_hex,
      sizeBytes: g.size_bytes,
      paths: g.paths,
      modifiedUnix: g.modified_unix,
    })),
    stats: {
      groupCount: d.stats.group_count,
      redundantBytes: d.stats.redundant_bytes,
      elapsedMs: d.stats.elapsed_ms,
    },
  };
}

function toSnapshotMeta(m: EngineSnapshotMeta): CleanupSnapshotMeta {
  return {
    id: m.id,
    volumeId: m.volume_id,
    capturedAt: iso(m.captured_at_unix),
    totalSizeBytes: m.total_size_bytes,
    fileCount: m.file_count,
    dirCount: m.dir_count,
    entryCount: m.entry_count,
  };
}

function toDelta(d: EngineDelta): SnapshotDelta {
  return {
    fromId: d.from_id,
    toId: d.to_id,
    fromCapturedAt: iso(d.from_captured_at),
    toCapturedAt: iso(d.to_captured_at),
    netChangeBytes: d.net_change_bytes,
    totalChangedPaths: d.total_changed_paths,
    added: d.added.map((a) => ({ path: a.path, sizeBytes: a.size_bytes })),
    removed: d.removed.map((r) => ({ path: r.path, sizeBytes: r.size_bytes })),
    grew: d.grew.map((g) => ({
      path: g.path,
      fromSize: g.from_size,
      toSize: g.to_size,
      deltaBytes: g.delta_bytes,
    })),
    shrunk: d.shrunk.map((g) => ({
      path: g.path,
      fromSize: g.from_size,
      toSize: g.to_size,
      deltaBytes: g.delta_bytes,
    })),
  };
}

function filterArgs(filter: ResultsFilter): Record<string, unknown> {
  return {
    nodeId: filter.nodeId,
    treeMode: filter.treeMode,
    search: filter.search,
    categories: filter.categories ? [...filter.categories] : null,
    dateFrom: filter.dateFrom !== null ? Math.floor(Date.parse(filter.dateFrom) / 1000) : null,
    dateTo: filter.dateTo !== null ? Math.floor(Date.parse(filter.dateTo) / 1000) : null,
    sizeMinBytes: filter.sizeMinBytes,
    sizeMaxBytes: filter.sizeMaxBytes,
    extensions: filter.extensions ? [...filter.extensions] : null,
    origins: filter.origins ? [...filter.origins] : null,
    hideEmptyFiles: filter.hideEmptyFiles,
  };
}

// ---------------------------------------------------------------------------
// The connector
// ---------------------------------------------------------------------------

export function createTauriConnector(): RecoveryConnector {
  const devices: DevicesDomain = {
    listDevices: async () => (await call<EngineDevice[]>('dw_devices_list')).map(toDevice),
    getDeviceInfo: async (deviceId: string) => {
      const all = (await call<EngineDevice[]>('dw_devices_list')).map(toDevice);
      const found = all.find(
        (d) => d.id === deviceId || d.volumes.some((v) => v.id === deviceId),
      );
      if (!found) {
        throw connErr(`device '${deviceId}' is gone`, { code: 'device-gone' });
      }
      return found;
    },
    refreshDevices: async () =>
      (await call<EngineDevice[]>('dw_devices_refresh')).map(toDevice),
  };

  const scan: ScanDomain = {
    startScan: (params: ScanParams) =>
      call<{ scanId: string }>('dw_scan_start', {
        targetId: params.targetId,
        pathScope: params.pathScope ?? null,
      }),
    pauseScan: (scanId: string) => call<void>('dw_scan_pause', { scanId }),
    resumeScan: (scanId: string) => call<void>('dw_scan_resume', { scanId }),
    cancelScan: (scanId: string) => call<void>('dw_scan_cancel', { scanId }),
    getScanState: async (scanId: string) => {
      const raw = await call<EngineScanState>('dw_scan_state', { scanId });
      return toScanState(raw, { targetId: raw.targetId });
    },
    onScanProgress: (scanId: string, cb: (event: ScanProgressEvent) => void) => {
      requireTauri();
      let unlisten: UnlistenFn | null = null;
      let disposed = false;
      void listen<EngineScanProgress>('dw-scan-progress', (event) => {
        if (event.payload.scanId === scanId) {
          cb(toScanProgressEvent(event.payload));
        }
      }).then((fn) => {
        if (disposed) {
          fn();
        } else {
          unlisten = fn;
        }
      });
      return () => {
        disposed = true;
        unlisten?.();
      };
    },
  };

  const results: ResultsDomain = {
    getResultTree: async (scanId: string, parentId: string | null, mode) => {
      const page = await call<{
        nodes: EngineTreeNode[];
        parentHasMore: boolean;
      }>('dw_results_tree', {
        scanId,
        parentId,
        mode,
      });
      const mapped: TreePage = {
        nodes: page.nodes.map(toTreeNode),
        parentHasMore: page.parentHasMore,
      };
      return mapped;
    },
    queryResults: async (
      scanId: string,
      filter: ResultsFilter,
      sort: { field: SortField; direction: SortDirection },
      page: { offset: number; limit: number },
    ) => {
      const raw = await call<EngineResultsPage>('dw_results_query', {
        scanId,
        filter: filterArgs(filter),
        sortField: sort.field,
        sortDirection: sort.direction,
        offset: page.offset,
        limit: page.limit,
      });
      const mapped: ResultsPage = {
        nodes: raw.nodes.map(toResultNode),
        total: raw.total,
        totalBytes: raw.totalBytes,
        offset: raw.offset,
        pageSize: raw.pageSize,
        elapsedMs: raw.elapsedMs,
      };
      return mapped;
    },
    getNodeDetail: async (scanId: string, nodeId: string) => {
      const raw = await call<EngineNodeDetail>('dw_node_detail', { scanId, nodeId });
      const mapped: NodeDetail = {
        node: toResultNode(raw.node),
        fullPhysicalPath: raw.fullPhysicalPath,
        createdAt: iso(raw.createdUnix),
        accessedAt: iso(raw.accessedUnix),
        confidenceReason: raw.confidenceReason,
        badSectorsOverlap: raw.badSectorsOverlap,
        clusterCount: raw.clusterCount,
      };
      return mapped;
    },
    countResults: async (scanId: string, filter: ResultsFilter) => {
      const n = await call<number>('dw_results_count', {
        scanId,
        filter: filterArgs(filter),
      });
      // SQLite may hand back a float through JSON — normalize.
      return typeof n === 'number' ? Math.floor(n) : 0;
    },
  };

  const preview: PreviewDomain = {
    requestPreview: async (scanId: string, nodeId: string) => {
      const raw = await call<EnginePreview>('dw_preview', { scanId, nodeId });
      if (raw.kind === 'image') {
        const bin = atob(raw.dataBase64);
        const head = new Uint8Array(Math.min(bin.length, 512));
        for (let i = 0; i < head.length; i += 1) head[i] = bin.charCodeAt(i);
        const { width, height } = imageDimensions(head);
        return {
          kind: 'image',
          dataUrl: `data:${raw.mime ?? 'application/octet-stream'};base64,${raw.dataBase64}`,
          width,
          height,
          bounded: true,
        } satisfies PreviewPayload;
      }
      if (raw.kind === 'text') {
        return {
          kind: 'text',
          text: raw.text ?? '',
          encoding: raw.encoding ?? 'utf-8',
          totalSize: raw.totalSize,
          truncated: raw.truncated,
        } satisfies PreviewPayload;
      }
      const bin = atob(raw.dataBase64);
      const bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i += 1) bytes[i] = bin.charCodeAt(i);
      return {
        kind: 'hex',
        bytes,
        totalSize: raw.totalSize,
        baseOffset: 0,
      } satisfies PreviewPayload;
    },
    getThumbnail: async (scanId: string, nodeId: string) => {
      const raw = await call<{ dataUrl: string } | null>('dw_thumbnail', {
        scanId,
        nodeId,
      });
      return raw ? { dataUrl: raw.dataUrl } : null;
    },
  };

  const sessions: SessionsDomain = {
    saveSession: async (scanId: string) => {
      const meta = await call<EngineSessionMeta>('dw_session_save', { scanId });
      return toSessionMeta(meta);
    },
    listSessions: async () => {
      const list = await call<EngineSessionMeta[]>('dw_sessions_list');
      return list.map(toSessionMeta);
    },
    loadSession: async (fileName: string) => {
      const reply = await call<{ scanId: string }>('dw_session_load', { fileName });
      return { scanId: reply.scanId };
    },
    getSessionMeta: async (fileName: string) => {
      const list = await call<EngineSessionMeta[]>('dw_sessions_list');
      const found = list.find((m) => m.fileName === fileName);
      if (!found) {
        throw connErr(`session '${fileName}' was not found`, {
          code: 'session-not-found',
        });
      }
      return toSessionMeta(found);
    },
  };

  const recovery: RecoveryDomain = {
    startRecovery: (
      scanId: string,
      selection: RecoverySelection,
      destination: string,
      options: RecoveryOptions,
    ) =>
      call<{ jobId: string }>('dw_recover_start', {
        scanId,
        selection:
          selection.kind === 'nodes'
            ? { Nodes: { nodeIds: [...selection.nodeIds] } }
            : { All: { filter: filterArgs(selection.filter) } },
        destination,
        options: {
          collision: options.collision,
          rebuildFolders: options.rebuildFolders,
          keepEmptyFolders: options.keepEmptyFolders,
          fileRepair: options.fileRepair,
          verifyOnWrite: options.verifyOnWrite,
        },
      }),
    cancelRecovery: (jobId: string) => call<void>('dw_recover_cancel', { jobId }),
    getRecoveryState: async (jobId: string) => {
      const p = await call<EngineJobProgress>('dw_recover_state', { jobId });
      return {
        jobId,
        scanId: p.scanId,
        nodeIds: [],
        destination: '',
        options: {
          collision: 'rename',
          rebuildFolders: true,
          keepEmptyFolders: false,
          fileRepair: false,
          verifyOnWrite: true,
        },
        status: toJobStatus(p.status),
      };
    },
    getRecoveryItems: async (jobId: string, page) => {
      const raw = await call<readonly EngineJobItem[]>('dw_recover_items', {
        jobId,
        offset: page.offset,
        limit: page.limit,
      });
      return raw.map((i) => ({
        nodeId: i.nodeId,
        name: i.name,
        sizeBytes: i.sizeBytes,
        state: i.state,
        error:
          i.state === 'failed'
            ? {
                code: mapItemErrorCode(i.errorCode ?? 'unknown'),
                reasonKey: 'recovery.itemFailedReason',
              }
            : undefined,
        savedAs: i.savedAs ?? undefined,
      }));
    },
    onRecoveryProgress: (jobId: string, cb: (event: RecoveryProgressEvent) => void) => {
      requireTauri();
      let unlisten: UnlistenFn | null = null;
      let disposed = false;
      void listen<EngineJobProgress>('dw-recovery-progress', (event) => {
        if (event.payload.jobId === jobId) {
          const p = event.payload;
          cb({
            jobId: p.jobId,
            status: toJobStatus(p.status),
            overallProgress: p.overallProgress,
            currentFile: p.currentFile,
            currentFileProgress: p.currentFileProgress,
            completedCount: p.completedCount,
            failedCount: p.failedCount,
            skippedCount: p.skippedCount,
            totalCount: p.totalCount,
            throughputBytesPerSecond: p.throughputBytesPerSecond,
            verifying: p.verifying,
            etaSeconds: p.etaSeconds,
          });
        }
      }).then((fn) => {
        if (disposed) {
          fn();
        } else {
          unlisten = fn;
        }
      });
      return () => {
        disposed = true;
        unlisten?.();
      };
    },
    getRecoveryReport: async (jobId: string) => {
      const raw = await call<EngineJobReport>('dw_recover_report', { jobId });
      const mapped: RecoveryReport = {
        jobId: raw.jobId,
        status: toJobStatus(raw.status),
        startedAt: iso(raw.startedAtUnix),
        finishedAt: iso(raw.finishedAtUnix),
        succeeded: raw.succeeded.map((f) => ({
          nodeId: f.nodeId,
          name: f.name,
          sizeBytes: f.sizeBytes,
          state: 'done' as const,
          savedAs: f.savedAs ?? undefined,
        })),
        failed: raw.failed.map((f) => ({
          nodeId: f.nodeId,
          name: f.name,
          sizeBytes: f.sizeBytes,
          state: 'failed' as const,
          error: {
            code: mapItemErrorCode(f.errorCode ?? 'unknown'),
            reasonKey: 'recovery.itemFailedReason',
          },
        })),
        skipped: raw.skipped.map((f) => ({
          nodeId: f.nodeId,
          name: f.name,
          sizeBytes: f.sizeBytes,
          state: 'skipped' as const,
        })),
        totalBytesWritten: raw.totalBytesWritten,
        verified: raw.verified,
        processedTotal: raw.processedTotal,
        truncatedItemLists: raw.truncatedItemLists,
      };
      return mapped;
    },
  };

  const health: HealthDomain = {
    getHealth: (deviceId: string) =>
      call<DiskHealth>('dw_disk_health', { deviceId }).catch((raw) => {
        throw toConnectorError(raw, 'health-unavailable');
      }),
  };

  const repair: RepairDomain = {
    dryRunRepair: (deviceId: string) => call<RepairDryRun>('dw_repair_dry_run', { deviceId }),
    applyRepair: (deviceId: string, options: { consentToken: string }) =>
      call<{ applied: number }>('dw_repair_apply', {
        deviceId,
        options: { consentToken: options.consentToken },
      }),
    getUndoLog: async (deviceId: string) => {
      const raw = await call<
        { index: number; action: string; where: string; atUnix: number; snapshotHex: string }[]
      >('dw_repair_undo_log', { deviceId });
      const mapped: readonly RepairUndoEntry[] = raw.map((e) => ({
        index: e.index,
        action: e.action,
        where: e.where,
        at: iso(e.atUnix),
        snapshotBytes: e.snapshotHex.length / 2,
      }));
      return mapped;
    },
    undoRepairs: (deviceId: string) =>
      call<{ rolledBack: number }>('dw_repair_undo', { deviceId }),
  };

  const system: SystemDomain = {
    getDriverStatus: async () => {
      const info = await call<{
        name: string;
        version: string;
        engineVersion: string;
        platform: string;
      }>('dw_app_info');
      const status: DriverStatus = {
        installed: true,
        version: info.version,
        running: true,
      };
      return status;
    },
    getAppInfo: async () => {
      const raw = await call<{
        name: string;
        version: string;
        connectorMode: string;
        engineVersion: string;
        platform: string;
      }>('dw_app_info');
      const mapped: AppInfo = {
        name: raw.name,
        version: raw.version,
        connectorMode: 'tauri',
        engineVersion: raw.engineVersion,
        platform: raw.platform,
      };
      return mapped;
    },
    validateDestination: (path, sourceDeviceId, neededBytes) =>
      call<DestinationValidation>('dw_validate_destination', {
        path,
        sourceDeviceId,
        neededBytes,
      }).catch((raw) => {
        throw toConnectorError(raw, 'destination-invalid');
      }),
    pickDirectory: () => call<string | null>('dw_pick_directory', {}),
    listMountPoints: async () => {
      const raw =
        await call<
          { id: string; label: string; path: string; freeBytes: number; totalBytes: number }[]
        >('dw_mount_points');
      const mapped: readonly MountPoint[] = raw.map((m) => ({ ...m }));
      return mapped;
    },
    openPath: (path: string) => call<void>('dw_open_path', { path }),
    listDiskLayouts: async () => {
      const raw = await call<
        {
          diskNo: number;
          sizeBytes: number;
          sectorSize: number;
          style: string;
          partitions: {
            index: number;
            kind: string;
            startByte: number;
            sizeBytes: number;
            label: string | null;
          }[];
          lostPartitions: {
            id: string;
            diskNo: number;
            startByte: number;
            sizeBytes: number;
            probableFs: string;
            note: string | null;
          }[];
        }[]
      >('dw_disk_layouts');
      const mapped: readonly DiskLayout[] = raw.map((d) => ({
        diskNo: d.diskNo,
        sizeBytes: d.sizeBytes,
        sectorSize: d.sectorSize,
        style: d.style === 'gpt' || d.style === 'mbr' ? d.style : 'unknown',
        partitions: d.partitions,
        lostPartitions: d.lostPartitions,
      }));
      return mapped;
    },
    getSpecialFolders: () => call<SpecialFolders>('dw_special_folders'),
  };

  const cleanup: CleanupDomain = {
    stagePath: async (path) =>
      toStagedItem(await call<EngineStagedItem>('dw_stage_path', { path })),
    listStaging: async () =>
      (await call<EngineStagedItem[]>('dw_list_staging')).map(toStagedItem),
    undoStaging: (id) => call<CleanupUndoOutcome>('dw_undo_staging', { id }),
    undoResolve: (id, resolution: CleanupUndoResolution) =>
      call<CleanupUndoOutcome>('dw_undo_resolve', { id, resolution }),
    permanentDelete: async (id, confirmPhrase, secureWipe) => {
      const raw = await call<{
        id: number;
        original_path: string;
        staged_path: string;
        size_bytes: number;
        deleted_at_unix: number;
        blake3_first4kb_hex: string | null;
        is_dir: boolean;
      }>('dw_permanent_delete', { id, confirmPhrase, secureWipe });
      const mapped: CleanupPermanentResult = {
        id: raw.id,
        originalPath: raw.original_path,
        stagedPath: raw.staged_path,
        sizeBytes: raw.size_bytes,
        deletedAt: iso(raw.deleted_at_unix),
        blake3First4kbHex: raw.blake3_first4kb_hex,
        isDir: raw.is_dir,
      };
      return mapped;
    },
    listExpired: async () => {
      const raw = await call<
        {
          id: number;
          original_path: string;
          size_bytes: number;
          expired_at_unix: number;
          is_dir: boolean;
          age_secs: number;
        }[]
      >('dw_list_expired');
      const mapped: readonly CleanupExpiredItem[] = raw.map((e) => ({
        id: e.id,
        originalPath: e.original_path,
        sizeBytes: e.size_bytes,
        expiredAt: iso(e.expired_at_unix),
        isDir: e.is_dir,
        ageSeconds: e.age_secs,
      }));
      return mapped;
    },
    cleanupExpired: (ratePerSec, force) =>
      call<CleanupReport>('dw_cleanup_expired', { ratePerSec, force }),
    listRules: async () => (await call<EngineUserRule[]>('dw_rules_list')).map(toRule),
    addRule: (pattern, patternType, score, explanation) =>
      call<EngineUserRule>('dw_rule_add', {
        pattern,
        patternType,
        score,
        explanation,
      }).then(toRule),
    deleteRule: (id) => call<void>('dw_rule_delete', { id }),
    toggleRule: (id, enabled) => call<void>('dw_rule_toggle', { id, enabled }),
    findDuplicates: (scanId, opts) =>
      call<EngineDuplicates>('dw_duplicates_find', {
        scanId,
        minSizeBytes: opts?.minSizeBytes ?? null,
        sizeOnly: opts?.sizeOnly ?? false,
      }).then(toDuplicates),
    captureSnapshot: (scanId) =>
      call<EngineSnapshotMeta>('dw_snapshot_capture', { scanId }).then(toSnapshotMeta),
    listSnapshots: async () =>
      (await call<EngineSnapshotMeta[]>('dw_snapshots_list')).map(toSnapshotMeta),
    snapshotDelta: (from, to) =>
      call<EngineDelta>('dw_snapshot_delta', { from, to }).then(toDelta),
    cleanupSnapshots: (retainDays) =>
      call<number>('dw_snapshots_cleanup', { retainDays: retainDays ?? null }),
    searchIndex: async (query, limit) => {
      const raw = await call<EngineIndexHit[]>('dw_index_search', {
        query,
        limit: limit ?? 50,
      });
      const mapped: readonly IndexHit[] = raw.map((h) => ({
        volumeId: h.volume_id,
        name: h.name,
        fullPath: h.full_path,
        isDir: (h.attrs & 0x10) !== 0,
      }));
      return mapped;
    },
    getSetting: (key) => call<string | null>('dw_get_setting', { key }),
    setSetting: (key, value) => call<void>('dw_set_setting', { key, value }),
  };

  return {
    devices,
    scan,
    results,
    preview,
    sessions,
    recovery,
    health,
    repair,
    system,
    cleanup,
  };
}

function mapItemErrorCode(code: string): ConnectorErrorCode {
  switch (code) {
    case 'record-reused':
    case 'clusters-overwritten':
    case 'unsupported-encoding':
    case 'source-missing':
    case 'source-unreadable':
    case 'verify-failed':
      return 'recovery-item-failed';
    case 'collision-skip':
      return 'unknown';
    default:
      return 'recovery-item-failed';
  }
}
