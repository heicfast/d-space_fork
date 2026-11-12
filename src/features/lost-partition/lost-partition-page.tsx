import * as React from 'react';
import { useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';
import { HardDriveIcon, HomeIcon, InfoIcon, RefreshCcwIcon, SearchIcon } from 'lucide-react';
import { useDiskLayouts } from '@/queries/system';
import { useStartScan } from '@/queries/scan';
import { formatBytes } from '@/lib/format';
import { EmptyState } from '@/components/ui/empty-state';
import { Skeleton } from '@/components/ui/skeleton';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { toast } from 'sonner';
import { cn } from '@/lib/utils';

/**
 * Lost Partition Rescue — REAL disk layouts from the engine (GPT/MBR
 * parsing + unallocated-space boot-signature probing). Each lost volume
 * found in unallocated space is scannable in place: the MFT engine runs
 * over the lost partition through an offset reader (see the engine's
 * partitions module). No canned fixtures.
 */
export function LostPartitionPage(): React.JSX.Element {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const layouts = useDiskLayouts();
  const startScan = useStartScan();

  const list = layouts.data ?? [];
  const anyLost = list.some((d) => d.lostPartitions.length > 0);

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto px-8 py-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-[20px] font-bold tracking-tight text-[#1E293B] dark:text-foreground">
            {t('lostPartition.title')}
          </h1>
          <p className="mt-1 text-[13px] text-[#64748B] dark:text-foreground-secondary">
            {t('lostPartition.subtitle')}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void layouts.refetch()}
            aria-label={t('home.refreshDevices')}
          >
            <RefreshCcwIcon aria-hidden />
          </Button>
          <Button variant="secondary" size="sm" onClick={() => void navigate('/')}>
            <HomeIcon aria-hidden />
            {t('lostPartition.goToHardwareDisk')}
          </Button>
        </div>
      </div>

      <div className="mt-5 space-y-4">
        {layouts.isLoading ? (
          <>
            <Skeleton className="h-32" />
            <Skeleton className="h-32" />
          </>
        ) : layouts.isError ? (
          <EmptyState
            icon={InfoIcon}
            title={t('health.notAvailable')}
            description={layouts.error instanceof Error ? layouts.error.message : ''}
          />
        ) : list.length === 0 ? (
          <EmptyState
            icon={HardDriveIcon}
            title={t('home.emptyTitle')}
            description={t('home.emptyBody')}
            actionLabel={t('home.refreshDevices')}
            onAction={() => void layouts.refetch()}
          />
        ) : (
          <>
            {!anyLost ? (
              <p className="flex items-center gap-2 rounded-[10px] border border-border bg-white px-4 py-3 text-sm text-foreground-secondary dark:bg-surface">
                <InfoIcon className="size-4 shrink-0" aria-hidden />
                {t('lostPartition.needRecoverFiles')}
              </p>
            ) : (
              <p className="flex items-center gap-2 rounded-[10px] border border-[#B8D0FA] bg-[#EBF2FF] px-4 py-3 text-sm text-[#0958D9] dark:border-primary/40 dark:bg-primary/10 dark:text-primary">
                <SearchIcon className="size-4 shrink-0" aria-hidden />
                {t('lostPartition.foundPartitions')}
              </p>
            )}

            {list.map((disk) => (
              <DiskLayoutCard
                key={disk.diskNo}
                diskNo={disk.diskNo}
                sizeBytes={disk.sizeBytes}
                style={disk.style}
                partitions={disk.partitions}
                lostPartitions={disk.lostPartitions}
                onScanLost={(targetId) =>
                  startScan.mutate(
                    { targetId },
                    {
                      onSuccess: ({ scanId }) => {
                        toast.success(
                          t('lostPartition.scanCompletedFor', { disk: `Disk ${disk.diskNo}` }),
                        );
                        void navigate(`/scan/${targetId}/live?scanId=${scanId}`);
                      },
                      onError: (err) => toast.error(err.message),
                    },
                  )
                }
                scanning={
                  startScan.isPending && startScan.variables?.targetId.startsWith('lp:')
                }
              />
            ))}
          </>
        )}
      </div>
    </div>
  );
}

