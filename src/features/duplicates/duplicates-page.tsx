import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { useNavigate } from 'react-router';
import { ChevronDownIcon, ChevronRightIcon, CopyIcon, SearchIcon } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { EmptyState, StatTile } from '@/components/ui/empty-state';
import { formatBytes } from '@/lib/format';
import { useFindDuplicates, useStagePath } from '@/queries/cleanup';
import { useSessions } from '@/queries/sessions';
import type { DuplicatesResult } from '@/connector/contract';

/**
 * Duplicate Finder — BLAKE3-verified duplicate groups over the most
 * recent scan (D-Space feature in the DiskWizard design language). The
 * engine hashes within the scan results; "stage all but the newest"
 * feeds the Cleanup Bin with the redundant copies.
 */
export function DuplicatesPage(): React.JSX.Element {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const sessions = useSessions();
  const find = useFindDuplicates();
  const stagePath = useStagePath();
  const [minKb, setMinKb] = React.useState(4);
  const [sizeOnly, setSizeOnly] = React.useState(false);
  const [result, setResult] = React.useState<DuplicatesResult | null>(null);

  // Session rows carry the originating scan id — the newest session is
  // the most recent completed scan.
  const scanId = sessions.data?.[0]?.id ?? null;

  const run = (): void => {
    if (!scanId) return;
    find.mutate(
      {
        scanId,
        minSizeBytes: Math.max(0, minKb) * 1024,
        sizeOnly,
      },
      { onSuccess: (res) => setResult(res), onError: (err) => toast.error(err.message) },
    );
  };

  const stageRedundant = (paths: readonly string[], newestPath: string): void => {
    const redundant = paths.filter((p) => p !== newestPath);
    for (const path of redundant) {
      stagePath.mutate(path, {
        onSuccess: () => toast.success(t('cleanup.stagedToast')),
        onError: (err) => toast.error(err.message),
      });
    }
  };

  /** Newest copy by the engine-reported modification time (the payload
   *  carries mtimes parallel to paths; the old code just kept the first
   *  path and called it the newest). */
  const newestOf = (paths: readonly string[], modified: readonly number[]): string => {
    if (paths.length === 0) return '';
    let best = 0;
    for (let i = 1; i < paths.length; i++) {
      if ((modified[i] ?? 0) > (modified[best] ?? 0)) best = i;
    }
    return paths[best];
  };

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto">
      <header className="px-8 pt-7 pb-4">
        <h1 className="text-xl font-semibold tracking-tight">{t('duplicates.title')}</h1>
        <p className="mt-1 text-sm text-foreground-secondary">{t('duplicates.subtitle')}</p>
      </header>

      <div className="mx-8 mb-8 space-y-4">
        {scanId === null && (sessions.data ?? []).length === 0 ? (
          <EmptyState
            icon={SearchIcon}
            title={t('duplicates.needsScan')}
            description={t('home.subtitle')}
            actionLabel={t('backup.startSearch')}
            onAction={() => void navigate('/')}
          />
        ) : (
          <>
            <div className="flex flex-wrap items-end gap-4 rounded-[10px] border border-border bg-white p-4 dark:bg-surface">
              <div className="w-36 space-y-1.5">
                <Label htmlFor="dup-minkb">{t('duplicates.minSize')}</Label>
                <Input
                  id="dup-minkb"
                  type="number"
                  min={0}
                  value={minKb}
                  onChange={(e) => setMinKb(Number(entegerOf(e.target.value)))}
                />
              </div>
              <div className="flex items-center gap-2 pb-1.5">
                <Checkbox
                  id="dup-sizeonly"
                  checked={sizeOnly}
                  onCheckedChange={(v) => setSizeOnly(v === true)}
                  aria-label={t('duplicates.sizeOnly')}
                />
                <Label htmlFor="dup-sizeonly" className="font-normal">
                  {t('duplicates.sizeOnly')}
                </Label>
              </div>
              <Button
                variant="primary"
                onClick={run}
                loading={find.isPending}
                className="ml-auto"
              >
                <CopyIcon aria-hidden />
                {find.isPending ? t('duplicates.busy') : t('duplicates.scan')}
              </Button>
            </div>

            {result ? (
              <DuplicatesResultCard
                result={result}
                onStageRedundant={stageRedundant}
                newestOf={newestOf}
              />
            ) : null}
          </>
        )}
      </div>
    </div>
  );
}

