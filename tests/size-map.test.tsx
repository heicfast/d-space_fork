import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { squarify } from '@/components/ui/size-map';
import { useUiStore } from '@/stores/ui-store';
import type { TreeNode } from '@/connector/contract';

/**
 * Size Map — the squarified treemap layout (pure function tests) plus the
 * view-mode store extension.
 */

function node(
  id: string,
  label: string,
  sizeBytes: number,
  icon: 'directory' | 'group' = 'group',
): TreeNode {
  return {
    id,
    parentId: 'root',
    label,
    icon,
    childCount: 0,
    fileCount: 0,
    sizeBytes,
    origin: 'existing',
  };
}

afterEach(() => {
  cleanup();
  useUiStore.getState().setViewMode('details');
});

describe('squarify layout', () => {
  it('fills the canvas with a single node', () => {
    const { cells, other } = squarify([node('a', 'A', 100, 'directory')], 400, 300);
    expect(other).toHaveLength(0);
    expect(cells).toHaveLength(1);
    expect(cells[0].w).toBeCloseTo(400);
    expect(cells[0].h).toBeCloseTo(300);
  });

  it('splits two nodes proportionally by area', () => {
    const { cells } = squarify(
      [node('big', 'Big', 300, 'directory'), node('small', 'Small', 100)],
      400,
      300,
    );
    expect(cells).toHaveLength(2);
    const big = cells.find((c) => c.node.id === 'big')!;
    const small = cells.find((c) => c.node.id === 'small')!;
    expect(big.w * big.h).toBeGreaterThan(small.w * small.h);
    // Areas are proportional to the sizes (300:100 → 3:1).
    expect((big.w * big.h) / (small.w * small.h)).toBeCloseTo(3, 0);
  });

  it('collapses tiny items into a single Other cell', () => {
    const nodes = [
      node('huge', 'Huge', 1_000_000, 'directory'),
      ...Array.from({ length: 30 }, (_, i) => node(`t${i}`, `t${i}`, 100)),
    ];
    const { cells } = squarify(nodes, 400, 300);
    const others = cells.filter((c) => c.node.id === '__other__');
    expect(others).toHaveLength(1);
    // The 30 tiny items are represented by exactly one cell.
    expect(cells.filter((c) => c.node.id.startsWith('t'))).toHaveLength(0);
  });

  it('returns empty for empty input or zero canvas', () => {
    expect(squarify([], 400, 300).cells).toHaveLength(0);
    expect(squarify([node('a', 'A', 5)], 0, 0).cells).toHaveLength(0);
    expect(squarify([node('a', 'A', 0)], 400, 300).cells).toHaveLength(0);
  });

  it('covers the canvas without overlaps (small fixture)', () => {
    const nodes = [
      node('a', 'A', 400, 'directory'),
      node('b', 'B', 300),
      node('c', 'C', 200),
      node('d', 'D', 100),
    ];
    const { cells } = squarify(nodes, 200, 100);
    const area = cells.reduce((acc, c) => acc + c.w * c.h, 0);
    expect(area).toBeCloseTo(200 * 100, -1);
    for (const a of cells) {
      for (const b of cells) {
        if (a === b) continue;
        const overlaps =
          a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
        expect(overlaps).toBe(false);
      }
    }
  });
});

describe('view mode store', () => {
  it('supports the map view mode', () => {
    useUiStore.getState().setViewMode('map');
    expect(useUiStore.getState().viewMode).toBe('map');
    useUiStore.getState().setViewMode('icons');
    expect(useUiStore.getState().viewMode).toBe('icons');
  });
});

describe('SizeMap component', () => {
  it('renders the empty state without measurable size (jsdom fallback)', async () => {
    const { SizeMap } = await import('@/components/ui/size-map');
    const onDrill = vi.fn();
    const nodes = [node('docs', 'Documents', 6000, 'directory'), node('b.bin', 'b.bin', 3000)];
    render(<SizeMap nodes={nodes} onDrill={onDrill} />);
    // jsdom has no layout: the fallback measures 0×0 and the honest empty
    // state renders instead of a broken canvas.
    expect(
      screen.getByText('Try a deeper search, or a different location.'),
    ).toBeInTheDocument();
  });
});
