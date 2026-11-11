import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import type { ResultNode, ResultTreeMode } from '@/connector/contract';
import {
  ArrowLeftIcon,
  ArrowUpIcon,
  ChevronRightIcon,
  FilterIcon,
  FolderTreeIcon,
  HomeIcon,
  LayoutGridIcon,
  ListIcon,
  SaveIcon,
  SearchIcon,
  SparklesIcon,
  Trash2Icon,
  UploadIcon,
  XIcon,
} from 'lucide-react';
import { useCountResults, useResultsPages, useResultTree } from '@/queries/results';
import { useSaveSession } from '@/queries/sessions';
import { useConnectorMode } from '@/queries/dev';
import { useFilterStore, useSortStore, resolveFilter } from '@/stores/filter-store';
import { selectedCountOf, useSelectionStore } from '@/stores/selection-store';
import { useUiStore } from '@/stores/ui-store';
import { formatBytes, formatCountCompact } from '@/lib/format';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ProgressGauge } from '@/components/ui/progress';
import { ResultsTree } from '@/components/ui/results-tree';
import { VirtualFileList } from '@/components/ui/virtual-file-list';
import { Segmented } from '@/components/ui/stepper';
import { EmptyState } from '@/components/ui/empty-state';
import { Skeleton } from '@/components/ui/skeleton';
import { Input } from '@/components/ui/input';
import { toast } from 'sonner';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/menus';
import { FilterBar } from './filter-bar';
import { PreviewPane } from '@/features/preview/preview-pane';

/**
 * Results Explorer — the 3-pane surface (tree / virtualized list / preview)
 * with toolbar (nav cluster, breadcrumb, view toggle, filter, search),
 * toggleable filter bar, the corpus 6-item context menu (atlas §8), and
 * the contextual bottom CTA. Also renders (live mode) while the scan runs.
 */

const CONFIDENCE_RANK: Record<string, number> = {
  excellent: 4,
  good: 3,
  fair: 2,
  poor: 1,
  unknown: 0,
};

