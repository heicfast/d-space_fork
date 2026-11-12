import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { ArrowDownRightIcon, ArrowUpRightIcon, CameraIcon, SearchIcon } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { Badge } from '@/components/ui/badge';
import { formatBytes, formatDateTime } from '@/lib/format';
import { useCaptureSnapshot, useSnapshotDelta, useSnapshots } from '@/queries/cleanup';
import { useSessions } from '@/queries/sessions';
import { cn } from '@/lib/utils';

/**
 * Size Snapshots — the D-Space time dimension: capture directory-size
 * snapshots from a scan, select two, see what grew (top-10 per bucket,
 * largest first — the engine computes the delta server-side).
 */
export function SnapshotsPage(): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const sessions = useSessions();
  const snapshots = useSnapshots();
  const capture = useCaptureSnapshot();
  const [fromId, setFromId] = React.useState<number | null>(null);
  const [toId, setToId] = React.useState<number | null>(null);

  const scanId = sessions.data?.[0]?.id ?? null;
  const delta = useSnapshotDelta(fromId, toId);

  const select = (id: number): void => {
    if (fromId === null) {
      setFromId(id);
      setToId(null);
      return;
    }
    if (toId === null) {
      if (id === fromId) {
        setFromId(null);
        return;
      }
      setToId(id);
      return;
    }
    // Third click shifts the window.
    setFromId(toId);
    setToId(id);
  };

  const list = snapshots.data ?? [];

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto">
      <header className="flex flex-wrap items-start justify-between gap-3 px-8 pt-7 pb-4">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">{t('snapshotsPage.title')}</h1>
          <p className="mt-1 text-sm text-foreground-secondary">
            {t('snapshotsPage.subtitle')}
          </p>
        </div>
        {scanId !== null ? (
          <Button
            variant="primary"
            loading={capture.isPending}
            onClick={() => capture.mutate(scanId)}
          >
            <CameraIcon aria-hidden />
            {capture.isPending ? t('snapshotsPage.capturing') : t('snapshotsPage.capture')}
          </Button>
        ) : null}
      </header>

      <div className="mx-8 mb-8 space-y-4">
        {scanId === null && list.length === 0 ? (
          <EmptyState
            icon={SearchIcon}
            title={t('snapshotsPage.needsScan')}
            description={t('home.subtitle')}
            actionLabel={t('backup.startSearch')}
            onAction={() => void navigate('/')}
          />
        ) : list.length === 0 ? (
          <EmptyState
            icon={CameraIcon}
            title={t('snapshotsPage.empty')}
            description={t('snapshotsPage.subtitle')}
          />
        ) : (
          <>
            <section className="rounded-[10px] border border-border bg-white dark:bg-surface">
              <h2 className="border-b border-border px-4 py-2.5 text-sm font-semibold">
                {t('snapshotsPage.listTitle')}
                <span className="ml-2 font-normal text-foreground-secondary">
                  {fromId !== null && toId !== null ? null : t('snapshotsPage.selectTwo')}
                </span>
              </h2>
              <ul className="divide-y divide-border">
                {list.map((snap) => {
                  const isFrom = snap.id === fromId;
                  const isTo = snap.id === toId;
                  return (
                    <li key={snap.id}>
                      <button
                        type="button"
                        onClick={() => select(snap.id)}
                        aria-pressed={isFrom || isTo}
                        className={cn(
                          'flex w-full items-center gap-3 px-4 py-2.5 text-left transition-colors',
                          isFrom && 'bg-[#EBF2FF] ring-1 ring-[#1677FF]/40 dark:bg-primary/10',
                          isTo &&
                            'bg-emerald-50 ring-1 ring-emerald-400/40 dark:bg-emerald-500/10',
                          !isFrom &&
                            !isTo &&
                            'hover:bg-[#F5F8FF] dark:hover:bg-surface-hover/50',
                        )}
                      >
                        <span className="text-xs tabular">
                          {formatDateTime(snap.capturedAt, i18n.language)}
                        </span>
                        <span className="text-xs text-foreground-secondary tabular">
                          {snap.entryCount.toLocaleString()} ·{' '}
                          {formatBytes(snap.totalSizeBytes, i18n.language)}
                        </span>
                        {isFrom ? (
                          <Badge
                            variant="neutral"
                            className="bg-[#D2E3FC] text-[#0958D9] dark:bg-primary/25 dark:text-primary"
                          >
                            {t('snapshotsPage.from')}
                          </Badge>
                        ) : null}
                        {isTo ? (
                          <Badge
                            variant="neutral"
                            className="bg-emerald-100 text-emerald-700 dark:bg-emerald-500/20 dark:text-emerald-300"
                          >
                            {t('snapshotsPage.to')}
                          </Badge>
                        ) : null}
                      </button>
                    </li>
                  );
                })}
              </ul>
            </section>

            {delta.data ? <SnapshotDeltaCard /> : null}
          </>
        )}
      </div>
    </div>
  );

  function SnapshotDeltaCard(): React.JSX.Element {
    const data = delta.data!;
    const net = data.netChangeBytes;
    return (
      <section className="rounded-[10px] border border-border bg-white p-4 dark:bg-surface">
        <div className="flex flex-wrap items-center gap-3">
          <span
            className={cn(
              'inline-flex items-center gap-1 rounded-full px-2.5 py-1 text-xs font-semibold tabular',
              net > 0
                ? 'bg-red-50 text-red-700 dark:bg-red-500/10 dark:text-red-400'
                : net < 0
                  ? 'bg-emerald-50 text-emerald-700 dark:bg-emerald-500/10 dark:text-emerald-400'
                  : 'bg-surface-muted text-foreground-secondary',
            )}
          >
            {net > 0 ? (
              <ArrowUpRightIcon className="size-3.5" aria-hidden />
            ) : net < 0 ? (
              <ArrowDownRightIcon className="size-3.5" aria-hidden />
            ) : null}
            {net > 0
              ? t('snapshotsPage.netGrew', { bytes: formatBytes(Math.abs(net), i18n.language) })
              : net < 0
                ? t('snapshotsPage.netShrunk', {
                    bytes: formatBytes(Math.abs(net), i18n.language),
                  })
                : t('snapshotsPage.netFlat')}
          </span>
          <span className="text-xs text-foreground-secondary tabular">
            {t('snapshotsPage.changedPaths', { count: data.totalChangedPaths })}
          </span>
          <span className="ml-auto text-[11px] text-foreground-disabled">
            {t('snapshotsPage.deltaHint')}
          </span>
        </div>
        <div className="mt-3 grid grid-cols-2 gap-3 lg:grid-cols-4">
          <DeltaList
            title={t('snapshotsPage.added')}
            items={data.added.map((a) => ({
              path: a.path,
              value: `+${formatBytes(a.sizeBytes, i18n.language)}`,
              positive: true,
            }))}
          />
          <DeltaList
            title={t('snapshotsPage.removed')}
            items={data.removed.map((r) => ({
              path: r.path,
              value: `−${formatBytes(r.sizeBytes, i18n.language)}`,
              positive: false,
            }))}
          />
          <DeltaList
            title={t('snapshotsPage.grew')}
            items={data.grew.map((g) => ({
              path: g.path,
              value: `+${formatBytes(g.deltaBytes, i18n.language)}`,
              positive: true,
            }))}
          />
          <DeltaList
            title={t('snapshotsPage.shrunk')}
            items={data.shrunk.map((g) => ({
              path: g.path,
              value: `−${formatBytes(Math.abs(g.deltaBytes), i18n.language)}`,
              positive: false,
            }))}
          />
        </div>
      </section>
    );
  }
}

function DeltaList({
  title,
  items,
}: {
  title: string;
  items: { path: string; value: string; positive: boolean }[];
}): React.JSX.Element {
  return (
    <div className="min-w-0 rounded-[8px] border border-border">
      <h3 className="border-b border-border px-3 py-1.5 text-xs font-semibold">{title}</h3>
      <ul className="max-h-48 space-y-1 overflow-y-auto px-3 py-2">
        {items.length === 0 ? (
          <li className="text-[11px] text-foreground-disabled">—</li>
        ) : (
          items.map((item) => (
            <li
              key={item.path}
              className="flex items-baseline justify-between gap-2 text-[11px]"
            >
              <span className="truncate font-mono" title={item.path}>
                {item.path}
              </span>
              <span
                className={cn(
                  'shrink-0 tabular',
                  item.positive
                    ? 'text-red-600 dark:text-red-400'
                    : 'text-emerald-600 dark:text-emerald-400',
                )}
              >
                {item.value}
              </span>
            </li>
          ))
        )}
      </ul>
    </div>
  );
}
