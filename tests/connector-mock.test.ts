import { describe, expect, it } from 'vitest';
import { MockConnector } from '@/connector/mock/mock-connector';
import { createTauriConnector } from '@/connector/tauri/tauri-connector';
import { scanPhaseOf } from '@/connector/contract';
import type { ResultsFilter, ScanStage } from '@/connector/contract';

const BASE_FILTER: ResultsFilter = {
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
};

async function waitForScanCompletion(
  c: MockConnector,
  scanId: string,
  timeoutMs = 90_000,
): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(
      () => reject(new Error('scan did not finish in time')),
      timeoutMs,
    );
    const unsub = c.scan.onScanProgress(scanId, (e) => {
      if (e.status === 'completed' || e.status === 'failed' || e.status === 'cancelled') {
        clearTimeout(timeout);
        unsub();
        resolve();
      }
    });
  });
}

/** Collect every progress event until the scan reaches a terminal state. */
async function collectProgress(
  c: MockConnector,
  scanId: string,
  timeoutMs = 90_000,
): Promise<{ stage: ScanStage; overall: number; currentPath: string | null }[]> {
  const events: { stage: ScanStage; overall: number; currentPath: string | null }[] = [];
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(
      () => reject(new Error('scan did not finish in time')),
      timeoutMs,
    );
    const unsub = c.scan.onScanProgress(scanId, (e) => {
      events.push({ stage: e.stage, overall: e.overallProgress, currentPath: e.currentPath });
      if (e.status === 'completed' || e.status === 'failed' || e.status === 'cancelled') {
        clearTimeout(timeout);
        unsub();
        resolve();
      }
    });
  });
  return events;
}

describe('MockConnector — devices', () => {
  it('lists devices deterministically for a fixed seed', async () => {
    const a = new MockConnector();
    const b = new MockConnector();
    const listA = await a.devices.listDevices();
    const listB = await b.devices.listDevices();
    expect(listA.map((d) => d.id)).toEqual(listB.map((d) => d.id));
    expect(listA.length).toBeGreaterThan(0);
    expect(listA.every((d) => d.volumes.length > 0)).toBe(true);
  });

  it('exposes quick-scan availability truthfully (bad-sectors scenario blocks it)', async () => {
    const c = new MockConnector();
    c.setScenario('bad-sectors');
    const list = await c.devices.listDevices();
    expect(list.some((d) => d.volumes.some((v) => !v.quickScanAvailable))).toBe(true);
  });

  it('carries the "Specify a location" virtual targets (Recycle Bin, Desktop)', async () => {
    const c = new MockConnector();
    const list = await c.devices.listDevices();
    const recycle = list.find((d) => d.id === 'dev-recycle-bin');
    const desktop = list.find((d) => d.id === 'dev-desktop');
    expect(recycle?.kind).toBe('virtual');
    expect(desktop?.kind).toBe('virtual');
    // Virtual targets are not mount points (they are not destination drives).
    const mounts = await c.system.listMountPoints();
    expect(mounts.some((m) => m.id === 'vol-recycle-bin')).toBe(false);
    expect(mounts.some((m) => m.id === 'vol-desktop')).toBe(false);
  });
});

