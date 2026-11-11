import * as React from 'react';
import { useNavigate, useSearchParams } from 'react-router';
import { useTranslation } from 'react-i18next';
import type { Device, Volume } from '@/connector/contract';
import {
  CheckIcon,
  ChevronRightIcon,
  ChevronUpIcon,
  CircleAlertIcon,
  FolderIcon,
  HardDriveIcon,
  LockIcon,
  PlugZapIcon,
  RefreshCcwIcon,
} from 'lucide-react';
import { useDevices, useRefreshDevices } from '@/queries/devices';
import { useLoadSession, useSessions } from '@/queries/sessions';
import { useStartScan } from '@/queries/scan';
import { usePickDirectory } from '@/queries/system';
import { formatBytes, formatDate, formatDriveGb } from '@/lib/format';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { EmptyState } from '@/components/ui/empty-state';
import { Skeleton } from '@/components/ui/skeleton';
import { Hint } from '@/components/ui/tooltip';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Desktop3dIcon,
  Drive3dIcon,
  RecycleBin3dIcon,
  SelectFolder3dIcon,
} from '@/components/ui/brand-icons';
import { toast } from 'sonner';

/**
 * Home / Hardware Disk page (reference: landing-baseline_53c354a9.png &
 * landing-after-activate-close_b5f42dd5.png) — "Select a location to search
 * for lost data" + circular refresh button, "Internal drives (2)" horizontal
 * cards with 3D drive icons + Windows OS badge + top-right blue checkmark on
 * selected card, "Specify a location" (Recycle Bin, Desktop, Select Folder),
 * and bottom bar with "Unable to find your hard drive? ^" + "Search for Lost Data".
 */

const RECYCLE_BIN_TARGET = 'vol-recycle-bin';
const DESKTOP_TARGET = 'vol-desktop';

