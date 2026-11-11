/**
 * Mock Connector — a fully typed, deterministic simulator implementing the
 * same command/event contracts the real backend will provide.
 *
 * Determinism: fixed seed per scenario (overridable via ScenarioControl).
 * Timing: progress ticks 100–250 ms; cancel responds within 300 ms.
 * No UI logic lives here: no date formatting, no label choices, no
 * client-side re-grading — the UI consumes connector data as-is.
 */

import { version as appVersion } from '../../../package.json';
import type {
  AppInfo,
  BadSectorRange,
  ConnectorError,
  ConnectorErrorCode,
  DestinationValidation,
  Device,
  DiskHealth,
  DriverStatus,
  MockScenarioId,
  MountPoint,
  NodeDetail,
  PreviewPayload,
  RecoverySelection,
  RecoveryItem,
  RecoveryJob,
  RecoveryJobStatus,
  RecoveryOptions,
  RecoveryProgressEvent,
  RecoveryReport,
  RepairDryRun,
  RepairUndoEntry,
  ResultsFilter,
  ResultsPage,
  ResultTreeMode,
  ScanParams,
  ScanProgressEvent,
  ScanStage,
  ScanState,
  SessionMeta,
  SmartAttribute,
  SortDirection,
  SortField,
  SystemDomain,
  TreeNode,
  TreePage,
  Volume,
} from '../contract';
import { fixturesFor } from './fixtures';
import { NodeSpace } from './node-space';
import { mulberry32, randBell, randInt } from './rng';

function connError(
  code: ConnectorErrorCode,
  message: string,
  reasonKey?: string,
): ConnectorError {
  return Object.assign(new Error(message), { code, reasonKey });
}

