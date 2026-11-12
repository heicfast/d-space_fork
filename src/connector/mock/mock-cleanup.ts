/**
 * Mock cleanup domain — the D-Space safety features simulated for tests
 * and browser development (staging/undo with conflict choreography, user
 * rules CRUD, duplicate groups, size snapshots + deltas, USN quick search,
 * settings KV).
 *
 * Deterministic: no wall-clock reads (fixed timestamps), no randomness
 * beyond the seeded PRNG handed in for duplicate group selection.
 */

import type {
  CleanupDomain,
  CleanupExpiredItem,
  CleanupPermanentResult,
  CleanupReport,
  CleanupRule,
  CleanupSnapshotMeta,
  CleanupStagedItem,
  CleanupUndoOutcome,
  CleanupUndoResolution,
  DuplicatesResult,
  IndexHit,
  SnapshotDelta,
} from '../contract';

async function latency(meanMs: number, jitterMs = 0): Promise<void> {
  const ms = meanMs + (Math.random() - 0.5) * 2 * jitterMs;
  await new Promise((resolve) => setTimeout(resolve, Math.max(5, ms)));
}

function connError(
  code: 'session-not-found' | 'destination-overlap' | 'scan-not-found' | 'unknown',
  message: string,
): Error {
  return Object.assign(new Error(message), { code });
}

/** Fixed simulation clock (deterministic tests). */
const NOW = Date.parse('2026-01-15T10:00:00Z');

interface StagedRecord {
  id: number;
  originalPath: string;
  stagedPath: string;
  sizeBytes: number;
  stagedAtUnix: number;
  isDir: boolean;
  fallbackTier: 'normal' | 'cross_volume';
  /** Simulates a conflicting file reappearing at the original path. */
  conflict: boolean;
}

export interface MockCleanupDeps {
  /** Look up a completed scan's file names for duplicates/snapshots. */
  readonly scanFileNames: (scanId: string) => string[] | null;
  readonly scanDriveLetter: (scanId: string) => string | null;
  /** Quick-search the most recent scan's space (USN-index simulation). */
  readonly searchLatest: (query: string, limit: number) => IndexHit[];
}

