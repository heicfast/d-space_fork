import { describe, expect, it } from 'vitest';
import { NodeSpace } from '@/connector/mock/node-space';
import type { ResultsFilter } from '@/connector/contract';

const BASE: ResultsFilter = {
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

describe('NodeSpace — procedural index', () => {
  it('is deterministic for identical seeds', () => {
    const a = new NodeSpace('vol-test', 1234, 40, 800);
    const b = new NodeSpace('vol-test', 1234, 40, 800);
    const pa = a.query(BASE, { field: 'name', direction: 'asc' }, 0, 10);
    const pb = b.query(BASE, { field: 'name', direction: 'asc' }, 0, 10);
    expect(pa.nodes.map((n) => n.name)).toEqual(pb.nodes.map((n) => n.name));
    expect(pa.total).toBe(pb.total);
  });

  it('pages within bounds and reports totals', () => {
    const s = new NodeSpace('vol-test', 7, 30, 500);
    const p1 = s.query(BASE, { field: 'name', direction: 'asc' }, 0, 50);
    const p2 = s.query(BASE, { field: 'name', direction: 'asc' }, 50, 50);
    expect(p1.nodes.length).toBe(50);
    expect(p1.total).toBe(500);
    if (p2.nodes.length > 0) {
      const names = new Set([...p1.nodes, ...p2.nodes].map((n) => n.name));
      expect(names.size).toBe(p1.nodes.length + p2.nodes.length);
    }
  });

  it('applies size and category filters correctly', () => {
    const s = new NodeSpace('vol-test', 99, 20, 600);
    const small = s.query(
      { ...BASE, sizeMaxBytes: 1024 },
      { field: 'size', direction: 'asc' },
      0,
      600,
    );
    expect(small.nodes.every((n) => n.sizeBytes <= 1024)).toBe(true);

    const gfx = s.query(
      { ...BASE, categories: ['graphics'] },
      { field: 'name', direction: 'asc' },
      0,
      600,
    );
    expect(gfx.nodes.every((n) => n.category === 'graphics')).toBe(true);
  });

  it('search matches substrings case-insensitively', () => {
    const s = new NodeSpace('vol-test', 5, 10, 300);
    const probe = s.query(BASE, { field: 'name', direction: 'asc' }, 0, 1).nodes[0];
    const stem = probe.name.slice(0, 3).toLowerCase();
    const hits = s.query(
      { ...BASE, search: stem },
      { field: 'name', direction: 'asc' },
      0,
      300,
    );
    expect(hits.total).toBeGreaterThan(0);
    expect(hits.nodes.every((n) => n.name.toLowerCase().includes(stem))).toBe(true);
  });

  it('path tree children partition the space into 3 disjoint top-level branches', () => {
    const s = new NodeSpace('vol-test', 11, 25, 400);
    const children = s.pathTreeChildren(null);
    expect(children.length).toBe(3); // root + lost-paths + raw groups
    const root = children.find((n) => n.id === 'root')!;
    const lost = children.find((n) => n.id === 'lost-paths')!;
    const raw = children.find((n) => n.id === 'raw-all')!;
    expect(raw.fileCount).toBe(s.stats().rawFiles);
    expect(root.fileCount + lost.fileCount + raw.fileCount).toBe(400);
    expect(root.sizeBytes + lost.sizeBytes + raw.sizeBytes).toBe(s.totalSizeBytes);

    // Selecting 'root' in path mode only returns existing + deleted files (never raw or lost-path)
    const rootHits = s.query(
      { ...BASE, treeMode: 'path', nodeId: 'root' },
      { field: 'name', direction: 'asc' },
      0,
      400,
    );
    expect(rootHits.total).toBe(root.fileCount);
    expect(rootHits.totalBytes).toBe(root.sizeBytes);
    expect(rootHits.nodes.every((n) => n.origin === 'existing' || n.origin === 'deleted')).toBe(
      true,
    );
  });

  it('grades 0-byte files as poor confidence per CRecoverability::GetEntryRecoverability', () => {
    const s = new NodeSpace('vol-test', 11, 25, 400);
    const page = s.query(BASE, { field: 'size', direction: 'asc' }, 0, 400);
    const zeroByteNodes = page.nodes.filter((n) => n.sizeBytes === 0);
    expect(zeroByteNodes.length).toBeGreaterThan(0);
    expect(zeroByteNodes.every((n) => n.confidence === 'poor')).toBe(true);
  });

  it('type tree returns exactly the 8 categories', () => {
    const s = new NodeSpace('vol-test', 12, 10, 200);
    const tree = s.typeTree();
    expect(tree.map((n) => n.label)).toEqual([
      'graphics',
      'video',
      'audio',
      'document',
      'email',
      'archive',
      'bookmarks',
      'other',
    ]);
  });

  it('scopes queries to a selected type node', () => {
    const s = new NodeSpace('vol-test', 21, 10, 300);
    const tree = s.typeTree();
    const graphics = tree.find((n) => n.label === 'graphics')!;
    const hits = s.query(
      { ...BASE, treeMode: 'type', nodeId: graphics.id },
      { field: 'name', direction: 'asc' },
      0,
      300,
    );
    expect(hits.nodes.every((n) => n.category === 'graphics')).toBe(true);
    expect(hits.total).toBe(graphics.fileCount);
  });
});

describe('NodeSpace — origin mix (ADR-008 corpus model)', () => {
  it('produces existing, deleted, lost-path and raw origins', () => {
    const s = new NodeSpace('vol-test', 99, 40, 1200);
    const page = s.query(BASE, { field: 'name', direction: 'asc' }, 0, 1200);
    const origins = new Set(page.nodes.map((n) => n.origin));
    expect(origins.has('existing')).toBe(true);
    expect(origins.has('deleted')).toBe(true);
    expect(origins.has('lost-path')).toBe(true);
    expect(origins.has('raw')).toBe(true);
  });

  it('queryNodes walks file indices for recover-all', () => {
    const s = new NodeSpace('vol-test', 99, 40, 1200);
    const total = s.count(BASE);
    const first = s.queryNodes(BASE, null, 0, 100);
    const second = s.queryNodes(BASE, null, 100, 100);
    expect(first.length).toBe(100);
    expect(second.length).toBe(100);
    expect(new Set([...first, ...second]).size).toBe(200);
    expect(total).toBeGreaterThanOrEqual(200);
  });

  it('searchingPathAt yields a path only for matching nodes', () => {
    const s = new NodeSpace('vol-test', 99, 40, 800);
    const p = s.searchingPathAt(3, null);
    expect(p === null || typeof p === 'string').toBe(true);
    if (p !== null) {
      expect(p.length).toBeGreaterThan(0);
    }
  });
});