function entegerOf(v: string): number {
  const n = Number.parseInt(v, 10);
  return Number.isFinite(n) && n >= 0 ? n : 0;
}

function DuplicatesResultCard({
  result,
  onStageRedundant,
  newestOf,
}: {
  result: DuplicatesResult;
  onStageRedundant: (paths: readonly string[], newestPath: string) => void;
  newestOf: (paths: readonly string[], modified: readonly number[]) => string;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const [openGroups, setOpenGroups] = React.useState<ReadonlySet<number>>(new Set());
  const toggle = (index: number): void => {
    setOpenGroups((prev) => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  };

  return (
    <>
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <StatTile
          label={t('duplicates.scannedFiles')}
          value={result.scannedFiles.toLocaleString()}
        />
        <StatTile
          label={t('duplicates.filteredSmall')}
          value={result.filteredSmall.toLocaleString()}
        />
        <StatTile
          label={t('duplicates.candidatePairs')}
          value={result.candidatePairs.toLocaleString()}
        />
        <StatTile
          label={t('duplicates.hashErrors')}
          value={result.hashErrors.toLocaleString()}
        />
      </div>

      <div className="rounded-[10px] border border-border bg-white dark:bg-surface">
        <div className="flex items-center justify-between border-b border-border px-4 py-2.5">
          <h2 className="text-sm font-semibold">
            {t('duplicates.groupCount')} · {result.groups.length}
            {result.stats.groupCount > result.groups.length ? (
              <span className="ml-2 font-normal text-foreground-secondary">
                {t('duplicates.shownOf', {
                  shown: result.groups.length,
                  total: result.stats.groupCount,
                })}
              </span>
            ) : null}
          </h2>
          <span className="text-sm font-semibold text-[#1677FF] tabular dark:text-primary">
            {t('duplicates.reclaimable', {
              bytes: formatBytes(result.stats.redundantBytes, i18n.language),
            })}
          </span>
        </div>
        {result.groups.length === 0 ? (
          <p className="px-4 py-8 text-center text-sm text-foreground-secondary">
            {t('duplicates.empty')}
          </p>
        ) : (
          <ul className="divide-y divide-border">
            {result.groups.map((group, index) => {
              const open = openGroups.has(index);
              return (
                <li key={`${group.hashHex}-${index}`}>
                  <div className="flex w-full items-center gap-3 px-4 py-2.5">
                    <button
                      type="button"
                      onClick={() => toggle(index)}
                      aria-expanded={open}
                      className="flex min-w-0 flex-1 items-center gap-2 text-left"
                    >
                      {open ? (
                        <ChevronDownIcon
                          className="size-4 shrink-0 text-foreground-secondary"
                          aria-hidden
                        />
                      ) : (
                        <ChevronRightIcon
                          className="size-4 shrink-0 text-foreground-secondary"
                          aria-hidden
                        />
                      )}
                      <span className="shrink-0 font-mono text-[11px] text-foreground-secondary">
                        {shortHash(group.hashHex)}
                      </span>
                      <span className="text-xs tabular">
                        {t('duplicates.groupPaths', {
                          count: group.paths.length,
                          size: formatBytes(group.sizeBytes, i18n.language),
                        })}
                      </span>
                      <span className="ml-auto shrink-0 text-xs font-semibold text-[#1677FF] tabular dark:text-primary">
                        +
                        {formatBytes(
                          group.sizeBytes * Math.max(0, group.paths.length - 1),
                          i18n.language,
                        )}
                      </span>
                    </button>
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() =>
                        onStageRedundant(
                          group.paths,
                          newestOf(group.paths, group.modifiedUnix ?? []),
                        )
                      }
                    >
                      {t('duplicates.stageButNewest')}
                    </Button>
                  </div>
                  {open ? (
                    <ul className="space-y-1 bg-[#F5F8FF] px-4 py-2 dark:bg-surface-muted/40">
                      {group.paths.map((path) => (
                        <li
                          key={path}
                          className="break-all font-mono text-[11px] text-foreground-secondary"
                        >
                          {path}
                        </li>
                      ))}
                    </ul>
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </>
  );
}

function shortHash(hash: string): string {
  if (hash.length <= 14) return hash;
  return `${hash.slice(0, 8)}…${hash.slice(-4)}`;
}
