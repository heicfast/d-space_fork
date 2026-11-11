import * as React from 'react';
import { ChevronRightIcon, FolderIcon } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { ResultTreeMode, TreeNode } from '@/connector/contract';
import { useResultTree } from '@/queries/results';
import { formatBytesCompact, formatCountCompact } from '@/lib/format';
import { cn } from '@/lib/utils';
import { Skeleton } from '@/components/ui/skeleton';

/**
 * Left navigation tree for the results explorer — path/type/raw modes with
 * per-level lazy loading through the query layer (never the connector
 * directly). Each expanded level is its own query-backed sub-component.
 */

function TreeIcon({ node }: { node: TreeNode }): React.JSX.Element {
  if (node.icon === 'device' || node.id === 'root') {
    return (
      <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
        <path
          d="M3.5 5C3.9 4.2 4.7 3.8 5.6 3.8H12.4C13.3 3.8 14.1 4.2 14.5 5L16.2 9.5H1.8L3.5 5Z"
          fill="#E2E8F0"
          stroke="#64748B"
          strokeWidth="1"
        />
        <ellipse cx="9" cy="6.7" rx="3.2" ry="1.3" fill="#94A3B8" fillOpacity="0.45" />
        <rect x="1.5" y="9.2" width="15" height="4.8" rx="1.4" fill="#64748B" />
        <rect x="3" y="10.9" width="4.2" height="1.4" rx="0.7" fill="#22C55E" />
        <circle cx="13.8" cy="11.6" r="0.75" fill="#CBD5E1" />
      </svg>
    );
  }

  if (node.icon === 'group') {
    if (node.special === 'lost-path' || node.id === 'lost-paths') {
      return (
        <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
          <path
            d="M2 4.5C2 3.7 2.7 3 3.5 3H7L8.5 4.8H14.5C15.3 4.8 16 5.5 16 6.3V13.5C16 14.3 15.3 15 14.5 15H3.5C2.7 15 2 14.3 2 13.5V4.5Z"
            fill="#FDE68A"
            stroke="#D97706"
            strokeWidth="1.1"
          />
          <path
            d="M7 9.5H11M9 7.5V11.5"
            stroke="#B45309"
            strokeWidth="1.2"
            strokeLinecap="round"
          />
        </svg>
      );
    }
    return (
      <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
        <path
          d="M2 4.5C2 3.7 2.7 3 3.5 3H7L8.5 4.8H14.5C15.3 4.8 16 5.5 16 6.3V13.5C16 14.3 15.3 15 14.5 15H3.5C2.7 15 2 14.3 2 13.5V4.5Z"
          fill="#BAE6FD"
          stroke="#0284C7"
          strokeWidth="1.1"
        />
        <path
          d="M6 10L8 12L12 8"
          stroke="#0369A1"
          strokeWidth="1.3"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    );
  }

  if (node.icon === 'category') {
    switch (node.label) {
      case 'graphics':
        return (
          <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
            <rect x="2" y="3" width="14" height="12" rx="2.2" fill="#F97316" />
            <circle cx="6.2" cy="6.8" r="1.4" fill="#FEF08A" />
            <path
              d="M3.2 13.2L7 9.5L9.8 12L12.2 9.8L14.8 13.2H3.2Z"
              fill="white"
              fillOpacity="0.92"
            />
          </svg>
        );
      case 'video':
        return (
          <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
            <rect x="2" y="3" width="14" height="12" rx="2.2" fill="#6366F1" />
            <path d="M7.2 6.3L12.2 9L7.2 11.7V6.3Z" fill="white" />
          </svg>
        );
      case 'audio':
        return (
          <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
            <rect x="2" y="3" width="14" height="12" rx="2.2" fill="#EC4899" />
            <path
              d="M7.5 11.5V6.2L12 5.2V10.5"
              stroke="white"
              strokeWidth="1.4"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
            <circle cx="6.5" cy="11.8" r="1.4" fill="white" />
            <circle cx="11" cy="10.8" r="1.4" fill="white" />
          </svg>
        );
      case 'document':
        return (
          <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
            <rect x="3" y="2" width="12" height="14" rx="2" fill="#0EA5E9" />
            <line
              x1="5.5"
              y1="6"
              x2="12.5"
              y2="6"
              stroke="white"
              strokeWidth="1.3"
              strokeLinecap="round"
            />
            <line
              x1="5.5"
              y1="9"
              x2="12.5"
              y2="9"
              stroke="white"
              strokeWidth="1.3"
              strokeLinecap="round"
            />
            <line
              x1="5.5"
              y1="12"
              x2="10"
              y2="12"
              stroke="white"
              strokeWidth="1.3"
              strokeLinecap="round"
            />
          </svg>
        );
      case 'email':
        return (
          <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
            <rect x="2" y="3.5" width="14" height="11" rx="2" fill="#14B8A6" />
            <path
              d="M3.5 5.5L9 9.5L14.5 5.5"
              stroke="white"
              strokeWidth="1.3"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        );
      case 'archive':
        return (
          <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
            <rect x="2.5" y="2.5" width="13" height="13" rx="2.2" fill="#F59E0B" />
            <rect x="7.8" y="3" width="2.4" height="2" fill="white" />
            <rect x="7.8" y="6" width="2.4" height="2" fill="white" />
            <rect x="7.8" y="9" width="2.4" height="3.5" rx="0.8" fill="white" />
          </svg>
        );
      case 'bookmarks':
        return (
          <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
            <path
              d="M4.5 2.5H13.5C14 2.5 14.5 3 14.5 3.5V15.5L9 12.2L3.5 15.5V3.5C3.5 3 4 2.5 4.5 2.5Z"
              fill="#8B5CF6"
            />
          </svg>
        );
      default:
        return (
          <svg viewBox="0 0 18 18" fill="none" className="size-4 shrink-0" aria-hidden>
            <rect x="2.5" y="2.5" width="13" height="13" rx="2.2" fill="#64748B" />
            <circle cx="6.5" cy="9" r="1.2" fill="white" />
            <circle cx="9" cy="9" r="1.2" fill="white" />
            <circle cx="11.5" cy="9" r="1.2" fill="white" />
          </svg>
        );
    }
  }

  return (
    <FolderIcon
      className="size-4 shrink-0 fill-[#93C5FD] text-[#3B82F6]"
      strokeWidth={1.6}
      aria-hidden
    />
  );
}

