import * as React from 'react';
import { useNavigate, useParams, useSearchParams } from 'react-router';
import { useTranslation } from 'react-i18next';
import type { CollisionPolicy, RecoveryItem, RecoveryOptions } from '@/connector/contract';
import {
  CheckCircle2Icon,
  ChevronDownIcon,
  CircleAlertIcon,
  DownloadIcon,
  FolderIcon,
  TriangleAlertIcon,
  XCircleIcon,
  XIcon,
} from 'lucide-react';
import { useMountPoints, usePickDirectory, useValidateDestination } from '@/queries/system';
import {
  useCancelRecovery,
  useRecoveryItems,
  useRecoveryJob,
  useRecoveryProgress,
  useRecoveryReport,
  useStartRecovery,
} from '@/queries/recovery';
import { useScanState } from '@/queries/scan';
import { resolveFilter } from '@/stores/filter-store';
import { useScanSessionStore } from '@/stores/scan-session-store';
import { useSelectionStore } from '@/stores/selection-store';
import { formatBytes, formatDriveGb, formatDuration, formatPercent } from '@/lib/format';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Progress } from '@/components/ui/progress';
import { Switch } from '@/components/ui/switch';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Badge } from '@/components/ui/badge';
import { Alert } from '@/components/ui/alert';
import { StatTile } from '@/components/ui/empty-state';
import { Skeleton } from '@/components/ui/skeleton';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Drive3dIcon, SaveDestinationIcon } from '@/components/ui/brand-icons';
import { toast } from 'sonner';

/**
 * Recovery surface (references: recovery-destdlg-early-state_a09ed0e8.png &
 * recovery-insufficient-space-gate_d768e6b3.png) — "Choose a destination path
 * to save the file:" dialog card with Local | Cloud tabs, drive radio rows,
 * source-drive amber recommendation banner, "Enable file repair" toggle,
 * Insufficient Disk Space modal gate, and live recovery queue + report.
 */
