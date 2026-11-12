/**
 * CONNECTOR CONTRACT — the single source of truth for every command and event
 * the recovery backend will ever expose. The UI talks ONLY to this interface
 * (via src/queries). Swapping the Mock Connector for a real IPC bridge must
 * require zero changes above this layer.
 *
 * Principles:
 *  - The UI renders and requests; the connector provides and grades.
 *  - No UI-side intelligence that belongs in the backend (no filesystem
 *    parsing, no confidence re-grading, no scan orchestration).
 *  - Errors carry machine codes + localizable reason keys; the UI never
 *    string-matches error text.
 */

// ---------------------------------------------------------------------------
// Shared primitives
// ---------------------------------------------------------------------------

/** ISO-8601 timestamp string. */
export type Iso8601 = string;

export type FileCategoryId =
  'graphics' | 'video' | 'audio' | 'document' | 'email' | 'archive' | 'bookmarks' | 'other';

/** Recovery-condition grades come from the connector; the UI never re-grades. */
export type ConfidenceGrade = 'excellent' | 'good' | 'fair' | 'poor' | 'unknown';

export type DeviceKind = 'internal' | 'usb' | 'sd' | 'optical' | 'network' | 'virtual';
export type VolumeFs =
  | 'ntfs'
  | 'fat32'
  | 'exfat'
  | 'ext4'
  | 'btrfs'
  | 'refs'
  | 'hfsplus'
  | 'zfs'
  | 'raw'
  | 'unknown';

export type DeviceStatus = 'ok' | 'permission-denied' | 'disconnected' | 'locked';

export interface Capacity {
  readonly totalBytes: number;
  readonly usedBytes: number;
  readonly freeBytes: number;
}

export interface ConnectorError extends Error {
  readonly code: ConnectorErrorCode;
  /** i18n key suffix under `errors.` or `preview.reason.` for preview failures. */
  readonly reasonKey?: string;
  readonly detail?: Record<string, unknown>;
}

export type ConnectorErrorCode =
  | 'device-gone'
  | 'device-permission'
  | 'scan-not-found'
  | 'scan-not-paused'
  | 'scan-not-running'
  | 'scan-scope-invalid'
  | 'session-corrupt'
  | 'session-not-found'
  | 'destination-overlap'
  | 'destination-invalid'
  | 'destination-unwritable'
  | 'out-of-space'
  | 'preview-unsupported'
  | 'preview-too-large'
  | 'preview-damaged'
  | 'preview-encrypted'
  | 'preview-timeout'
  | 'preview-missing-data'
  | 'recovery-item-failed'
  | 'repair-consent-required'
  | 'repair-failed'
  | 'health-unavailable'
  | 'not-implemented'
  | 'unknown';

// ---------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------

export interface Device {
  readonly id: string;
  readonly kind: DeviceKind;
  /** e.g. "NVMe SSD" or "USB Mass Storage". */
  readonly model: string;
  /** Friendly label, e.g. "Local Disk (C:)". */
  readonly label: string;
  readonly serial: string;
  readonly capacity: Capacity;
  readonly status: DeviceStatus;
  readonly isSourceOfActiveScan?: boolean;
  readonly volumes: Volume[];
  /** Whole-disk entry point exists (deep search on raw device). */
  readonly canScanWholeDisk: boolean;
}

export interface Volume {
  readonly id: string;
  readonly deviceId: string;
  readonly label: string;
  readonly letter: string | null;
  readonly fs: VolumeFs;
  readonly capacity: Capacity;
  readonly status: DeviceStatus;
  /** Candidate lost partition found on this device (deep-search entry point). */
  readonly lostPartition?: LostPartitionHint;
  /** Quick search availability — when false the UI must offer deep only. */
  readonly quickScanAvailable: boolean;
  /** Optional connector-exposed sector size override for advanced settings. */
  readonly sectorSize?: number;
  /** Whether BitLocker encryption is enabled on this volume ('unlocked' | 'locked' | boolean). */
  readonly bitlocker?: boolean | 'unlocked' | 'locked';
}

export interface LostPartitionHint {
  readonly id: string;
  readonly startByte: number;
  readonly sizeBytes: number;
  readonly probableFs: VolumeFs;
  readonly note?: string;
}