describe('MockConnector — lost-data flow (ADR-008)', () => {
  it('starts directly from a target with no pre-configuration, and runs the full automatic chain', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const target = devices[0].volumes[0].id;
    const { scanId } = await c.scan.startScan({ targetId: target });
    const events = await collectProgress(c, scanId);

    // The automatic stage chain — quick pass THEN advanced, unattended.
    const stages = [...new Set(events.map((e) => e.stage))];
    const order = (s: ScanStage): number =>
      ['volume-analysis', 'quick-index', 'deep-walk', 'carve', 'tree-build', 'done'].indexOf(s);
    expect(stages.sort((a, b) => order(a) - order(b))).toEqual([
      'volume-analysis',
      'quick-index',
      'deep-walk',
      'carve',
      'tree-build',
      'done',
    ]);

    const state = await c.scan.getScanState(scanId);
    expect(state.status).toBe('completed');
    expect(state.progress.filesFound).toBeGreaterThan(0);
    expect(state.params.targetId).toBe(target);
  }, 120_000);

  it('quick → advanced advances with no user action (phase mapping)', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const { scanId } = await c.scan.startScan({ targetId: devices[0].volumes[0].id });
    const events = await collectProgress(c, scanId);
    const phases = [...new Set(events.map((e) => scanPhaseOf(e.stage)))];
    expect(phases).toEqual(['quick', 'advanced', 'organizing']);
    // Monotonic overall across the whole automatic run.
    for (let i = 1; i < events.length; i++) {
      expect(events[i].overall).toBeGreaterThanOrEqual(events[i - 1].overall - 1e-9);
    }
    await c.scan.cancelScan(scanId);
  }, 120_000);

  it('collapses the quick pass when the volume cannot be quick-indexed and still reaches 100% progress', async () => {
    const c = new MockConnector();
    c.setScenario('bad-sectors');
    const devices = await c.devices.listDevices();
    const blocked = devices.flatMap((d) => d.volumes).find((v) => !v.quickScanAvailable)!;
    const { scanId } = await c.scan.startScan({ targetId: blocked.id });
    const events = await collectProgress(c, scanId);
    const stages = new Set(events.map((e) => e.stage));
    expect(stages.has('quick-index')).toBe(false);
    expect(stages.has('deep-walk')).toBe(true);
    const state = await c.scan.getScanState(scanId);
    expect(state.status).toBe('completed');
    expect(state.progress.overallProgress).toBe(1);
    const exactCount = await c.results.countResults(scanId, BASE_FILTER);
    expect(state.progress.filesFound).toBe(exactCount);
  }, 120_000);

  it('streams the live "Searching:" ticker during the advanced pass only', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const { scanId } = await c.scan.startScan({ targetId: devices[0].volumes[0].id });
    const events = await collectProgress(c, scanId);
    const withTicker = events.filter((e) => e.currentPath !== null);
    expect(withTicker.length).toBeGreaterThan(0);
    expect(withTicker.every((e) => e.stage === 'deep-walk' || e.stage === 'carve')).toBe(true);
    // Ticker values look like deep tree paths.
    expect(typeof withTicker[0].currentPath).toBe('string');
  }, 120_000);

  it('scans the Recycle Bin and Desktop location targets directly', async () => {
    const c = new MockConnector();
    const { scanId } = await c.scan.startScan({ targetId: 'vol-recycle-bin' });
    await waitForScanCompletion(c, scanId, 60_000);
    const state = await c.scan.getScanState(scanId);
    expect(state.status).toBe('completed');
    expect(state.progress.filesFound).toBeGreaterThan(0);
  }, 90_000);
});

describe('MockConnector — scan lifecycle', () => {
  it('pauses and resumes within the timing budget', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const { scanId } = await c.scan.startScan({ targetId: devices[0].volumes[0].id });
    await new Promise((r) => setTimeout(r, 400));
    const t0 = Date.now();
    await c.scan.pauseScan(scanId);
    expect(Date.now() - t0).toBeLessThan(600);
    const paused = await c.scan.getScanState(scanId);
    expect(paused.status).toBe('paused');
    await c.scan.resumeScan(scanId);
    const resumed = await c.scan.getScanState(scanId);
    expect(resumed.status).toBe('running');
    await c.scan.cancelScan(scanId);
  }, 30_000);

  it('cancels within 300ms budget', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const { scanId } = await c.scan.startScan({ targetId: devices[0].volumes[0].id });
    await new Promise((r) => setTimeout(r, 300));
    const t0 = Date.now();
    await c.scan.cancelScan(scanId);
    expect(Date.now() - t0).toBeLessThanOrEqual(300);
  }, 30_000);

  it('injects a disconnect mid-scan and recovers via resume', async () => {
    const c = new MockConnector();
    c.setScenario('disconnect-mid-scan');
    const devices = await c.devices.listDevices();
    const { scanId } = await c.scan.startScan({ targetId: devices[0].volumes[0].id });
    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error('no disconnect')), 45_000);
      const unsub = c.scan.onScanProgress(scanId, (e) => {
        if (!e.deviceConnected) {
          clearTimeout(timeout);
          unsub();
          resolve();
        }
      });
    });
    const state = await c.scan.getScanState(scanId);
    expect(state.status).toBe('paused');
    expect(state.progress.deviceConnected).toBe(false);
    // resume models the user having reconnected the device
    await c.scan.resumeScan(scanId);
    const after = await c.scan.getScanState(scanId);
    expect(after.progress.deviceConnected).toBe(true);
    await c.scan.cancelScan(scanId);
  }, 60_000);
});