function treeLabel(node: TreeNode, t: (k: string) => string): string {
  if (node.special === 'lost-path') return t('results.tree.lostPaths');
  if (node.special === 'raw-found') return t('results.tree.rawFound');
  if (node.id === 'root') return t('results.tree.existingFiles');
  if (node.icon === 'category') return t(`categories.${node.label}`);
  if (node.icon === 'group' && node.label === 'raw') return t('results.tree.rawFound');
  return node.label;
}

function TreeRow({
  node,
  depth,
  mode,
  scanId,
  selectedId,
  onSelect,
  expanded,
  onToggleExpand,
}: {
  node: TreeNode;
  depth: number;
  mode: ResultTreeMode;
  scanId: string;
  selectedId: string | null;
  onSelect: (nodeId: string | null) => void;
  expanded: ReadonlySet<string>;
  onToggleExpand: (id: string) => void;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const hasKids = (mode === 'path' || mode === 'type') && node.childCount > 0;
  const isExpanded = expanded.has(node.id);
  const isSelected = selectedId === node.id || (selectedId === null && node.id === 'root');
  const label = treeLabel(node, t);

  return (
    <>
      <button
        type="button"
        role="treeitem"
        aria-expanded={hasKids ? isExpanded : undefined}
        aria-selected={isSelected}
        onClick={() => {
          onSelect(node.id);
          if (hasKids) onToggleExpand(node.id);
        }}
        className={cn(
          'flex h-8 w-full items-center gap-1.5 rounded-sm px-2 text-left text-xs outline-none transition-colors',
          'focus-visible:ring-2 focus-visible:ring-ring',
          isSelected
            ? 'bg-[#CFE2FF] font-semibold text-slate-900 dark:bg-surface-selected dark:text-primary'
            : 'text-foreground hover:bg-[#E4EEFC] dark:text-foreground-secondary dark:hover:bg-surface-hover',
        )}
        style={{ paddingLeft: `${8 + depth * 16}px` }}
      >
        {hasKids ? (
          <ChevronRightIcon
            className={cn(
              'size-3.5 shrink-0 text-foreground-secondary transition-transform duration-150',
              isExpanded && 'rotate-90',
            )}
            aria-hidden
          />
        ) : (
          <span className="w-3.5 shrink-0" aria-hidden />
        )}
        <span
          aria-hidden
          className={cn(
            'inline-flex size-3.5 shrink-0 items-center justify-center rounded-[3px] border',
            isSelected
              ? 'border-[#1677FF] bg-white dark:bg-surface'
              : 'border-slate-400/80 bg-white dark:border-border-strong dark:bg-surface',
          )}
        />
        <TreeIcon node={node} />
        <span className="min-w-0 flex-1 truncate">{label}</span>
        <span className="shrink-0 text-2xs tabular text-foreground-secondary">
          ({formatCountCompact(node.fileCount, i18n.language)})
        </span>
        <span className="sr-only">{formatBytesCompact(node.sizeBytes, i18n.language)}</span>
      </button>
      {hasKids && isExpanded ? (
        <TreeLevel
          scanId={scanId}
          parentId={node.id}
          mode={mode}
          depth={depth + 1}
          selectedId={selectedId}
          onSelect={onSelect}
          expanded={expanded}
          onToggleExpand={onToggleExpand}
        />
      ) : null}
    </>
  );
}

function TreeLevel({
  scanId,
  parentId,
  mode,
  depth,
  selectedId,
  onSelect,
  expanded,
  onToggleExpand,
}: {
  scanId: string;
  parentId: string | null;
  mode: ResultTreeMode;
  depth: number;
  selectedId: string | null;
  onSelect: (nodeId: string | null) => void;
  expanded: ReadonlySet<string>;
  onToggleExpand: (id: string) => void;
}): React.JSX.Element {
  const query = useResultTree(scanId, parentId, mode);
  if (query.isLoading) {
    return (
      <>
        {Array.from({ length: 4 }, (_, i) => (
          <div
            key={i}
            className="flex items-center gap-2 py-1"
            style={{ paddingLeft: `${6 + depth * 14}px` }}
          >
            <Skeleton className="size-4" />
            <Skeleton className="h-3 flex-1" />
          </div>
        ))}
      </>
    );
  }
  if (query.isError || !query.data) {
    return (
      <div className="px-3 py-1 text-xs text-danger">
        {query.error instanceof Error ? query.error.message : 'error'}
      </div>
    );
  }
  return (
    <>
      {query.data.nodes.map((node) => (
        <TreeRow
          key={node.id}
          node={node}
          depth={depth}
          mode={mode}
          scanId={scanId}
          selectedId={selectedId}
          onSelect={onSelect}
          expanded={expanded}
          onToggleExpand={onToggleExpand}
        />
      ))}
    </>
  );
}

export function ResultsTree({
  scanId,
  mode,
  selectedId,
  onSelect,
  className,
}: {
  scanId: string;
  mode: ResultTreeMode;
  selectedId: string | null;
  onSelect: (nodeId: string | null) => void;
  className?: string;
}): React.JSX.Element {
  const { t } = useTranslation();
  // Expanded set is derived per mode; toggling writes the current mode's set.
  const [expandedByMode, setExpandedByMode] = React.useState<
    Record<string, ReadonlySet<string>>
  >({
    path: new Set(),
    type: new Set(),
    raw: new Set(),
  });
  const expanded = expandedByMode[mode] ?? new Set<string>();

  const toggleExpand = (id: string): void => {
    setExpandedByMode((prev) => {
      const current = prev[mode] ?? new Set<string>();
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return { ...prev, [mode]: next };
    });
  };

  return (
    <div role="tree" aria-label={t('a11y.sidebarRegion')} className={cn('p-1', className)}>
      <TreeLevel
        scanId={scanId}
        parentId={null}
        mode={mode}
        depth={0}
        selectedId={selectedId}
        onSelect={onSelect}
        expanded={expanded}
        onToggleExpand={toggleExpand}
      />
    </div>
  );
}