export interface DevicesDomain {
  listDevices(): Promise<Device[]>;
  /**
   * Device info for a scan target. `deviceId` may be a device id or a volume
   * id — the connector resolves a volume to its owning device (matching
   * startScan's target semantics).
   */
  getDeviceInfo(deviceId: string): Promise<Device>;
  refreshDevices(): Promise<Device[]>;
}

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

export type ScanStatus =
  | 'idle'
  | 'starting'
  | 'running'
  | 'paused'
  | 'pausing'
  | 'resuming'
  | 'stopping'
  | 'completed'
  | 'cancelled'
  | 'failed';

export type ScanStage =
  'volume-analysis' | 'quick-index' | 'deep-walk' | 'carve' | 'tree-build' | 'done';

/** Which reference-visible phase a stage belongs to (status-line labels). */
export type ScanPhase = 'quick' | 'advanced' | 'organizing';

export function scanPhaseOf(stage: ScanStage): ScanPhase {
  if (stage === 'quick-index' || stage === 'volume-analysis') return 'quick';
  if (stage === 'deep-walk' || stage === 'carve') return 'advanced';
  return 'organizing';
}

export interface ScanParams {
  readonly targetId: string;
  /** Optional folder restriction ("Specify a location" -> Select Folder).
   * Null/omitted = whole volume. The scan ALWAYS runs quick -> advanced
   * automatically (ADR-008): the reference exposes no mode selection. */
  readonly pathScope?: string | null;
}

export interface ScanProgressEvent {
  readonly scanId: string;
  readonly status: ScanStatus;
  readonly stage: ScanStage;
  /** 0..1 within the current stage. */
  readonly stageProgress: number;
  /** 0..1 weighted across stages; monotonic — never regresses. */
  readonly overallProgress: number;
  readonly bytesScanned: number;
  readonly bytesTotal: number;
  readonly filesFound: number;
  readonly dataFoundBytes: number;
  /** Recoverable (deleted) entries found so far — the recovery headline. */
  readonly deletedFound: number;
  readonly errors: number;
  readonly badSectorMap: readonly BadSectorRange[];
  readonly etaSeconds: number | null;
  readonly throughputBytesPerSecond: number;
  /** True when results so far are safe to recover while the scan runs. */
  readonly recoverableDuringScan: boolean;
  readonly deviceConnected: boolean;
  /** The item currently being examined — the live "Searching: ..." ticker. */
  readonly currentPath: string | null;
}

export interface BadSectorRange {
  readonly startByte: number;
  readonly sizeBytes: number;
}

export interface ScanState {
  readonly scanId: string;
  readonly params: ScanParams;
  readonly target: Device | Volume;
  readonly status: ScanStatus;
  readonly startedAt: Iso8601;
  readonly completedAt: Iso8601 | null;
  readonly progress: ScanProgressEvent;
  /** Present on failed scans. */
  readonly failure?: { code: ConnectorErrorCode; reasonKey?: string };
}