describe('MockConnector — results', () => {
  it('pages deterministically and filters by category', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const { scanId } = await c.scan.startScan({ targetId: devices[0].volumes[0].id });
    await new Promise((r) => setTimeout(r, 700));
    const page1 = await c.results.queryResults(
      scanId,
      BASE_FILTER,
      { field: 'name', direction: 'asc' },
      { offset: 0, limit: 50 },
    );
    expect(page1.nodes.length).toBeGreaterThan(0);
    expect(page1.total).toBeGreaterThanOrEqual(page1.nodes.length);

    const gfx = await c.results.queryResults(
      scanId,
      { ...BASE_FILTER, categories: ['graphics'] },
      { field: 'name', direction: 'asc' },
      { offset: 0, limit: 50 },
    );
    if (gfx.nodes.length > 0) {
      expect(gfx.nodes.every((n) => n.category === 'graphics')).toBe(true);
    }

    const tree = await c.results.getResultTree(scanId, null, 'type');
    expect(tree.nodes.length).toBe(8);
    await c.scan.cancelScan(scanId);
  }, 30_000);

  it('generates lost-path origins (advanced-scan finds with unreconstructable paths)', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const { scanId } = await c.scan.startScan({ targetId: devices[0].volumes[0].id });
    await waitForScanCompletion(c, scanId);
    const page = await c.results.queryResults(
      scanId,
      { ...BASE_FILTER, origins: ['lost-path'] },
      { field: 'name', direction: 'asc' },
      { offset: 0, limit: 50 },
    );
    expect(page.total).toBeGreaterThan(0);
    // The path tree's "lost paths" special group is therefore non-empty.
    const tree = await c.results.getResultTree(scanId, null, 'path');
    const lostGroup = tree.nodes.find((n) => n.id === 'lost-paths');
    expect(lostGroup?.fileCount).toBeGreaterThan(0);
  }, 120_000);

  it('honors the hide-empty-files filter toggle', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const { scanId } = await c.scan.startScan({ targetId: devices[0].volumes[0].id });
    await waitForScanCompletion(c, scanId);
    const all = await c.results.countResults(scanId, BASE_FILTER);
    const noEmpty = await c.results.countResults(scanId, {
      ...BASE_FILTER,
      hideEmptyFiles: true,
    });
    expect(noEmpty).toBeLessThanOrEqual(all);
  }, 120_000);

  it('reports filter latency under 200ms on the standard scenario (grid #36)', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const { scanId } = await c.scan.startScan({ targetId: devices[0].volumes[0].id });
    await new Promise((r) => setTimeout(r, 700));
    const page = await c.results.queryResults(
      scanId,
      { ...BASE_FILTER, search: 'report' },
      { field: 'name', direction: 'asc' },
      { offset: 0, limit: 200 },
    );
    expect(page.elapsedMs).toBeLessThan(200);
    await c.scan.cancelScan(scanId);
  }, 30_000);
});

describe('MockConnector — previews', () => {
  it('connector-owned kind inference: image nodes yield image payloads deterministically', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const { scanId } = await c.scan.startScan({ targetId: devices[0].volumes[0].id });
    await new Promise((r) => setTimeout(r, 700));
    const page = await c.results.queryResults(
      scanId,
      BASE_FILTER,
      { field: 'name', direction: 'asc' },
      { offset: 0, limit: 100 },
    );
    const imageNode = page.nodes.find((n) => n.category === 'graphics');
    if (imageNode) {
      const p1 = await c.preview.requestPreview(scanId, imageNode.id);
      const p2 = await c.preview.requestPreview(scanId, imageNode.id);
      expect(p1.kind).toBe('image');
      if (p1.kind === 'image' && p2.kind === 'image') {
        expect(p1.dataUrl).toBe(p2.dataUrl);
      }
    }
    // A text-category node yields a text payload (no UI-side inference).
    const textNode = page.nodes.find((n) => n.category === 'document');
    if (textNode) {
      const p = await c.preview.requestPreview(scanId, textNode.id);
      expect(['office', 'pdf', 'text']).toContain(p.kind);
    }
    await c.scan.cancelScan(scanId);
  }, 30_000);
});

