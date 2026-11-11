import * as React from 'react';
import { useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';
import {
  CheckIcon,
  HardDriveIcon,
  HomeIcon,
  InfoIcon,
  LockOpenIcon,
  RefreshCcwIcon,
  UsbIcon,
  XIcon,
} from 'lucide-react';
import { useDevices, useRefreshDevices } from '@/queries/devices';
import { useStartScan } from '@/queries/scan';
import { cn } from '@/lib/utils';
import { EmptyState } from '@/components/ui/empty-state';
import { Skeleton } from '@/components/ui/skeleton';
import { toast } from 'sonner';

interface RescueDiskSpec {
  id: string;
  targetDeviceId: string;
  shortName: string;
  headerTitle: string;
  isUsb?: boolean;
  previewLabel: string;
  previewSize: string;
  segments: readonly {
    label: string;
    size: string;
    bitlocker?: boolean;
    colSpanClass?: string;
  }[];
  foundPartitions: readonly {
    id: string;
    fs: string;
    label: string;
    usedSize: string;
    size: string;
    startLba: string;
    endLba: string;
    recoverability: string;
    status: string;
  }[];
}

const RESCUE_DISK_SPECS: readonly RescueDiskSpec[] = [
  {
    id: 'disk-2',
    targetDeviceId: 'dev-sd',
    shortName: 'Disk 2',
    headerTitle: 'Disk 2 (Basic, MBR, 58.31 GB, Generic- SD/MMC/MS PRO USB Device)',
    isUsb: true,
    previewLabel: '(Others)',
    previewSize: '58.31 GB',
    segments: [{ label: '(Others)', size: '58.31 GB', colSpanClass: 'col-span-4' }],
    foundPartitions: [
      {
        id: 'part-d2-1',
        fs: 'exFAT',
        label: 'SD/MMC/MS PRO',
        usedSize: '41.12 GB',
        size: '58.31 GB',
        startLba: '2048',
        endLba: '122288127',
        recoverability: 'High',
        status: 'Lost',
      },
    ],
  },
  {
    id: 'disk-0',
    targetDeviceId: 'dev-sata-1',
    shortName: 'Disk 0',
    headerTitle: 'Disk 0 (Basic, GPT, 476.94 GB, CONSISTENT M.2 S7 512GB)',
    previewLabel: 'D: SATA(NTFS)',
    previewSize: '476.92 GB',
    segments: [
      {
        label: 'D: SATA(NTFS)',
        size: '476.92 GB',
        bitlocker: true,
        colSpanClass: 'col-span-4',
      },
    ],
    foundPartitions: [
      {
        id: 'part-d0-1',
        fs: 'NTFS',
        label: 'D: SATA',
        usedSize: '214.50 GB',
        size: '476.92 GB',
        startLba: '34816',
        endLba: '1000214527',
        recoverability: 'High',
        status: 'Lost',
      },
    ],
  },
  {
    id: 'disk-1',
    targetDeviceId: 'dev-nvme-0',
    shortName: 'Disk 1',
    headerTitle: 'Disk 1 (Basic, GPT, 476.94 GB, AP512GPD4480-0G PD4480 512GB)',
    previewLabel: 'C: (NTFS)',
    previewSize: '475.83 GB',
    segments: [
      { label: '(FAT32)', size: '200 MB', colSpanClass: 'col-span-1' },
      { label: '(Others)', size: '16 MB', colSpanClass: 'col-span-1' },
      { label: 'C: (NTFS)', size: '475.83 GB', bitlocker: true, colSpanClass: 'col-span-1' },
      { label: '(NTFS)', size: '917 MB', colSpanClass: 'col-span-1' },
    ],
    foundPartitions: [
      {
        id: 'part-d1-1',
        fs: 'NTFS',
        label: 'Windows (C:)',
        usedSize: '360.86 GB',
        size: '475.83 GB',
        startLba: '444416',
        endLba: '998332415',
        recoverability: 'High',
        status: 'Existing',
      },
    ],
  },
];

/**
 * Lost Partition Rescue — 2-step direct flow (no Start landing screen):
 * - Step 1 ('select-disk'): "Choose a disk to start searching for lost partition(s)" with Disk 2, Disk 0, Disk 1 cards, segmented partition bars, BitLocker lock icons, "Go to Hardware Disk" link, and "Search for Lost Partition" CTA (Screenshot_2026-10-25_015251_c30d29c5.png)
 * - Step 2 ('scan-completed'): "Smart Partition Scan Completed", found partitions table with "Double click the partition to explore the content." banner, "Preview for disk 2" bar, and "Try Data Recovery" CTA (Screenshot_2026-10-25_015717_8af44478.png).
 */
export function LostPartitionPage(): React.JSX.Element {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const devicesQuery = useDevices();
  const refresh = useRefreshDevices();
  const startScan = useStartScan();

  const [step, setStep] = React.useState<'select-disk' | 'scan-completed'>('select-disk');
  const [selectedSpecId, setSelectedSpecId] = React.useState<string>('disk-2');
  const [showInfoTip, setShowInfoTip] = React.useState(true);

  const disks = (devicesQuery.data ?? []).filter(
    (d) => d.kind !== 'virtual' && d.canScanWholeDisk,
  );
  const selectedSpec =
    RESCUE_DISK_SPECS.find((s) => s.id === selectedSpecId) ?? RESCUE_DISK_SPECS[0];

  const beginDataRecovery = (targetDiskId?: string): void => {
    const resolvedId =
      targetDiskId ??
      disks.find((d) => d.id === selectedSpec.targetDeviceId)?.id ??
      disks[0]?.id ??
      'dev-sd';
    startScan.mutate(
      { targetId: resolvedId },
      {
        onSuccess: ({ scanId }) => {
          void navigate(`/scan/${resolvedId}/live?scanId=${scanId}`);
        },
        onError: (err) => {
          const reasonKey = (err as { reasonKey?: string }).reasonKey;
          toast.error(reasonKey ? t(reasonKey) : err.message);
        },
      },
    );
  };

  if (devicesQuery.isLoading) {
    return (
      <div className="p-8">
        <Skeleton className="mb-6 h-8 w-80" />
        <Skeleton className="h-64" />
      </div>
    );
  }

  if (devicesQuery.isError) {
    return (
      <div className="p-8">
        <EmptyState
          icon={HardDriveIcon}
          title={t('errors.title')}
          description={t('errors.deviceGone')}
          actionLabel={t('common.retry')}
          onAction={() => void devicesQuery.refetch()}
        />
      </div>
    );
  }

  /* Step 2: Smart Partition Scan Completed (matching Screenshot_2026-10-25_015717_8af44478.png) */
  if (step === 'scan-completed') {
    return (
      <div className="flex h-full min-h-0 flex-col justify-between overflow-y-auto px-8 py-5">
        <div>
          {/* Top-left Home Pill Button */}
          <div>
            <button
              type="button"
              onClick={() => setStep('select-disk')}
              className="inline-flex h-8 items-center gap-1.5 rounded-[6px] bg-[#DCE7F9] px-3.5 text-[13px] font-medium text-[#1E293B] transition-colors hover:bg-[#C8DAF5] focus-visible:ring-2 focus-visible:ring-ring dark:bg-surface-elevated dark:text-foreground dark:hover:bg-surface-hover"
            >
              <HomeIcon
                className="size-3.5 text-[#475569] dark:text-foreground-secondary"
                aria-hidden
              />
              <span>{t('nav.home')}</span>
            </button>
          </div>

          {/* Completion Banner with Blue Checkmark Circle */}
          <div className="mt-4 flex items-center gap-3.5">
            <div
              className="flex size-10 shrink-0 items-center justify-center rounded-full bg-[#1677FF] text-white shadow-xs"
              aria-hidden
            >
              <CheckIcon className="size-5" strokeWidth={2.75} />
            </div>
            <div>
              <h1 className="text-[18px] font-bold tracking-tight text-[#1E293B] dark:text-foreground">
                {t('lostPartition.scanCompletedTitle')}
              </h1>
              <p className="text-[12.5px] text-[#64748B] dark:text-foreground-secondary">
                {t('lostPartition.scanCompletedFor', { disk: selectedSpec.shortName })}
              </p>
            </div>
          </div>

          {/* Found Partitions Table */}
          <div className="mt-5">
            <div className="mb-2 text-[12.5px] font-medium text-[#64748B] dark:text-foreground-secondary">
              {t('lostPartition.foundPartitions')}
            </div>

            <div className="overflow-hidden rounded-[8px] border border-[#E2ECF9] bg-white dark:border-border dark:bg-surface-elevated">
              {/* Table Header */}
              <div className="grid grid-cols-8 border-b border-[#E2ECF9] bg-[#F8FAFF] px-3 py-2 text-[12px] font-medium text-[#64748B] dark:border-border dark:bg-surface dark:text-foreground-secondary">
                <div className="flex items-center gap-1.5">
                  <input
                    type="checkbox"
                    defaultChecked
                    aria-label={t('results.selection.selectAll')}
                    className="size-3.5 rounded-xs accent-[#1677FF]"
                  />
                  <span>{t('lostPartition.colFilesystem')}</span>
                </div>
                <div>{t('lostPartition.colLabel')}</div>
                <div>{t('lostPartition.colUsedSize')}</div>
                <div>{t('lostPartition.colSize')}</div>
                <div>{t('lostPartition.colStartLba')}</div>
                <div>{t('lostPartition.colEndLba')}</div>
                <div>{t('lostPartition.colRecoverability')}</div>
                <div>{t('lostPartition.colStatus')}</div>
              </div>

              {/* Blue Info Callout Strip (Screenshot_2026-10-25_015717_8af44478.png) */}
              {showInfoTip ? (
                <div className="flex items-center justify-between bg-[#D8E7FF] px-3.5 py-1.5 text-[12px] text-[#1E293B] dark:bg-primary/20 dark:text-foreground">
                  <div className="flex items-center gap-2">
                    <InfoIcon className="size-3.5 shrink-0 text-[#1677FF]" aria-hidden />
                    <span>{t('lostPartition.exploreHint')}</span>
                  </div>
                  <button
                    type="button"
                    onClick={() => setShowInfoTip(false)}
                    aria-label={t('common.close')}
                    className="rounded-xs p-0.5 text-[#64748B] hover:text-[#1E293B] dark:text-foreground-secondary dark:hover:text-foreground"
                  >
                    <XIcon className="size-3.5" aria-hidden />
                  </button>
                </div>
              ) : null}

              {/* Found Partition Rows */}
              <div className="min-h-[150px] divide-y divide-[#F1F5F9] dark:divide-border">
                {selectedSpec.foundPartitions.map((part) => (
                  <div
                    key={part.id}
                    onDoubleClick={() => beginDataRecovery()}
                    className="grid cursor-pointer grid-cols-8 items-center px-3 py-2.5 text-[12px] text-[#1E293B] transition-colors hover:bg-[#EFF6FF] dark:text-foreground dark:hover:bg-surface-hover"
                  >
                    <div className="flex items-center gap-1.5 font-medium">
                      <input
                        type="checkbox"
                        defaultChecked
                        aria-label={part.label}
                        onClick={(e) => e.stopPropagation()}
                        className="size-3.5 rounded-xs accent-[#1677FF]"
                      />
                      <span>{part.fs}</span>
                    </div>
                    <div className="truncate font-medium">{part.label}</div>
                    <div className="tabular text-[#64748B] dark:text-foreground-secondary">
                      {part.usedSize}
                    </div>
                    <div className="tabular text-[#64748B] dark:text-foreground-secondary">
                      {part.size}
                    </div>
                    <div className="tabular text-[#64748B] dark:text-foreground-secondary">
                      {part.startLba}
                    </div>
                    <div className="tabular text-[#64748B] dark:text-foreground-secondary">
                      {part.endLba}
                    </div>
                    <div>
                      <span className="rounded-full bg-[#DCFCE7] px-2 py-0.5 text-[11px] font-semibold text-[#15803D]">
                        {part.recoverability}
                      </span>
                    </div>
                    <div className="text-[#64748B] dark:text-foreground-secondary">
                      {part.status}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>

        {/* Bottom Section: "Preview for disk 2" bar + "Try Data Recovery" CTA */}
        <div className="mt-4 space-y-4">
          <div>
            <div className="mb-1.5 text-[12.5px] font-medium text-[#64748B] dark:text-foreground-secondary">
              {t('lostPartition.previewFor', {
                disk: selectedSpec.shortName.toLowerCase(),
              })}
            </div>
            <div className="rounded-[10px] border border-[#E2ECF9] bg-[#F4F7FC] p-2.5 dark:border-border dark:bg-surface-elevated">
              <div className="rounded-[6px] bg-white px-4 py-2.5 dark:bg-surface">
                <div className="text-[13px] font-bold text-[#1E293B] dark:text-foreground">
                  {selectedSpec.previewLabel}
                </div>
                <div className="mt-0.5 text-[11.5px] text-[#64748B] tabular dark:text-foreground-secondary">
                  {selectedSpec.previewSize}
                </div>
              </div>
            </div>
          </div>

          <div className="flex justify-end">
            <button
              type="button"
              disabled={startScan.isPending}
              onClick={() => beginDataRecovery()}
              className="inline-flex h-10 min-w-[180px] items-center justify-center rounded-[8px] bg-[#1677FF] px-6 text-[13.5px] font-semibold text-white shadow-xs transition-all hover:bg-[#0958D9] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 dark:bg-primary dark:text-primary-foreground dark:hover:bg-primary-hover"
            >
              {t('lostPartition.tryDataRecovery')}
            </button>
          </div>
        </div>
      </div>
    );
  }

  /* Step 1: Disk Selection (matching Screenshot_2026-10-25_015251_c30d29c5.png) */
  return (
    <div className="flex h-full min-h-0 flex-col justify-between overflow-y-auto px-8 py-6">
      <div>
        {/* Header with inline refresh icon */}
        <div className="flex items-center gap-2.5">
          <h1 className="text-[20px] font-bold tracking-tight text-[#1E293B] dark:text-foreground">
            {t('lostPartition.title')}
          </h1>
          <button
            type="button"
            onClick={() => void refresh.mutate()}
            disabled={refresh.isPending}
            aria-label={t('home.refreshDevices')}
            title={t('home.refreshDevices')}
            className="inline-flex size-7 items-center justify-center rounded-full text-[#1E293B] transition-colors hover:bg-[#E2E8F0]/70 hover:text-[#1677FF] dark:text-foreground dark:hover:bg-surface-hover"
          >
            <RefreshCcwIcon
              className={cn('size-4', refresh.isPending && 'animate-spin')}
              aria-hidden
            />
          </button>
        </div>
        <p className="mt-1 text-[13px] text-[#64748B] dark:text-foreground-secondary">
          {t('lostPartition.subtitle')}
        </p>

        {/* Disk Cards Stack (Disk 2, Disk 0, Disk 1) */}
        <div
          role="radiogroup"
          aria-label={t('lostPartition.diskList')}
          className="mt-5 space-y-3.5"
        >
          {RESCUE_DISK_SPECS.map((spec) => {
            const isSelected = spec.id === selectedSpec.id;
            return (
              <div
                key={spec.id}
                role="radio"
                aria-checked={isSelected}
                tabIndex={0}
                onClick={() => setSelectedSpecId(spec.id)}
                onDoubleClick={() => {
                  setSelectedSpecId(spec.id);
                  setStep('scan-completed');
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    setSelectedSpecId(spec.id);
                  }
                }}
                className={cn(
                  'cursor-pointer rounded-[10px] border-2 px-4 py-3 transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring',
                  isSelected
                    ? 'border-[#1677FF] bg-[#E8F1FF] dark:border-primary dark:bg-surface-selected'
                    : 'border-transparent bg-[#F4F7FC] hover:border-[#BFDBFE] hover:bg-[#EDF4FF] dark:bg-surface-elevated dark:hover:border-primary/50',
                )}
              >
                <div className="mb-2 flex items-center justify-between gap-2">
                  <span className="text-[12.5px] font-medium text-[#475569] dark:text-foreground-secondary">
                    {spec.headerTitle}
                  </span>
                  {spec.isUsb ? (
                    <UsbIcon
                      className="size-4 text-[#1677FF] dark:text-primary"
                      strokeWidth={1.75}
                      aria-hidden
                    />
                  ) : null}
                </div>

                {/* Partition Segment Bar(s) */}
                <div className="grid grid-cols-4 gap-1.5">
                  {spec.segments.map((seg, idx) => (
                    <div
                      key={idx}
                      className={cn(
                        'rounded-[6px] border border-[#E2ECF9] bg-white px-3.5 py-2.5 dark:border-border dark:bg-surface',
                        seg.colSpanClass ?? 'col-span-1',
                      )}
                    >
                      <div className="flex items-center gap-1.5 text-[12.5px] font-bold text-[#1E293B] dark:text-foreground">
                        <span className="truncate">{seg.label}</span>
                        {seg.bitlocker ? (
                          <span
                            className="inline-flex size-4 shrink-0 items-center justify-center rounded-xs bg-[#E2E8F0] text-[#1677FF]"
                            title="BitLocker"
                          >
                            <LockOpenIcon className="size-3" aria-hidden />
                          </span>
                        ) : null}
                      </div>
                      <div className="mt-0.5 text-[11.5px] text-[#64748B] tabular dark:text-foreground-secondary">
                        {seg.size}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Bottom Footer Bar: Left "Go to Hardware Disk" link + Right "Search for Lost Partition" CTA */}
      <div className="mt-5 flex flex-wrap items-center justify-between gap-4 pt-2">
        <div className="text-[13px] text-[#64748B] dark:text-foreground-secondary">
          <span>{t('lostPartition.needRecoverFiles')}</span>
          <button
            type="button"
            onClick={() => void navigate('/')}
            className="font-medium text-[#1677FF] underline decoration-[#1677FF]/60 underline-offset-2 hover:text-[#0958D9] dark:text-primary"
          >
            {t('lostPartition.goToHardwareDisk')}
          </button>
        </div>

        <button
          type="button"
          onClick={() => setStep('scan-completed')}
          className="inline-flex h-10 min-w-[204px] items-center justify-center rounded-[8px] bg-[#1677FF] px-6 text-[13.5px] font-semibold text-white shadow-xs transition-all hover:bg-[#0958D9] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring dark:bg-primary dark:text-primary-foreground dark:hover:bg-primary-hover"
        >
          {t('lostPartition.searchForLostPartition')}
        </button>
      </div>
    </div>
  );
}