export interface ScanDomain {
  startScan(params: ScanParams): Promise<{ scanId: string }>;
  pauseScan(scanId: string): Promise<void>;
  resumeScan(scanId: string): Promise<void>;
  cancelScan(scanId: string): Promise<void>;
  getScanState(scanId: string): Promise<ScanState>;
  /** Subscribe to progress events; returns an unsubscribe function. */
  onScanProgress(scanId: string, cb: (event: ScanProgressEvent) => void): () => void;
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

export type ResultNodeKind = 'file' | 'directory' | 'group';

export type ResultOrigin = 'existing' | 'deleted' | 'raw' | 'lost-path' | 'unsaved';

export interface ResultNode {
  readonly id: string;
  readonly scanId: string;
  readonly kind: ResultNodeKind;
  readonly name: string;
  readonly path: string | null;
  readonly sizeBytes: number;
  readonly modifiedAt: Iso8601 | null;
  readonly extension: string | null;
  readonly category: FileCategoryId | null;
  readonly confidence: ConfidenceGrade;
  readonly origin: ResultOrigin;
  readonly childCount: number | null;
  readonly thumbnailAvailable: boolean;
  readonly parentId: string | null;
  readonly attributes?: Readonly<Record<string, string>>;
}

export type ResultTreeMode = 'path' | 'type' | 'raw';

export interface TreeNode {
  readonly id: string;
  readonly parentId: string | null;
  readonly label: string;
  readonly icon: 'directory' | 'category' | 'group' | 'device';
  readonly childCount: number;
  readonly fileCount: number;
  readonly sizeBytes: number;
  readonly origin: ResultOrigin;
  readonly special?: 'deleted' | 'lost-path' | 'raw-found' | 'unsaved' | 'existing';
}

export interface TreePage {
  readonly nodes: readonly TreeNode[];
  readonly parentHasMore: boolean;
}

export type SortField = 'name' | 'modified' | 'type' | 'size' | 'confidence';
export type SortDirection = 'asc' | 'desc';

export interface ResultsFilter {
  readonly nodeId: string | null;
  readonly treeMode: ResultTreeMode;
  readonly search: string | null;
  readonly categories: readonly FileCategoryId[] | null;
  readonly dateFrom: Iso8601 | null;
  readonly dateTo: Iso8601 | null;
  readonly sizeMinBytes: number | null;
  readonly sizeMaxBytes: number | null;
  readonly extensions: readonly string[] | null;
  readonly origins: readonly ResultOrigin[] | null;
  /** True = drop zero-byte files ("Hide 0 KB files" toggle). */
  readonly hideEmptyFiles: boolean;
}

export interface ResultsPage {
  readonly nodes: readonly ResultNode[];
  readonly total: number;
  readonly totalBytes: number;
  readonly offset: number;
  readonly pageSize: number;
  /** Wall-time the connector took to resolve this page (perf grid #36). */
  readonly elapsedMs: number;
}

export interface NodeDetail {
  readonly node: ResultNode;
  readonly fullPhysicalPath: string | null;
  readonly createdAt: Iso8601 | null;
  readonly accessedAt: Iso8601 | null;
  readonly confidenceReason: string | null;
  readonly badSectorsOverlap: boolean;
  readonly clusterCount: number | null;
}

export interface ResultsDomain {
  getResultTree(
    scanId: string,
    parentId: string | null,
    mode: ResultTreeMode,
  ): Promise<TreePage>;
  queryResults(
    scanId: string,
    filter: ResultsFilter,
    sort: { field: SortField; direction: SortDirection },
    page: { offset: number; limit: number },
  ): Promise<ResultsPage>;
  getNodeDetail(scanId: string, nodeId: string): Promise<NodeDetail>;
  countResults(scanId: string, filter: ResultsFilter): Promise<number>;
}

// ---------------------------------------------------------------------------
// Preview
// ---------------------------------------------------------------------------

export type PreviewPayload =
  | {
      readonly kind: 'image';
      readonly dataUrl: string;
      /** Pixel dimensions when the engine could parse the header; null otherwise. */
      readonly width: number | null;
      readonly height: number | null;
      readonly bounded: boolean;
    }
  | {
      readonly kind: 'hex';
      readonly bytes: Uint8Array;
      readonly totalSize: number;
      readonly baseOffset: number;
    }
  | {
      readonly kind: 'text';
      readonly text: string;
      readonly encoding: string;
      readonly totalSize: number;
      readonly truncated: boolean;
    }
  | {
      readonly kind: 'pdf';
      readonly pages: readonly {
        readonly dataUrl: string;
        readonly width: number;
        readonly height: number;
      }[];
      readonly pageCount: number;
    }
  | {
      readonly kind: 'av';
      readonly metadata: Readonly<Record<string, string>>;
      readonly thumbnailDataUrl: string | null;
      readonly durationSeconds: number | null;
    }
  | {
      readonly kind: 'office';
      readonly metadata: Readonly<Record<string, string>>;
      readonly extractedText: string | null;
      readonly truncated: boolean;
    };

export interface PreviewDomain {
  /** The connector decides the preview kind from its own file knowledge;
   * the UI never classifies extensions (engine knowledge stays behind the
   * connector boundary). */
  requestPreview(scanId: string, nodeId: string): Promise<PreviewPayload>;
  getThumbnail(scanId: string, nodeId: string): Promise<{ dataUrl: string } | null>;
}

// ---------------------------------------------------------------------------
// Sessions (.rsf-style scan session persistence)
// ---------------------------------------------------------------------------

export interface SessionMeta {
  readonly id: string;
  readonly fileName: string;
  readonly savedAt: Iso8601;
  readonly targetLabel: string;
  readonly filesFound: number;
  readonly dataFoundBytes: number;
  readonly completed: boolean;
}

export interface SessionsDomain {
  saveSession(scanId: string, path: string | null): Promise<SessionMeta>;
  listSessions(): Promise<SessionMeta[]>;
  loadSession(path: string): Promise<{ scanId: string }>;
  getSessionMeta(path: string): Promise<SessionMeta>;
}

// ---------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------

export type CollisionPolicy = 'rename' | 'overwrite' | 'skip';

export interface RecoveryOptions {
  readonly collision: CollisionPolicy;
  readonly rebuildFolders: boolean;
  readonly keepEmptyFolders: boolean;
  readonly fileRepair: boolean;
  readonly verifyOnWrite: boolean;
}

export interface RecoveryJob {
  readonly jobId: string;
  readonly scanId: string;
  readonly nodeIds: readonly string[];
  readonly destination: string;
  readonly options: RecoveryOptions;
  readonly status: RecoveryJobStatus;
}

export type RecoveryJobStatus =
  'validating' | 'running' | 'completed' | 'completed-partial' | 'cancelled' | 'failed';

export interface RecoveryItem {
  readonly nodeId: string;
  readonly name: string;
  readonly sizeBytes: number;
  readonly state: 'pending' | 'running' | 'done' | 'failed' | 'skipped' | 'verifying';
  readonly error?: { code: ConnectorErrorCode; reasonKey?: string };
  readonly savedAs?: string;
}

export interface RecoveryProgressEvent {
  readonly jobId: string;
  readonly status: RecoveryJobStatus;
  readonly overallProgress: number;
  readonly currentFile: string | null;
  readonly currentFileProgress: number;
  readonly completedCount: number;
  readonly failedCount: number;
  readonly skippedCount: number;
  readonly totalCount: number;
  readonly throughputBytesPerSecond: number;
  readonly verifying: boolean;
  readonly etaSeconds: number | null;
}

export interface RecoveryReport {
  readonly jobId: string;
  readonly status: RecoveryJobStatus;
  readonly startedAt: Iso8601;
  readonly finishedAt: Iso8601 | null;
  readonly succeeded: readonly RecoveryItem[];
  readonly failed: readonly RecoveryItem[];
  readonly skipped: readonly RecoveryItem[];
  readonly totalBytesWritten: number;
  readonly verified: boolean;
  /** Exact number of items the job processed (succeeded+failed+skipped),
   *  independent of the bounded per-item lists. */
  readonly processedTotal: number;
  /** True when the item lists were capped (recover-all at scale); counts
   *  remain exact — only the per-item arrays are bounded. */
  readonly truncatedItemLists: boolean;
}

/** What to recover: an explicit node list, or everything matching a filter
 *  (the reference's Recover All — the export loop iterates the whole set). */
export type RecoverySelection =
  | { readonly kind: 'nodes'; readonly nodeIds: readonly string[] }
  | { readonly kind: 'all'; readonly filter: ResultsFilter };

export interface RecoveryDomain {
  startRecovery(
    scanId: string,
    selection: RecoverySelection,
    destination: string,
    options: RecoveryOptions,
  ): Promise<{ jobId: string }>;
  cancelRecovery(jobId: string): Promise<void>;
  getRecoveryState(jobId: string): Promise<RecoveryJob>;
  getRecoveryItems(
    jobId: string,
    page: { offset: number; limit: number },
  ): Promise<readonly RecoveryItem[]>;
  onRecoveryProgress(jobId: string, cb: (event: RecoveryProgressEvent) => void): () => void;
  getRecoveryReport(jobId: string): Promise<RecoveryReport>;
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

export type HealthStatus = 'good' | 'caution' | 'bad';

export interface SmartAttribute {
  readonly id: number;
  readonly name: string;
  readonly current: number | null;
  readonly worst: number | null;
  readonly threshold: number | null;
  readonly raw: number;
  readonly status: 'ok' | 'failing' | 'unknown';
}

export interface DiskHealth {
  readonly deviceId: string;
  readonly status: HealthStatus;
  readonly temperatureC: number | null;
  readonly powerOnHours: number | null;
  readonly diskClass: 'ssd' | 'hdd' | 'unknown';
  readonly wearPercent: number | null;
  readonly trimEnabled: boolean | null;
  readonly reallocatedSectors: number | null;
  readonly pendingSectors: number | null;
  readonly attributes: readonly SmartAttribute[];
  /** Engine-side honest limitation notes (e.g. NVMe health passthrough). */
  readonly note?: string;
}

export interface HealthDomain {
  getHealth(deviceId: string): Promise<DiskHealth>;
}

// ---------------------------------------------------------------------------
// Repair (transactional volume structure repair)
// ---------------------------------------------------------------------------

export type RepairFindingKind =
  | 'boot-sector-mismatch'
  | 'backup-dbr-stale'
  | 'fat-table-damaged'
  | 'mft-record-error'
  | 'sectors-per-cluster-corrupt'
  | 'checksum-mismatch';

export interface RepairFinding {
  readonly id: string;
  readonly kind: RepairFindingKind;
  readonly where: string;
  readonly descriptionKey: string;
  readonly fixable: boolean;
  readonly severity: 'info' | 'warning' | 'critical';
}

export interface RepairDryRun {
  readonly deviceId: string;
  readonly findings: readonly RepairFinding[];
  readonly canApply: boolean;
  /** Engine-side honest limitation notes (e.g. system-volume repairs blocked). */
  readonly note?: string;
}

export interface RepairUndoEntry {
  readonly index: number;
  readonly action: string;
  readonly where: string;
  readonly at: Iso8601;
  readonly snapshotBytes: number;
}

export interface RepairDomain {
  dryRunRepair(deviceId: string): Promise<RepairDryRun>;
  applyRepair(
    deviceId: string,
    options: { consentToken: string },
  ): Promise<{ applied: number }>;
  getUndoLog(deviceId: string): Promise<readonly RepairUndoEntry[]>;
  undoRepairs(deviceId: string): Promise<{ rolledBack: number }>;
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

export interface DriverStatus {
  readonly installed: boolean;
  readonly version: string | null;
  readonly running: boolean;
  readonly note?: string;
}

export interface AppInfo {
  readonly name: string;
  readonly version: string;
  readonly connectorMode: 'mock' | 'tauri';
  readonly engineVersion: string | null;
  readonly platform: string;
}

export interface MountPoint {
  readonly id: string;
  readonly label: string;
  readonly path: string;
  readonly freeBytes: number;
  readonly totalBytes: number;
}

export interface DestinationValidation {
  readonly allowed: boolean;
  readonly reason?: 'overlap' | 'unwritable' | 'invalid' | 'out-of-space';
  readonly reasonKey?: string;
  readonly freeBytes: number | null;
  readonly neededBytes: number | null;
}

/** One partition (or unallocated gap) on a physical disk. */
export interface DiskPartition {
  readonly index: number;
  readonly kind: string;
  readonly startByte: number;
  readonly sizeBytes: number;
  readonly label: string | null;
}

/** A lost volume found in unallocated space (boot sector detected). */
export interface LostPartition {
  /** Stable scan target id (`lp:<disk>:<offset>`). */
  readonly id: string;
  readonly diskNo: number;
  readonly startByte: number;
  readonly sizeBytes: number;
  readonly probableFs: string;
  readonly note: string | null;
}

export interface DiskLayout {
  readonly diskNo: number;
  readonly sizeBytes: number;
  readonly sectorSize: number;
  readonly style: 'gpt' | 'mbr' | 'unknown';
  readonly partitions: readonly DiskPartition[];
  readonly lostPartitions: readonly LostPartition[];
}

/** Well-known quick-target folders (Recycle Bin, Desktop, ...). */
export interface SpecialFolders {
  readonly recycleBin: string | null;
  readonly desktop: string | null;
  readonly documents: string | null;
  readonly downloads: string | null;
}

export interface SystemDomain {
  getDriverStatus(): Promise<DriverStatus>;
  getAppInfo(): Promise<AppInfo>;
  /** Physical disk layouts incl. lost-partition candidates (admin). */
  listDiskLayouts(): Promise<readonly DiskLayout[]>;
  /** Well-known folders for the Home quick targets. */
  getSpecialFolders(): Promise<SpecialFolders>;
  validateDestination(
    path: string,
    sourceDeviceId: string,
    neededBytes: number,
  ): Promise<DestinationValidation>;
  /**
   * Native folder picker; mock returns from a canned set in dev/browser mode.
   * The optional hint roots the dialog at a volume (scan-scope picking on the
   * scanned device) or asks it to avoid a device (destination picking must
   * not suggest the recovery source); the connector still owns validation of
   * the result.
   */
  pickDirectory(hint?: {
    readonly volumeId?: string | null;
    readonly excludeDeviceId?: string | null;
  }): Promise<string | null>;
  listMountPoints(): Promise<readonly MountPoint[]>;
  openPath(path: string): Promise<void>;
}

// ---------------------------------------------------------------------------
// Cleanup — the D-Space safety features carried into DiskWizard
// ---------------------------------------------------------------------------

/** A file/folder staged for deletion (recoverable for 24h). */
export interface CleanupStagedItem {
  readonly id: number;
  readonly originalPath: string;
  readonly stagedPath: string;
  readonly sizeBytes: number;
  readonly stagedAt: Iso8601;
  readonly expiresAt: Iso8601;
  readonly isDir: boolean;
  /** 'normal' = same-volume atomic rename; 'cross_volume' = verified copy. */
  readonly fallbackTier: 'normal' | 'cross_volume';
}

export interface CleanupFileSnapshot {
  readonly path: string;
  readonly sizeBytes: number;
  readonly modifiedAt: Iso8601;
  readonly blake3First4kbHex: string | null;
  readonly isDir: boolean;
}

export type CleanupUndoOutcome =
  | { readonly kind: 'restored'; readonly originalPath: string }
  | { readonly kind: 'idempotent'; readonly originalPath: string }
  | {
      readonly kind: 'conflict';
      readonly originalPath: string;
      readonly staged: CleanupFileSnapshot;
      readonly target: CleanupFileSnapshot;
    };

export type CleanupUndoResolution = 'overwrite' | 'rename' | 'keep_both' | 'cancel';

export interface CleanupExpiredItem {
  readonly id: number;
  readonly originalPath: string;
  readonly sizeBytes: number;
  readonly expiredAt: Iso8601;
  readonly isDir: boolean;
  readonly ageSeconds: number;
}

export interface CleanupReport {
  readonly deleted: number;
  readonly failed: number;
  readonly totalBytes: number;
  readonly elapsedMs: number;
  readonly abortedThreshold: boolean;
}

export interface CleanupPermanentResult {
  readonly id: number;
  readonly originalPath: string;
  readonly stagedPath: string;
  readonly sizeBytes: number;
  readonly deletedAt: Iso8601;
  readonly blake3First4kbHex: string | null;
  readonly isDir: boolean;
}

/** A user-defined safe-cleanup scoring rule. */
export interface CleanupRule {
  readonly id: number;
  readonly pattern: string;
  readonly patternType: 'name' | 'extension';
  readonly score: number;
  readonly explanation: string;
  readonly enabled: boolean;
  readonly createdAt: Iso8601;
  readonly updatedAt: Iso8601;
}

export interface DuplicateGroup {
  readonly hashHex: string;
  readonly sizeBytes: number;
  readonly paths: readonly string[];
}

export interface DuplicatesResult {
  readonly driveLetter: string;
  readonly scannedFiles: number;
  readonly filteredSmall: number;
  readonly candidatePairs: number;
  readonly hashErrors: number;
  readonly groups: readonly DuplicateGroup[];
  readonly stats: {
    readonly groupCount: number;
    readonly redundantBytes: number;
    readonly elapsedMs: number;
  };
}

/** Size snapshot of a volume (directory-level, captured from a scan). */
export interface CleanupSnapshotMeta {
  readonly id: number;
  readonly volumeId: string;
  readonly capturedAt: Iso8601;
  readonly totalSizeBytes: number;
  readonly fileCount: number;
  readonly dirCount: number;
  readonly entryCount: number;
}

export interface SnapshotDeltaEntry {
  readonly path: string;
  readonly fromSize: number;
  readonly toSize: number;
  readonly deltaBytes: number;
}

export interface SnapshotDelta {
  readonly fromId: number;
  readonly toId: number;
  readonly fromCapturedAt: Iso8601;
  readonly toCapturedAt: Iso8601;
  readonly netChangeBytes: number;
  readonly totalChangedPaths: number;
  readonly added: readonly { path: string; sizeBytes: number }[];
  readonly removed: readonly { path: string; sizeBytes: number }[];
  readonly grew: readonly SnapshotDeltaEntry[];
  readonly shrunk: readonly SnapshotDeltaEntry[];
}

/** Quick-search hit from the live USN index (Everything-like). */
export interface IndexHit {
  readonly volumeId: string;
  readonly name: string;
  readonly fullPath: string | null;
  readonly isDir: boolean;
}

export interface CleanupDomain {
  /** Stage a file/folder for deletion (atomic move; 24h undo window). */
  stagePath(path: string): Promise<CleanupStagedItem>;
  listStaging(): Promise<readonly CleanupStagedItem[]>;
  undoStaging(id: number): Promise<CleanupUndoOutcome>;
  undoResolve(id: number, resolution: CleanupUndoResolution): Promise<CleanupUndoOutcome>;
  permanentDelete(
    id: number,
    confirmPhrase: string,
    secureWipe: boolean,
  ): Promise<CleanupPermanentResult>;
  listExpired(): Promise<readonly CleanupExpiredItem[]>;
  cleanupExpired(ratePerSec: number, force: boolean): Promise<CleanupReport>;

