import * as React from 'react';
import { useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';
import {
  CheckCircle2Icon,
  CheckIcon,
  CircleAlertIcon,
  HardDriveIcon,
  HeartPulseIcon,
  RefreshCcwIcon,
  ShieldCheckIcon,
  ThermometerIcon,
  TimerIcon,
  ZapIcon,
} from 'lucide-react';
import { useHealth } from '@/queries/health';
import { useStartScan } from '@/queries/scan';
import { usePhysicalDevices } from '@/hooks/use-physical-devices';
import { formatDriveGb, formatInteger, formatPercent } from '@/lib/format';
import { cn } from '@/lib/utils';
import { Drive3dIcon } from '@/components/ui/brand-icons';
import { EmptyState } from '@/components/ui/empty-state';
import { Skeleton } from '@/components/ui/skeleton';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { toast } from 'sonner';

/**
 * Disk Health & S.M.A.R.T. Telemetry surface — redesigned to match the
 * EaseUS Data Recovery Wizard desktop suite aesthetic, color semantics,
 * 3D drive cards, circular health gauge, and tabular S.M.A.R.T. table.
 */
export function HealthPage(): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const startScan = useStartScan();
  const {
    devices: list,
    selectedId: deviceId,
    setSelectedId,
    isLoading,
    refetch,
  } = usePhysicalDevices();

  const health = useHealth(deviceId || null);
  const selectedDevice = list.find((d) => d.id === deviceId) ?? list[0] ?? null;

  const handleScanSelectedDrive = (): void => {
    if (!selectedDevice) return;
    startScan.mutate(
      { targetId: selectedDevice.id },
      {
        onSuccess: ({ scanId }) => {
          void navigate(`/scan/${selectedDevice.id}/live?scanId=${scanId}`);
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : t('errors.unknown'));
        },
      },
    );
  };

  if (isLoading) {
    return (
      <div className="h-full overflow-y-auto px-8 py-6">
        <div className="mx-auto w-full max-w-5xl space-y-5">
          <Skeleton className="h-8 w-56" />
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
            <Skeleton className="h-24 rounded-[10px]" />
            <Skeleton className="h-24 rounded-[10px]" />
            <Skeleton className="h-24 rounded-[10px]" />
          </div>
          <Skeleton className="h-44 rounded-[12px]" />
          <Skeleton className="h-64 rounded-[12px]" />
        </div>
      </div>
    );
  }

  if (list.length === 0) {
    return (
      <div className="h-full overflow-y-auto p-8">
        <EmptyState
          icon={HardDriveIcon}
          title={t('home.emptyTitle')}
          description={t('home.emptyBody')}
          actionLabel={t('home.refreshDevices')}
          onAction={refetch}
        />
      </div>
    );
  }

  const status = health.data?.status ?? 'good';
  const healthScore =
    status === 'good'
      ? Math.max(88, 100 - (health.data?.wearPercent ?? 4))
      : status === 'caution'
        ? 72
        : 38;
  const ringCircumference = 2 * Math.PI * 34;
  const ringOffset = ringCircumference * (1 - healthScore / 100);

  return (
    <div className="h-full overflow-y-auto px-8 py-6">
      <div className="mx-auto w-full max-w-5xl space-y-6 pb-6">
        {/* Top Header + Circular Refresh Button */}
        <div className="flex items-center justify-between gap-4">
          <div>
            <h1 className="text-[20px] font-bold tracking-tight text-[#1E293B] dark:text-foreground">
              {t('health.title')}
            </h1>
            <p className="mt-1 text-[13px] text-[#64748B] dark:text-foreground-secondary">
              {t('health.pickDisk')}
            </p>
          </div>
          <button
            type="button"
            onClick={() => {
              refetch();
              void health.refetch();
            }}
            aria-label={t('home.refreshDevices')}
            title={t('home.refreshDevices')}
            className="flex size-8 shrink-0 items-center justify-center rounded-full border border-[#E2E8F0] bg-white text-[#475569] shadow-2xs transition-all hover:border-[#93C5FD] hover:bg-[#EBF2FF] hover:text-[#1677FF] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring dark:border-border dark:bg-surface dark:text-foreground-secondary dark:hover:border-primary dark:hover:bg-surface-hover dark:hover:text-primary"
          >
            <RefreshCcwIcon className="size-4" aria-hidden />
          </button>
        </div>

        {/* Physical Disk Selector Cards (matching Home page drive cards) */}
        <div
          className="grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-3"
          role="radiogroup"
          aria-label={t('health.pickDisk')}
        >
          {list.map((d) => {
            const isSelected = deviceId === d.id;
            const isSys = d.volumes.some((v) => v.letter === 'C');
            const bitlockerState = d.volumes.find((v) => v.bitlocker)?.bitlocker;
            const freeBytes = Math.max(0, d.capacity.totalBytes - d.capacity.usedBytes);
            const usedRatio =
              d.capacity.totalBytes > 0
                ? Math.min(
                    100,
                    Math.max(
                      2,
                      Math.round((d.capacity.usedBytes / d.capacity.totalBytes) * 100),
                    ),
                  )
                : 0;
            const compactLabel = d.label.replace(/\s+\(([A-Z]:)\)/, '($1)');

            return (
              <button
                key={d.id}
                type="button"
                role="radio"
                aria-checked={isSelected}
                onClick={() => setSelectedId(d.id)}
                className={cn(
                  'group relative flex items-center gap-3.5 rounded-[10px] border px-4 py-3.5 text-left transition-all duration-150 outline-none focus-visible:ring-2 focus-visible:ring-ring',
                  isSelected
                    ? 'border-[#1677FF] bg-[#F4F8FF] ring-1 ring-[#1677FF] dark:border-primary dark:bg-primary-muted/40 dark:ring-primary'
                    : 'border-transparent bg-[#F8FAFD] hover:border-[#93C5FD] hover:bg-[#EEF4FF] dark:border-border dark:bg-surface dark:hover:border-border-strong dark:hover:bg-surface-hover',
                )}
              >
                {isSelected ? (
                  <span
                    className="absolute -right-2 -top-2 z-10 flex size-5 items-center justify-center rounded-full bg-[#1677FF] text-white shadow-xs ring-2 ring-white dark:bg-primary-fill dark:text-primary-foreground dark:ring-background"
                    aria-hidden
                  >
                    <CheckIcon className="size-3 stroke-[3]" />
                  </span>
                ) : null}

                <Drive3dIcon isSystemDrive={isSys} bitlocker={bitlockerState} />

                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13.5px] font-semibold text-[#1E293B] dark:text-foreground">
                    {compactLabel}
                  </div>
                  <div className="truncate text-[11.5px] text-[#64748B] dark:text-foreground-secondary">
                    {d.model}
                  </div>
                  <div
                    className="mt-2 h-[5px] w-full overflow-hidden rounded-full bg-[#DCE6F5] dark:bg-surface-tint"
                    aria-hidden
                  >
                    <div
                      className="h-full rounded-full bg-[#1677FF] transition-all dark:bg-primary-fill"
                      style={{ width: `${usedRatio}%` }}
                    />
                  </div>
                  <div className="mt-1 text-[11px] text-[#64748B] tabular dark:text-foreground-secondary">
                    {formatDriveGb(freeBytes)}/{formatDriveGb(d.capacity.totalBytes)}
                  </div>
                </div>
              </button>
            );
          })}
        </div>

        {health.isLoading ? (
          <div className="space-y-4">
            <Skeleton className="h-40 rounded-[12px]" />
            <Skeleton className="h-64 rounded-[12px]" />
          </div>
        ) : health.isError ? (
          <EmptyState
            icon={HeartPulseIcon}
            title={t('health.notAvailable')}
            description={health.error instanceof Error ? health.error.message : undefined}
            actionLabel={t('common.retry')}
            onAction={() => void health.refetch()}
          />
        ) : health.data ? (
          <>
            {/* Hero Telemetry Summary Card + 4 KPI Tiles */}
            <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
              {/* Left Health Score & Drive Overview Card (5 cols) */}
              <div
                className={cn(
                  'flex flex-col justify-between rounded-[12px] border p-5 shadow-[0_2px_12px_rgba(22,119,255,0.04)] lg:col-span-5',
                  status === 'good'
                    ? 'border-[#DCE8FF] bg-gradient-to-br from-[#F4F8FF] via-white to-[#EEF5FF] dark:border-border dark:from-surface-elevated dark:via-surface dark:to-surface-tint'
                    : status === 'caution'
                      ? 'border-warning/40 bg-warning-muted/60'
                      : 'border-danger/40 bg-danger-muted/60',
                )}
              >
                <div className="flex items-center gap-4">
                  {/* Circular Health Score Gauge */}
                  <div className="relative flex size-20 shrink-0 items-center justify-center">
                    <svg viewBox="0 0 80 80" className="size-20 -rotate-90">
                      <circle
                        cx="40"
                        cy="40"
                        r="34"
                        stroke="#DCE6F5"
                        strokeWidth="6.5"
                        fill="none"
                        className="dark:stroke-border"
                      />
                      <circle
                        cx="40"
                        cy="40"
                        r="34"
                        stroke={
                          status === 'good'
                            ? '#10B981'
                            : status === 'caution'
                              ? '#F59E0B'
                              : '#EF4444'
                        }
                        strokeWidth="6.5"
                        strokeLinecap="round"
                        strokeDasharray={ringCircumference}
                        strokeDashoffset={ringOffset}
                        fill="none"
                      />
                    </svg>
                    <div className="absolute inset-0 flex flex-col items-center justify-center">
                      <span className="text-[18px] font-bold text-[#1E293B] tabular dark:text-foreground">
                        {healthScore}%
                      </span>
                    </div>
                  </div>

                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span
                        className={cn(
                          'inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[12px] font-semibold',
                          status === 'good'
                            ? 'bg-[#DCFCE7] text-[#15803D] dark:bg-success/20 dark:text-success'
                            : status === 'caution'
                              ? 'bg-[#FEF3C7] text-[#B45309] dark:bg-warning/20 dark:text-warning'
                              : 'bg-[#FEE2E2] text-[#B91C1C] dark:bg-danger/20 dark:text-danger',
                        )}
                      >
                        <ShieldCheckIcon className="size-3.5" aria-hidden />
                        {status === 'good'
                          ? t('health.statusGood')
                          : status === 'caution'
                            ? t('health.statusCaution')
                            : t('health.statusBad')}
                      </span>
                    </div>
                    <div className="mt-1.5 truncate text-[15px] font-bold text-[#1E293B] dark:text-foreground">
                      {selectedDevice?.model ?? selectedDevice?.label}
                    </div>
                    <div className="mt-0.5 font-mono text-[11.5px] text-[#64748B] dark:text-foreground-secondary">
                      SN: {selectedDevice?.serial ?? '—'}
                    </div>
                  </div>
                </div>

                {status !== 'good' ? (
                  <p className="mt-3 text-[12px] leading-relaxed text-[#B45309] dark:text-warning">
                    {t('health.cautionNote')}
                  </p>
                ) : null}

                <div className="mt-4 flex items-center justify-between border-t border-[#E2ECF9] pt-3.5 dark:border-border">
                  <span className="text-[12px] text-[#64748B] dark:text-foreground-secondary">
                    {selectedDevice?.label}
                  </span>
                  <button
                    type="button"
                    onClick={handleScanSelectedDrive}
                    disabled={startScan.isPending}
                    className="inline-flex h-8 items-center justify-center rounded-[6px] bg-[#1677FF] px-3.5 text-[12px] font-semibold text-white shadow-2xs transition-all hover:bg-[#0958D9] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 dark:bg-primary-fill dark:text-primary-foreground dark:hover:bg-primary-fill-hover"
                  >
                    {t('home.scanAction')}
                  </button>
                </div>
              </div>

              {/* Right 2×2 Telemetry Metric Grid (7 cols) */}
              <div className="grid grid-cols-2 gap-3.5 lg:col-span-7">
                {/* Temperature */}
                <div className="flex flex-col justify-between rounded-[12px] border border-[#E2ECF9] bg-[#F8FAFD] p-4 dark:border-border dark:bg-surface">
                  <div className="flex items-center justify-between">
                    <span className="text-[12px] font-medium text-[#64748B] dark:text-foreground-secondary">
                      {t('health.temperature')}
                    </span>
                    <span className="flex size-7 items-center justify-center rounded-[7px] bg-[#EBF2FF] text-[#1677FF] dark:bg-surface-tint dark:text-primary">
                      <ThermometerIcon className="size-4" aria-hidden />
                    </span>
                  </div>
                  <div className="mt-2">
                    <div className="text-[20px] font-bold text-[#1E293B] tabular dark:text-foreground">
                      {health.data.temperatureC != null
                        ? `${formatInteger(health.data.temperatureC, i18n.language)} °C`
                        : '—'}
                    </div>
                    <div className="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-[#DCE6F5] dark:bg-surface-tint">
                      <div
                        className={cn(
                          'h-full rounded-full transition-all',
                          (health.data.temperatureC ?? 35) > 55
                            ? 'bg-[#F59E0B]'
                            : 'bg-[#10B981]',
                        )}
                        style={{
                          width: `${Math.min(100, Math.max(15, ((health.data.temperatureC ?? 36) / 75) * 100))}%`,
                        }}
                      />
                    </div>
                  </div>
                </div>

                {/* Power-On Hours */}
                <div className="flex flex-col justify-between rounded-[12px] border border-[#E2ECF9] bg-[#F8FAFD] p-4 dark:border-border dark:bg-surface">
                  <div className="flex items-center justify-between">
                    <span className="text-[12px] font-medium text-[#64748B] dark:text-foreground-secondary">
                      {t('health.powerOnHours')}
                    </span>
                    <span className="flex size-7 items-center justify-center rounded-[7px] bg-[#EBF2FF] text-[#1677FF] dark:bg-surface-tint dark:text-primary">
                      <TimerIcon className="size-4" aria-hidden />
                    </span>
                  </div>
                  <div className="mt-2">
                    <div className="text-[20px] font-bold text-[#1E293B] tabular dark:text-foreground">
                      {health.data.powerOnHours != null
                        ? `${formatInteger(health.data.powerOnHours, i18n.language)} h`
                        : '—'}
                    </div>
                    <div className="mt-1 text-[11px] text-[#64748B] tabular dark:text-foreground-secondary">
                      {health.data.powerOnHours != null
                        ? `${Math.floor(health.data.powerOnHours / 24)}d ${health.data.powerOnHours % 24}h`
                        : '—'}
                    </div>
                  </div>
                </div>

                {/* Drive Class */}
                <div className="flex flex-col justify-between rounded-[12px] border border-[#E2ECF9] bg-[#F8FAFD] p-4 dark:border-border dark:bg-surface">
                  <div className="flex items-center justify-between">
                    <span className="text-[12px] font-medium text-[#64748B] dark:text-foreground-secondary">
                      {t('health.class')}
                    </span>
                    <span className="flex size-7 items-center justify-center rounded-[7px] bg-[#EBF2FF] text-[#1677FF] dark:bg-surface-tint dark:text-primary">
                      <HardDriveIcon className="size-4" aria-hidden />
                    </span>
                  </div>
                  <div className="mt-2">
                    <div className="text-[15px] font-bold text-[#1E293B] dark:text-foreground">
                      {health.data.diskClass === 'ssd'
                        ? t('health.ssd')
                        : health.data.diskClass === 'hdd'
                          ? t('health.hdd')
                          : '—'}
                    </div>
                    <div className="mt-1 text-[11px] text-[#64748B] uppercase dark:text-foreground-secondary">
                      {selectedDevice?.volumes[0]?.fs ?? 'NTFS'} · 512B
                    </div>
                  </div>
                </div>

                {/* TRIM / Wear */}
                <div className="flex flex-col justify-between rounded-[12px] border border-[#E2ECF9] bg-[#F8FAFD] p-4 dark:border-border dark:bg-surface">
                  <div className="flex items-center justify-between">
                    <span className="text-[12px] font-medium text-[#64748B] dark:text-foreground-secondary">
                      {health.data.diskClass === 'ssd' ? t('health.trim') : t('health.wear')}
                    </span>
                    <span className="flex size-7 items-center justify-center rounded-[7px] bg-[#EBF2FF] text-[#1677FF] dark:bg-surface-tint dark:text-primary">
                      <ZapIcon className="size-4" aria-hidden />
                    </span>
                  </div>
                  <div className="mt-2">
                    <div className="text-[16px] font-bold text-[#1E293B] tabular dark:text-foreground">
                      {health.data.diskClass === 'ssd'
                        ? health.data.trimEnabled == null
                          ? '—'
                          : health.data.trimEnabled
                            ? t('health.trimEnabled')
                            : t('health.trimDisabled')
                        : health.data.wearPercent != null
                          ? formatPercent(health.data.wearPercent / 100, i18n.language)
                          : '—'}
                    </div>
                    <div className="mt-1 text-[11px] text-[#64748B] tabular dark:text-foreground-secondary">
                      {t('health.wear')}:{' '}
                      {health.data.wearPercent != null
                        ? formatPercent(health.data.wearPercent / 100, i18n.language)
                        : '2%'}
                    </div>
                  </div>
                </div>
              </div>
            </div>

            {/* S.M.A.R.T. Attributes Card */}
            <div className="overflow-hidden rounded-[12px] border border-[#E2ECF9] bg-white shadow-[0_2px_12px_rgba(22,119,255,0.04)] dark:border-border dark:bg-surface">
              <div className="flex items-center justify-between border-b border-[#E8EEF7] bg-[#F8FAFD] px-5 py-3.5 dark:border-border dark:bg-surface-elevated">
                <div className="flex items-center gap-2.5">
                  <HeartPulseIcon
                    className="size-4 text-[#1677FF] dark:text-primary"
                    aria-hidden
                  />
                  <h2 className="text-[14px] font-bold text-[#1E293B] dark:text-foreground">
                    {t('health.smartTable')}
                  </h2>
                </div>
                <span className="rounded-full bg-[#EBF2FF] px-2.5 py-0.5 text-[11px] font-semibold text-[#1677FF] tabular dark:bg-surface-tint dark:text-primary">
                  {health.data.attributes.length}
                </span>
              </div>

              <Table>
                <TableHeader>
                  <TableRow className="border-b border-[#E8EEF7] bg-[#FBFDFF] dark:border-border dark:bg-surface">
                    <TableHead className="w-16 pl-5 text-[11.5px] font-semibold text-[#64748B] dark:text-foreground-secondary">
                      {t('health.smartCol.id')}
                    </TableHead>
                    <TableHead className="text-[11.5px] font-semibold text-[#64748B] dark:text-foreground-secondary">
                      {t('health.smartCol.attribute')}
                    </TableHead>
                    <TableHead className="w-24 text-right text-[11.5px] font-semibold text-[#64748B] dark:text-foreground-secondary">
                      {t('health.smartCol.current')}
                    </TableHead>
                    <TableHead className="w-24 text-right text-[11.5px] font-semibold text-[#64748B] dark:text-foreground-secondary">
                      {t('health.smartCol.worst')}
                    </TableHead>
                    <TableHead className="w-24 text-right text-[11.5px] font-semibold text-[#64748B] dark:text-foreground-secondary">
                      {t('health.smartCol.threshold')}
                    </TableHead>
                    <TableHead className="w-28 text-right text-[11.5px] font-semibold text-[#64748B] dark:text-foreground-secondary">
                      {t('health.smartCol.raw')}
                    </TableHead>
                    <TableHead className="w-28 pr-5 text-right text-[11.5px] font-semibold text-[#64748B] dark:text-foreground-secondary">
                      {t('health.smartCol.status')}
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {health.data.attributes.map((a) => (
                    <TableRow
                      key={a.id}
                      className="border-b border-[#F1F5F9] transition-colors hover:bg-[#F4F8FF] dark:border-border dark:hover:bg-surface-hover"
                    >
                      <TableCell className="pl-5 font-mono text-[12px] text-[#64748B] tabular dark:text-foreground-secondary">
                        {String(a.id).padStart(2, '0')}
                      </TableCell>
                      <TableCell className="text-[12.5px] font-medium text-[#1E293B] dark:text-foreground">
                        {a.name}
                      </TableCell>
                      <TableCell className="text-right text-[12.5px] text-[#334155] tabular dark:text-foreground">
                        {a.current ?? '—'}
                      </TableCell>
                      <TableCell className="text-right text-[12.5px] text-[#334155] tabular dark:text-foreground">
                        {a.worst ?? '—'}
                      </TableCell>
                      <TableCell className="text-right text-[12.5px] text-[#64748B] tabular dark:text-foreground-secondary">
                        {a.threshold ?? '—'}
                      </TableCell>
                      <TableCell className="text-right font-mono text-[12px] text-[#334155] tabular dark:text-foreground">
                        {formatInteger(a.raw, i18n.language)}
                      </TableCell>
                      <TableCell className="pr-5 text-right">
                        <span
                          className={cn(
                            'inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold',
                            a.status === 'ok'
                              ? 'bg-[#DCFCE7] text-[#15803D] dark:bg-success/20 dark:text-success'
                              : a.status === 'failing'
                                ? 'bg-[#FEE2E2] text-[#B91C1C] dark:bg-danger/20 dark:text-danger'
                                : 'bg-[#F1F5F9] text-[#64748B] dark:bg-surface-tint dark:text-foreground-secondary',
                          )}
                        >
                          {a.status === 'ok' ? (
                            <CheckCircle2Icon className="size-3" aria-hidden />
                          ) : a.status === 'failing' ? (
                            <CircleAlertIcon className="size-3" aria-hidden />
                          ) : null}
                          {a.status === 'ok'
                            ? t('health.smartOk')
                            : a.status === 'failing'
                              ? t('health.smartFailing')
                              : '—'}
                        </span>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </>
        ) : null}
      </div>
    </div>
  );
}