function DiskLayoutCard({
  diskNo,
  sizeBytes,
  style,
  partitions,
  lostPartitions,
  onScanLost,
  scanning,
}: {
  diskNo: number;
  sizeBytes: number;
  style: 'gpt' | 'mbr' | 'unknown';
  partitions: readonly {
    index: number;
    kind: string;
    startByte: number;
    sizeBytes: number;
    label: string | null;
  }[];
  lostPartitions: readonly {
    id: string;
    diskNo: number;
    startByte: number;
    sizeBytes: number;
    probableFs: string;
    note: string | null;
  }[];
  onScanLost: (targetId: string) => void;
  scanning: boolean;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const total = Math.max(sizeBytes, 1);
  const allocated = partitions.reduce((acc, p) => acc + p.sizeBytes, 0);
  const unallocated = Math.max(0, sizeBytes - allocated);

  return (
    <section className="rounded-[10px] border border-[#DCE8FF] bg-white p-5 dark:border-border dark:bg-surface">
      <header className="flex flex-wrap items-center gap-3">
        <HardDriveIcon className="size-5 text-[#1677FF]" aria-hidden />
        <h2 className="text-base font-semibold">Disk {diskNo}</h2>
        <span className="text-sm text-foreground-secondary tabular">
          {formatBytes(sizeBytes, i18n.language)}
        </span>
        <Badge variant="neutral" className="uppercase">
          {style}
        </Badge>
      </header>

      {/* Segmented partition bar — allocated segments + unallocated remainder */}
      <div
        className="mt-4 flex h-9 w-full overflow-hidden rounded-[6px] border border-border"
        role="img"
        aria-label={`${t('lostPartition.diskList')} — Disk ${diskNo}`}
      >
        {partitions.map((p) => (
          <div
            key={p.index}
            className="flex min-w-0 items-center justify-center border-r border-white/60 bg-[#1677FF]/80 px-1 last:border-r-0"
            style={{ width: `${Math.max((p.sizeBytes / total) * 100, 0.5)}%` }}
            title={`${p.label ?? p.kind} — ${formatBytes(p.sizeBytes, i18n.language)}`}
          >
            <span className="truncate text-[10px] font-medium text-white">
              {p.label ?? p.kind}
            </span>
          </div>
        ))}
        {unallocated > 0 ? (
          <div
            className="flex items-center justify-center bg-[repeating-linear-gradient(45deg,#E2E8F0_0,#E2E8F0_6px,#F1F5F9_6px,#F1F5F9_12px)] dark:bg-[repeating-linear-gradient(45deg,#1e242c_0,#1e242c_6px,#262d37_6px,#262d37_12px)]"
            style={{ width: `${(unallocated / total) * 100}%` }}
            title={`${formatBytes(unallocated, i18n.language)} unallocated`}
          />
        ) : null}
      </div>

      {lostPartitions.length === 0 ? null : (
        <div className="mt-4">
          <h3 className="text-sm font-semibold">{t('lostPartition.foundPartitions')}</h3>
          <ul className="mt-2 space-y-2">
            {lostPartitions.map((lp) => (
              <li
                key={lp.id}
                className={cn(
                  'flex flex-wrap items-center gap-3 rounded-[8px] border border-[#F59E0B]/40 bg-[#FFFBEB] px-3 py-2.5',
                  'dark:border-amber-500/30 dark:bg-amber-500/10',
                )}
              >
                <Badge
                  variant="neutral"
                  className="bg-amber-100 uppercase text-amber-700 dark:bg-amber-500/20 dark:text-amber-300"
                >
                  {lp.probableFs}
                </Badge>
                <span className="text-xs tabular">
                  @ {(lp.startByte / 1024 ** 3).toFixed(1)} GB ·{' '}
                  {formatBytes(lp.sizeBytes, i18n.language)}
                </span>
                {lp.note ? (
                  <span className="min-w-0 flex-1 truncate text-xs text-foreground-secondary">
                    {lp.note}
                  </span>
                ) : null}
                <Button
                  variant="primary"
                  size="sm"
                  disabled={scanning}
                  onClick={() => onScanLost(lp.id)}
                >
                  <SearchIcon aria-hidden />
                  {t('lostPartition.tryDataRecovery')}
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
