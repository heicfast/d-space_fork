import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';
import type { TreeNode } from '@/connector/contract';

/**
 * Size Map — a squarified treemap of the current folder's children
 * (the D-Space "See" pillar in the DiskWizard design language).
 *
 * Pure SVG, no external layout library: the classic squarified algorithm
 * (Bruls, Huizing & van Wijk) fills each row with the widest-aspect-ratio
 * tiles first. Cells too small to label collapse into a single "Other"
 * cell so the canvas stays readable on huge folders.
 */

interface Cell {
  readonly node: TreeNode;
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

const MIN_CELL_AREA = 24 * 14;

/** Squarified layout over `width × height` for nodes sorted desc by size. */
export function squarify(
  nodes: readonly TreeNode[],
  width: number,
  height: number,
): { cells: Cell[]; other: TreeNode[] } {
  const total = nodes.reduce((acc, n) => acc + n.sizeBytes, 0);
  if (total <= 0 || width <= 0 || height <= 0) return { cells: [], other: [] };

  // Split off items too small to render; they collapse into "Other".
  const sorted = [...nodes].sort((a, b) => b.sizeBytes - a.sizeBytes);
  const area = width * height;
  const renderable: TreeNode[] = [];
  const other: TreeNode[] = [];
  let usedArea = 0;
  for (const n of sorted) {
    const itemArea = (Math.max(n.sizeBytes, 1) / total) * area;
    if (itemArea < MIN_CELL_AREA && renderable.length > 0) {
      other.push(n);
    } else {
      renderable.push(n);
      usedArea += itemArea;
    }
  }
  if (other.length > 0) {
    // Reserve visual space for the Other cell proportionally.
    const otherArea = area - usedArea;
    if (otherArea > MIN_CELL_AREA) {
      renderable.push({
        ...other[0],
        id: '__other__',
        label: '__other__',
        sizeBytes: Math.max(1, Math.round((otherArea / area) * total)),
      } satisfies TreeNode);
    }
  }

  const cells: Cell[] = [];
  let x = 0;
  let y = 0;
  let w = width;
  let h = height;
  const remaining = [...renderable];
  const scale = area / renderable.reduce((acc, n) => acc + Math.max(n.sizeBytes, 1), 0);

  while (remaining.length > 0 && w > 0 && h > 0) {
    const horizontal = w >= h; // fill along the shorter side
    const shortSide = Math.min(w, h);
    const row: TreeNode[] = [];
    let rowArea = 0;
    let bestWorst = Number.POSITIVE_INFINITY;

    // Grow the row while the worst aspect ratio improves.
    for (;;) {
      const candidate = remaining[0];
      if (!candidate) break;
      const candidateArea = Math.max(candidate.sizeBytes, 1) * scale;
      const nextArea = rowArea + candidateArea;
      if (nextArea <= 0) break;
      // Worst ratio for the row laid along shortSide.
      const rowLen = nextArea / shortSide;
      const worst = Math.max(
        ...[...row, candidate].map((n) => {
          const a = Math.max(n.sizeBytes, 1) * scale;
          const side = a / rowLen;
          return Math.max(rowLen / side, side / rowLen);
        }),
      );
      if (row.length > 0 && worst > bestWorst) break;
      row.push(remaining.shift()!);
      rowArea = nextArea;
      bestWorst = worst;
    }
    if (row.length === 0) break;

    const rowLen = rowArea / shortSide;
    let offset = 0;
    for (const n of row) {
      const a = Math.max(n.sizeBytes, 1) * scale;
      const side = a / rowLen;
      const cell: Cell = horizontal
        ? { node: n, x, y: y + offset, w: rowLen, h: side }
        : { node: n, x: x + offset, y, w: side, h: rowLen };
      cells.push(cell);
      offset += side;
    }
    if (horizontal) {
      x += rowLen;
      w -= rowLen;
    } else {
      y += rowLen;
      h -= rowLen;
    }
  }
  return { cells, other: other.slice(1) };
}

function originFill(origin: TreeNode['origin']): string {
  switch (origin) {
    case 'deleted':
      return 'fill-[#F59E0B]';
    case 'lost-path':
      return 'fill-[#E11D48]';
    default:
      return 'fill-[#1677FF]';
  }
}

function labelFits(cell: Cell, label: string): boolean {
  const chars = Math.floor((cell.w - 8) / 6.2);
  return cell.w >= 44 && cell.h >= 18 && chars >= 3 && label.length > 0;
}

function truncateLabel(label: string, cell: Cell): string {
  const chars = Math.max(1, Math.floor((cell.w - 8) / 6.2));
  return label.length > chars ? `${label.slice(0, Math.max(1, chars - 1))}…` : label;
}

export function SizeMap({
  nodes,
  onDrill,
  className,
}: {
  nodes: readonly TreeNode[];
  onDrill: (node: TreeNode) => void;
  className?: string;
}): React.JSX.Element {
  const { t } = useTranslation();
  const containerRef = React.useRef<HTMLDivElement | null>(null);
  const [size, setSize] = React.useState<{ w: number; h: number }>({ w: 0, h: 0 });

  React.useEffect(() => {
    const el = containerRef.current;
    if (!el || typeof ResizeObserver === 'undefined') {
      // Fallback: measure once (also covers jsdom).
      if (el) setSize({ w: el.clientWidth, h: el.clientHeight });
      return;
    }
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (rect) setSize({ w: rect.width, h: rect.height });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const isFolder = (node: TreeNode): boolean => node.icon === 'directory';
  const folders = nodes.filter((n) => isFolder(n) && n.sizeBytes > 0);
  const files = nodes.filter((n) => !isFolder(n) && n.sizeBytes > 0);
  const layout = React.useMemo(
    () => squarify([...folders, ...files], size.w, size.h),
    [folders, files, size.w, size.h],
  );
  const totalBytes = nodes.reduce((acc, n) => acc + n.sizeBytes, 0);

  return (
    <div className={cn('flex h-full min-h-0 flex-col', className)}>
      <div className="flex items-center justify-between gap-3 px-3 py-1.5">
        <p className="text-xs text-foreground-secondary">{t('results.mapHint')}</p>
        <div
          className="flex shrink-0 items-center gap-3 text-[11px] text-foreground-secondary"
          aria-hidden
        >
          <span className="flex items-center gap-1.5">
            <svg viewBox="0 0 10 10" className="size-2.5">
              <rect width="10" height="10" rx="2" className="fill-[#1677FF]" />
            </svg>
            {t('results.mapLegendExisting')}
          </span>
          <span className="flex items-center gap-1.5">
            <svg viewBox="0 0 10 10" className="size-2.5">
              <rect width="10" height="10" rx="2" className="fill-[#F59E0B]" />
            </svg>
            {t('results.mapLegendDeleted')}
          </span>
          <span className="flex items-center gap-1.5">
            <svg viewBox="0 0 10 10" className="size-2.5">
              <rect width="10" height="10" rx="2" className="fill-[#E11D48]" />
            </svg>
            {t('results.mapLegendLost')}
          </span>
        </div>
      </div>
      <div ref={containerRef} className="min-h-0 flex-1 overflow-hidden p-2">
        {size.w > 0 && layout.cells.length > 0 ? (
          <svg
            viewBox={`0 0 ${size.w} ${size.h}`}
            width="100%"
            height="100%"
            role="img"
            aria-label={t('results.mapTab')}
            className="block"
          >
            {layout.cells.map((cell) => {
              const isOther = cell.node.id === '__other__';
              const folder = isFolder(cell.node) && !isOther;
              const pct =
                totalBytes > 0 ? Math.round((cell.node.sizeBytes / totalBytes) * 100) : 0;
              const label = isOther
                ? t('results.countOnly', { count: layout.other.length + 1 })
                : cell.node.label;
              const showLabel = labelFits(cell, label);
              return (
                <g
                  key={cell.node.id}
                  {...(folder
                    ? {
                        onClick: () => onDrill(cell.node),
                        onKeyDown: (e: React.KeyboardEvent) => {
                          if (e.key === 'Enter' || e.key === ' ') onDrill(cell.node);
                        },
                      }
                    : {})}
                >
                  <rect
                    x={cell.x + 1}
                    y={cell.y + 1}
                    width={Math.max(cell.w - 2, 1)}
                    height={Math.max(cell.h - 2, 1)}
                    rx="4"
                    className={cn(
                      originFill(cell.node.origin),
                      'stroke-white/70 transition-opacity',
                      folder && 'cursor-pointer hover:opacity-80',
                    )}
                    fillOpacity={folder ? 0.85 : 0.55}
                    tabIndex={folder ? 0 : -1}
                    role={folder ? 'button' : undefined}
                    aria-label={folder ? `${cell.node.label} — ${pct}%` : cell.node.label}
                  />
                  {showLabel ? (
                    <>
                      <text
                        x={cell.x + 6}
                        y={cell.y + 15}
                        fontSize="10.5"
                        className="pointer-events-none fill-white font-medium"
                      >
                        {truncateLabel(label, cell)}
                      </text>
                      {cell.h >= 32 ? (
                        <text
                          x={cell.x + 6}
                          y={cell.y + 27}
                          fontSize="9.5"
                          className="pointer-events-none fill-white/75 tabular"
                        >
                          {pct}%
                        </text>
                      ) : null}
                    </>
                  ) : null}
                </g>
              );
            })}
          </svg>
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-foreground-secondary">
            {t('results.emptyBody')}
          </div>
        )}
      </div>
    </div>
  );
}
