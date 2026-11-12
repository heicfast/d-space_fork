import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * Real-adapter wiring tests — the bridge contract between the React UI
 * and the Rust engine (src-tauri/src/recovery/commands.rs).
 *
 * `invoke` and `listen` are mocked (no Tauri runtime); every assertion
 * pins the exact command name, argument casing and result mapping so a
 * rename on either side fails here instead of at runtime.
 */

const invoke = vi.fn();
const listen = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listen(...args),
}));

import { createTauriConnector } from '@/connector/tauri/tauri-connector';

function tauriRuntime(): void {
  Object.defineProperty(window, '__TAURI_INTERNALS__', {
    value: {},
    configurable: true,
  });
}

function browserRuntime(): void {
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
}

beforeEach(() => {
  invoke.mockReset();
  listen.mockReset();
  listen.mockResolvedValue(() => undefined);
  tauriRuntime();
});

afterEach(() => {
  browserRuntime();
});

const connector = createTauriConnector();

describe('devices domain wiring', () => {
  it('listDevices invokes dw_devices_list and maps nested capacity', async () => {
    invoke.mockResolvedValueOnce([
      {
        id: 'dev:C',
        kind: 'internal',
        model: 'Samsung SSD',
        label: 'Windows (C:)',
        serial: null,
        totalBytes: 500,
        usedBytes: 300,
        freeBytes: 200,
        status: 'ok',
        volumes: [
          {
            id: 'vol:C',
            deviceId: 'dev:C',
            label: 'Windows (C:)',
            letter: 'C',
            fs: 'ntfs',
            totalBytes: 500,
            usedBytes: 300,
            freeBytes: 200,
            status: 'ok',
            quickScanAvailable: true,
            canRecoverDeleted: true,
            bitlocker: 'unlocked',
          },
        ],
        canScanWholeDisk: true,
        isSourceOfActiveScan: false,
      },
    ]);
    const devices = await connector.devices.listDevices();
    expect(invoke).toHaveBeenCalledWith('dw_devices_list', undefined);
    expect(devices).toHaveLength(1);
    expect(devices[0].capacity).toEqual({ totalBytes: 500, usedBytes: 300, freeBytes: 200 });
    expect(devices[0].serial).toBe('');
    expect(devices[0].volumes[0].bitlocker).toBe('unlocked');
    expect(devices[0].volumes[0].capacity.freeBytes).toBe(200);
  });

  it('getDeviceInfo resolves by volume id and maps device-gone errors', async () => {
    invoke.mockResolvedValueOnce([]);
    await expect(connector.devices.getDeviceInfo('vol:X')).rejects.toMatchObject({
      code: 'device-gone',
    });
  });
});

