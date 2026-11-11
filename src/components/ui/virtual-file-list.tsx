import * as React from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import type { ResultNode, SortField } from '@/connector/contract';
import { ArrowDownIcon, ArrowUpIcon, ChevronsUpDownIcon } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';
import { formatBytes, formatDate } from '@/lib/format';
import { ConfidencePillInline } from './confidence-badge';
import { Checkbox } from './checkbox';
import { FileIconFor } from './file-icon';
import { useThumbnail } from '@/queries/results';

/**
 * Virtualized results table (1 M+ rows). Row height from the density token;
 * columns fixed-width so horizontal scrolling is stable; selection via the
 * shared selection store contract passed in as props.
 */

const COLUMN_CLASSES: Record<string, string> = {
  checkbox: 'w-9 shrink-0',
  name: 'min-w-[140px] flex-1',
  modified: 'w-[120px] shrink-0',
  type: 'w-[68px] shrink-0',
  size: 'w-[84px] shrink-0',
  condition: 'w-[148px] shrink-0',
};

/** Shared row-membership test across both selection modes. */
function rowSelected(
  selectionMode: 'ids' | 'all',
  selectedIds: ReadonlySet<string>,
  exclusions: ReadonlySet<string>,
  id: string,
): boolean {
  return selectionMode === 'all' ? !exclusions.has(id) : selectedIds.has(id);
}

function SortHeader({
  field,
  label,
  currentField,
  currentDirection,
  onSort,
  align = 'left',
  colClass,
}: {
  field: SortField;
  label: string;
  currentField: SortField;
  currentDirection: 'asc' | 'desc';
  onSort: (f: SortField) => void;
  align?: 'left' | 'right';
  colClass: string;
}): React.JSX.Element {
  const { t } = useTranslation();
  const active = currentField === field;
  const Icon = active
    ? currentDirection === 'asc'
      ? ArrowUpIcon
      : ArrowDownIcon
    : ChevronsUpDownIcon;
  return (
    <div
      role="columnheader"
      aria-sort={active ? (currentDirection === 'asc' ? 'ascending' : 'descending') : 'none'}
      className={cn('flex border-r border-b border-border/70 last:border-r-0', colClass)}
    >
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onSort(field);
        }}
        aria-label={`${label} — ${active ? (currentDirection === 'asc' ? t('a11y.sortDescending') : t('a11y.sortAscending')) : t('a11y.sortAscending')}`}
        className={cn(
          'flex h-9 w-full select-none items-center justify-between gap-1 px-2.5 text-xs font-medium text-foreground-secondary transition-colors hover:bg-surface-hover/50 hover:text-foreground',
          'focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset',
          align === 'right' && 'justify-end',
        )}
      >
        <span className="truncate">{label}</span>
        <Icon
          className={cn(
            'size-3.5 shrink-0',
            active ? 'opacity-90' : 'opacity-0 group-hover/column:opacity-40',
          )}
          aria-hidden
        />
      </button>
    </div>
  );
}