export function RecoveryPage(): React.JSX.Element {
  const { scanId = '' } = useParams();
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const { t, i18n } = useTranslation();

  const selection = useSelectionStore();
  const recoverAll = params.has('all') || selection.mode === 'all';
  const nodeIds = React.useMemo(
    () => (recoverAll ? [] : Array.from(selection.ids)),
    [recoverAll, selection.ids],
  );
  const selectedCount = recoverAll
    ? Math.max(0, selection.allCount - selection.exclusions.size)
    : selection.ids.size;

  const scanState = useScanState(scanId);
  const avgFileBytes =
    scanState.data && scanState.data.progress.filesFound > 0
      ? scanState.data.progress.dataFoundBytes / scanState.data.progress.filesFound
      : 2_400_000;
  const neededBytes = Math.max(
    1,
    Math.round((recoverAll ? selection.allCount : Math.max(1, selectedCount)) * avgFileBytes),
  );

  const mounts = useMountPoints();
  const startRecovery = useStartRecovery(scanId);
  const sourceTargetId = useScanSessionStore((s) =>
    scanId ? s.known[scanId]?.targetId : null,
  );
  const validateDestination = useValidateDestination(sourceTargetId);
  const pickDirectory = usePickDirectory();

  const [destTab, setDestTab] = React.useState<'local' | 'cloud'>('local');
  const [destination, setDestination] = React.useState('');
  const [showAdvanced, setShowAdvanced] = React.useState(false);
  const [spaceModalOpen, setSpaceModalOpen] = React.useState(false);
  const [validation, setValidation] = React.useState<{
    allowed: boolean;
    reason?: 'overlap' | 'unwritable' | 'invalid' | 'out-of-space';
    reasonKey?: string;
    freeBytes: number | null;
    neededBytes: number | null;
  } | null>(null);
  const [validating, setValidating] = React.useState(false);

  const [options, setOptions] = React.useState<RecoveryOptions>({
    collision: 'rename',
    rebuildFolders: true,
    keepEmptyFolders: false,
    fileRepair: true,
    verifyOnWrite: true,
  });

  const jobId = startRecovery.data?.jobId ?? null;
  const jobQuery = useRecoveryJob(jobId);

  const validate = (path: string): void => {
    if (!path) {
      setValidation(null);
      return;
    }
    setValidating(true);
    validateDestination.mutate(
      { path, neededBytes },
      {
        onSuccess: (v) => {
          setValidation(v);
          if (!v.allowed && v.reason === 'out-of-space') {
            setSpaceModalOpen(true);
          }
        },
        onError: () =>
          setValidation({
            allowed: false,
            reason: 'invalid',
            freeBytes: null,
            neededBytes: null,
          }),
        onSettled: () => setValidating(false),
      },
    );
  };

  const handlePickFolder = (): void => {
    pickDirectory.mutate(
      { excludeDeviceId: sourceTargetId ?? null },
      {
        onSuccess: (picked) => {
          if (picked) {
            setDestination(picked);
            validate(picked);
          }
        },
      },
    );
  };

  const jobStatus = jobQuery.data?.status ?? (jobId ? 'running' : null);
  const startLabel =
    selectedCount > 0
      ? t('recovery.start', { count: selectedCount })
      : t('results.status.recoverAll');

  return (
    <div className="flex h-full min-h-0 flex-col items-center justify-center overflow-y-auto bg-[#F1F5F9]/70 p-4 sm:p-6 dark:bg-background/80">
      <div className="w-full max-w-[600px] rounded-[12px] border border-[#E2E8F0] bg-white p-6 shadow-xl sm:p-7 dark:border-border dark:bg-surface-elevated">
        {/* Top Row with Save Icon and Close X */}
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-start gap-4">
            <SaveDestinationIcon />
            <div>
              <h1
                aria-label={`${t('recovery.destination')} — ${t('recovery.title')}`}
                className="text-[18px] font-bold tracking-tight text-[#1E293B] dark:text-foreground"
              >
                {t('recovery.destination')}
              </h1>
              <p className="mt-1 text-[13px] text-[#64748B] tabular dark:text-foreground-secondary">
                {t('recovery.filesToRecover', {
                  count: Math.max(1, selectedCount),
                  size: formatBytes(neededBytes, i18n.language),
                })}
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={() => navigate(-1)}
            aria-label={t('common.back')}
            className="rounded-md p-1 text-[#94A3B8] transition-colors hover:bg-[#E2E8F0] hover:text-[#1E293B] dark:text-foreground-muted dark:hover:bg-surface-hover dark:hover:text-foreground"
          >
            <XIcon className="size-4" aria-hidden />
          </button>
        </div>

        {jobId ? (
          <div className="mt-6">
            <RecoveryQueue jobId={jobId} />
            {jobStatus === 'running' ? (
              <div className="mt-4 flex justify-end">
                <CancelRecoveryButton jobId={jobId} />
              </div>
            ) : null}
          </div>
        ) : (
          <>
            {/* Segmented Local | Cloud Tabs */}
            <div className="mt-5 grid grid-cols-2 rounded-[8px] bg-[#F1F5F9] p-1 dark:bg-surface">
              <button
                type="button"
                onClick={() => setDestTab('local')}
                className={cn(
                  'h-8 rounded-[6px] text-[13px] font-semibold transition-all',
                  destTab === 'local'
                    ? 'bg-white text-[#1E293B] shadow-2xs dark:bg-surface-elevated dark:text-foreground'
                    : 'text-[#64748B] hover:text-[#1E293B] dark:text-foreground-secondary dark:hover:text-foreground',
                )}
              >
                {t('recovery.tabLocal')}
              </button>
              <button
                type="button"
                onClick={() => setDestTab('cloud')}
                className={cn(
                  'h-8 rounded-[6px] text-[13px] font-semibold transition-all',
                  destTab === 'cloud'
                    ? 'bg-white text-[#1E293B] shadow-2xs dark:bg-surface-elevated dark:text-foreground'
                    : 'text-[#64748B] hover:text-[#1E293B] dark:text-foreground-secondary dark:hover:text-foreground',
                )}
              >
                {t('recovery.tabCloud')}
              </button>
            </div>

            {/* Destination Radio List */}
            {destTab === 'local' ? (
              <div className="mt-4 max-h-[270px] divide-y divide-[#F1F5F9] overflow-y-auto rounded-[8px] border border-[#E2E8F0] bg-white dark:divide-border dark:border-border dark:bg-surface">
                {mounts.isLoading
                  ? Array.from({ length: 2 }, (_, i) => (
                      <Skeleton key={i} className="m-2 h-12" />
                    ))
                  : mounts.data?.map((m) => {
                      const isSel = destination === m.path;
                      const isOverlap =
                        (isSel && validation?.reason === 'overlap') ||
                        m.path.toUpperCase().startsWith('C:');
                      const isLowSpace = neededBytes > m.freeBytes;

                      return (
                        <div key={m.id} className="px-4 py-3">
                          <button
                            type="button"
                            onClick={() => {
                              setDestination(m.path);
                              validate(m.path);
                            }}
                            aria-pressed={isSel}
                            className="flex w-full items-center gap-3 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          >
                            <span
                              className={cn(
                                'flex size-4 shrink-0 items-center justify-center rounded-full border-2 transition-colors',
                                isSel
                                  ? 'border-[#1677FF] dark:border-primary'
                                  : 'border-[#CBD5E1] dark:border-border-strong',
                              )}
                              aria-hidden
                            >
                              {isSel ? (
                                <span className="size-2 rounded-full bg-[#1677FF] dark:bg-primary" />
                              ) : null}
                            </span>

                            <Drive3dIcon
                              isSystemDrive={m.path.toUpperCase().startsWith('C:')}
                              className="size-8"
                            />

                            <div className="flex min-w-0 flex-1 items-center gap-1.5 text-[13px]">
                              <span className="truncate font-semibold text-[#1E293B] dark:text-foreground">
                                {m.label}
                              </span>
                              <span className="ml-1.5 tabular">
                                <span
                                  className={cn(
                                    isOverlap
                                      ? 'text-[#EF4444]'
                                      : 'text-[#64748B] dark:text-foreground-secondary',
                                  )}
                                >
                                  {formatDriveGb(m.freeBytes)}
                                </span>
                                <span className="text-[#64748B] dark:text-foreground-secondary">
                                  {' / '}
                                  {formatDriveGb(m.totalBytes)}
                                </span>
                              </span>
                              {isOverlap ? (
                                <TriangleAlertIcon
                                  className="size-4 shrink-0 text-[#EF4444]"
                                  aria-hidden
                                />
                              ) : isLowSpace ? (
                                <TriangleAlertIcon
                                  className="size-4 shrink-0 text-[#F59E0B]"
                                  aria-hidden
                                />
                              ) : null}
                            </div>
                          </button>

                          {isSel && isOverlap ? (
                            <div className="mt-2.5 ml-7 rounded-[6px] bg-[#FFF7ED] px-3.5 py-2 text-[12px] leading-relaxed text-[#EA580C] dark:bg-warning-muted dark:text-warning">
                              {t('recovery.destinationHint')}
                            </div>
                          ) : null}
                        </div>
                      );
                    })}

                {/* Choose folder... row */}
                <div className="px-4 py-3">
                  <button
                    type="button"
                    onClick={handlePickFolder}
                    className="flex w-full items-center gap-3 text-left text-[13px] font-medium text-[#1E293B] outline-none hover:text-[#0958D9] focus-visible:ring-2 focus-visible:ring-ring dark:text-foreground dark:hover:text-primary"
                  >
                    <span
                      className={cn(
                        'flex size-4 shrink-0 items-center justify-center rounded-full border-2 transition-colors',
                        destination && !mounts.data?.some((m) => m.path === destination)
                          ? 'border-[#1677FF] dark:border-primary'
                          : 'border-[#CBD5E1] dark:border-border-strong',
                      )}
                      aria-hidden
                    >
                      {destination && !mounts.data?.some((m) => m.path === destination) ? (
                        <span className="size-2 rounded-full bg-[#1677FF] dark:bg-primary" />
                      ) : null}
                    </span>
                    <FolderIcon
                      className="size-5 shrink-0 fill-[#60A5FA] text-[#1677FF] dark:text-primary"
                      aria-hidden
                    />
                    <span className="truncate">
                      {t('recovery.chooseFolder')}
                      {destination && !mounts.data?.some((m) => m.path === destination)
                        ? ` (${destination})`
                        : ''}
                    </span>
                  </button>
                </div>
              </div>
            ) : (
              <div className="mt-4 rounded-[8px] border border-[#E2E8F0] bg-[#F8FAFC] p-6 text-center text-[13px] text-[#64748B] dark:border-border dark:bg-surface dark:text-foreground-secondary">
                {t('tools.providerBody')}
              </div>
            )}

            {/* Collapsible manual path & collision options */}
            <div className="mt-3">
              <button
                type="button"
                onClick={() => setShowAdvanced((v) => !v)}
                className="inline-flex items-center gap-1 text-[12px] text-[#64748B] hover:text-[#0958D9] dark:text-foreground-secondary dark:hover:text-primary"
              >
                <span>{t('recovery.pathLabel')}</span>
                <ChevronDownIcon
                  className={cn('size-3.5 transition-transform', showAdvanced && 'rotate-180')}
                  aria-hidden
                />
              </button>

              {showAdvanced ? (
                <div className="mt-2 space-y-3 rounded-[8px] border border-[#E2E8F0] bg-[#F8FAFC] p-3 dark:border-border dark:bg-surface">
                  <div className="space-y-1">
                    <Label htmlFor="dest-path" className="text-xs">
                      {t('recovery.pathLabel')}
                    </Label>
                    <Input
                      id="dest-path"
                      value={destination}
                      placeholder="D:\\Recovered Files"
                      onChange={(e) => setDestination(e.target.value)}
                      onBlur={() => validate(destination)}
                      className="font-mono text-xs"
                    />
                  </div>
                  <div className="space-y-1">
                    <Label className="text-xs">{t('recovery.collisions')}</Label>
                    <RadioGroup
                      value={options.collision}
                      onValueChange={(v) =>
                        setOptions((o) => ({ ...o, collision: v as CollisionPolicy }))
                      }
                      className="flex flex-wrap gap-3"
                    >
                      {(['rename', 'overwrite', 'skip'] as const).map((c) => (
                        <label
                          key={c}
                          className="flex cursor-pointer items-center gap-1.5 text-xs"
                        >
                          <RadioGroupItem value={c} />
                          {t(`recovery.collision${c[0].toUpperCase()}${c.slice(1)}`)}
                        </label>
                      ))}
                    </RadioGroup>
                  </div>
                </div>
              ) : null}
            </div>

            {validating ? (
              <p className="mt-2 text-xs text-[#64748B] dark:text-foreground-secondary">
                {t('recovery.validating')}
              </p>
            ) : null}

            {validation && !validation.allowed ? (
              <Alert variant="danger" className="mt-3">
                <div className="space-y-1">
                  <p className="font-semibold">
                    {validation.reason === 'overlap'
                      ? t('recovery.overlapRefused')
                      : validation.reason === 'out-of-space'
                        ? t('recovery.insufficientSpace')
                        : t('errors.title')}
                  </p>
                  <p>
                    {validation.reason === 'overlap'
                      ? t('recovery.overlapReason')
                      : validation.reason === 'out-of-space' &&
                          validation.neededBytes &&
                          validation.freeBytes
                        ? t('recovery.insufficientSpaceBody', {
                            needed: formatBytes(
                              validation.neededBytes - validation.freeBytes,
                              i18n.language,
                            ),
                          })
                        : t('errors.unknown')}
                  </p>
                </div>
              </Alert>
            ) : null}

            {/* Bottom Footer Bar: Enable file repair switch on left, Save / Recover button on right */}
            <div className="mt-6 flex items-center justify-between gap-4 pt-2">
              <div className="flex items-center gap-2.5">
                <Label
                  htmlFor="file-repair-switch"
                  className="text-[13px] text-[#334155] dark:text-foreground"
                >
                  {t('recovery.fileRepair')}
                </Label>
                <Switch
                  id="file-repair-switch"
                  checked={options.fileRepair}
                  onCheckedChange={(v) => setOptions((o) => ({ ...o, fileRepair: v }))}
                />
              </div>

              <button
                type="button"
                disabled={
                  !destination || validating || (validation !== null && !validation.allowed)
                }
                aria-label={`${t('recovery.saveButton')} (${startLabel})`}
                title={
                  !destination
                    ? t('recovery.destinationRequired')
                    : validation && !validation.allowed
                      ? t('recovery.overlapReason')
                      : undefined
                }
                onClick={() => {
                  startRecovery.mutate(
                    {
                      selection: recoverAll
                        ? {
                            kind: 'all',
                            filter: selection.allFilter ?? resolveFilter(),
                          }
                        : { kind: 'nodes', nodeIds },
                      destination,
                      options,
                    },
                    {
                      onError: (err) => toast.error(err.message),
                    },
                  );
                }}
                className="inline-flex h-9 min-w-[116px] items-center justify-center rounded-[6px] bg-[#1677FF] px-6 text-[13.5px] font-semibold text-white shadow-xs transition-all hover:bg-[#0958D9] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 dark:bg-primary dark:text-primary-foreground dark:hover:bg-primary-hover"
              >
                {t('recovery.saveButton')}
              </button>
            </div>
          </>
        )}
      </div>

      {/* Insufficient Disk Space Modal Gate (reference: recovery-insufficient-space-gate_d768e6b3.png) */}
      <Dialog open={spaceModalOpen} onOpenChange={setSpaceModalOpen}>
        <DialogContent
          className="max-w-[440px] rounded-[12px] border border-[#E2E8F0] bg-white p-6 shadow-2xl dark:border-border dark:bg-surface-elevated"
          aria-label={t('recovery.insufficientSpace')}
        >
          <div className="flex items-start gap-4">
            <SaveDestinationIcon withWarning />
            <div className="min-w-0 flex-1">
              <DialogHeader>
                <DialogTitle className="text-[16px] font-bold text-[#1E293B] dark:text-foreground">
                  {t('recovery.insufficientSpace')}
                </DialogTitle>
                <DialogDescription className="mt-1.5 text-[13px] leading-relaxed text-[#64748B] dark:text-foreground-secondary">
                  {t('recovery.insufficientSpaceBody', {
                    needed:
                      validation?.neededBytes && validation?.freeBytes
                        ? formatBytes(
                            Math.max(0, validation.neededBytes - validation.freeBytes),
                            i18n.language,
                          )
                        : formatBytes(neededBytes, i18n.language),
                  })}
                </DialogDescription>
              </DialogHeader>
            </div>
          </div>
          <DialogFooter className="mt-5 flex justify-end gap-2.5">
            <button
              type="button"
              onClick={() => setSpaceModalOpen(false)}
              className="h-9 min-w-[104px] rounded-[6px] bg-[#EBF3FF] px-5 text-[13px] font-semibold text-[#0958D9] transition-colors hover:bg-[#DCE8FF] dark:bg-surface-tint dark:text-primary dark:hover:bg-surface-selected"
            >
              {t('common.continue')}
            </button>
            <button
              type="button"
              onClick={() => {
                setSpaceModalOpen(false);
                handlePickFolder();
              }}
              className="h-9 min-w-[104px] rounded-[6px] bg-[#1677FF] px-5 text-[13px] font-semibold text-white transition-colors hover:bg-[#0958D9] dark:bg-primary dark:text-primary-foreground dark:hover:bg-primary-hover"
            >
              {t('recovery.reselect')}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function CancelRecoveryButton({ jobId }: { jobId: string }): React.JSX.Element {
  const { t } = useTranslation();
  const cancel = useCancelRecovery(jobId);
  const [confirmOpen, setConfirmOpen] = React.useState(false);

  const requestCancel = (): void => {
    setConfirmOpen(false);
    cancel.mutate();
  };

  return (
    <>
      <Button
        variant="dangerOutline"
        onClick={() => setConfirmOpen(true)}
        loading={cancel.isPending}
      >
        {t('recovery.cancel')}
      </Button>
      <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <DialogContent className="max-w-sm" aria-label={t('recovery.cancelConfirm')}>
          <DialogHeader>
            <DialogTitle className="text-base">{t('recovery.cancelConfirm')}</DialogTitle>
            <DialogDescription>{t('recovery.cancelBody')}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setConfirmOpen(false)}>
              {t('common.continue')}
            </Button>
            <Button variant="danger" onClick={requestCancel}>
              {t('recovery.cancel')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

function RecoveryQueue({ jobId }: { jobId: string }): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const items = useRecoveryItems(jobId);
  const [visibleCount, setVisibleCount] = React.useState(100);
  const all = items.data ?? [];
  return (
    <div className="space-y-5">
      <QueueProgress jobId={jobId} />
      <div className="space-y-1.5">
        <h2 className="text-base font-semibold">{t('recovery.queue')}</h2>
        <div className="max-h-72 overflow-y-auto rounded-lg border border-border">
          {all.slice(0, visibleCount).map((item) => (
            <div
              key={item.nodeId}
              className="flex items-center gap-3 border-b border-border px-4 py-2 text-sm last:border-0"
            >
              <span className="min-w-0 flex-1 truncate" title={item.name}>
                {item.name}
              </span>
              <span className="shrink-0 text-xs text-foreground-secondary tabular">
                {formatBytes(item.sizeBytes, i18n.language)}
              </span>
              <QueueStateBadge state={item.state} reasonKey={item.error?.reasonKey} />
            </div>
          ))}
          {all.length === 0 ? <Skeleton className="m-2 h-8" /> : null}
          {all.length > visibleCount ? (
            <button
              type="button"
              className="w-full py-2 text-xs text-primary hover:bg-surface-hover"
              onClick={() => setVisibleCount((c) => Math.min(c + 200, all.length))}
            >
              {t('recovery.showMoreItems', {
                shown: Math.min(visibleCount, all.length),
                total: all.length,
              })}
            </button>
          ) : null}
        </div>
      </div>
      <QueueReport jobId={jobId} />
    </div>
  );
}

function QueueProgress({ jobId }: { jobId: string }): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const progress = useRecoveryProgress(jobId);
  if (!progress) return <Skeleton className="h-20" />;
  const done = progress.status !== 'running' && progress.status !== 'validating';
  return (
    <div className="space-y-2 rounded-lg border border-border bg-surface p-4">
      <div className="flex items-center justify-between text-sm">
        <span className="font-medium">
          {done ? t('recovery.reportTitle') : t('recovery.progressOverall')}
        </span>
        <span className="font-semibold text-primary tabular">
          {formatPercent(progress.overallProgress, i18n.language)}
        </span>
      </div>
      <Progress
        value={progress.overallProgress * 100}
        aria-label={t('recovery.progressOverall')}
      />
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-foreground-secondary tabular">
        {progress.currentFile ? (
          <span className="max-w-64 truncate">
            {t('recovery.progressFile')}: {progress.currentFile}
          </span>
        ) : null}
        <span>
          {progress.completedCount}/{progress.totalCount}
        </span>
        {progress.failedCount > 0 ? (
          <span className="text-danger">{progress.failedCount} ✕</span>
        ) : null}
        {progress.verifying ? (
          <span className="text-primary">{t('recovery.queueItem.verifying')}…</span>
        ) : null}
        {!done && progress.etaSeconds ? (
          <span>{formatDuration(progress.etaSeconds, i18n.language)}</span>
        ) : null}
      </div>
    </div>
  );
}

function QueueReport({ jobId }: { jobId: string }): React.JSX.Element | null {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const { scanId = '' } = useParams();
  const { data } = useRecoveryReport(jobId);
  if (!data || data.status === 'running' || data.status === 'validating') return null;
  const success = data.status === 'completed';
  const partial = data.status === 'completed-partial';
  return (
    <div
      className={cn(
        'space-y-4 rounded-lg border p-5',
        success
          ? 'border-success/40 bg-success-muted'
          : partial
            ? 'border-warning/40 bg-warning-muted'
            : 'border-danger/40 bg-danger-muted',
      )}
      role="status"
    >
      <div className="flex items-center gap-3">
        {success ? (
          <CheckCircle2Icon className="size-6 text-success" aria-hidden />
        ) : partial ? (
          <CircleAlertIcon className="size-6 text-warning" aria-hidden />
        ) : (
          <XCircleIcon className="size-6 text-danger" aria-hidden />
        )}
        <div>
          <h2 className="text-lg font-semibold">
            {success
              ? t('recovery.reportSuccess')
              : partial
                ? t('recovery.reportPartial')
                : t('recovery.reportFailed')}
          </h2>
          <p className="text-sm tabular">
            {t('recovery.reportSummary', {
              done: data.processedTotal - data.failed.length - data.skipped.length,
              total: data.processedTotal,
              failed: data.failed.length,
            })}
          </p>
        </div>
      </div>
      <div className="grid grid-cols-3 gap-3">
        <StatTile
          label={t('recovery.queueItem.done')}
          value={String(data.succeeded.length)}
          tone="success"
        />
        <StatTile
          label={t('recovery.queueItem.failed')}
          value={String(data.failed.length)}
          tone={data.failed.length > 0 ? 'danger' : 'default'}
        />
        <StatTile label={t('recovery.queueItem.skipped')} value={String(data.skipped.length)} />
      </div>
      {data.failed.length > 0 ? (
        <div className="max-h-40 overflow-y-auto rounded-md border border-border bg-surface p-2 text-xs">
          {data.failed.slice(0, 20).map((f) => (
            <div key={f.nodeId} className="flex items-center justify-between gap-3 py-1">
              <span className="truncate">{f.name}</span>
              <span className="text-danger">
                {f.error?.reasonKey ? t(f.error.reasonKey) : ''}
              </span>
            </div>
          ))}
        </div>
      ) : null}
      {data.truncatedItemLists ? (
        <p className="text-xs text-foreground-secondary">
          {t('recovery.listsTruncated', { total: data.processedTotal })}
        </p>
      ) : null}
      <div className="flex items-center justify-between gap-3">
        <span className="text-xs text-foreground-secondary tabular">
          {formatBytes(data.totalBytesWritten, i18n.language)}
          {data.verified ? ` · ${t('recovery.verifyOn')}` : ''}
        </span>
        <div className="flex gap-2">
          <Button variant="secondary" size="sm" onClick={() => navigate(`/results/${scanId}`)}>
            {t('common.close')}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => {
              const text = JSON.stringify(
                {
                  status: data.status,
                  startedAt: data.startedAt,
                  finishedAt: data.finishedAt,
                  succeeded: data.succeeded.map((s) => ({
                    name: s.name,
                    savedAs: s.savedAs ?? s.name,
                    sizeBytes: s.sizeBytes,
                  })),
                  failed: data.failed.map((f) => ({ name: f.name, reason: f.error?.code })),
                  skipped: data.skipped.map((s) => s.name),
                  totalBytesWritten: data.totalBytesWritten,
                  verified: data.verified,
                },
                null,
                2,
              );
              const blob = new Blob([text], { type: 'application/json' });
              const url = URL.createObjectURL(blob);
              const a = document.createElement('a');
              a.href = url;
              a.download = `recovery-report-${jobId}.json`;
              a.click();
              URL.revokeObjectURL(url);
              toast.success(t('recovery.exported'));
            }}
          >
            <DownloadIcon aria-hidden />
            {t('recovery.reportExport')}
          </Button>
        </div>
      </div>
    </div>
  );
}

function QueueStateBadge({
  state,
  reasonKey,
}: {
  state: RecoveryItem['state'];
  reasonKey?: string;
}): React.JSX.Element {
  const { t } = useTranslation();
  const map: Record<
    string,
    { variant: 'neutral' | 'primary' | 'success' | 'warning' | 'danger'; label: string }
  > = {
    pending: { variant: 'neutral', label: t('recovery.queueItem.pending') },
    running: { variant: 'primary', label: t('recovery.queueItem.running') },
    verifying: { variant: 'primary', label: t('recovery.queueItem.verifying') },
    done: { variant: 'success', label: t('recovery.queueItem.done') },
    failed: { variant: 'danger', label: t('recovery.queueItem.failed') },
    skipped: { variant: 'warning', label: t('recovery.queueItem.skipped') },
  };
  const cfg = map[state] ?? map.pending;
  const badge = (
    <Badge variant={cfg.variant}>
      {state === 'running' ? (
        <span
          className="mr-1 inline-block size-1.5 animate-status-pulse rounded-full bg-current"
          aria-hidden
        />
      ) : null}
      {cfg.label}
    </Badge>
  );
  if (state === 'failed' && reasonKey) {
    return (
      <span title={t(reasonKey)} className="shrink-0">
        {badge}
      </span>
    );
  }
  return <span className="shrink-0">{badge}</span>;
}