describe('scan domain wiring', () => {
  it('startScan passes camelCase args and returns the scan id', async () => {
    invoke.mockResolvedValueOnce({ scanId: 's1-0' });
    const reply = await connector.scan.startScan({ targetId: 'vol:C', pathScope: 'Users' });
    expect(invoke).toHaveBeenCalledWith('dw_scan_start', {
      targetId: 'vol:C',
      pathScope: 'Users',
    });
    expect(reply.scanId).toBe('s1-0');
  });

  it('pause/resume/cancel target the right commands', async () => {
    invoke.mockResolvedValue(undefined);
    await connector.scan.pauseScan('s1');
    await connector.scan.resumeScan('s1');
    await connector.scan.cancelScan('s1');
    expect(invoke).toHaveBeenNthCalledWith(1, 'dw_scan_pause', { scanId: 's1' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'dw_scan_resume', { scanId: 's1' });
    expect(invoke).toHaveBeenNthCalledWith(3, 'dw_scan_cancel', { scanId: 's1' });
  });

  it('getScanState maps status, dates and the failure object', async () => {
    invoke.mockResolvedValueOnce({
      scanId: 's1',
      targetId: 'vol:C',
      targetLabel: 'Windows (C:)',
      status: 'failed',
      startedAtUnix: 1760000000,
      completedAtUnix: null,
      progress: {
        scanId: 's1',
        status: 'failed',
        stage: 'deep-walk',
        stageProgress: 0.5,
        overallProgress: 0.4,
        bytesScanned: 10,
        bytesTotal: 100,
        filesFound: 5,
        dataFoundBytes: 500,
        deletedFound: 2,
        errors: 0,
        badSectorMap: [],
        etaSeconds: null,
        throughputBytesPerSecond: 0,
        recoverableDuringScan: false,
        deviceConnected: true,
        currentPath: null,
      },
      failure: 'boom',
    });
    const state = await connector.scan.getScanState('s1');
    expect(invoke).toHaveBeenCalledWith('dw_scan_state', { scanId: 's1' });
    expect(state.status).toBe('failed');
    expect(state.startedAt).toBe(new Date(1760000000 * 1000).toISOString());
    expect(state.progress.deletedFound).toBe(2);
    // The failure string now maps through the shared error-code table
    // (an unmapped string still yields code 'unknown' but keeps the
    // message on the error object).
    expect(state.failure?.code).toBe('unknown');
    expect((state.failure as Error).message).toBe('boom');
  });

  it('onScanProgress subscribes to dw-scan-progress and filters by scan id', () => {
    const seen: number[] = [];
    const unsub = connector.scan.onScanProgress('s9', (e) => seen.push(e.overallProgress));
    expect(listen).toHaveBeenCalledWith('dw-scan-progress', expect.any(Function));
    const handler = listen.mock.calls[0][1] as (event: { payload: unknown }) => void;
    const payload = (scanId: string, overall: number) => ({
      scanId,
      status: 'running',
      stage: 'quick-index',
      stageProgress: overall,
      overallProgress: overall,
      bytesScanned: 0,
      bytesTotal: 0,
      filesFound: 0,
      dataFoundBytes: 0,
      deletedFound: 0,
      errors: 0,
      badSectorMap: [],
      etaSeconds: null,
      throughputBytesPerSecond: 0,
      recoverableDuringScan: false,
      deviceConnected: true,
      currentPath: null,
    });
    handler({ payload: payload('other', 0.1) });
    handler({ payload: payload('s9', 0.5) });
    expect(seen).toEqual([0.5]);
    unsub();
  });
});

describe('results domain wiring', () => {
  it('queryResults sends the camelCase filter and maps nodes', async () => {
    invoke.mockResolvedValueOnce({
      nodes: [
        {
          id: '42',
          scanId: 's1',
          kind: 'file',
          name: 'photo.jpg',
          path: 'C:\\Users\\photo.jpg',
          sizeBytes: 1234,
          modifiedUnix: 1760000000,
          extension: 'jpg',
          category: 'graphics',
          confidence: 'excellent',
          origin: 'existing',
          childCount: null,
          thumbnailAvailable: true,
          parentId: '5',
          attributes: { cleanupScore: '87' },
        },
      ],
      total: 1,
      totalBytes: 1234,
      offset: 0,
      pageSize: 200,
      elapsedMs: 3,
    });
    const page = await connector.results.queryResults(
      's1',
      {
        nodeId: null,
        treeMode: 'path',
        search: 'photo',
        categories: null,
        dateFrom: '2026-01-01T00:00:00.000Z',
        dateTo: null,
        sizeMinBytes: 10,
        sizeMaxBytes: null,
        extensions: null,
        origins: null,
        hideEmptyFiles: true,
      },
      { field: 'size', direction: 'desc' },
      { offset: 0, limit: 200 },
    );
    const arg = invoke.mock.calls[0][1] as Record<string, unknown>;
    expect(invoke.mock.calls[0][0]).toBe('dw_results_query');
    expect(arg.scanId).toBe('s1');
    expect(arg.sortField).toBe('size');
    expect(arg.offset).toBe(0);
    expect(arg.limit).toBe(200);
    const filter = arg.filter as Record<string, unknown>;
    expect(filter.search).toBe('photo');
    expect(filter.dateFrom).toBe(Math.floor(Date.parse('2026-01-01T00:00:00.000Z') / 1000));
    expect(page.nodes[0].extension).toBe('.jpg');
    expect(page.nodes[0].category).toBe('graphics');
    expect(page.nodes[0].attributes?.cleanupScore).toBe('87');
    expect(page.nodes[0].modifiedAt).toBe(new Date(1760000000 * 1000).toISOString());
  });

  it('getResultTree maps tree nodes and special ids', async () => {
    invoke.mockResolvedValueOnce({
      nodes: [
        {
          id: 'lost-paths',
          parentId: 'root',
          label: 'Lost Path',
          icon: 'group',
          childCount: 3,
          fileCount: 3,
          sizeBytes: 900,
          origin: 'lost-path',
          special: 'lost-path',
        },
      ],
      parentHasMore: false,
    });
    const tree = await connector.results.getResultTree('s1', null, 'path');
    expect(invoke).toHaveBeenCalledWith('dw_results_tree', {
      scanId: 's1',
      parentId: null,
      mode: 'path',
    });
    expect(tree.nodes[0].special).toBe('lost-path');
  });

  it('countResults normalizes float counts', async () => {
    invoke.mockResolvedValueOnce(41.0);
    const n = await connector.results.countResults('s1', {
      nodeId: null,
      treeMode: 'path',
      search: null,
      categories: null,
      dateFrom: null,
      dateTo: null,
      sizeMinBytes: null,
      sizeMaxBytes: null,
      extensions: null,
      origins: null,
      hideEmptyFiles: false,
    });
    expect(n).toBe(41);
  });
});

describe('preview domain wiring', () => {
  it('image previews become data URLs with parsed dimensions', async () => {
    // 1x1 PNG — IHDR says width=1 height=1.
    const pngBase64 =
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';
    invoke.mockResolvedValueOnce({
      kind: 'image',
      dataBase64: pngBase64,
      mime: 'image/png',
      text: null,
      encoding: null,
      truncated: false,
      totalSize: 70,
      reasonKey: null,
    });
    const payload = await connector.preview.requestPreview('s1', '9');
    expect(invoke).toHaveBeenCalledWith('dw_preview', { scanId: 's1', nodeId: '9' });
    if (payload.kind !== 'image') throw new Error('expected image');
    expect(payload.dataUrl.startsWith('data:image/png;base64,')).toBe(true);
    expect(payload.width).toBe(1);
    expect(payload.height).toBe(1);
  });

  it('hex previews decode base64 into bytes', async () => {
    invoke.mockResolvedValueOnce({
      kind: 'hex',
      dataBase64: 'AQID',
      mime: null,
      text: null,
      encoding: null,
      truncated: false,
      totalSize: 3,
      reasonKey: null,
    });
    const payload = await connector.preview.requestPreview('s1', '9');
    if (payload.kind !== 'hex') throw new Error('expected hex');
    expect([...payload.bytes]).toEqual([1, 2, 3]);
  });

  it('thumbnails pass through data URLs', async () => {
    invoke.mockResolvedValueOnce({ dataUrl: 'data:image/png;base64,eA==' });
    const thumb = await connector.preview.getThumbnail('s1', '9');
    expect(thumb?.dataUrl).toBe('data:image/png;base64,eA==');
  });
});

describe('sessions domain wiring', () => {
  it('loadSession maps file names to fresh scan ids', async () => {
    invoke.mockResolvedValueOnce({ scanId: 's2-0' });
    const reply = await connector.sessions.loadSession('auto_s1.dws');
    expect(invoke).toHaveBeenCalledWith('dw_session_load', { fileName: 'auto_s1.dws' });
    expect(reply.scanId).toBe('s2-0');
  });

  it('session-not-found maps for unknown files', async () => {
    invoke.mockResolvedValueOnce([
      {
        id: 's1',
        fileName: 'a.dws',
        savedAtUnix: 1,
        targetLabel: 'x',
        filesFound: 1,
        dataFoundBytes: 1,
        deletedFound: 0,
        completed: true,
      },
    ]);
    await expect(connector.sessions.getSessionMeta('missing.dws')).rejects.toMatchObject({
      code: 'session-not-found',
    });
  });
});

describe('recovery domain wiring', () => {
  it('startRecovery wraps the selection and options', async () => {
    invoke.mockResolvedValueOnce({ jobId: 'j1-0' });
    const reply = await connector.recovery.startRecovery(
      's1',
      { kind: 'nodes', nodeIds: ['1', '2'] },
      'D:\\Recovered',
      {
        collision: 'rename',
        rebuildFolders: true,
        keepEmptyFolders: false,
        fileRepair: false,
        verifyOnWrite: true,
      },
    );
    const arg = invoke.mock.calls[0][1] as Record<string, unknown>;
    expect(invoke.mock.calls[0][0]).toBe('dw_recover_start');
    expect(arg.scanId).toBe('s1');
    expect(arg.selection).toEqual({ Nodes: { nodeIds: ['1', '2'] } });
    expect(arg.destination).toBe('D:\\Recovered');
    expect((arg.options as Record<string, unknown>).verifyOnWrite).toBe(true);
    expect(reply.jobId).toBe('j1-0');
  });

  it('job progress maps completed_partial to the contract kebab form', () => {
    const statuses: string[] = [];
    void connector.recovery.onRecoveryProgress('j1', (e) => statuses.push(e.status));
    const handler = listen.mock.calls[0][1] as (event: { payload: unknown }) => void;
    handler({
      payload: {
        jobId: 'j1',
        scanId: 's1',
        status: 'completed_partial',
        overallProgress: 1,
        currentFile: null,
        currentFileProgress: 1,
        completedCount: 3,
        failedCount: 1,
        skippedCount: 0,
        totalCount: 4,
        throughputBytesPerSecond: 100,
        verifying: false,
        etaSeconds: 0,
      },
    });
    expect(statuses).toEqual(['completed-partial']);
  });

  it('report failures carry mapped item error codes', async () => {
    invoke.mockResolvedValueOnce({
      jobId: 'j1',
      status: 'completed_partial',
      startedAtUnix: 1,
      finishedAtUnix: 2,
      succeeded: [],
      failed: [
        {
          nodeId: '7',
          name: 'gone.dat',
          sizeBytes: 10,
          state: 'failed',
          errorCode: 'record-reused',
          savedAs: null,
        },
      ],
      skipped: [],
      totalBytesWritten: 0,
      verified: false,
      processedTotal: 1,
      truncatedItemLists: false,
    });
    const report = await connector.recovery.getRecoveryReport('j1');
    expect(report.failed[0].error?.code).toBe('recovery-item-failed');
  });
});

describe('health / repair / system wiring', () => {
  it('health failures map to health-unavailable', async () => {
    invoke.mockRejectedValueOnce({ kind: 'volume', message: 'SMART query failed on disk 0' });
    await expect(connector.health.getHealth('dev:C')).rejects.toMatchObject({
      code: 'health-unavailable',
    });
  });

  it('repair undo log converts unix dates and snapshot sizes', async () => {
    invoke.mockResolvedValueOnce([
      {
        index: 0,
        action: 'boot-sector-restore',
        where: 'sector 0',
        atUnix: 1760000000,
        snapshotHex: 'AABB',
      },
    ]);
    const log = await connector.repair.getUndoLog('dev:D');
    expect(invoke).toHaveBeenCalledWith('dw_repair_undo_log', { deviceId: 'dev:D' });
    expect(log[0].snapshotBytes).toBe(2);
    expect(log[0].at).toBe(new Date(1760000000 * 1000).toISOString());
  });

  it('destination validation errors map to overlap', async () => {
    invoke.mockRejectedValueOnce({
      kind: 'staging',
      message: "destination 'C:\\x' is on the source volume C:",
    });
    await expect(
      connector.system.validateDestination('C:\\x', 'vol:C', 100),
    ).rejects.toMatchObject({ code: 'destination-overlap' });
  });

  it('openPath invokes dw_open_path', async () => {
    invoke.mockResolvedValueOnce(undefined);
    await connector.system.openPath('D:\\Recovered');
    expect(invoke).toHaveBeenCalledWith('dw_open_path', { path: 'D:\\Recovered' });
  });
});

describe('cleanup domain wiring', () => {
  it('staging payloads map snake_case to the contract', async () => {
    invoke.mockResolvedValueOnce({
      id: 3,
      original_path: 'C:\\tmp\\x',
      staged_path: 'C:\\Users\\dev\\AppData\\Local\\DiskWizard\\staging\\1\\x',
      size_bytes: 128,
      staged_at_unix: 1760000000,
      expires_at_unix: 1760086400,
      is_dir: false,
      fallback_tier: 'normal',
    });
    const item = await connector.cleanup.stagePath('C:\\tmp\\x');
    expect(invoke).toHaveBeenCalledWith('dw_stage_path', { path: 'C:\\tmp\\x' });
    expect(item.originalPath).toBe('C:\\tmp\\x');
    expect(item.fallbackTier).toBe('normal');
    expect(item.expiresAt).toBe(new Date(1760086400 * 1000).toISOString());
  });

  it('rules map pattern_type and dates', async () => {
    invoke.mockResolvedValueOnce([
      {
        id: 1,
        pattern: '*.tmp',
        pattern_type: 'extension',
        score: 90,
        explanation: 'Temp files',
        enabled: true,
        created_at_unix: 1760000000,
        updated_at_unix: 1760000000,
      },
    ]);
    const rules = await connector.cleanup.listRules();
    expect(rules[0].patternType).toBe('extension');
    expect(rules[0].score).toBe(90);
  });

  it('duplicates map snake_case stats', async () => {
    invoke.mockResolvedValueOnce({
      drive_letter: 'C',
      scanned_files: 1000,
      filtered_small: 40,
      candidate_pairs: 20,
      hash_errors: 0,
      groups: [{ hash_hex: 'ab'.padEnd(64, '0'), size_bytes: 8192, paths: ['C:\\a', 'C:\\b'] }],
      stats: { group_count: 1, redundant_bytes: 8192, elapsed_ms: 50 },
    });
    const result = await connector.cleanup.findDuplicates('s1', { minSizeBytes: 4096 });
    expect(invoke).toHaveBeenCalledWith('dw_duplicates_find', {
      scanId: 's1',
      minSizeBytes: 4096,
      sizeOnly: false,
    });
    expect(result.stats.redundantBytes).toBe(8192);
    expect(result.groups[0].paths).toHaveLength(2);
  });

  it('index search maps attrs to isDir', async () => {
    invoke.mockResolvedValueOnce([
      { volume_id: '\\\\.\\C:', name: 'file.txt', full_path: 'C:\\file.txt', attrs: 32 },
      { volume_id: '\\\\.\\C:', name: 'dir', full_path: 'C:\\dir', attrs: 16 },
    ]);
    const hits = await connector.cleanup.searchIndex('file', 10);
    expect(hits[0].isDir).toBe(false);
    expect(hits[1].isDir).toBe(true);
  });

  it('settings round-trip the KV commands', async () => {
    invoke.mockResolvedValueOnce('1');
    const value = await connector.cleanup.getSetting('tray_monitor_enabled');
    expect(invoke).toHaveBeenCalledWith('dw_get_setting', { key: 'tray_monitor_enabled' });
    expect(value).toBe('1');
    invoke.mockResolvedValueOnce(undefined);
    await connector.cleanup.setSetting('theme', 'dark');
    expect(invoke).toHaveBeenCalledWith('dw_set_setting', { key: 'theme', value: 'dark' });
  });
});

describe('runtime guard', () => {
  it('rejects cleanly outside the Tauri runtime', async () => {
    browserRuntime();
    await expect(connector.system.getAppInfo()).rejects.toThrow(/Tauri runtime/);
  });
});

describe('connector mapping edge branches', () => {
  it('imageDimensions parses GIF, BMP and JPEG headers', async () => {
    // GIF 64x32: header + logical screen descriptor.
    const gif = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 64, 0, 32, 0];
    invoke.mockResolvedValueOnce({
      kind: 'image',
      dataBase64: btoa(String.fromCharCode(...new Uint8Array(gif))),
      mime: 'image/gif',
      text: null,
      encoding: null,
      truncated: false,
      totalSize: gif.length,
      reasonKey: null,
    });
    const gifPayload = await connector.preview.requestPreview('s', '1');
    if (gifPayload.kind !== 'image') throw new Error('expected image');
    expect(gifPayload.width).toBe(64);
    expect(gifPayload.height).toBe(32);

    // BMP 4x2 (BITMAPINFOHEADER, little-endian).
    const bmp = new Uint8Array(26);
    bmp[0] = 0x42;
    bmp[1] = 0x4d;
    bmp[18] = 4;
    bmp[22] = 2;
    invoke.mockResolvedValueOnce({
      kind: 'image',
      dataBase64: btoa(String.fromCharCode(...bmp)),
      mime: 'image/bmp',
      text: null,
      encoding: null,
      truncated: false,
      totalSize: bmp.length,
      reasonKey: null,
    });
    const bmpPayload = await connector.preview.requestPreview('s', '1');
    if (bmpPayload.kind !== 'image') throw new Error('expected image');
    expect(bmpPayload.width).toBe(4);
    expect(bmpPayload.height).toBe(2);

    // JPEG 16x8 — SOF0 segment after a fake APP0.
    const jpg = new Uint8Array([
      0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x0b, 8, 0, 8, 0, 16,
    ]);
    invoke.mockResolvedValueOnce({
      kind: 'image',
      dataBase64: btoa(String.fromCharCode(...jpg)),
      mime: 'image/jpeg',
      text: null,
      encoding: null,
      truncated: false,
      totalSize: jpg.length,
      reasonKey: null,
    });
    const jpgPayload = await connector.preview.requestPreview('s', '1');
    if (jpgPayload.kind !== 'image') throw new Error('expected image');
    expect(jpgPayload.width).toBe(16);
    expect(jpgPayload.height).toBe(8);

    // Unknown format → null dimensions.
    invoke.mockResolvedValueOnce({
      kind: 'image',
      dataBase64: btoa('x'),
      mime: 'image/webp',
      text: null,
      encoding: null,
      truncated: false,
      totalSize: 1,
      reasonKey: null,
    });
    const webpPayload = await connector.preview.requestPreview('s', '1');
    if (webpPayload.kind !== 'image') throw new Error('expected image');
    expect(webpPayload.width).toBeNull();
    expect(webpPayload.height).toBeNull();
  });

  it('maps unknown engine enum strings to safe defaults', async () => {
    invoke.mockResolvedValueOnce({
      nodes: [
        {
          id: '1',
          scanId: 's',
          kind: 'file',
          name: 'x.weird',
          path: null,
          sizeBytes: 1,
          modifiedUnix: 0,
          extension: 'weird',
          category: 'bogus',
          confidence: 'nope',
          origin: 'elsewhere',
          childCount: null,
          thumbnailAvailable: false,
          parentId: null,
          attributes: null,
        },
      ],
      total: 1,
      totalBytes: 1,
      offset: 0,
      pageSize: 1,
      elapsedMs: 0,
    });
    const page = await connector.results.queryResults(
      's',
      {
        nodeId: null,
        treeMode: 'path',
        search: null,
        categories: null,
        dateFrom: null,
        dateTo: null,
        sizeMinBytes: null,
        sizeMaxBytes: null,
        extensions: null,
        origins: null,
        hideEmptyFiles: false,
      },
      { field: 'name', direction: 'asc' },
      { offset: 0, limit: 1 },
    );
    expect(page.nodes[0].category).toBe('other');
    expect(page.nodes[0].confidence).toBe('unknown');
    expect(page.nodes[0].origin).toBe('existing');
    expect(page.nodes[0].modifiedAt).toBeNull();
  });

  it('system listDiskLayouts + getSpecialFolders pass through', async () => {
    invoke.mockResolvedValueOnce([
      {
        diskNo: 1,
        sizeBytes: 1000,
        sectorSize: 512,
        style: 'weird',
        partitions: [],
        lostPartitions: [],
      },
    ]);
    const layouts = await connector.system.listDiskLayouts();
    expect(layouts[0].style).toBe('unknown');
    invoke.mockResolvedValueOnce({
      recycleBin: 'C:\\$Recycle.Bin',
      desktop: null,
      documents: null,
      downloads: null,
    });
    const folders = await connector.system.getSpecialFolders();
    expect(folders.recycleBin).toBeTruthy();
  });

  it('scan-scope errors map to scan-scope-invalid', async () => {
    invoke.mockRejectedValueOnce({
      kind: 'scan',
      message: "scan scope 'Nope' was not found on the volume",
    });
    await expect(
      connector.scan.startScan({ targetId: 'vol:C', pathScope: 'Nope' }),
    ).rejects.toMatchObject({ code: 'scan-scope-invalid' });
  });

  it('recovery item mapItemErrorCode passes ConnectorError-shaped failures through', async () => {
    const passthrough = Object.assign(new Error('already mapped'), { code: 'device-gone' });
    invoke.mockRejectedValueOnce(passthrough);
    await expect(connector.devices.listDevices()).rejects.toMatchObject({
      code: 'device-gone',
    });
  });
});