export function ResultsExplorer({
  scanId,
  liveMode = false,
  recoverableDuringScan = true,
}: {
  scanId: string;
  liveMode?: boolean;
  recoverableDuringScan?: boolean;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const filter = useFilterStore();
  const sortStore = useSortStore();
  const selection = useSelectionStore();
  const ui = useUiStore();
  const saveSession = useSaveSession(scanId);
  const connectorMode = useConnectorMode();

  const [filterBarOpen, setFilterBarOpen] = React.useState(false);
  const [searchDraft, setSearchDraft] = React.useState(filter.search);
  const [activeNodeId, setActiveNodeId] = React.useState<string | null>(null);
  const [smartSelecting, setSmartSelecting] = React.useState(false);

  // resolveFilter() reads the store imperatively, so the subscribed state
  // object itself is the dependency (identity changes on every store update).
  const filterPayload = React.useMemo(
    () => ({ ...resolveFilter(), nodeId: filter.treeNodeId, treeMode: filter.treeMode }),
    [filter],
  );

  const sort = { field: sortStore.field, direction: sortStore.direction };
  const pages = useResultsPages(scanId, filterPayload, sort);
  const rootTree = useResultTree(scanId, null, filter.treeMode);
  const countQuery = useCountResults(scanId, filterPayload);

  const rows = React.useMemo(
    () => pages.data?.pages.flatMap((p) => p.nodes) ?? [],
    [pages.data],
  );
  const total = pages.data?.pages[pages.data.pages.length - 1]?.total ?? 0;

  React.useEffect(() => {
    if (!activeNodeId) return;
    const onKeyDown = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        setActiveNodeId(null);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [activeNodeId]);

  const rowHeight = ui.viewMode === 'details' ? 42 : 96;
  const selectionSnapshot = React.useMemo(
    () => ({
      mode: selection.mode,
      ids: selection.ids,
      exclusions: selection.exclusions,
      allCount: selection.allCount,
    }),
    [selection.mode, selection.ids, selection.exclusions, selection.allCount],
  );
  const selectedCount = selectedCountOf(selectionSnapshot);
  const selectedBytes = React.useMemo(
    () =>
      rows.filter((r) => selection.isSelected(r.id)).reduce((acc, r) => acc + r.sizeBytes, 0),
    [rows, selection],
  );

  const onRowClick = (node: ResultNode, index: number, e: React.MouseEvent): void => {
    // Corpus row dynamics (atlas §8): single click → checkbox fills + details
    // pane updates; the pane preview follows the active row.
    setActiveNodeId(node.id);
    if (e.shiftKey) {
      const anchorIdx = rows.findIndex((r) => r.id === selection.anchor);
      if (anchorIdx >= 0) {
        const [from, to] = anchorIdx < index ? [anchorIdx, index] : [index, anchorIdx];
        selection.setOnly(rows.slice(from, to + 1).map((r) => r.id));
        return;
      }
    }
    if (e.ctrlKey || e.metaKey) {
      selection.toggle(node.id);
    }
    selection.setAnchor(node.id);
  };

  const onRecover = (): void => {
    if (selection.mode === 'all') {
      void navigate(`/recover/${scanId}?all=1`);
      return;
    }
    if (selection.ids.size === 0) {
      void navigate(`/recover/${scanId}?all=1`);
      return;
    }
    void navigate(`/recover/${scanId}?ids=1`);
  };

  /** "Smart select best files" (atlas §8, magic wand): keep the best
   *  recovery-condition file per name over the loaded result window —
   *  confidence-ranked, non-existing preferred (deleted/raw beat existing). */
  const onSmartSelect = (): void => {
    setSmartSelecting(true);
    try {
      const best = new Map<string, ResultNode>();
      for (const r of rows) {
        if (r.kind !== 'file') continue;
        const key = r.name.toLowerCase();
        const score =
          (r.origin === 'existing' ? 0 : 1_000_000) + (CONFIDENCE_RANK[r.confidence] ?? 0);
        const prev = best.get(key);
        const prevScore = prev
          ? (prev.origin === 'existing' ? 0 : 1_000_000) +
            (CONFIDENCE_RANK[prev.confidence] ?? 0)
          : -1;
        if (score > prevScore) best.set(key, r);
      }
      const picked = [...best.values()].map((r) => r.id);
      selection.setOnly(picked);
      toast.success(t('results.smartSelectDone', { count: picked.length }));
    } finally {
      setSmartSelecting(false);
    }
  };

  const treeModeLabel: Record<ResultTreeMode, string> = {
    path: t('results.pathTab'),
    type: t('results.typeTab'),
    raw: t('results.tree.rawFound'),
  };

  const someRowsSelected = rows.length > 0 && rows.some((r) => selection.isSelected(r.id));
  const allRowsSelected =
    rows.length > 0 &&
    (selection.mode === 'all'
      ? selection.exclusions.size === 0
      : rows.every((r) => selection.ids.has(r.id)));

  return (
    <div
      className="flex h-full min-h-0 flex-col overflow-hidden bg-surface"
      onClick={() => setActiveNodeId(null)}
    >
      {/* Sub-header Toolbar (matching reference screenshots: #EDF3FF bar with soft-blue nav buttons, breadcrumb, view toggle, Filter button, and search box) */}
      <div className="flex h-12 shrink-0 items-center gap-2 border-b border-[#D8E5FA] bg-[#EDF3FF] px-3 dark:border-border dark:bg-surface">
        <div className="flex items-center gap-1.5">
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('nav.home')}
            className="size-8 rounded-md bg-[#DCE7F9] hover:bg-[#CEDDF6] dark:bg-surface-muted"
            onClick={() => navigate('/')}
          >
            <HomeIcon aria-hidden />
          </Button>
          <span className="mx-0.5 h-4 w-px bg-[#C5D5EE] dark:bg-border" aria-hidden />
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('common.back')}
            className="size-8 rounded-md bg-[#DCE7F9] hover:bg-[#CEDDF6] dark:bg-surface-muted"
            onClick={() => navigate(-1)}
          >
            <ArrowLeftIcon aria-hidden />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('nav.upOneLevel')}
            className="size-8 rounded-md bg-[#DCE7F9] hover:bg-[#CEDDF6] dark:bg-surface-muted"
            onClick={() => filter.setTreeNode(null)}
            disabled={filter.treeMode !== 'path' || !filter.treeNodeId}
          >
            <ArrowUpIcon aria-hidden />
          </Button>
        </div>

        {/* Breadcrumb — "Search results for <q>" while searching (atlas §8).
            Carries the surface's level-one heading semantics (grid #1/#26). */}
        <div
          role="heading"
          aria-level={1}
          className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden px-2 py-1 text-xs"
        >
          {filter.searchApplied ? (
            <>
              <SearchIcon className="size-3.5 shrink-0 text-foreground-secondary" aria-hidden />
              <span className="truncate font-medium">
                {t('results.searchResultsFor', { query: filter.searchApplied })}
              </span>
              <button
                type="button"
                className="ml-auto rounded-full p-0.5 text-foreground-secondary transition-colors hover:bg-surface-hover hover:text-foreground active:bg-surface-tint focus-visible:ring-2 focus-visible:ring-ring"
                aria-label={t('results.clearFilter')}
                onClick={() => {
                  filter.setSearch('');
                  filter.commitSearch();
                  setSearchDraft('');
                }}
              >
                <XIcon className="size-3.5" aria-hidden />
              </button>
            </>
          ) : (
            <>
              <FolderTreeIcon className="size-4 shrink-0 text-[#1677FF]" aria-hidden />
              <span className="truncate font-medium">
                {filter.treeMode === 'path'
                  ? (rootTree.data?.nodes.find((n) => n.id === filter.treeNodeId)?.label ??
                    treeModeLabel[filter.treeMode])
                  : treeModeLabel[filter.treeMode]}
              </span>
              <ChevronRightIcon
                className="size-3.5 shrink-0 text-foreground-disabled"
                aria-hidden
              />
              <span className="truncate text-foreground-secondary tabular">
                {t('results.countOnly', {
                  count: formatCountCompact(total, i18n.language),
                })}
              </span>
            </>
          )}
        </div>

        <Segmented
          ariaLabel={t('results.views.details')}
          value={ui.viewMode}
          onChange={(v) => ui.setViewMode(v)}
          options={[
            {
              value: 'details',
              label: '',
              icon: ListIcon,
              srLabel: t('results.views.details'),
            },
            {
              value: 'icons',
              label: '',
              icon: LayoutGridIcon,
              srLabel: t('results.views.icons'),
            },
          ]}
        />
        <Button
          variant={filterBarOpen ? 'primary' : 'secondary'}
          size="sm"
          className={
            filterBarOpen
              ? 'bg-[#D0E2FF] [background-image:none] text-[#0958D9] shadow-none hover:bg-[#B8D0FA] dark:bg-primary/25 dark:text-primary dark:hover:bg-primary/35'
              : 'border-transparent bg-[#DCE7F9] hover:bg-[#C8DAF5] dark:bg-surface-muted dark:hover:bg-surface-hover'
          }
          onClick={() => setFilterBarOpen((v) => !v)}
          aria-expanded={filterBarOpen}
        >
          <FilterIcon aria-hidden />
          {t('results.filter')}
          {filter.hasActiveFilters() ? (
            <Badge variant="neutral" className="ml-1 px-1.5">
              •
            </Badge>
          ) : null}
        </Button>
        <div className="relative w-52">
          <Input
            className="h-8 border-transparent bg-[#DCE7F9]/90 pr-8 pl-3 text-xs dark:bg-surface-muted"
            placeholder={t('results.searchPlaceholder')}
            value={searchDraft}
            onChange={(e) => setSearchDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                filter.setSearch(searchDraft);
                filter.commitSearch();
              }
            }}
            aria-label={t('results.searchPlaceholder')}
          />
          <SearchIcon
            className="pointer-events-none absolute top-1/2 right-2.5 size-3.5 -translate-y-1/2 text-foreground-secondary"
            aria-hidden
          />
        </div>
      </div>

      {/* Filter bar */}
      {filterBarOpen ? <FilterBar /> : null}

      {/* 3-pane */}
      <div className="flex min-h-0 flex-1 overflow-hidden">
        {/* Left tree */}
        <aside
          className="h-full w-60 shrink-0 overflow-y-auto border-r border-border bg-[#F5F8FF] dark:bg-surface"
          aria-label={t('results.pathTab')}
        >
          <div className="p-2.5">
            <Segmented
              ariaLabel={t('results.pathTab')}
              value={filter.treeMode === 'raw' ? 'type' : filter.treeMode}
              onChange={(v: ResultTreeMode) => {
                setActiveNodeId(null);
                filter.setTreeMode(v);
              }}
              className="w-full"
              options={[
                { value: 'path', label: t('results.pathTab') },
                { value: 'type', label: t('results.typeTab') },
              ]}
            />
          </div>
          <ResultsTree
            scanId={scanId}
            mode={filter.treeMode}
            selectedId={filter.treeNodeId}
            onSelect={(nodeId) => {
              setActiveNodeId(null);
              filter.setTreeNode(nodeId);
            }}
          />
        </aside>

        {/* Center list */}
        <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <ContextMenu>
            <ContextMenuTrigger asChild>
              <div className="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
                {pages.isLoading ? (
                  <div className="space-y-px p-2">
                    {Array.from({ length: 14 }, (_, i) => (
                      <Skeleton key={i} className="h-8" />
                    ))}
                  </div>
                ) : rows.length === 0 ? (
                  <EmptyState
                    icon={filter.hasActiveFilters() ? FilterIcon : FolderTreeIcon}
                    title={
                      filter.hasActiveFilters()
                        ? t('results.emptyFilterTitle')
                        : t('results.emptyTitle')
                    }
                    description={
                      filter.hasActiveFilters()
                        ? t('results.emptyFilterBody')
                        : t('results.emptyBody')
                    }
                    actionLabel={
                      filter.hasActiveFilters() ? t('results.clearFilter') : undefined
                    }
                    onAction={filter.hasActiveFilters() ? () => filter.reset() : undefined}
                  />
                ) : (
                  <VirtualFileList
                    scanId={scanId}
                    rows={rows}
                    total={total}
                    rowHeight={rowHeight}
                    viewMode={ui.viewMode}
                    selectedIds={selection.ids}
                    exclusions={selection.exclusions}
                    selectionMode={selection.mode}
                    activeId={activeNodeId}
                    onRowClick={onRowClick}
                    onRowDoubleClick={(node) => setActiveNodeId(node.id)}
                    onCheckboxChange={(node) => selection.toggle(node.id)}
                    onClearActive={() => setActiveNodeId(null)}
                    onSelectAll={(checked) => {
                      if (!checked) {
                        selection.clear();
                        return;
                      }
                      // Corpus select-all is tri-state over the FULL filtered
                      // set (CSelAllCheckBox) — loaded rows are just the view.
                      if ((countQuery.data ?? total) > rows.length) {
                        selection.setAll(filterPayload, countQuery.data ?? total);
                        toast(
                          t('results.selectedAll', {
                            count: formatCountCompact(countQuery.data ?? total, i18n.language),
                          }),
                        );
                      } else {
                        selection.setOnly(rows.map((r) => r.id));
                      }
                    }}
                    allSelected={allRowsSelected}
                    someSelected={someRowsSelected}
                    sort={sort}
                    onSortChange={(field) => sortStore.setSort(field)}
                    loadMore={() => void pages.fetchNextPage()}
                    hasNextPage={pages.hasNextPage}
                  />
                )}
              </div>
            </ContextMenuTrigger>
            <ContextMenuContent>
              <ContextMenuItem onClick={() => onRecover()}>
                <UploadIcon aria-hidden />
                {t('results.context.recover')}
              </ContextMenuItem>
              <ContextMenuItem
                onClick={() => {
                  const single =
                    selection.mode === 'ids' && selection.ids.size === 1
                      ? [...selection.ids][0]
                      : null;
                  const target = single ?? activeNodeId ?? rows[0]?.id ?? null;
                  if (target) setActiveNodeId(target);
                }}
              >
                <SearchIcon aria-hidden />
                {t('results.context.preview')}
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                disabled={connectorMode === 'mock'}
                title={
                  connectorMode === 'mock' ? t('results.context.openLocationMock') : undefined
                }
              >
                <FolderTreeIcon aria-hidden />
                {t('results.context.openLocation')}
              </ContextMenuItem>
              <ContextMenuItem
                onClick={() =>
                  selection.setOnly(
                    rows.filter((r) => r.origin !== 'existing').map((r) => r.id),
                  )
                }
              >
                <Trash2Icon aria-hidden />
                {t('results.context.selectUnrecovered')}
              </ContextMenuItem>
              <ContextMenuItem onClick={() => onSmartSelect()} disabled={smartSelecting}>
                <SparklesIcon aria-hidden />
                {t('results.context.smartSelect')}
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                onClick={() => {
                  selection.clear();
                  setActiveNodeId(null);
                }}
              >
                <XIcon aria-hidden />
                {t('results.context.clearSelection')}
              </ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>
        </div>

        {/* Right preview — opens ONLY when a file row is clicked/selected, and closes when clicking outside */}
        {activeNodeId ? (
          <aside
            className="relative h-full shrink-0 overflow-hidden border-l border-border bg-surface"
            style={{ width: ui.previewPaneWidth }}
            aria-label={t('preview.title')}
            onClick={(e) => e.stopPropagation()}
          >
            <PreviewPane
              scanId={scanId}
              nodeId={activeNodeId}
              onClose={() => setActiveNodeId(null)}
            />
            <PreviewPaneResizer />
          </aside>
        ) : null}
      </div>

      {/* Bottom status bar — rendered here only when NOT in liveMode
          (during liveMode, ScanProgressPage renders the single unified scan footer
          so there is never a double-stacked bottom bar; after scan completion,
          this renders the reference "Scan Completed, Files Found: N (size)" bar
          from complete-celebration-modal.png and preview-panel-live.png). */}
      {!liveMode ? (
        <div className="flex h-[68px] shrink-0 items-center justify-between gap-4 border-t border-[#C8DAF5] bg-[#E2ECFA] px-4 dark:border-border dark:bg-surface">
          <div className="flex min-w-0 items-center gap-3.5 text-sm">
            <ProgressGauge value={1} completed size={48} label={t('a11y.gauge')} />
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-1.5 font-semibold text-foreground">
                <span>{t('scan.completedStatus')},</span>
                <span className="tabular text-[#1677FF] dark:text-primary">
                  {t('results.status.found', {
                    count: total.toLocaleString(i18n.language),
                    size: formatBytes(totalOf(pages.data?.pages), i18n.language),
                  })}
                </span>
              </div>
              {selectedCount > 0 ? (
                <div className="mt-0.5 text-xs tabular text-foreground-secondary">
                  {`${t('results.selection.count', { count: selectedCount })} · ${formatBytes(selectedBytes, i18n.language)}`}
                </div>
              ) : null}
            </div>
          </div>
          <div className="flex items-center gap-2.5">
            <Button
              variant="ghost"
              size="sm"
              onClick={() =>
                saveSession.mutate(null, {
                  onSuccess: () => toast.success(t('results.session.saved')),
                })
              }
            >
              <SaveIcon aria-hidden />
              {t('results.session.save')}
            </Button>
            <Button
              variant="primary"
              size="lg"
              className="min-w-[164px] [background-image:none] bg-[#1677FF] font-semibold text-white shadow-none hover:bg-[#0958D9]"
              onClick={() => onRecover()}
              disabled={!recoverableDuringScan}
              title={!recoverableDuringScan ? t('scan.recoverNowUnsafe') : undefined}
            >
              {selectedCount > 0
                ? t('results.status.recoverSelected', { count: selectedCount })
                : t('results.status.recoverAll')}
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function totalOf(pagesData: readonly { totalBytes: number }[] | undefined): number {
  if (!pagesData || pagesData.length === 0) return 0;
  return pagesData[pagesData.length - 1].totalBytes;
}

/** Drag handle that resizes the preview pane (persisted width, corpus splitter). */
function PreviewPaneResizer(): React.JSX.Element {
  const { t } = useTranslation();
  const width = useUiStore((s) => s.previewPaneWidth);
  const setWidth = useUiStore((s) => s.setPreviewPaneWidth);
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>): void => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = width;
    const onMove = (ev: PointerEvent): void => {
      setWidth(startWidth - (ev.clientX - startX));
    };
    const onUp = (): void => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  };
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      tabIndex={0}
      aria-label={t('a11y.resizePreview')}
      aria-valuenow={width}
      aria-valuemin={240}
      aria-valuemax={560}
      onPointerDown={onPointerDown}
      onKeyDown={(e) => {
        if (e.key === 'ArrowLeft') setWidth(width - 24);
        if (e.key === 'ArrowRight') setWidth(width + 24);
      }}
      className="absolute inset-y-0 left-0 z-10 w-1.5 -translate-x-1/2 cursor-col-resize bg-transparent transition-colors hover:bg-primary/40 focus-visible:bg-primary/40"
    />
  );
}