  listRules(): Promise<readonly CleanupRule[]>;
  addRule(
    pattern: string,
    patternType: 'name' | 'extension',
    score: number,
    explanation: string,
  ): Promise<CleanupRule>;
  deleteRule(id: number): Promise<void>;
  toggleRule(id: number, enabled: boolean): Promise<void>;

  findDuplicates(
    scanId: string,
    opts?: { readonly minSizeBytes?: number; readonly sizeOnly?: boolean },
  ): Promise<DuplicatesResult>;

  captureSnapshot(scanId: string): Promise<CleanupSnapshotMeta>;
  listSnapshots(): Promise<readonly CleanupSnapshotMeta[]>;
  snapshotDelta(from: number, to: number): Promise<SnapshotDelta>;
  cleanupSnapshots(retainDays?: number): Promise<number>;

  searchIndex(query: string, limit?: number): Promise<readonly IndexHit[]>;

  getSetting(key: string): Promise<string | null>;
  setSetting(key: string, value: string): Promise<void>;
}

// ---------------------------------------------------------------------------
// The connector
// ---------------------------------------------------------------------------

export interface RecoveryConnector {
  readonly devices: DevicesDomain;
  readonly scan: ScanDomain;
  readonly results: ResultsDomain;
  readonly preview: PreviewDomain;
  readonly sessions: SessionsDomain;
  readonly recovery: RecoveryDomain;
  readonly health: HealthDomain;
  readonly repair: RepairDomain;
  readonly system: SystemDomain;
  /** D-Space cleanup features (staging/undo, rules, duplicates, snapshots). */
  readonly cleanup: CleanupDomain;
}

/** Dev-panel scenario control (mock-only surface; real connector throws). */
export interface ScenarioControl {
  setScenario(scenario: MockScenarioId): void;
  getScenario(): MockScenarioId;
  setSeed(seed: number): void;
  reset(): void;
}

export type MockScenarioId =
  | 'standard'
  | 'large-ntfs'
  | 'sd-card'
  | 'btrfs-disk'
  | 'lost-partition'
  | 'permission-denied'
  | 'disconnect-mid-scan'
  | 'bad-sectors'
  | 'failing-disk'
  | 'out-of-space'
  | 'corrupt-session';