const IMAGE_EXTS = new Set([
  'jpg',
  'jpeg',
  'png',
  'gif',
  'bmp',
  'tif',
  'tiff',
  'webp',
  'heic',
  'svg',
  'psd',
  'cr2',
  'cr3',
  'nef',
  'arw',
  'dng',
  'raw',
  'ico',
]);
const AV_EXTS = new Set([
  'mp4',
  'mov',
  'avi',
  'mkv',
  'wmv',
  'flv',
  'm4v',
  'webm',
  'mts',
  'm2ts',
  'mp3',
  'wav',
  'flac',
  'aac',
  'ogg',
  'wma',
]);
const TEXT_EXTS = new Set([
  'txt',
  'log',
  'md',
  'json',
  'xml',
  'yml',
  'yaml',
  'ini',
  'cfg',
  'csv',
  'tsv',
]);
const OFFICE_EXTS = new Set(['doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'odt']);

/** Connector-side preview-kind dispatch (mirrors the engine's per-type
 *  previewer routing — artifact 10 §2). */
function previewKindOf(node: {
  category: string | null;
  extension: string | null;
}): 'image' | 'av' | 'pdf' | 'text' | 'office' | 'hex' {
  const ext = (node.extension ?? '').replace(/^\./, '').toLowerCase();
  if (ext === 'pdf') return 'pdf';
  if (IMAGE_EXTS.has(ext)) return 'image';
  if (AV_EXTS.has(ext)) return 'av';
  if (OFFICE_EXTS.has(ext)) return 'office';
  if (TEXT_EXTS.has(ext)) return 'text';
  return 'hex';
}
/**
 * The ONE automatic stage chain (ADR-008; architecture/04 §3-4, GT-scan
 * §3/§7): volume probe -> quick pass (FS metadata) -> advanced pass (full
 * searchers + RAW carving) -> organize. The quick pass is a short prefix
 * (~21 s on the reference's runner volume); advanced dominates the run.
 */
const STAGE_WEIGHTS: Partial<Record<ScanStage, number>> = {
  'volume-analysis': 0.04,
  'quick-index': 0.14,
  'deep-walk': 0.42,
  carve: 0.24,
  'tree-build': 0.16,
};
const STAGE_ORDER: readonly ScanStage[] = [
  'volume-analysis',
  'quick-index',
  'deep-walk',
  'carve',
  'tree-build',
  'done',
];

interface ScanRuntime {
  params: ScanParams;
  target: Device | Volume;
  status: ScanState['status'];
  startedAt: string;
  completedAt: string | null;
  stage: ScanStage;
  stageProgress: number;
  overall: number;
  bytesScanned: number;
  bytesTotal: number;
  filesFound: number;
  dataFound: number;
  errors: number;
  badSectors: BadSectorRange[];
  eta: number | null;
  throughput: number;
  tick: number;
  timer: ReturnType<typeof setInterval> | null;
  disconnectFired: boolean;
  disconnectHandled: boolean;
  space: NodeSpace;
  volumeId: string;
  scanId: string;
  /** Folder-scope directory mask (null = whole volume). */
  scopeMask: Uint8Array | null;
  /** Live "Searching: ..." ticker item (deep pass + carve). */
  currentPath: string | null;
  /** Stages to run (quick pass skipped when the engine cannot index). */
  stages: readonly ScanStage[];
}

interface JobRuntime {
  jobId: string;
  scanId: string;
  destination: string;
  options: RecoveryOptions;
  /** Explicit selection (kind: 'nodes'). */
  items: MutableRecoveryItem[];
  /** Recover-all: lazily walk every matching node (kind: 'all'). */
  allMode: null | {
    filter: ResultsFilter;
    space: NodeSpace;
    scopeMask: Uint8Array | null;
    cursor: number;
    /** Deterministic per-index outcome rolls, resolved on demand. */
    total: number;
  };
  status: RecoveryJobStatus;
  progress: MutableJobProgress;
  timer: ReturnType<typeof setInterval> | null;
  startedAt: string;
  finishedAt: string | null;
  bytesWritten: number;
  verified: boolean;
  /** Items advanced per tick (scale-aware). */
  batch: number;
  /** First item index not yet known settled (avoids O(n) re-scans). */
  advanceCursor: number;
}

/** Locally mutable mirror of contract progress/item types (mock-internal only). */
type MutableJobProgress = {
  -readonly [K in keyof RecoveryProgressEvent]: RecoveryProgressEvent[K];
};
type MutableRecoveryItem = {
  -readonly [K in keyof RecoveryItem]: RecoveryItem[K];
};

export class MockConnector {
  private scenario: MockScenarioId = 'standard';
  private seed = 42;
  private deviceList: Device[] = [];
  private scans = new Map<string, ScanRuntime>();
  private nodeSpaces = new Map<string, NodeSpace>();
  private sessionStore = new Map<string, SessionMeta>();
  private sessionPathToScan = new Map<string, string>();
  private jobs = new Map<string, JobRuntime>();
  private repairUndoLogs = new Map<string, RepairUndoEntry[]>();
  private repairApplied = new Map<string, number>();
  private nextScanId = 1;
  private nextJobId = 1;
  private nextSessionId = 1;
  private pickIndex = 0;

  constructor() {
    this.rebuildDevices();
  }

  // ------------------------------------------------------------ scenario API

  setScenario(scenario: MockScenarioId): void {
    this.scenario = scenario;
    this.rebuildDevices();
  }

  getScenario(): MockScenarioId {
    return this.scenario;
  }

  setSeed(seed: number): void {
    this.seed = seed;
    this.rebuildDevices();
  }

  reset(): void {
    for (const scan of this.scans.values()) {
      if (scan.timer) clearInterval(scan.timer);
    }
    for (const job of this.jobs.values()) {
      if (job.timer) clearInterval(job.timer);
    }
    this.scans.clear();
    this.jobs.clear();
    this.sessionStore.clear();
    this.sessionPathToScan.clear();
    this.repairUndoLogs.clear();
    this.repairApplied.clear();
    this.seed = 42;
    this.scenario = 'standard';
    this.rebuildDevices();
  }

  private rebuildDevices(): void {
    const fixtures = fixturesFor(this.scenario);
    this.deviceList = [];
    this.nodeSpaces.clear();
    for (const f of fixtures) {
      const device: Device = {
        ...f.device,
        volumes: f.volumes.map((v) => ({
          ...v,
          status:
            this.scenario === 'permission-denied' && v.letter === 'C'
              ? 'permission-denied'
              : v.status,
          quickScanAvailable: this.scenario === 'bad-sectors' ? false : v.quickScanAvailable,
        })),
        isSourceOfActiveScan: false,
      };
      this.deviceList.push(device);
      const primary = f.volumes[0];
      const space = new NodeSpace(
        primary.id,
        f.nodeSeed ^ this.seed,
        f.directoryCount,
        f.fileCount,
        Math.round(primary.capacity.totalBytes * 0.985),
      );
      this.nodeSpaces.set(primary.id, space);
    }
  }

  private spaceFor(targetId: string): NodeSpace {
    const direct = this.nodeSpaces.get(targetId);
    if (direct) return direct;
    // A device id resolves to ITS first volume's space — never another
    // device's (the old any-space fallback silently scanned the wrong disk).
    const { volume } = this.resolveTarget(targetId);
    const byVolume = this.nodeSpaces.get(volume.id);
    if (byVolume) return byVolume;
    throw connError('scan-not-found', `no node space for ${targetId}`);
  }

  private resolveTarget(targetId: string): { device: Device; volume: Volume } {
    for (const d of this.deviceList) {
      for (const v of d.volumes) {
        if (v.id === targetId || d.id === targetId) {
          return { device: d, volume: v };
        }
      }
    }
    throw connError('device-gone', `target ${targetId} not found`, 'errors.deviceGone');
  }

  // ------------------------------------------------------------ devices

  readonly devices = {
    listDevices: async (): Promise<Device[]> => {
      await latency(180, 60);
      return this.deviceList;
    },
    getDeviceInfo: async (deviceId: string): Promise<Device> => {
      await latency(90, 40);
      const d = this.deviceList.find((x) => x.id === deviceId);
      if (d) return d;
      // A volume id resolves to the owning device — scan targets may be
      // either a whole device or one of its volumes (see resolveTarget).
      const owner = this.deviceList.find((x) => x.volumes.some((v) => v.id === deviceId));
      if (owner) return owner;
      throw connError('device-gone', 'device not found', 'errors.deviceGone');
    },
    refreshDevices: async (): Promise<Device[]> => {
      await latency(700, 200);
      this.rebuildDevices();
      return this.deviceList;
    },
  };

  // ------------------------------------------------------------ scan

  readonly scan = {
    startScan: async (params: ScanParams): Promise<{ scanId: string }> => {
      await latency(420, 120);
      const { device, volume } = this.resolveTarget(params.targetId);
      if (volume.status === 'permission-denied' || device.status === 'permission-denied') {
        throw connError('device-permission', 'access denied', 'errors.devicePermission');
      }
      const scanId = `scan-${this.nextScanId++}`;
      const space = this.spaceFor(params.targetId);
      const bytesTotal = volume.capacity.totalBytes;
      // Engine truthfulness: when the volume cannot be quick-indexed the
      // quick pass collapses to a probe and the advanced pass carries the run.
      const stages = volume.quickScanAvailable
        ? STAGE_ORDER
        : (['volume-analysis', 'deep-walk', 'carve', 'tree-build', 'done'] as const);

      // Folder scope (null/empty = whole volume). Unresolvable paths are a
      // hard contract error — the UI cannot override the connector verdict.
      let scopeMask: Uint8Array | null = null;
      if (params.pathScope != null && params.pathScope !== '') {
        const scope = space.resolveScope(params.pathScope);
        if (!scope) {
          throw connError(
            'scan-scope-invalid',
            `folder ${params.pathScope} not found on ${volume.label}`,
            'errors.scanScopeInvalid',
          );
        }
        scopeMask = scope.mask;
      }

      const rt: ScanRuntime = {
        params,
        target: params.targetId.startsWith('vol-') ? volume : device,
        status: 'running',
        startedAt: new Date().toISOString(),
        completedAt: null,
        stage: 'volume-analysis',
        stageProgress: 0,
        overall: 0,
        bytesScanned: 0,
        bytesTotal,
        filesFound: 0,
        dataFound: 0,
        errors: 0,
        badSectors: this.scenario === 'bad-sectors' ? this.makeBadSectorMap(bytesTotal) : [],
        eta: null,
        throughput: 0,
        tick: 0,
        timer: null,
        disconnectFired: false,
        disconnectHandled: false,
        space,
        volumeId: volume.id,
        scanId,
        scopeMask,
        currentPath: null,
        stages,
      };
      this.scans.set(scanId, rt);
      rt.timer = setInterval(() => this.scanTick(rt), 55 + (scanId.length % 3) * 25);
      return { scanId };
    },

    pauseScan: async (scanId: string): Promise<void> => {
      const rt = this.requireScan(scanId);
      if (rt.status !== 'running') throw connError('scan-not-running', 'scan is not running');
      rt.status = 'paused';
      if (rt.timer) clearInterval(rt.timer);
      rt.timer = null;
      await latency(180);
    },

    resumeScan: async (scanId: string): Promise<void> => {
      const rt = this.requireScan(scanId);
      if (rt.status !== 'paused') throw connError('scan-not-paused', 'scan is not paused');
      // A resume after a disconnect injection models the user having reconnected
      // the device (the UI shows the reconnect prompt when deviceConnected=false).
      if (rt.disconnectFired) {
        rt.disconnectFired = false;
      }
      rt.status = 'running';
      rt.timer = setInterval(() => this.scanTick(rt), 55 + (scanId.length % 3) * 25);
      await latency(200);
    },

    cancelScan: async (scanId: string): Promise<void> => {
      const rt = this.requireScan(scanId);
      if (rt.timer) clearInterval(rt.timer);
      rt.timer = null;
      rt.status = 'cancelled';
      rt.completedAt = new Date().toISOString();
      await latency(220);
    },

    getScanState: async (scanId: string): Promise<ScanState> => {
      await latency(40, 20);
      const rt = this.requireScan(scanId);
      return this.toScanState(rt);
    },

    onScanProgress: (scanId: string, cb: (event: ScanProgressEvent) => void): (() => void) => {
      const handler = (): void => {
        const rt = this.scans.get(scanId);
        if (rt) cb(this.toProgress(rt));
      };
      const interval = setInterval(handler, 150);
      return () => clearInterval(interval);
    },
  };

  private requireScan(scanId: string): ScanRuntime {
    const rt = this.scans.get(scanId);
    if (!rt) throw connError('scan-not-found', `scan ${scanId} not found`);
    return rt;
  }

  private makeBadSectorMap(totalBytes: number): BadSectorRange[] {
    const rng = mulberry32(this.seed ^ 0xbad5e);
    const ranges: BadSectorRange[] = [];
    let at = randInt(rng, 0, Math.floor(totalBytes * 0.3));
    for (let i = 0; i < 14; i++) {
      const size = randInt(rng, 512 * 200, 512 * 40_000);
      ranges.push({ startByte: at, sizeBytes: size });
      at += size + randInt(rng, 512 * 1000, 512 * 900_000);
    }
    return ranges;
  }

  private visibleFraction(rt: ScanRuntime): number {
    return rt.status === 'completed' || rt.status === 'cancelled'
      ? 1
      : Math.max(0.05, 1 - Math.exp(-3.4 * rt.overall));
  }

  private scanTick(rt: ScanRuntime): void {
    rt.tick++;
    const stages = rt.stages;
    const curIdx = stages.indexOf(rt.stage);
    const totalTicks = 96 + (rt.space.fileCount > 500_000 ? 60 : 0);

    // per-tick increments (faster in automated test runner while preserving every stage transition)
    const isTest = import.meta.env?.MODE === 'test';
    const stageSpeed = isTest ? 0.22 : 0.014 + (rt.tick % 5 === 0 ? 0.006 : 0);
    rt.stageProgress = Math.min(1, rt.stageProgress + stageSpeed);

    // device disconnect injection (one-shot per scan: reconnects stick)
    if (
      this.scenario === 'disconnect-mid-scan' &&
      !rt.disconnectHandled &&
      !rt.disconnectFired &&
      rt.overall > 0.38
    ) {
      rt.disconnectFired = true;
      rt.disconnectHandled = true;
      rt.status = 'paused';
      if (rt.timer) clearInterval(rt.timer);
      rt.timer = null;
    }

    // bad-sector error accumulation
    if (this.scenario === 'bad-sectors' && rt.tick % 9 === 0) {
      rt.errors += 1;
    }

    // bytes/throughput
    const byteSpeed = (rt.bytesTotal / totalTicks) * (0.8 + ((rt.tick * 37) % 40) / 100);
    rt.bytesScanned = Math.min(rt.bytesTotal, rt.bytesScanned + byteSpeed);
    rt.throughput = byteSpeed * 8;
    rt.eta = Math.max(1, Math.round(((totalTicks - rt.tick) * 150) / 1000));

    // overall from active stage weights (normalized so omitting 'quick-index' still spans 0..1)
    const activeWeightTotal = stages.reduce((acc, s) => acc + (STAGE_WEIGHTS[s] ?? 0), 0) || 1;
    let before = 0;
    for (let i = 0; i < curIdx; i++) {
      before += (STAGE_WEIGHTS[stages[i]] ?? 0) / activeWeightTotal;
    }
    const curWeight = (STAGE_WEIGHTS[rt.stage] ?? 0) / activeWeightTotal;
    rt.overall = Math.min(1, before + curWeight * rt.stageProgress);

    if (rt.stageProgress >= 1) {
      const next = stages[curIdx + 1];
      if (next) {
        rt.stage = next;
        rt.stageProgress = 0;
      } else {
        rt.stage = 'done';
        rt.status = 'completed';
        rt.completedAt = new Date().toISOString();
        rt.overall = 1;
        rt.bytesScanned = rt.bytesTotal;
        rt.eta = 0;
        if (rt.timer) clearInterval(rt.timer);
        rt.timer = null;
      }
    }

    // found-file curve: fast then saturating during scan, exact 100% upon completion
    const stats = rt.space.stats(rt.scopeMask);
    if (rt.status === 'completed') {
      rt.filesFound = stats.files;
      rt.dataFound = stats.size;
      this.autoSaveSession(rt);
    } else {
      const curve = 1 - Math.exp(-3.4 * rt.overall);
      rt.filesFound = Math.floor(stats.files * curve);
      rt.dataFound = Math.floor(stats.size * curve);
    }

    // Live "Searching: <path>" ticker — the item currently examined
    // (corpus GT-scan §2/§7). Assigned AFTER the stage advance so the
    // ticker and stage are always consistent in a polled snapshot.
    rt.currentPath =
      rt.stage === 'deep-walk' || rt.stage === 'carve'
        ? rt.space.searchingPathAt(rt.tick, rt.scopeMask)
        : null;
  }

  private autoSaveSession(rt: ScanRuntime): void {
    const meta: SessionMeta = {
      id: `sess-${this.nextSessionId}`,
      fileName: `auto_${rt.volumeId}_${new Date().toISOString().replace(/[:.]/g, '-')}.rsf`,
      savedAt: new Date().toISOString(),
      targetLabel: rt.target.label,
      filesFound: rt.filesFound,
      dataFoundBytes: rt.dataFound,
      completed: true,
    };
    this.sessionStore.set(meta.id, meta);
    this.sessionPathToScan.set(meta.fileName, rt.scanId);
    this.nextSessionId++;
  }

  private toProgress(rt: ScanRuntime): ScanProgressEvent {
    return {
      scanId: rt.scanId,
      status: rt.status,
      stage: rt.stage,
      stageProgress: rt.stageProgress,
      overallProgress: rt.overall,
      bytesScanned: rt.bytesScanned,
      bytesTotal: rt.bytesTotal,
      filesFound: rt.filesFound,
      dataFoundBytes: rt.dataFound,
      errors: rt.errors,
      badSectorMap: rt.badSectors,
      etaSeconds: rt.eta,
      throughputBytesPerSecond: rt.throughput,
      recoverableDuringScan:
        rt.stage === 'tree-build' || rt.stage === 'deep-walk'
          ? rt.overall > 0.15
          : rt.overall > 0.4,
      deviceConnected: !rt.disconnectFired,
      currentPath: rt.currentPath,
    };
  }

  private toScanState(rt: ScanRuntime): ScanState {
    return {
      scanId: rt.scanId,
      params: rt.params,
      target: rt.target,
      status: rt.status,
      startedAt: rt.startedAt,
      completedAt: rt.completedAt,
      progress: this.toProgress(rt),
    };
  }

  // ------------------------------------------------------------ results

  readonly results = {
    getResultTree: async (
      scanId: string,
      parentId: string | null,
      mode: ResultTreeMode,
    ): Promise<TreePage> => {
      await latency(60, 30);
      const rt = this.requireScan(scanId);
      let nodes: TreeNode[];
      if (mode === 'type') nodes = rt.space.typeTree(rt.scopeMask, parentId);
      else if (mode === 'raw') nodes = rt.space.rawTree(rt.scopeMask);
      else nodes = rt.space.pathTreeChildren(parentId, rt.scopeMask);
      const vf = this.visibleFraction(rt);
      if (vf < 0.999) {
        nodes = nodes.map((n) => ({
          ...n,
          fileCount: Math.floor(n.fileCount * vf),
          sizeBytes: Math.floor(n.sizeBytes * vf),
        }));
      }
      return { nodes, parentHasMore: false };
    },

    queryResults: async (
      scanId: string,
      filter: ResultsFilter,
      sort: { field: SortField; direction: SortDirection },
      page: { offset: number; limit: number },
    ): Promise<ResultsPage> => {
      await latency(90, 50);
      const rt = this.requireScan(scanId);
      // During a live scan only a fraction of the space is "found" yet.
      const visibleFraction = this.visibleFraction(rt);
      const result = rt.space.query(filter, sort, page.offset, page.limit, rt.scopeMask);
      // approximate live fraction by trimming the total (deterministic)
      const total = Math.floor(result.total * visibleFraction);
      const totalBytes = Math.floor(result.totalBytes * visibleFraction);
      const nodes =
        visibleFraction >= 0.999
          ? result.nodes
          : result.nodes.slice(0, Math.ceil(result.nodes.length * visibleFraction));
      return {
        nodes,
        total,
        totalBytes,
        offset: page.offset,
        pageSize: page.limit,
        elapsedMs: result.elapsedMs,
      };
    },

    getNodeDetail: async (scanId: string, nodeId: string): Promise<NodeDetail> => {
      await latency(50, 25);
      const rt = this.requireScan(scanId);
      const idx = Number(nodeId.replace(/^f/, ''));
      const node = rt.space.nodeAt(idx);
      const reasonMap: Record<string, string> = {
        excellent: 'Headers and extents fully readable',
        good: 'Slight fragmentation, all clusters readable',
        fair: 'Some clusters marked pending reallocation',
        poor: 'Overlaps an unreadable area',
        unknown: 'Not yet verified',
      };
      return {
        node,
        fullPhysicalPath: node.path ? `${node.path}\\${node.name}` : null,
        createdAt: node.modifiedAt,
        accessedAt: new Date(
          new Date(node.modifiedAt ?? Date.now()).getTime() + 86400_000,
        ).toISOString(),
        confidenceReason: reasonMap[node.confidence] ?? null,
        badSectorsOverlap: node.confidence === 'poor',
        clusterCount: Math.max(1, Math.ceil(node.sizeBytes / 4096)),
      };
    },

    countResults: async (scanId: string, filter: ResultsFilter): Promise<number> => {
      await latency(60, 30);
      const rt = this.requireScan(scanId);
      return Math.floor(rt.space.count(filter, rt.scopeMask) * this.visibleFraction(rt));
    },
  };

  // ------------------------------------------------------------ preview

  readonly preview = {
    requestPreview: async (scanId: string, nodeId: string): Promise<PreviewPayload> => {
      await latency(240, 160);
      const rt = this.requireScan(scanId);
      const idx = Number(nodeId.replace(/^f/, ''));
      const node = rt.space.nodeAt(idx);
      const rng = mulberry32((idx ^ 0x5eed) >>> 0);
      // The connector owns file knowledge: kind inference from the node's
      // own category/extension, mirroring the engine's per-type previewer
      // dispatch (artifact 10 §2).
      const kind = previewKindOf(node);

      if (kind === 'image') {
        if (node.extension === '.crypt')
          throw connError('preview-encrypted', 'encrypted', 'preview.reason.encrypted');
        if (idx % 97 === 0)
          throw connError('preview-damaged', 'damaged', 'preview.reason.damaged');
        const { dataUrl, width, height } = this.syntheticImage(rng, node.name);
        return { kind: 'image', dataUrl, width, height, bounded: false };
      }
      if (kind === 'hex') {
        if (node.sizeBytes > 64 * 1024 ** 3)
          throw connError('preview-too-large', 'too large', 'preview.reason.tooLarge');
        const cap = Math.min(node.sizeBytes, 64 * 1024);
        const bytes = new Uint8Array(cap);
        const brng = mulberry32(idx ^ 0xbeef);
        for (let i = 0; i < cap; i++) bytes[i] = Math.floor(brng() * 256);
        return { kind: 'hex', bytes, totalSize: node.sizeBytes, baseOffset: 0 };
      }
      if (kind === 'text') {
        const lines: string[] = [];
        const trng = mulberry32(idx ^ 0x7e7e);
        const words = [
          'recovered',
          'fragment',
          'volume',
          'sector',
          'index',
          'journal',
          'entry',
          'cluster',
          'stream',
          'buffer',
        ];
        for (let l = 0; l < 220; l++) {
          const parts: string[] = [];
          for (let w = 0; w < 10; w++) parts.push(words[Math.floor(trng() * words.length)]);
          lines.push(`${String(l).padStart(4, ' ')}  ${parts.join(' ')}`);
        }
        return {
          kind: 'text',
          text: lines.join('\n'),
          encoding: idx % 7 === 0 ? 'UTF-16LE' : 'UTF-8',
          totalSize: node.sizeBytes,
          truncated: node.sizeBytes > 16_000,
        };
      }
      if (kind === 'pdf') {
        const pages = 1 + (idx % 3);
        const pageImages = Array.from({ length: pages }, (_, p) =>
          this.syntheticPage(rng, p + 1, pages),
        );
        return { kind: 'pdf', pages: pageImages, pageCount: pages };
      }
      if (kind === 'av') {
        const dur = 30 + Math.floor(rng() * 240);
        const thumb = this.syntheticImage(rng, node.name);
        return {
          kind: 'av',
          metadata: {
            container: node.extension === '.mp4' ? 'ISO-BMFF' : 'Matroska',
            videoCodec: 'H.264',
            audioCodec: 'AAC-LC',
            width: String(1280 + (idx % 3) * 640),
            height: '720',
            fps: '30',
            durationSeconds: String(dur),
          },
          thumbnailDataUrl: thumb.dataUrl,
          durationSeconds: dur,
        };
      }
      // office
      const textRng = mulberry32(idx ^ 0x0ff1ce);
      const sentences = [
        'Quarterly summary of recovered assets.',
        'The content below was extracted from the document stream.',
        'Tables and images appear as placeholders in extracted text.',
        'Section headers and paragraphs are preserved in reading order.',
      ];
      const text = Array.from(
        { length: 24 },
        (_, i) => sentences[Math.floor(textRng() * sentences.length)] + ` (#${i + 1})`,
      ).join(' ');
      return {
        kind: 'office',
        metadata: {
          app: 'WordProcessor',
          author: 'recovered-stream',
          pages: String(2 + (idx % 9)),
          words: String(400 + (idx % 1500)),
        },
        extractedText: text,
        truncated: node.sizeBytes > 128_000,
      };
    },

    getThumbnail: async (
      scanId: string,
      nodeId: string,
    ): Promise<{ dataUrl: string } | null> => {
      await latency(120, 60);
      const rt = this.requireScan(scanId);
      const idx = Number(nodeId.replace(/^f/, ''));
      const node = rt.space.nodeAt(idx);
      if (!node.thumbnailAvailable) return null;
      const rng = mulberry32((idx ^ 0x7777) >>> 0);
      return { dataUrl: this.syntheticImage(rng, node.name).dataUrl };
    },
  };

  private syntheticImage(
    rng: () => number,
    name: string,
  ): { dataUrl: string; width: number; height: number } {
    const width = 640;
    const height = 400;
    const hue = Math.floor(rng() * 360);
    const hue2 = (hue + 40 + Math.floor(rng() * 120)) % 360;
    const shapes: string[] = [];
    for (let i = 0; i < 7; i++) {
      const x = Math.floor(rng() * width);
      const y = Math.floor(rng() * height);
      const r = 20 + Math.floor(rng() * 120);
      const c = `hsl(${Math.floor(rng() * 360)} 70% ${45 + Math.floor(rng() * 25)}%)`;
      shapes.push(`<circle cx="${x}" cy="${y}" r="${r}" fill="${c}" opacity="0.55"/>`);
    }
    const label = name.replace(/[<>&"']/g, '');
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
<stop offset="0" stop-color="hsl(${hue} 80% 62%)"/><stop offset="1" stop-color="hsl(${hue2} 75% 38%)"/>
</linearGradient></defs>
<rect width="${width}" height="${height}" fill="url(#g)"/>
${shapes.join('')}
<rect x="16" y="${height - 56}" width="${width - 32}" height="40" rx="8" fill="rgba(0,0,0,0.45)"/>
<text x="28" y="${height - 30}" font-family="Segoe UI, sans-serif" font-size="18" fill="#fff">${label.slice(0, 42)}</text>
</svg>`;
    return {
      dataUrl: `data:image/svg+xml;base64,${btoa(unescape(encodeURIComponent(svg)))}`,
      width,
      height,
    };
  }

  private syntheticPage(
    rng: () => number,
    page: number,
    total: number,
  ): { dataUrl: string; width: number; height: number } {
    const width = 595;
    const height = 842;
    const lines = Array.from({ length: 18 }, (_, i) => {
      const w = 120 + Math.floor(rng() * 380);
      return `<rect x="70" y="${120 + i * 28}" width="${w}" height="12" rx="4" fill="rgba(30,40,70,0.35)"/>`;
    });
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
<rect width="${width}" height="${height}" fill="#ffffff"/>
<rect x="0" y="0" width="${width}" height="8" fill="#0560fd"/>
<text x="70" y="80" font-family="Segoe UI, sans-serif" font-size="26" fill="#1a1a1a">Page ${page} of ${total}</text>
${lines.join('')}
</svg>`;
    return {
      dataUrl: `data:image/svg+xml;base64,${btoa(unescape(encodeURIComponent(svg)))}`,
      width,
      height,
    };
  }

  // ------------------------------------------------------------ sessions

  readonly sessions = {
    saveSession: async (scanId: string, path: string | null): Promise<SessionMeta> => {
      await latency(320, 120);
      const rt = this.requireScan(scanId);
      const fileName = path ?? `session_${new Date().toISOString().replace(/[:.]/g, '-')}.rsf`;
      const meta: SessionMeta = {
        id: `sess-${this.nextSessionId++}`,
        fileName,
        savedAt: new Date().toISOString(),
        targetLabel: rt.target.label,
        filesFound: rt.filesFound,
        dataFoundBytes: rt.dataFound,
        completed: rt.status === 'completed',
      };
      this.sessionStore.set(meta.id, meta);
      this.sessionPathToScan.set(fileName, scanId);
      return meta;
    },

    listSessions: async (): Promise<SessionMeta[]> => {
      await latency(120, 60);
      return Array.from(this.sessionStore.values()).sort((a, b) =>
        b.savedAt.localeCompare(a.savedAt),
      );
    },

    loadSession: async (path: string): Promise<{ scanId: string }> => {
      await latency(420, 180);
      if (this.scenario === 'corrupt-session' && path.includes('auto_')) {
        throw connError('session-corrupt', 'corrupt session', 'errors.sessionCorrupt');
      }
      const scanId = this.sessionPathToScan.get(path);
      if (!scanId)
        throw connError('session-not-found', 'session not found', 'errors.sessionNotFound');
      return { scanId };
    },

    getSessionMeta: async (path: string): Promise<SessionMeta> => {
      await latency(100, 50);
      const scanId = this.sessionPathToScan.get(path);
      if (!scanId)
        throw connError('session-not-found', 'session not found', 'errors.sessionNotFound');
      const rt = this.requireScan(scanId);
      return {
        id: `sess-${path.length}`,
        fileName: path,
        savedAt: rt.completedAt ?? rt.startedAt,
        targetLabel: rt.target.label,
        filesFound: rt.filesFound,
        dataFoundBytes: rt.dataFound,
        completed: rt.status === 'completed',
      };
    },
  };

  // ------------------------------------------------------------ recovery

  readonly recovery = {
    startRecovery: async (
      scanId: string,
      selection: RecoverySelection,
      destination: string,
      options: RecoveryOptions,
    ): Promise<{ jobId: string }> => {
      await latency(300, 100);
      const rt = this.requireScan(scanId);
      const space = rt.space;
      const items: RecoveryItem[] =
        selection.kind === 'nodes'
          ? selection.nodeIds.map((id) => {
              const idx = Number(id.replace(/^f/, ''));
              const node = space.nodeAt(idx);
              return {
                nodeId: id,
                name: node.name,
                sizeBytes: node.sizeBytes,
                state: 'pending',
              };
            })
          : [];
      const allMode =
        selection.kind === 'all'
          ? {
              filter: selection.filter,
              space,
              scopeMask: rt.scopeMask,
              cursor: 0,
              total: space.count(selection.filter, rt.scopeMask),
            }
          : null;
      const totalCount = items.length + (allMode?.total ?? 0);
      const jobId = `job-${this.nextJobId++}`;
      // Items advanced per tick — scales with job size so even a
      // recover-all over the whole result set finishes in bounded wall time
      // (the real engine streams at disk speed; the simulator batches).
      const targetTicks =
        import.meta.env?.MODE === 'test' && selection.kind === 'all' ? 25 : 200;
      const batch = Math.max(1, Math.ceil(Math.max(1, totalCount) / targetTicks));
      const job: JobRuntime = {
        jobId,
        scanId,
        destination,
        options,
        items,
        allMode,
        status: 'running',
        progress: {
          jobId,
          status: 'running',
          overallProgress: 0,
          currentFile: null,
          currentFileProgress: 0,
          completedCount: 0,
          failedCount: 0,
          skippedCount: 0,
          totalCount: Math.max(1, totalCount),
          throughputBytesPerSecond: 0,
          verifying: false,
          etaSeconds: null,
        },
        timer: null,
        startedAt: new Date().toISOString(),
        finishedAt: null,
        bytesWritten: 0,
        verified: options.verifyOnWrite,
        batch,
        advanceCursor: 0,
      };
      this.jobs.set(jobId, job);
      job.timer = setInterval(() => this.jobTick(job), 120);
      return { jobId };
    },

    cancelRecovery: async (jobId: string): Promise<void> => {
      const job = this.requireJob(jobId);
      if (job.timer) clearInterval(job.timer);
      job.timer = null;
      job.status = 'cancelled';
      job.finishedAt = new Date().toISOString();
      await latency(200);
    },

    getRecoveryState: async (jobId: string): Promise<RecoveryJob> => {
      await latency(30);
      const job = this.requireJob(jobId);
      return {
        jobId: job.jobId,
        scanId: job.scanId,
        nodeIds: job.allMode ? [] : job.items.map((i) => i.nodeId),
        destination: job.destination,
        options: job.options,
        status: job.status,
      };
    },

    getRecoveryItems: async (
      jobId: string,
      page: { offset: number; limit: number },
    ): Promise<readonly RecoveryItem[]> => {
      await latency(30);
      const job = this.requireJob(jobId);
      return job.items.slice(page.offset, page.offset + page.limit);
    },

    onRecoveryProgress: (
      jobId: string,
      cb: (event: RecoveryProgressEvent) => void,
    ): (() => void) => {
      const handler = (): void => {
        const job = this.jobs.get(jobId);
        if (job) cb({ ...job.progress });
      };
      const interval = setInterval(handler, 150);
      return () => clearInterval(interval);
    },

    getRecoveryReport: async (jobId: string): Promise<RecoveryReport> => {
      await latency(40);
      const job = this.requireJob(jobId);
      const cap = 500;
      return {
        jobId: job.jobId,
        status: job.status,
        startedAt: job.startedAt,
        finishedAt: job.finishedAt,
        succeeded: job.items.filter((i) => i.state === 'done').slice(0, cap),
        failed: job.items.filter((i) => i.state === 'failed').slice(0, cap),
        skipped: job.items.filter((i) => i.state === 'skipped').slice(0, cap),
        totalBytesWritten: job.bytesWritten,
        verified: job.verified,
        processedTotal:
          job.progress.completedCount + job.progress.failedCount + job.progress.skippedCount,
        truncatedItemLists: job.allMode != null && job.items.length > cap,
      };
    },
  };

  private requireJob(jobId: string): JobRuntime {
    const job = this.jobs.get(jobId);
    if (!job) throw connError('unknown', `job ${jobId} not found`);
    return job;
  }

  private jobTick(job: JobRuntime): void {
    // Recover-all: materialize the next chunk of matching nodes on demand
    // (bounded working set — the export loop streams, it does not preload).
    if (job.allMode) {
      // Lookahead window: keep a bounded working set of materialized items
      // (the export loop streams; it never preloads the whole set).
      const windowSize = Math.max(2048, job.batch * 2);
      const inFlight = job.items.length - job.advanceCursor;
      if (inFlight < windowSize) {
        const { space, filter, scopeMask } = job.allMode;
        const window = space.queryNodes(filter, scopeMask, job.allMode.cursor, windowSize);
        if (window.length > 0) {
          job.allMode.cursor += window.length;
          for (const idx of window) {
            const node = space.nodeAt(idx);
            job.items.push({
              nodeId: `f${idx}`,
              name: node.name,
              sizeBytes: node.sizeBytes,
              state: 'pending',
            });
          }
        }
      }
    }

    let pendingIdx = -1;
    for (let k = job.advanceCursor; k < job.items.length; k++) {
      if (job.items[k].state === 'pending' || job.items[k].state === 'running') {
        pendingIdx = k;
        break;
      }
    }
    if (pendingIdx === -1) {
      const failed = job.progress.failedCount;
      const done = job.progress.completedCount;
      if (
        job.options.verifyOnWrite &&
        !job.progress.verifying &&
        done > 0 &&
        job.progress.completedCount === done
      ) {
        job.progress.verifying = true;
        return;
      }
      if (job.progress.verifying) {
        // verification pass ends the job
        job.status = failed > 0 ? 'completed-partial' : 'completed';
        job.finishedAt = new Date().toISOString();
        if (job.timer) clearInterval(job.timer);
        job.timer = null;
        job.progress.status = job.status;
        job.progress.verifying = false;
        return;
      }
      job.status = failed > 0 ? 'completed-partial' : 'completed';
      job.finishedAt = new Date().toISOString();
      if (job.timer) clearInterval(job.timer);
      job.timer = null;
      job.progress.status = job.status;
      return;
    }

    // Advance a scale-aware batch per tick so any job (including a
    // recover-all over the entire result set) completes in bounded wall
    // time — the real engine streams at disk speed; the simulator batches.
    for (let step = 0; step < job.batch; step++) {
      while (
        job.advanceCursor < job.items.length &&
        job.items[job.advanceCursor].state !== 'pending' &&
        job.items[job.advanceCursor].state !== 'running'
      ) {
        job.advanceCursor++;
      }
      if (job.advanceCursor >= job.items.length) break;
      const pending = job.advanceCursor;
      const item = job.items[pending];
      if (item.state === 'pending') {
        item.state = 'running';
        job.progress.currentFile = item.name;
        job.progress.currentFileProgress = 0;
        if (job.batch > 1) {
          // Batched (at-scale) jobs settle one item per step — the per-file
          // progress animation only exists for small explicit selections.
          job.progress.currentFileProgress = 1;
        } else {
          continue;
        }
      }
      job.progress.currentFileProgress = Math.min(1, job.progress.currentFileProgress + 0.25);
      if (job.progress.currentFileProgress >= 1) {
        const roll = mulberry32(item.nodeId.length + pending)();
        if (roll < 0.04) {
          item.state = 'failed';
          item.error = {
            code: 'recovery-item-failed',
            reasonKey: 'recovery.itemFailedReason',
          };
          job.progress.failedCount++;
        } else if (job.options.collision === 'skip' && roll > 0.92) {
          item.state = 'skipped';
          job.progress.skippedCount++;
        } else {
          item.state = 'done';
          item.savedAs =
            job.options.collision === 'rename' && roll > 0.88
              ? item.name.replace(/(\.[^.]+)$/, ' (1)$1')
              : item.name;
          job.progress.completedCount++;
          job.bytesWritten += item.sizeBytes;
        }
        job.progress.currentFile = null;
        job.progress.currentFileProgress = 0;
        job.advanceCursor = pending + 1;
      }
    }
    job.progress.overallProgress =
      (job.progress.completedCount +
        job.progress.failedCount +
        job.progress.skippedCount +
        job.progress.currentFileProgress) /
      Math.max(1, job.progress.totalCount);
    job.progress.throughputBytesPerSecond = randBell(mulberry32(pendingIdx + 1), 48e6, 30e6);
    // ETA from the job's own batch cadence (tick interval 120 ms) — not a
    // per-item constant, which reads absurd at recover-all scale.
    const remainingTicks = Math.ceil(
      (job.progress.totalCount -
        job.progress.completedCount -
        job.progress.failedCount -
        job.progress.skippedCount) /
        Math.max(1, job.batch),
    );
    job.progress.etaSeconds = Math.max(1, Math.round(remainingTicks * 0.12));
  }

  // ------------------------------------------------------------ health

  readonly health = {
    getHealth: async (deviceId: string): Promise<DiskHealth> => {
      await latency(280, 120);
      const device = this.deviceList.find((d) => d.id === deviceId);
      if (!device) throw connError('device-gone', 'device not found', 'errors.deviceGone');
      const rng = mulberry32(this.seed ^ device.serial.length ^ 0xea17);
      const isSsd =
        device.model.toLowerCase().includes('ssd') ||
        device.model.toLowerCase().includes('evo') ||
        device.kind === 'usb';
      const caution = device.id === 'dev-data' || this.scenario === 'bad-sectors';
      const failing = this.scenario === 'failing-disk';
      const temp = Math.round(randBell(rng, 38, 10));
      const hours = Math.round(randBell(rng, 6400, 5200));
      const reallocated = failing
        ? randInt(rng, 1200, 6400)
        : caution
          ? randInt(rng, 12, 480)
          : randInt(rng, 0, 3);
      const pending = failing ? randInt(rng, 800, 3200) : caution ? randInt(rng, 8, 260) : 0;

      const mkAttr = (
        id: number,
        name: string,
        current: number | null,
        worst: number | null,
        threshold: number | null,
        raw: number,
        status: SmartAttribute['status'],
      ): SmartAttribute => ({ id, name, current, worst, threshold, raw, status });

      const attributes: SmartAttribute[] = [
        mkAttr(
          5,
          'Reallocated Sectors Count',
          100 - Math.min(64, Math.floor(reallocated / 8)),
          100 - Math.min(80, Math.floor(reallocated / 4)),
          36,
          reallocated,
          reallocated > 36 ? 'failing' : 'ok',
        ),
        mkAttr(9, 'Power-On Hours', 98, 98, 0, hours, 'ok'),
        mkAttr(
          10,
          'Spin Retry Count',
          isSsd ? null : 100,
          isSsd ? null : 100,
          isSsd ? null : 97,
          isSsd ? 0 : randInt(rng, 0, 2),
          'ok',
        ),
        mkAttr(12, 'Power Cycle Count', 99, 99, 0, Math.round(hours / 42), 'ok'),
        ...(isSsd
          ? [
              mkAttr(
                177,
                'Wear Leveling Count',
                Math.max(1, 100 - Math.round(randBell(rng, 22, 20))),
                100,
                5,
                0,
                'ok',
              ),
              mkAttr(178, 'Used Reserved Blocks', 100, 100, 10, randInt(rng, 0, 14), 'ok'),
            ]
          : []),
        mkAttr(194, 'Temperature', temp, temp, 60, temp, temp > 55 ? 'failing' : 'ok'),
        ...(isSsd
          ? []
          : [
              mkAttr(
                196,
                'Reallocation Event Count',
                100,
                100,
                36,
                reallocated,
                reallocated > 36 ? 'failing' : 'ok',
              ),
            ]),
        mkAttr(
          197,
          'Current Pending Sectors',
          100,
          100,
          36,
          pending,
          pending > 36 ? 'failing' : 'ok',
        ),
        mkAttr(
          198,
          'Uncorrectable Sectors',
          100,
          100,
          36,
          pending > 12 ? randInt(rng, 2, pending) : 0,
          pending > 36 ? 'failing' : 'ok',
        ),
        ...(isSsd
          ? [
              mkAttr(
                231,
                'SSD Life Left',
                Math.max(3, 100 - Math.round(randBell(rng, 28, 24))),
                100,
                10,
                0,
                'ok',
              ),
            ]
          : []),
      ];

      const status: DiskHealth['status'] = failing ? 'bad' : caution ? 'caution' : 'good';
      return {
        deviceId,
        status,
        temperatureC: temp,
        powerOnHours: hours,
        diskClass: isSsd ? 'ssd' : 'hdd',
        wearPercent: isSsd
          ? Math.min(99, Math.max(2, Math.round(randBell(rng, 24, 20))))
          : null,
        trimEnabled: isSsd ? rng() > 0.15 : null,
        reallocatedSectors: reallocated,
        pendingSectors: pending,
        attributes,
      };
    },
  };

  // ------------------------------------------------------------ repair

  readonly repair = {
    dryRunRepair: async (deviceId: string): Promise<RepairDryRun> => {
      await latency(600, 200);
      const device = this.deviceList.find((d) => d.id === deviceId);
      if (!device) throw connError('device-gone', 'device not found', 'errors.deviceGone');
      if (this.scenario === 'bad-sectors' || device.id === 'dev-data') {
        return {
          deviceId,
          findings: [
            {
              id: 'fnd-1',
              kind: 'boot-sector-mismatch',
              where: 'LBA 0 (boot sector)',
              descriptionKey: 'repair.finding.bootSector',
              fixable: true,
              severity: 'warning',
            },
            {
              id: 'fnd-2',
              kind: 'backup-dbr-stale',
              where: 'LBA 32 (backup)',
              descriptionKey: 'repair.finding.backupDbr',
              fixable: true,
              severity: 'info',
            },
            {
              id: 'fnd-3',
              kind: 'fat-table-damaged',
              where: 'FAT region 1',
              descriptionKey: 'repair.finding.fatTable',
              fixable: true,
              severity: 'critical',
            },
          ],
          canApply: true,
        };
      }
      return { deviceId, findings: [], canApply: false };
    },

    applyRepair: async (
      deviceId: string,
      options: { consentToken: string },
    ): Promise<{ applied: number }> => {
      if (options.consentToken !== 'REPAIR') {
        throw connError(
          'repair-consent-required',
          'consent token missing',
          'repair.consentRequired',
        );
      }
      await latency(900, 300);
      const existing = this.repairUndoLogs.get(deviceId) ?? [];
      const entries: RepairUndoEntry[] = [
        {
          index: existing.length + 1,
          action: 'Restore boot sector from backup',
          where: 'LBA 0',
          at: new Date().toISOString(),
          snapshotBytes: 512,
        },
        {
          index: existing.length + 2,
          action: 'Rebuild FAT chain from directory walk',
          where: 'FAT region 1',
          at: new Date().toISOString(),
          snapshotBytes: 131_072,
        },
        {
          index: existing.length + 3,
          action: 'Refresh DBR checksum',
          where: 'LBA 32',
          at: new Date().toISOString(),
          snapshotBytes: 512,
        },
      ];
      this.repairUndoLogs.set(deviceId, [...existing, ...entries]);
      this.repairApplied.set(
        deviceId,
        (this.repairApplied.get(deviceId) ?? 0) + entries.length,
      );
      return { applied: entries.length };
    },

    getUndoLog: async (deviceId: string): Promise<readonly RepairUndoEntry[]> => {
      await latency(150);
      return this.repairUndoLogs.get(deviceId) ?? [];
    },

    undoRepairs: async (deviceId: string): Promise<{ rolledBack: number }> => {
      await latency(700, 250);
      const n = (this.repairUndoLogs.get(deviceId) ?? []).length;
      this.repairUndoLogs.set(deviceId, []);
      return { rolledBack: n };
    },
  };

  // ------------------------------------------------------------ system

  readonly system: SystemDomain = {
    getDriverStatus: async (): Promise<DriverStatus> => {
      await latency(60);
      return {
        installed: true,
        version: 'mock-1.0.0',
        running: true,
      };
    },

    getAppInfo: async (): Promise<AppInfo> => {
      await latency(40);
      return {
        name: 'Recovery Wizard',
        version: appVersion,
        connectorMode: 'mock',
        engineVersion: null,
        platform: typeof navigator !== 'undefined' ? navigator.platform : 'unknown',
      };
    },

    validateDestination: async (
      path: string,
      sourceDeviceId: string,
      neededBytes: number,
    ): Promise<DestinationValidation> => {
      await latency(180, 80);
      // Accept a device OR volume id (scan targets are either); resolve to
      // the owning device for the same-volume comparison.
      const source = this.deviceList.find(
        (d) => d.id === sourceDeviceId || d.volumes.some((v) => v.id === sourceDeviceId),
      );
      const srcLetters = new Set(
        (source?.volumes ?? []).map((v) => v.letter?.toUpperCase()).filter(Boolean) as string[],
      );
      const destLetter = /^[A-Za-z]:/.exec(path)?.[0]?.[0]?.toUpperCase();
      if (destLetter && srcLetters.has(destLetter)) {
        return {
          allowed: false,
          reason: 'overlap',
          reasonKey: 'recovery.overlapReason',
          freeBytes: null,
          neededBytes,
        };
      }
      const mount = this.mountPoints().find((m) =>
        path.toUpperCase().startsWith(m.path[0]?.toUpperCase() ?? ''),
      );
      if (!mount) {
        return {
          allowed: false,
          reason: 'invalid',
          reasonKey: 'errors.unknown',
          freeBytes: null,
          neededBytes,
        };
      }
      if (this.scenario === 'out-of-space' && neededBytes > mount.freeBytes) {
        return {
          allowed: false,
          reason: 'out-of-space',
          reasonKey: 'recovery.insufficientSpaceBody',
          freeBytes: mount.freeBytes,
          neededBytes,
        };
      }
      return { allowed: true, freeBytes: mount.freeBytes, neededBytes };
    },

    pickDirectory: async (hint?: {
      volumeId?: string | null;
      excludeDeviceId?: string | null;
    }): Promise<string | null> => {
      await latency(150);
      // Scan-scope picking: root the suggestion at the hinted volume so the
      // chosen folder resolves on the scanned device.
      if (hint?.volumeId) {
        const hinted = this.mountPoints().find((m) => m.id === hint.volumeId);
        const folder = hinted ? this.nodeSpaces.get(hinted.id)?.suggestedScopePath() : null;
        const mount = hinted ?? this.mountPoints()[0];
        if (mount && folder) return `${mount.path}${folder}`;
      }
      // Destination picking: never suggest a folder on the excluded (source)
      // device — writing recoveries back to the source is a hard refusal.
      const all = this.mountPoints();
      let pool = all;
      if (hint?.excludeDeviceId) {
        const excludedVolumes = new Set(
          this.deviceList
            .filter(
              (d) =>
                d.id === hint.excludeDeviceId ||
                d.volumes.some((v) => v.id === hint.excludeDeviceId),
            )
            .flatMap((d) => d.volumes.map((v) => v.id)),
        );
        const others = all.filter((m) => !excludedVolumes.has(m.id));
        if (others.length > 0) pool = others;
      }
      const suggestion = pool[this.pickIndex % pool.length] ?? pool[0];
      this.pickIndex++;
      return suggestion ? `${suggestion.path}Recovered Files` : null;
    },

    listMountPoints: async (): Promise<MountPoint[]> => {
      await latency(100, 40);
      return this.mountPoints();
    },

    openPath: async (path: string): Promise<void> => {
      await latency(30);
      void path;
      // Shell open is a real-adapter capability (the Tauri connector
      // implements it via the OS opener); the mock has no OS access and
      // reports so honestly instead of pretending.
      throw connError(
        'not-implemented',
        'opening paths requires the desktop shell',
        'errors.openPathMock',
      );
    },
  };

  private mountPoints(): MountPoint[] {
    return this.deviceList
      .flatMap((d) => d.volumes)
      .filter((v) => v.letter)
      .map((v) => ({
        id: v.id,
        label: v.label,
        path: `${v.letter}:\\`,
        freeBytes: v.capacity.freeBytes,
        totalBytes: v.capacity.totalBytes,
      }));
  }

  // ------------------------------------------------------------ helpers exposed for tests
}

function latency(meanMs: number, jitterMs = 0): Promise<void> {
  const ms = meanMs + (Math.random() - 0.5) * 2 * jitterMs;
  return new Promise((resolve) => setTimeout(resolve, Math.max(20, ms)));
}