export function VirtualFileList({
  scanId,
  rows,
  total,
  rowHeight,
  viewMode = 'details',
  selectionMode = 'ids',
  selectedIds,
  exclusions,
  activeId,
  onRowClick,
  onRowDoubleClick,
  onCheckboxChange,
  onSelectAll,
  onClearActive,
  allSelected,
  someSelected = false,
  sort,
  onSortChange,
  loadMore,
  hasNextPage,
  className,
}: {
  scanId: string;
  rows: readonly ResultNode[];
  total: number;
  rowHeight: number;
  viewMode?: 'details' | 'icons';
  selectionMode?: 'ids' | 'all';
  selectedIds: ReadonlySet<string>;
  exclusions: ReadonlySet<string>;
  activeId: string | null;
  onRowClick: (node: ResultNode, index: number, e: React.MouseEvent) => void;
  onRowDoubleClick: (node: ResultNode) => void;
  onCheckboxChange: (node: ResultNode) => void;
  onSelectAll: (checked: boolean) => void;
  onClearActive?: () => void;
  allSelected: boolean;
  someSelected?: boolean;
  sort: { field: SortField; direction: 'asc' | 'desc' };
  onSortChange: (field: SortField) => void;
  loadMore?: () => void;
  hasNextPage?: boolean;
  className?: string;
}): React.JSX.Element {
  // TanStack Virtual's useVirtualizer() returns function values that cannot be
  // memoized safely, so React Compiler skips this component by design (ADR-002
  // documents the skip as accepted). The 'use no memo' directive below makes
  // that skip explicit at the compiler level with zero runtime effect; the
  // eslint-plugin-react-hooks 7.1.1 incompatible-library diagnostic does not
  // honor the directive yet, so the call site also carries a targeted disable.
  'use no memo';
  const { t, i18n } = useTranslation();
  const ref = React.useRef<HTMLDivElement | null>(null);

  // eslint-disable-next-line react-hooks/incompatible-library -- ADR-002: documented, accepted compiler skip (TanStack Virtual)
  const virtualizer = useVirtualizer({
    count: rows.length + (hasNextPage ? 1 : 0),
    getScrollElement: () => ref.current,
    estimateSize: () => rowHeight,
    overscan: 16,
  });

  React.useEffect(() => {
    const el = ref.current;
    if (!el || !loadMore) return;
    let cooldown = false;
    const onScroll = (): void => {
      if (cooldown) return;
      const nearBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 400;
      if (nearBottom && hasNextPage) {
        cooldown = true;
        loadMore();
        window.setTimeout(() => {
          cooldown = false;
        }, 120);
      }
    };
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => el.removeEventListener('scroll', onScroll);
  }, [loadMore, hasNextPage, ref]);

  return (
    <div
      className={cn('flex h-full min-h-0 flex-col overflow-hidden bg-surface', className)}
      role="grid"
      aria-rowcount={total}
      onClick={() => onClearActive?.()}
    >
      {viewMode === 'icons' ? (
        <div
          className="flex h-9 shrink-0 items-center gap-2.5 border-b border-border bg-surface px-3.5 text-xs font-medium text-foreground"
          role="row"
          onClick={(e) => e.stopPropagation()}
        >
          <div role="columnheader" className="flex items-center gap-2">
            <Checkbox
              checked={
                rows.length > 0 && allSelected ? true : someSelected ? 'indeterminate' : false
              }
              onCheckedChange={(v: boolean | 'indeterminate') => onSelectAll(v === true)}
              aria-label={t('results.selection.selectAll')}
            />
            <span>{t('results.selection.selectAll')}</span>
          </div>
        </div>
      ) : (
        <div
          className="group/column flex w-full shrink-0 border-b border-border bg-surface text-sm"
          role="row"
        >
          <div
            role="columnheader"
            className={cn(
              'flex h-9 items-center justify-center border-b border-border/70',
              COLUMN_CLASSES.checkbox,
            )}
            onClick={(e) => e.stopPropagation()}
          >
            <Checkbox
              checked={
                rows.length > 0 && allSelected ? true : someSelected ? 'indeterminate' : false
              }
              onCheckedChange={(v: boolean | 'indeterminate') => onSelectAll(v === true)}
              aria-label={t('results.selection.selectAll')}
            />
          </div>
          <SortHeader
            field="name"
            label={t('results.column.name')}
            currentField={sort.field}
            currentDirection={sort.direction}
            onSort={onSortChange}
            colClass={COLUMN_CLASSES.name}
          />
          <SortHeader
            field="modified"
            label={t('results.column.modified')}
            currentField={sort.field}
            currentDirection={sort.direction}
            onSort={onSortChange}
            colClass={COLUMN_CLASSES.modified}
          />
          <SortHeader
            field="type"
            label={t('results.column.type')}
            currentField={sort.field}
            currentDirection={sort.direction}
            onSort={onSortChange}
            colClass={COLUMN_CLASSES.type}
          />
          <SortHeader
            field="size"
            label={t('results.column.size')}
            currentField={sort.field}
            currentDirection={sort.direction}
            onSort={onSortChange}
            colClass={COLUMN_CLASSES.size}
            align="right"
          />
          <SortHeader
            field="confidence"
            label={t('results.column.condition')}
            currentField={sort.field}
            currentDirection={sort.direction}
            onSort={onSortChange}
            colClass={COLUMN_CLASSES.condition}
          />
        </div>
      )}
      <div
        ref={ref}
        role="rowgroup"
        className="min-h-0 flex-1 overflow-x-auto overflow-y-auto"
        tabIndex={0}
        aria-label={t('results.column.name')}
      >
        {viewMode === 'icons' ? (
          <IconGridBody
            scanId={scanId}
            rows={rows}
            selectionMode={selectionMode}
            selectedIds={selectedIds}
            exclusions={exclusions}
            activeId={activeId}
            onRowClick={onRowClick}
            onRowDoubleClick={onRowDoubleClick}
            onCheckboxChange={onCheckboxChange}
          />
        ) : (
          <div
            className="w-full min-w-[560px]"
            style={{
              height: virtualizer.getTotalSize(),
              position: 'relative',
            }}
          >
            {virtualizer.getVirtualItems().map((vRow) => {
              const idx = vRow.index;
              const isLoader = idx >= rows.length;
              const node = rows[idx];
              if (isLoader) {
                return (
                  <div
                    key="loader"
                    role="presentation"
                    className="absolute inset-x-0 flex items-center justify-center"
                    style={{ top: vRow.start, height: rowHeight }}
                  >
                    <span className="text-xs text-foreground-secondary">
                      {t('common.loading')}
                    </span>
                  </div>
                );
              }
              const isSelected =
                selectionMode === 'all' ? !exclusions.has(node.id) : selectedIds.has(node.id);
              const isActive = activeId === node.id;
              return (
                <div
                  key={node.id}
                  role="row"
                  aria-selected={isSelected}
                  data-active={isActive || undefined}
                  className={cn(
                    'absolute inset-x-0 flex w-full cursor-default items-center rounded-sm text-xs transition-colors',
                    isSelected || isActive
                      ? 'bg-[#D2E3FC]/85 dark:bg-surface-selected/60'
                      : 'hover:bg-[#EBF2FF]/70 dark:hover:bg-surface-hover/50',
                  )}
                  style={{ top: vRow.start, height: rowHeight }}
                  onClick={(e) => {
                    e.stopPropagation();
                    onRowClick(node, idx, e);
                  }}
                  onDoubleClick={(e) => {
                    e.stopPropagation();
                    onRowDoubleClick(node);
                  }}
                >
                  <div
                    role="gridcell"
                    className={cn(
                      'flex h-full items-center justify-center',
                      COLUMN_CLASSES.checkbox,
                    )}
                    onClick={(e) => e.stopPropagation()}
                  >
                    <Checkbox
                      checked={isSelected}
                      onCheckedChange={() => onCheckboxChange(node)}
                      aria-label={t('a11y.selectRow')}
                    />
                  </div>
                  <div
                    role="gridcell"
                    className={cn('flex h-full items-center gap-2 px-2.5', COLUMN_CLASSES.name)}
                  >
                    <FileIconFor node={node} />
                    <span className="truncate font-medium" title={node.name}>
                      {node.name}
                    </span>
                  </div>
                  <div
                    role="gridcell"
                    className={cn(
                      'flex h-full items-center px-2.5 text-foreground-secondary',
                      COLUMN_CLASSES.modified,
                    )}
                  >
                    <span className="truncate tabular">
                      {formatDate(node.modifiedAt, i18n.language)}
                    </span>
                  </div>
                  <div
                    role="gridcell"
                    className={cn(
                      'flex h-full items-center px-2.5 text-foreground-secondary',
                      COLUMN_CLASSES.type,
                    )}
                  >
                    <span className="truncate uppercase">
                      {node.extension?.replace('.', '') ?? ''}
                    </span>
                  </div>
                  <div
                    role="gridcell"
                    className={cn(
                      'flex h-full items-center justify-end px-2.5 tabular',
                      COLUMN_CLASSES.size,
                    )}
                  >
                    {formatBytes(node.sizeBytes, i18n.language)}
                  </div>
                  <div
                    role="gridcell"
                    className={cn('flex h-full items-center px-2.5', COLUMN_CLASSES.condition)}
                  >
                    <ConfidencePillInline grade={node.confidence} />
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

/** Icon (tile) view — corpus view toggle list⇄icon (atlas §8, preview-panel-live.png).
 *  Tiles render thumbnail card with top-left checkbox overlay and centered filename below. */
function IconGridBody({
  scanId,
  rows,
  selectionMode,
  selectedIds,
  exclusions,
  activeId,
  onRowClick,
  onRowDoubleClick,
  onCheckboxChange,
}: {
  scanId: string;
  rows: readonly ResultNode[];
  selectionMode: 'ids' | 'all';
  selectedIds: ReadonlySet<string>;
  exclusions: ReadonlySet<string>;
  activeId: string | null;
  onRowClick: (node: ResultNode, index: number, e: React.MouseEvent) => void;
  onRowDoubleClick: (node: ResultNode) => void;
  onCheckboxChange: (node: ResultNode) => void;
}): React.JSX.Element {
  const { t } = useTranslation();
  const visibleRows = rows.slice(0, 120);
  return (
    <div className="grid grid-cols-2 gap-3.5 p-3.5 sm:grid-cols-3 md:grid-cols-4">
      {visibleRows.map((node, idx) => {
        const isSelected = rowSelected(selectionMode, selectedIds, exclusions, node.id);
        const isActive = activeId === node.id;
        return (
          <div
            key={node.id}
            role="row"
            aria-selected={isSelected}
            data-active={isActive || undefined}
            className={cn(
              'group/tile relative flex h-[142px] cursor-default flex-col items-center justify-between rounded-[8px] border p-2.5 transition-all',
              isSelected || isActive
                ? 'border-[#1677FF] bg-[#EDF4FF] ring-1 ring-[#1677FF] dark:border-primary dark:bg-surface-selected dark:ring-primary'
                : 'border-[#E2E8F0] bg-white hover:border-[#93C5FD] hover:bg-[#EFF5FF] dark:border-border dark:bg-surface-elevated dark:hover:border-primary/60 dark:hover:bg-surface-hover',
            )}
            onClick={(e) => {
              e.stopPropagation();
              onRowClick(node, idx, e);
            }}
            onDoubleClick={(e) => {
              e.stopPropagation();
              onRowDoubleClick(node);
            }}
          >
            <div
              role="gridcell"
              className="absolute left-2.5 top-2.5 z-10"
              onClick={(e) => e.stopPropagation()}
            >
              <Checkbox
                checked={isSelected}
                onCheckedChange={() => onCheckboxChange(node)}
                aria-label={t('a11y.selectRow')}
              />
            </div>
            <div
              role="gridcell"
              className="flex min-h-0 w-full flex-1 flex-col items-center justify-center pt-2"
            >
              <IconTile scanId={scanId} node={node} />
              <div
                className="mt-2 w-full truncate text-center text-xs font-medium text-[#1E293B] dark:text-foreground"
                title={node.name}
              >
                {node.name}
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

function IconTile({ scanId, node }: { scanId: string; node: ResultNode }): React.JSX.Element {
  const thumb = useThumbnail(scanId, node.id, node.thumbnailAvailable);
  if (thumb.data?.dataUrl) {
    return (
      <img
        src={thumb.data.dataUrl}
        alt=""
        className="h-[82px] w-[112px] shrink-0 rounded-[4px] border border-[#E2E8F0] object-cover dark:border-border"
        loading="lazy"
      />
    );
  }
  return (
    <div className="flex h-[82px] w-[112px] shrink-0 items-center justify-center rounded-[4px] bg-[#F2F6FC] dark:bg-surface-tint">
      <FileIconFor node={node} />
    </div>
  );
}