export function HomePage(): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const devicesQuery = useDevices();
  const refresh = useRefreshDevices();
  const sessionsQuery = useSessions();
  const loadSession = useLoadSession();
  const startScan = useStartScan();
  const pickDirectory = usePickDirectory();
  const [selectedId, setSelectedId] = React.useState<string | null>(null);
  const [helpOpen, setHelpOpen] = React.useState(false);
  const [folderDialogOpen, setFolderDialogOpen] = React.useState(false);
  const [selectedFolderVolId, setSelectedFolderVolId] = React.useState<string>('vol-sys');
  const [selectedFolderSubpath, setSelectedFolderSubpath] =
    React.useState<string>('Users\\Documents');
  const [expandedDrives, setExpandedDrives] = React.useState<Record<string, boolean>>({
    'vol-sys': true,
    'vol-data': true,
  });
  const showRemovable = params.has('media');

  const devices = devicesQuery.data ?? [];
  const physical = devices.filter((d) => d.kind !== 'virtual');
  const internal = physical.filter((d) => d.kind === 'internal');
  const removable = physical.filter((d) => d.kind !== 'internal');
  const visible = showRemovable ? removable : internal;

  const effectiveSelectedId = selectedId ?? visible[0]?.id ?? null;
  const selected = devices.find((d) => d.id === effectiveSelectedId) ?? null;

  /** Direct start (ADR-008): device selection → scan, no config step. */
  const beginScan = (targetId: string, pathScope?: string): void => {
    startScan.mutate(
      { targetId, ...(pathScope ? { pathScope } : {}) },
      {
        onSuccess: ({ scanId }) => {
          void navigate(`/scan/${targetId}/live?scanId=${scanId}`);
        },
        onError: (err) => {
          const reasonKey = (err as { reasonKey?: string }).reasonKey;
          toast.error(reasonKey ? t(reasonKey) : err.message);
        },
      },
    );
  };

  const onChooseFolder = (): void => {
    const firstVol = physical.flatMap((d) => d.volumes)[0];
    if (firstVol) {
      setSelectedFolderVolId(firstVol.id);
    }
    setFolderDialogOpen(true);
  };

  const onConfirmFolder = (): void => {
    const allVols = physical.flatMap((d) => d.volumes);
    const targetVol = allVols.find((v) => v.id === selectedFolderVolId) ?? allVols[0];
    if (!targetVol) return;
    setFolderDialogOpen(false);
    pickDirectory.mutate(
      { volumeId: targetVol.id },
      {
        onSuccess: (picked) => {
          if (!picked) return;
          beginScan(targetVol.id, picked);
        },
        onError: (err) => toast.error(err.message),
      },
    );
  };

  const onResume = (path: string): void => {
    void loadSession.mutate(path, {
      onSuccess: ({ scanId }) => {
        toast.success(t('home.resume'));
        void navigate(`/results/${scanId}`);
      },
      onError: (err) => {
        toast.error(
          err instanceof Error
            ? (err as { reasonKey?: string }).reasonKey
              ? t((err as { reasonKey?: string }).reasonKey!)
              : err.message
            : t('errors.sessionCorrupt'),
        );
      },
    });
  };

  if (devicesQuery.isLoading) {
    return (
      <div className="p-7">
        <Skeleton className="mb-5 h-7 w-72" />
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          {Array.from({ length: 2 }, (_, i) => (
            <Skeleton key={i} className="h-24" />
          ))}
        </div>
      </div>
    );
  }

  if (devicesQuery.isError) {
    return (
      <div className="p-7">
        <EmptyState
          icon={CircleAlertIcon}
          title={t('errors.title')}
          description={
            devicesQuery.error instanceof Error
              ? devicesQuery.error.message
              : t('errors.unknown')
          }
          actionLabel={t('common.retry')}
          onAction={() => void devicesQuery.refetch()}
          secondaryLabel={t('home.refreshDevices')}
          onSecondary={() => void refresh.mutate()}
        />
      </div>
    );
  }

  if (physical.length === 0) {
    return (
      <div className="p-7">
        <EmptyState
          icon={HardDriveIcon}
          title={t('home.emptyTitle')}
          description={t('home.emptyBody')}
          actionLabel={t('home.refreshDevices')}
          onAction={() => refresh.mutate()}
        />
      </div>
    );
  }

  const hasPermissionIssue = physical.some(
    (d) =>
      d.status === 'permission-denied' ||
      d.volumes.some((v) => v.status === 'permission-denied'),
  );
  const hasDisconnected = physical.some(
    (d) => d.status === 'disconnected' || d.volumes.some((v) => v.status === 'disconnected'),
  );

  return (
    <div className="flex h-full min-h-0 flex-col justify-between overflow-y-auto">
      <div className="mx-auto w-full max-w-5xl flex-1 px-8 pt-6 pb-4">
        {/* Title + Circular Refresh Button */}
        <div className="mb-6 flex items-center justify-between gap-4">
          <div>
            <h1
              aria-label={`${t('home.title')} — Choose where to look for lost data`}
              className="text-[20px] font-bold tracking-tight text-[#1E293B] dark:text-foreground"
            >
              {t('home.title')}
            </h1>
            <p className="sr-only">{t('home.subtitle')}</p>
          </div>
          <button
            type="button"
            onClick={() => void refresh.mutate()}
            disabled={refresh.isPending}
            aria-label={t('home.refreshDevices')}
            title={t('home.refreshDevices')}
            className="flex size-8 shrink-0 items-center justify-center rounded-full border border-[#E2E8F0] bg-white text-[#475569] shadow-2xs transition-all hover:border-[#93C5FD] hover:bg-[#EBF2FF] hover:text-[#1677FF] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 dark:border-border dark:bg-surface dark:text-foreground-secondary dark:hover:border-primary dark:hover:bg-surface-hover dark:hover:text-primary"
          >
            <RefreshCcwIcon className="size-4" aria-hidden />
          </button>
        </div>

        {hasPermissionIssue ? (
          <div className="mb-4 flex items-center gap-2 rounded-md border border-warning/40 bg-warning-muted px-4 py-2.5 text-sm">
            <LockIcon className="size-4 text-warning" aria-hidden />
            <span>{t('home.permissionDenied')}</span>
          </div>
        ) : null}
        {hasDisconnected ? (
          <div className="mb-4 flex items-center gap-2 rounded-md border border-danger/40 bg-danger-muted px-4 py-2.5 text-sm">
            <PlugZapIcon className="size-4 text-danger" aria-hidden />
            <span>{t('home.disconnected')}</span>
          </div>
        ) : null}

        {/* Internal drives (N) */}
        <h2 className="mb-3.5 text-[14px] font-bold text-[#1E293B] dark:text-foreground">
          {showRemovable ? t('home.removableGroup') : t('home.internalGroup')} ({visible.length}
          )
        </h2>

        <div
          className="grid grid-cols-1 gap-4 sm:grid-cols-2"
          role="radiogroup"
          aria-label={t('home.title')}
        >
          {visible.map((device) => (
            <DeviceCard
              key={device.id}
              device={device}
              selected={effectiveSelectedId === device.id}
              onSelect={() => setSelectedId(device.id)}
              onScan={() => beginScan(device.id)}
              onScanVolume={(volumeId) => beginScan(volumeId)}
            />
          ))}
        </div>

        {/* Specify a location */}
        <h2 className="mb-3.5 mt-8 text-[14px] font-bold text-[#1E293B] dark:text-foreground">
          {t('home.quickTargets')}
        </h2>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <QuickTarget
            icon={<RecycleBin3dIcon />}
            label={t('home.recycleBin')}
            onClick={() => beginScan(RECYCLE_BIN_TARGET)}
            loading={startScan.isPending}
          />
          <QuickTarget
            icon={<Desktop3dIcon />}
            label={t('home.desktop')}
            onClick={() => beginScan(DESKTOP_TARGET)}
            loading={startScan.isPending}
          />
          <QuickTarget
            icon={<SelectFolder3dIcon />}
            label={t('home.chooseFolder')}
            onClick={onChooseFolder}
            loading={pickDirectory.isPending || startScan.isPending}
          />
        </div>

        {(sessionsQuery.data?.length ?? 0) > 0 ? (
          <>
            <h2 className="mb-3 mt-7 text-[13px] font-bold text-[#475569] dark:text-foreground-secondary">
              {t('home.recentSessions')}
            </h2>
            <div className="space-y-2">
              {sessionsQuery.data?.slice(0, 3).map((s) => (
                <button
                  key={s.id}
                  type="button"
                  onClick={() => onResume(s.fileName)}
                  className="flex w-full items-center justify-between gap-4 rounded-[8px] border border-[#E2E8F0] bg-[#F8FAFD] px-4 py-2.5 text-left transition-colors hover:border-[#93C5FD] hover:bg-[#EBF2FF]/60 focus-visible:ring-2 focus-visible:ring-ring dark:border-border dark:bg-surface dark:hover:bg-surface-hover"
                >
                  <div className="min-w-0">
                    <div className="truncate text-[13px] font-medium text-[#1E293B] dark:text-foreground">
                      {s.targetLabel}
                    </div>
                    <div className="text-[11px] text-[#64748B] tabular dark:text-foreground-secondary">
                      {formatDate(s.savedAt, i18n.language)} ·{' '}
                      {s.filesFound.toLocaleString(i18n.language)} {t('scan.filesFound')}
                    </div>
                  </div>
                  <Button variant="secondary" size="sm">
                    {t('home.resume')}
                  </Button>
                </button>
              ))}
            </div>
          </>
        ) : null}
      </div>

      {/* Bottom Footer Bar — "Unable to find your hard drive? ^" on left, "Search for Lost Data" on right */}
      <div className="sticky bottom-0 flex items-center justify-between gap-3 bg-white/95 px-8 py-4 backdrop-blur dark:bg-background/95">
        <div className="relative">
          <button
            type="button"
            onClick={() => setHelpOpen((v) => !v)}
            className="inline-flex items-center gap-1 rounded-xs px-1 py-0.5 text-[13px] text-[#64748B] transition-colors hover:text-[#1677FF] focus-visible:ring-2 focus-visible:ring-ring dark:text-foreground-secondary dark:hover:text-primary"
          >
            <span>{t('home.unableToFindDrive')}</span>
            <ChevronUpIcon
              className={cn('size-3.5 transition-transform', helpOpen && 'rotate-180')}
              aria-hidden
            />
          </button>
          {helpOpen ? (
            <div className="absolute bottom-8 left-0 z-20 w-72 rounded-[10px] border border-[#DCE8FF] bg-white p-3.5 shadow-lg dark:border-border dark:bg-surface">
              <p className="text-[12px] leading-relaxed text-[#475569] dark:text-foreground-secondary">
                {t('lostPartition.subtitle')}
              </p>
              <button
                type="button"
                onClick={() => {
                  setHelpOpen(false);
                  void navigate('/lost-partition');
                }}
                className="mt-2 text-[12px] font-semibold text-[#1677FF] hover:underline dark:text-primary"
              >
                {t('sidebar.lostPartition')} →
              </button>
            </div>
          ) : null}
        </div>

        <button
          type="button"
          disabled={!selected || startScan.isPending}
          onClick={() => selected && beginScan(selected.id)}
          aria-keyshortcuts="Enter"
          className="inline-flex h-10 min-w-[184px] items-center justify-center rounded-[6px] bg-[#1677FF] px-6 text-[14px] font-semibold text-white shadow-xs transition-all hover:bg-[#0958D9] hover:text-white active:bg-[#003EB3] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 dark:bg-primary-fill dark:text-primary-foreground dark:hover:bg-primary-fill-hover"
        >
          {t('home.scanAction')}
        </button>
      </div>

      {/* Windows-style "Select Folder" Mini Popup Dialog */}
      <Dialog open={folderDialogOpen} onOpenChange={setFolderDialogOpen}>
        <DialogContent
          className="w-[440px] max-w-[92vw] rounded-[12px] border border-[#DCE6F5] bg-white p-5 shadow-2xl dark:border-border dark:bg-surface-elevated"
          aria-label={t('home.chooseFolder')}
        >
          <DialogHeader>
            <DialogTitle className="text-[15px] font-bold text-[#1E293B] dark:text-foreground">
              {t('home.chooseFolder')}
            </DialogTitle>
            <DialogDescription className="text-[12px] text-[#64748B] dark:text-foreground-secondary">
              {t('home.subtitle')}
            </DialogDescription>
          </DialogHeader>

          <div className="mt-3 h-[230px] overflow-y-auto rounded-[8px] border border-[#E2E8F0] bg-[#F8FAFC] p-2 dark:border-border dark:bg-surface">
            {physical
              .flatMap((d) => d.volumes)
              .map((vol) => {
                const isExp = expandedDrives[vol.id] ?? true;
                const drivePrefix = vol.letter ? `${vol.letter}:\\` : '';
                const subFolders =
                  vol.letter === 'C'
                    ? [
                        'Users\\Documents',
                        'Users\\Downloads',
                        'Users\\Pictures',
                        'Users\\Desktop',
                        'Projects',
                        'Backups',
                      ]
                    : ['Projects', 'Media', 'Backups', 'Shared', 'Archive'];
                return (
                  <div key={vol.id} className="mb-1">
                    <div className="flex items-center gap-1 rounded-[6px] px-1.5 py-1 text-[12.5px] font-semibold text-[#1E293B] hover:bg-[#EBF2FF] dark:text-foreground dark:hover:bg-surface-hover">
                      <button
                        type="button"
                        onClick={() =>
                          setExpandedDrives((prev) => ({ ...prev, [vol.id]: !isExp }))
                        }
                        className="flex size-5 items-center justify-center text-[#64748B]"
                        aria-label={vol.label}
                      >
                        <ChevronRightIcon
                          className={cn('size-3.5 transition-transform', isExp && 'rotate-90')}
                          aria-hidden
                        />
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          setSelectedFolderVolId(vol.id);
                          setSelectedFolderSubpath(subFolders[0] ?? 'Users');
                        }}
                        className="flex flex-1 items-center gap-2 text-left"
                      >
                        <Drive3dIcon
                          isSystemDrive={vol.letter === 'C'}
                          bitlocker={vol.bitlocker}
                          className="size-6"
                        />
                        <span className="truncate">{vol.label}</span>
                      </button>
                    </div>
                    {isExp ? (
                      <div className="ml-6 space-y-0.5 border-l border-[#E2E8F0] pl-2 dark:border-border">
                        {subFolders.map((sub) => {
                          const isPicked =
                            selectedFolderVolId === vol.id && selectedFolderSubpath === sub;
                          return (
                            <button
                              key={sub}
                              type="button"
                              onClick={() => {
                                setSelectedFolderVolId(vol.id);
                                setSelectedFolderSubpath(sub);
                              }}
                              onDoubleClick={onConfirmFolder}
                              className={cn(
                                'flex w-full items-center gap-2 rounded-[5px] px-2 py-1 text-left text-[12px] transition-colors',
                                isPicked
                                  ? 'bg-[#D6E6FF] font-semibold text-[#0958D9] dark:bg-surface-selected dark:text-primary'
                                  : 'text-[#334155] hover:bg-[#EBF2FF]/70 dark:text-foreground-secondary dark:hover:bg-surface-hover',
                              )}
                            >
                              <FolderIcon
                                className="size-4 shrink-0 fill-[#93C5FD] text-[#3B82F6]"
                                aria-hidden
                              />
                              <span className="truncate">
                                {drivePrefix}
                                {sub}
                              </span>
                            </button>
                          );
                        })}
                      </div>
                    ) : null}
                  </div>
                );
              })}
          </div>

          <div className="mt-2.5 flex items-center gap-2 rounded-[6px] border border-[#E2E8F0] bg-[#F8FAFC] px-3 py-1.5 text-[12px] dark:border-border dark:bg-surface">
            <span className="shrink-0 font-medium text-[#64748B] dark:text-foreground-secondary">
              {t('results.pathTab')}:
            </span>
            <span className="truncate font-mono text-[#1E293B] dark:text-foreground">
              {(physical.flatMap((d) => d.volumes).find((v) => v.id === selectedFolderVolId)
                ?.letter ?? 'C') +
                ':\\' +
                selectedFolderSubpath}
            </span>
          </div>

          <DialogFooter className="mt-4 flex justify-end gap-2">
            <Button variant="secondary" size="sm" onClick={() => setFolderDialogOpen(false)}>
              {t('common.close')}
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={onConfirmFolder}
              loading={pickDirectory.isPending || startScan.isPending}
              className="[background-image:none] bg-[#1677FF] text-white hover:bg-[#0958D9]"
            >
              {t('common.continue')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function DeviceCard({
  device,
  selected,
  onSelect,
  onScan,
  onScanVolume,
}: {
  device: Device;
  selected: boolean;
  onSelect: () => void;
  onScan: () => void;
  onScanVolume: (volumeId: string) => void;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const locked =
    device.status === 'permission-denied' ||
    device.volumes.some((v) => v.status === 'permission-denied');
  const offline =
    device.status === 'disconnected' || device.volumes.some((v) => v.status === 'disconnected');
  const hasLost = device.volumes.some((v) => v.lostPartition);
  const isSystemDrive = device.volumes.some((v) => v.letter === 'C');
  const bitlockerState =
    device.volumes.find((v) => v.bitlocker)?.bitlocker ??
    (device.status === 'locked' || device.volumes.some((v) => v.status === 'locked')
      ? 'locked'
      : undefined);

  const freeBytes = Math.max(0, device.capacity.totalBytes - device.capacity.usedBytes);
  const usedRatio =
    device.capacity.totalBytes > 0
      ? Math.min(
          100,
          Math.max(
            2,
            Math.round((device.capacity.usedBytes / device.capacity.totalBytes) * 100),
          ),
        )
      : 0;

  // Format label to match reference "Windows(C:)" / "Temporary Storage(D:)"
  const compactLabel = device.label.replace(/\s+\(([A-Z]:)\)/, '($1)');
  const showVolumeChips =
    device.volumes.length > 1 || device.volumes.some((v) => v.status !== 'ok');

  return (
    <div
      onDoubleClick={() => {
        if (!locked && !offline) onScan();
      }}
      className={cn(
        'group relative flex flex-col justify-center rounded-[10px] border bg-[#F8FAFD] px-5 py-4 transition-all duration-150 dark:bg-surface',
        selected
          ? 'border-[#1677FF] bg-[#F4F8FF] ring-1 ring-[#1677FF] dark:border-primary dark:bg-primary-muted/40 dark:ring-primary'
          : 'border-transparent hover:border-[#93C5FD] hover:bg-[#EEF4FF] dark:border-border dark:hover:border-border-strong dark:hover:bg-surface-hover',
        offline && 'opacity-60',
      )}
    >
      {/* Top-right blue checkmark circle on selected card */}
      {selected ? (
        <span
          className="absolute -right-2 -top-2 z-10 flex size-5 items-center justify-center rounded-full bg-[#1677FF] text-white shadow-xs ring-2 ring-white dark:bg-primary-fill dark:text-primary-foreground dark:ring-background"
          aria-hidden
        >
          <CheckIcon className="size-3 stroke-[3]" />
        </span>
      ) : null}

      <button
        type="button"
        role="radio"
        aria-checked={selected}
        onClick={onSelect}
        className="flex w-full items-center gap-4 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <Drive3dIcon isSystemDrive={isSystemDrive} bitlocker={bitlockerState} />

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-[14px] font-semibold text-[#1E293B] dark:text-foreground">
              {compactLabel}
            </span>
            {hasLost ? <Badge variant="primary">{t('home.lostPartitionBadge')}</Badge> : null}
            {locked ? (
              <Hint text={t('home.permissionDenied')}>
                <LockIcon className="size-3.5 text-warning" aria-hidden />
              </Hint>
            ) : null}
            {offline ? (
              <Hint text={t('home.disconnected')}>
                <PlugZapIcon className="size-3.5 text-danger" aria-hidden />
              </Hint>
            ) : null}
          </div>

          {/* Reference capacity bar */}
          <div
            className="mt-2 h-[6px] w-full overflow-hidden rounded-full bg-[#DCE6F5] dark:bg-surface-tint"
            aria-hidden
          >
            <div
              className="h-full rounded-full bg-[#1677FF] transition-all dark:bg-primary-fill"
              style={{ width: `${usedRatio}%` }}
            />
          </div>

          <div
            className="mt-1.5 text-[12px] text-[#64748B] tabular dark:text-foreground-secondary"
            aria-label={t('home.capacityUsed', {
              used: formatBytes(device.capacity.usedBytes, i18n.language),
              total: formatBytes(device.capacity.totalBytes, i18n.language),
            })}
          >
            {formatDriveGb(freeBytes)}/{formatDriveGb(device.capacity.totalBytes)}
          </div>
        </div>
      </button>

      {showVolumeChips ? (
        <div className="mt-3 flex items-center justify-between gap-2 border-t border-[#E2E8F0] pt-2 dark:border-border">
          <span className="text-[11px] font-medium text-[#64748B] dark:text-foreground-secondary">
            {locked || offline ? t('home.healthCaution') : t('home.healthGood')}
          </span>
          <span className="flex items-center gap-1.5">
            {device.volumes.map((v: Volume) => (
              <button
                key={v.id}
                type="button"
                onClick={() => onScanVolume(v.id)}
                disabled={v.status === 'permission-denied' || v.status === 'disconnected'}
                title={
                  v.status === 'permission-denied'
                    ? t('home.permissionDenied')
                    : v.status === 'disconnected'
                      ? t('home.disconnected')
                      : undefined
                }
                className="rounded-xs px-1.5 py-0.5 text-xs font-medium text-[#1677FF] hover:bg-[#DCE8FF] focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 dark:text-primary dark:hover:bg-surface-hover"
              >
                {v.label}
              </button>
            ))}
          </span>
        </div>
      ) : null}
    </div>
  );
}

function QuickTarget({
  icon,
  label,
  onClick,
  loading,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  loading?: boolean;
}): React.JSX.Element {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={loading}
      className="flex items-center gap-3.5 rounded-[10px] border border-transparent bg-[#F8FAFD] px-5 py-4 text-left text-[13.5px] font-medium text-[#1E293B] transition-all hover:border-[#93C5FD] hover:bg-[#EEF4FF] hover:text-[#0F172A] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-60 dark:border-border dark:bg-surface dark:text-foreground dark:hover:border-border-strong dark:hover:bg-surface-hover"
    >
      {icon}
      <span className="truncate">{label}</span>
    </button>
  );
}