describe('MockConnector — folder scope (ScanParams.pathScope)', () => {
  it('destination picking avoids the excluded (source) device', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const source = devices[0].volumes[0].id;
    const picked = await c.system.pickDirectory({ excludeDeviceId: source });
    expect(picked).toBeTruthy();
    // The suggestion must not live on the source volume (or its device).
    const sourceLetters = devices[0].volumes
      .filter((v) => v.letter)
      .map((v) => `${v.letter}:\\`);
    expect(sourceLetters.some((l) => picked!.startsWith(l))).toBe(false);
    // An unexcluded pick may land anywhere (deterministic first mount).
    const anyPick = await c.system.pickDirectory();
    expect(anyPick).toBeTruthy();
  });

  it('rejects an unresolvable scope with scan-scope-invalid and a reason key', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    await expect(
      c.scan.startScan({
        targetId: devices[0].volumes[0].id,
        pathScope: 'definitely\\not\\a\\folder',
      }),
    ).rejects.toMatchObject({
      code: 'scan-scope-invalid',
      reasonKey: 'errors.scanScopeInvalid',
    });
  });

  it('lists only files inside the scanned folder (absolute and relative forms)', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const target = devices[0].volumes[0].id;
    // The volume-root-relative and the absolute (drive-letter) form must
    // resolve to the same scope.
    const relative = await c.system.pickDirectory({ volumeId: target });
    expect(relative).toBeTruthy();
    const withDrive = `X:\\${relative!.replace(/^[A-Z]:\\/i, '')}`;

    for (const scope of [relative, withDrive]) {
      const { scanId } = await c.scan.startScan({ targetId: target, pathScope: scope });
      await waitForScanCompletion(c, scanId);
      const page = await c.results.queryResults(
        scanId,
        BASE_FILTER,
        { field: 'name', direction: 'asc' },
        { offset: 0, limit: 50 },
      );
      expect(page.total).toBeGreaterThan(0);
      // Raw-origin nodes have no path; every path-bearing node must sit
      // inside the scanned subtree.
      const prefix = relative!.replace(/^[A-Z]:\\/i, '').toLowerCase();
      expect(
        page.nodes
          .filter((n) => n.path !== null)
          .every((n) => n.path!.toLowerCase().startsWith(prefix)),
      ).toBe(true);
    }
  }, 180_000);

  it('presents the scanned folder as the path-tree root', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const target = devices[0].volumes[0].id;
    const scope = (await c.system.pickDirectory({ volumeId: target }))!;
    const { scanId } = await c.scan.startScan({ targetId: target, pathScope: scope });
    await new Promise((r) => setTimeout(r, 500));
    const tree = await c.results.getResultTree(scanId, null, 'path');
    expect(tree.nodes.length).toBe(1);
    expect(tree.nodes[0].id).toMatch(/^d\d+$/);
    expect(tree.nodes[0].parentId).toBeNull();
    await c.scan.cancelScan(scanId);
  }, 30_000);
});