export function createMockCleanup(deps: MockCleanupDeps): CleanupDomain {
  let nextId = 1;
  const staging = new Map<number, StagedRecord>();
  const rules = new Map<number, CleanupRule>();
  const snapshots = new Map<number, CleanupSnapshotMeta & { entries: Map<string, number> }>();
  let nextSnapshotId = 1;
  const settings = new Map<string, string>();

  const iso = (unix: number): string => new Date(unix * 1000).toISOString();

  return {
    async stagePath(path: string): Promise<CleanupStagedItem> {
      await latency(30, 10);
      const id = nextId++;
      const isDir = !/\.[a-z0-9]{1,8}$/i.test(path.split('\\').pop() ?? '');
      const rec: StagedRecord = {
        id,
        originalPath: path,
        stagedPath: `C:\\Users\\dev\\AppData\\Local\\DiskWizard\\staging\\1760000000\\${path
          .split('\\')
          .pop()}`,
        sizeBytes: 4096 * ((id % 97) + 1),
        stagedAtUnix: NOW / 1000,
        isDir,
        fallbackTier: path.startsWith('D:') ? 'cross_volume' : 'normal',
        conflict: id % 7 === 0,
      };
      staging.set(id, rec);
      return {
        id: rec.id,
        originalPath: rec.originalPath,
        stagedPath: rec.stagedPath,
        sizeBytes: rec.sizeBytes,
        stagedAt: iso(rec.stagedAtUnix),
        expiresAt: iso(rec.stagedAtUnix + 86400),
        isDir: rec.isDir,
        fallbackTier: rec.fallbackTier,
      };
    },

    async listStaging(): Promise<readonly CleanupStagedItem[]> {
      await latency(30, 10);
      return [...staging.values()]
        .sort((a, b) => b.id - a.id)
        .map((r) => ({
          id: r.id,
          originalPath: r.originalPath,
          stagedPath: r.stagedPath,
          sizeBytes: r.sizeBytes,
          stagedAt: iso(r.stagedAtUnix),
          expiresAt: iso(r.stagedAtUnix + 86400),
          isDir: r.isDir,
          fallbackTier: r.fallbackTier,
        }));
    },

    async undoStaging(id: number): Promise<CleanupUndoOutcome> {
      await latency(30, 10);
      const rec = staging.get(id);
      if (!rec) throw connError('unknown', `staging item ${id} not found`);
      if (rec.conflict) {
        return {
          kind: 'conflict',
          originalPath: rec.originalPath,
          staged: {
            path: rec.stagedPath,
            sizeBytes: rec.sizeBytes,
            modifiedAt: iso(rec.stagedAtUnix),
            blake3First4kbHex: 'a1b2c3d4e5f60718',
            isDir: rec.isDir,
          },
          target: {
            path: rec.originalPath,
            sizeBytes: rec.sizeBytes + 128,
            modifiedAt: iso(rec.stagedAtUnix + 3600),
            blake3First4kbHex: '0011223344556677',
            isDir: rec.isDir,
          },
        };
      }
      staging.delete(id);
      return { kind: 'restored', originalPath: rec.originalPath };
    },

    async undoResolve(
      id: number,
      resolution: CleanupUndoResolution,
    ): Promise<CleanupUndoOutcome> {
      await latency(30, 10);
      const rec = staging.get(id);
      if (!rec) throw connError('unknown', `staging item ${id} not found`);
      if (resolution === 'cancel') {
        throw connError('unknown', 'undo cancelled');
      }
      staging.delete(id);
      const renamed =
        resolution === 'rename' || resolution === 'keep_both'
          ? rec.originalPath.replace(/(\.[^.]*)?$/, ' (1)$1')
          : rec.originalPath;
      return { kind: 'restored', originalPath: renamed };
    },

    async permanentDelete(
      id: number,
      confirmPhrase: string,
      secureWipe: boolean,
    ): Promise<CleanupPermanentResult> {
      await latency(30, 10);
      const rec = staging.get(id);
      if (!rec) throw connError('unknown', `staging item ${id} not found`);
      const name = rec.originalPath.split('\\').pop() ?? '';
      if (confirmPhrase.trim().toLowerCase() !== name.toLowerCase()) {
        throw connError('unknown', 'confirmation phrase does not match the file name');
      }
      staging.delete(id);
      void secureWipe;
      return {
        id,
        originalPath: rec.originalPath,
        stagedPath: rec.stagedPath,
        sizeBytes: rec.sizeBytes,
        deletedAt: iso(NOW / 1000),
        blake3First4kbHex: '9f8e7d6c5b4a3021',
        isDir: rec.isDir,
      };
    },

    async listExpired(): Promise<readonly CleanupExpiredItem[]> {
      await latency(30, 10);
      // Deterministic expired subset: every third staged item (simulates
      // the 24h window having lapsed for older deletions).
      return [...staging.values()]
        .filter((r) => r.id % 3 === 0)
        .map((r) => ({
          id: r.id,
          originalPath: r.originalPath,
          sizeBytes: r.sizeBytes,
          expiredAt: iso(r.stagedAtUnix + 86400),
          isDir: r.isDir,
          ageSeconds: 3600 * ((r.id % 5) + 1),
        }));
    },

    async cleanupExpired(ratePerSec: number, force: boolean): Promise<CleanupReport> {
      await latency(30, 10);
      const expired = [...staging.values()].filter((r) => r.id % 3 === 0);
      if (expired.length > 3 && !force) {
        return {
          deleted: 0,
          failed: 0,
          totalBytes: 0,
          elapsedMs: 12,
          abortedThreshold: true,
        };
      }
      for (const r of expired) staging.delete(r.id);
      return {
        deleted: expired.length,
        failed: 0,
        totalBytes: expired.reduce((s, r) => s + r.sizeBytes, 0),
        elapsedMs: expired.length * (1000 / Math.max(1, ratePerSec)),
        abortedThreshold: false,
      };
    },

    async listRules(): Promise<readonly CleanupRule[]> {
      await latency(30, 10);
      return [...rules.values()].sort((a, b) => b.id - a.id);
    },

    async addRule(
      pattern: string,
      patternType: 'name' | 'extension',
      score: number,
      explanation: string,
    ): Promise<CleanupRule> {
      await latency(30, 10);
      if (!pattern.trim()) throw connError('unknown', 'pattern must not be empty');
      if (score < 0 || score > 100) throw connError('unknown', 'score must be 0-100');
      const id = rules.size + 1;
      const rule: CleanupRule = {
        id,
        pattern,
        patternType,
        score,
        explanation: explanation || 'User rule',
        enabled: true,
        createdAt: iso(NOW / 1000),
        updatedAt: iso(NOW / 1000),
      };
      rules.set(id, rule);
      return rule;
    },

    async deleteRule(id: number): Promise<void> {
      await latency(30, 10);
      if (!rules.delete(id)) throw connError('unknown', `rule ${id} not found`);
    },

    async toggleRule(id: number, enabled: boolean): Promise<void> {
      await latency(30, 10);
      const rule = rules.get(id);
      if (!rule) throw connError('unknown', `rule ${id} not found`);
      rules.set(id, { ...rule, enabled, updatedAt: iso(NOW / 1000) });
    },

    async findDuplicates(
      scanId: string,
      opts?: { readonly minSizeBytes?: number; readonly sizeOnly?: boolean },
    ): Promise<DuplicatesResult> {
      await latency(30, 10);
      const names = deps.scanFileNames(scanId);
      if (!names) throw connError('scan-not-found', `scan ${scanId} not found`);
      const min = opts?.minSizeBytes ?? 4096;
      const byStem = new Map<string, string[]>();
      for (const name of names) {
        const stem = name.replace(/\d+\.?[a-z]*$/i, '').toLowerCase();
        if (!stem) continue;
        const list = byStem.get(stem) ?? [];
        if (!list.includes(name)) list.push(name);
        byStem.set(stem, list);
      }
      const letter = deps.scanDriveLetter(scanId) ?? 'C';
      const groups = [...byStem.entries()]
        .filter(([, names2]) => names2.length > 1)
        .slice(0, 200)
        .map(([stem, names2], i) => ({
          hashHex: opts?.sizeOnly
            ? `size:${min * (i + 2)}`
            : `${stem.slice(0, 6).padEnd(6, '0')}${i.toString(16)}`.padEnd(64, 'a'),
          sizeBytes: min * (i + 2),
          paths: names2.map((n) => `${letter}:\\Users\\dev\\${n}`),
        }));
      return {
        driveLetter: letter,
        scannedFiles: names.length,
        filteredSmall: Math.floor(names.length * 0.04),
        candidatePairs: groups.length * 2,
        hashErrors: 0,
        groups,
        stats: {
          groupCount: groups.length,
          redundantBytes: groups.reduce((s, g) => s + g.sizeBytes * (g.paths.length - 1), 0),
          elapsedMs: 120,
        },
      };
    },

    async captureSnapshot(scanId: string): Promise<CleanupSnapshotMeta> {
      await latency(30, 10);
      const names = deps.scanFileNames(scanId);
      if (!names) throw connError('scan-not-found', `scan ${scanId} not found`);
      const entries = new Map<string, number>();
      for (const [i, name] of names.entries()) {
        entries.set(`C:\\Users\\dev\\${name}`, 4096 * ((i % 13) + 1));
      }
      const id = nextSnapshotId++;
      const meta: CleanupSnapshotMeta & { entries: Map<string, number> } = {
        id,
        volumeId: `\\\\.\\${deps.scanDriveLetter(scanId) ?? 'C'}:`,
        capturedAt: iso(NOW / 1000 + id * 60),
        totalSizeBytes: [...entries.values()].reduce((s, v) => s + v, 0),
        fileCount: 0,
        dirCount: Math.floor(names.length / 10),
        entryCount: entries.size,
        entries,
      };
      snapshots.set(id, meta);
      return meta;
    },

    async listSnapshots(): Promise<readonly CleanupSnapshotMeta[]> {
      await latency(30, 10);
      return [...snapshots.values()].sort((a, b) => b.id - a.id);
    },

    async snapshotDelta(from: number, to: number): Promise<SnapshotDelta> {
      await latency(30, 10);
      const a = snapshots.get(from);
      const b = snapshots.get(to);
      if (!a || !b) throw connError('unknown', 'snapshot not found');
      const added: { path: string; sizeBytes: number }[] = [];
      const removed: { path: string; sizeBytes: number }[] = [];
      const grew: {
        path: string;
        fromSize: number;
        toSize: number;
        deltaBytes: number;
      }[] = [];
      const shrunk: {
        path: string;
        fromSize: number;
        toSize: number;
        deltaBytes: number;
      }[] = [];
      for (const [path, size] of b.entries) {
        const prev = a.entries.get(path);
        if (prev === undefined) added.push({ path, sizeBytes: size });
        else if (size > prev) {
          grew.push({ path, fromSize: prev, toSize: size, deltaBytes: size - prev });
        } else if (size < prev) {
          shrunk.push({ path, fromSize: prev, toSize: size, deltaBytes: size - prev });
        }
      }
      for (const [path, size] of a.entries) {
        if (!b.entries.has(path)) removed.push({ path, sizeBytes: size });
      }
      const cap = <T>(list: T[], key: (item: T) => number): T[] =>
        [...list].sort((x, y) => key(y) - key(x)).slice(0, 10);
      return {
        fromId: from,
        toId: to,
        fromCapturedAt: a.capturedAt,
        toCapturedAt: b.capturedAt,
        netChangeBytes:
          [...b.entries.values()].reduce((s, v) => s + v, 0) -
          [...a.entries.values()].reduce((s, v) => s + v, 0),
        totalChangedPaths: added.length + removed.length + grew.length + shrunk.length,
        added: cap(added, (e) => e.sizeBytes),
        removed: cap(removed, (e) => e.sizeBytes),
        grew: cap(grew, (e) => e.deltaBytes),
        shrunk: cap(shrunk, (e) => e.deltaBytes).sort((x, y) => x.deltaBytes - y.deltaBytes),
      };
    },

    async cleanupSnapshots(retainDays?: number): Promise<number> {
      await latency(30, 10);
      void retainDays;
      const n = Math.max(0, snapshots.size - 5);
      const sorted = [...snapshots.keys()].sort((a, b) => b - a);
      for (const id of sorted.slice(5)) snapshots.delete(id);
      return n;
    },

    async searchIndex(query: string, limit?: number): Promise<readonly IndexHit[]> {
      await latency(30, 10);
      // The mock has no persistent USN index; searching the most recent
      // scan's space is the honest simulation.
      if (!query.trim()) return [];
      return deps.searchLatest(query, limit ?? 50);
    },

    async getSetting(key: string): Promise<string | null> {
      await latency(30, 10);
      return settings.get(key) ?? null;
    },

    async setSetting(key: string, value: string): Promise<void> {
      await latency(30, 10);
      settings.set(key, value);
    },
  };
}
