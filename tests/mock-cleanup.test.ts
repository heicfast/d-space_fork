import { beforeEach, describe, expect, it } from 'vitest';
import { createMockCleanup, type MockCleanupDeps } from '@/connector/mock/mock-cleanup';
import type { CleanupStagedItem } from '@/connector/contract';

/**
 * Mock cleanup domain — behavioral spec for the D-Space safety features
 * carried into DiskWizard (staging/undo with conflict choreography, the
 * typed-phrase permanent delete, rules CRUD, duplicates, snapshots and
 * their deltas).
 */

const NAMES = [
  'report1.pdf',
  'report2.pdf',
  'photo.jpg',
  'cache.tmp',
  'notes.txt',
  'video.mp4',
  'old_backup.zip',
  'debug.log',
  'music.mp3',
  'spreadsheet.xlsx',
];

const deps: MockCleanupDeps = {
  scanFileNames: (scanId) => (scanId === 's-known' ? NAMES : null),
  scanDriveLetter: (scanId) => (scanId === 's-known' ? 'C' : null),
  searchLatest: (query, limit) =>
    NAMES.filter((n) => n.includes(query))
      .slice(0, limit)
      .map((n) => ({ volumeId: 'vol:C', name: n, fullPath: `C:\\Users\\${n}`, isDir: false })),
};

let cleanup = createMockCleanup(deps);

beforeEach(() => {
  cleanup = createMockCleanup(deps);
});

describe('staging + undo lifecycle', () => {
  it('stages a path and lists it (normal tier)', async () => {
    const item = await cleanup.stagePath('C:\\Users\\dev\\cache.tmp');
    expect(item.fallbackTier).toBe('normal');
    expect(item.originalPath).toBe('C:\\Users\\dev\\cache.tmp');
    const list = await cleanup.listStaging();
    expect(list).toHaveLength(1);
    expect(list[0].id).toBe(item.id);
  });

  it('cross-volume paths stage with the verified-copy tier', async () => {
    const item = await cleanup.stagePath('D:\\somewhere\\big.vhdx');
    expect(item.fallbackTier).toBe('cross_volume');
  });

  it('undo restores cleanly when the target is free', async () => {
    const item = await cleanup.stagePath('C:\\a\\b.txt');
    const outcome = await cleanup.undoStaging(item.id);
    expect(outcome.kind).toBe('restored');
    expect(await cleanup.listStaging()).toHaveLength(0);
  });

  it('every 7th staged item surfaces a conflict with both snapshots', async () => {
    let conflict: CleanupStagedItem | null = null;
    for (let i = 0; i < 7; i += 1) {
      const item = await cleanup.stagePath(`C:\\gen\\f${i}.dat`);
      if ((await cleanup.undoStaging(item.id)).kind === 'conflict') conflict = item;
    }
    expect(conflict).not.toBeNull();
    // Re-stage + resolve keeps both.
    const item = await cleanup.stagePath('C:\\gen\\f0.dat');
    const resolved = await cleanup.undoResolve(item.id, 'keep_both');
    expect(resolved.kind).toBe('restored');
    expect(resolved.originalPath).toContain('(1)');
  });

  it('permanent delete demands the exact file name', async () => {
    const item = await cleanup.stagePath('C:\\precious\\final.docx');
    await expect(cleanup.permanentDelete(item.id, 'wrong.txt', false)).rejects.toThrow(
      /phrase/i,
    );
    const done = await cleanup.permanentDelete(item.id, 'Final.DOCX', true);
    expect(done.originalPath).toContain('final.docx');
    expect(done.blake3First4kbHex).toBeTruthy();
    expect(await cleanup.listStaging()).toHaveLength(0);
  });

  it('cleanup honors the >3-items threshold abort and force', async () => {
    for (let i = 0; i < 12; i += 1) {
      // ids 1..12 — every third (3, 6, 9, 12) is past the 24h window.
      await cleanup.stagePath(`C:\\exp\\e${i}.dat`);
    }
    expect(await cleanup.listExpired()).toHaveLength(4);
    const aborted = await cleanup.cleanupExpired(20, false);
    expect(aborted.abortedThreshold).toBe(true);
    expect(aborted.deleted).toBe(0);
    const forced = await cleanup.cleanupExpired(20, true);
    expect(forced.abortedThreshold).toBe(false);
    expect(forced.deleted).toBeGreaterThan(0);
  });
});

describe('rules CRUD', () => {
  it('adds, lists, toggles and deletes rules with validation', async () => {
    const rule = await cleanup.addRule('*.tmp', 'extension', 90, 'Temp files');
    expect(rule.patternType).toBe('extension');
    expect(rule.score).toBe(90);
    await expect(cleanup.addRule('', 'name', 50, '')).rejects.toThrow(/pattern/i);
    await expect(cleanup.addRule('x', 'name', 101, '')).rejects.toThrow(/score/i);
    await cleanup.toggleRule(rule.id, false);
    expect((await cleanup.listRules())[0].enabled).toBe(false);
    await cleanup.deleteRule(rule.id);
    await expect(cleanup.deleteRule(rule.id)).rejects.toThrow(/not found/i);
  });
});

describe('duplicates', () => {
  it('groups same-stem names and reports reclaimable bytes', async () => {
    const result = await cleanup.findDuplicates('s-known', { minSizeBytes: 4096 });
    expect(result.driveLetter).toBe('C');
    expect(result.scannedFiles).toBe(NAMES.length);
    expect(result.groups.length).toBeGreaterThan(0);
    expect(result.stats.redundantBytes).toBeGreaterThan(0);
  });

  it('size-only mode uses size: prefixed hashes', async () => {
    const result = await cleanup.findDuplicates('s-known', { sizeOnly: true });
    expect(result.groups.every((g) => g.hashHex.startsWith('size:'))).toBe(true);
  });

  it('unknown scans reject with scan-not-found', async () => {
    await expect(cleanup.findDuplicates('s-missing')).rejects.toMatchObject({
      code: 'scan-not-found',
    });
  });
});

describe('snapshots + delta', () => {
  it('captures, lists and computes the four delta buckets', async () => {
    const first = await cleanup.captureSnapshot('s-known');
    const second = await cleanup.captureSnapshot('s-known');
    expect(second.id).not.toBe(first.id);
    const list = await cleanup.listSnapshots();
    expect(list).toHaveLength(2);
    const delta = await cleanup.snapshotDelta(first.id, second.id);
    expect(delta.fromId).toBe(first.id);
    expect(delta.totalChangedPaths).toBeGreaterThanOrEqual(0);
  });

  it('rejects unknown snapshot ids', async () => {
    await expect(cleanup.snapshotDelta(99, 98)).rejects.toThrow(/not found/i);
  });
});

describe('quick search + settings', () => {
  it('searches the latest scan space', async () => {
    const hits = await cleanup.searchIndex('report', 5);
    expect(hits.length).toBeGreaterThan(0);
    expect(hits[0].name).toContain('report');
    expect(await cleanup.searchIndex('  ')).toHaveLength(0);
  });

  it('settings round-trip', async () => {
    expect(await cleanup.getSetting('any')).toBeNull();
    await cleanup.setSetting('any', 'value');
    expect(await cleanup.getSetting('any')).toBe('value');
  });
});