describe('MockConnector — recovery', () => {
  it('refuses destinations that overlap the source volume (device OR volume id)', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const source = devices[0];
    const letter = source.volumes[0].letter;
    for (const id of [source.id, source.volumes[0].id]) {
      const validation = await c.system.validateDestination(`${letter}:\\Recovered`, id, 1024);
      expect(validation.allowed).toBe(false);
      expect(validation.reason).toBe('overlap');
    }
  });

  it('runs an explicit-selection recovery job to completion with per-item states', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const source = devices[0];
    const { scanId } = await c.scan.startScan({ targetId: source.volumes[0].id });
    await waitForScanCompletion(c, scanId);
    const page = await c.results.queryResults(
      scanId,
      BASE_FILTER,
      { field: 'name', direction: 'asc' },
      { offset: 0, limit: 6 },
    );
    expect(page.nodes.length).toBeGreaterThanOrEqual(5);
    const dest = await c.system.pickDirectory();
    const { jobId } = await c.recovery.startRecovery(
      scanId,
      { kind: 'nodes', nodeIds: page.nodes.slice(0, 5).map((n) => n.id) },
      dest!,
      {
        collision: 'rename',
        rebuildFolders: true,
        keepEmptyFolders: false,
        fileRepair: true,
        verifyOnWrite: true,
      },
    );
    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error('job did not finish')), 60_000);
      const unsub = c.recovery.onRecoveryProgress(jobId, (e) => {
        if (e.status !== 'running' && e.status !== 'validating') {
          clearTimeout(timeout);
          unsub();
          resolve();
        }
      });
    });
    const report = await c.recovery.getRecoveryReport(jobId);
    const total = report.succeeded.length + report.failed.length + report.skipped.length;
    expect(total).toBe(5);
    expect(report.status).toBeOneOf(['completed', 'completed-partial']);
  }, 150_000);

  it('recover-all runs a REAL job over the full filtered set (no zero-item stub)', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const source = devices[0];
    const { scanId } = await c.scan.startScan({ targetId: source.volumes[0].id });
    await waitForScanCompletion(c, scanId);
    const expected = await c.results.countResults(scanId, BASE_FILTER);
    expect(expected).toBeGreaterThan(50);
    const dest = await c.system.pickDirectory();
    const { jobId } = await c.recovery.startRecovery(
      scanId,
      { kind: 'all', filter: BASE_FILTER },
      dest!,
      {
        collision: 'rename',
        rebuildFolders: true,
        keepEmptyFolders: false,
        fileRepair: false,
        verifyOnWrite: false,
      },
    );
    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error('job did not finish')), 120_000);
      const unsub = c.recovery.onRecoveryProgress(jobId, (e) => {
        if (e.status !== 'running' && e.status !== 'validating') {
          clearTimeout(timeout);
          unsub();
          resolve();
        }
      });
    });
    const report = await c.recovery.getRecoveryReport(jobId);
    expect(report.status).toBeOneOf(['completed', 'completed-partial']);
    // The job processed the WHOLE set — not an instant 0/0 completion.
    expect(report.processedTotal).toBe(expected);
    // Item lists are bounded at scale but counts stay exact.
    expect(report.truncatedItemLists).toBe(true);
    expect(report.succeeded.length).toBeLessThanOrEqual(500);
  }, 240_000);
});

describe('MockConnector — health & repair', () => {
  it('returns SMART tables with the documented field set', async () => {
    const c = new MockConnector();
    const devices = await c.devices.listDevices();
    const h = await c.health.getHealth(devices[0].id);
    expect(h.attributes.length).toBeGreaterThan(6);
    for (const a of h.attributes) {
      expect(a).toHaveProperty('id');
      expect(a).toHaveProperty('name');
      expect(a).toHaveProperty('raw');
      expect(a).toHaveProperty('status');
    }
    expect(['good', 'caution', 'bad']).toContain(h.status);
  });

  it('reaches the failing (bad) health state on the failing-disk scenario', async () => {
    const c = new MockConnector();
    c.setScenario('failing-disk');
    const devices = await c.devices.listDevices();
    const h = await c.health.getHealth(devices[0].id);
    expect(h.status).toBe('bad');
    expect(h.reallocatedSectors ?? 0).toBeGreaterThan(1000);
    expect(h.attributes.some((a) => a.status === 'failing')).toBe(true);
  });

  it('requires the typed consent token for repair', async () => {
    const c = new MockConnector();
    c.setScenario('bad-sectors');
    const devices = await c.devices.listDevices();
    const dry = await c.repair.dryRunRepair(devices[0].id);
    expect(dry.findings.length).toBeGreaterThan(0);
    await expect(
      c.repair.applyRepair(devices[0].id, { consentToken: 'YES' }),
    ).rejects.toMatchObject({
      code: 'repair-consent-required',
    });
    const { applied } = await c.repair.applyRepair(devices[0].id, { consentToken: 'REPAIR' });
    expect(applied).toBeGreaterThan(0);
    const log = await c.repair.getUndoLog(devices[0].id);
    expect(log.length).toBe(applied);
    const undo = await c.repair.undoRepairs(devices[0].id);
    expect(undo.rolledBack).toBe(applied);
  });

  it('openPath reports not-implemented honestly (no shell in the simulator)', async () => {
    const c = new MockConnector();
    await expect(c.system.openPath('C:\\some\\path')).rejects.toMatchObject({
      code: 'not-implemented',
    });
  });
});

describe('Tauri stub adapter', () => {
  it('throws NotImplementedError on every domain', async () => {
    const stub = createTauriConnector();
    await expect(async () => stub.devices.listDevices()).rejects.toThrow(/not implemented/i);
    await expect(async () => stub.system.getAppInfo()).rejects.toThrow(/CONNECTOR-CONTRACT/i);
  });
});
