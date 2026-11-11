import * as React from 'react';
import { useNavigate, useSearchParams } from 'react-router';
import { useTranslation } from 'react-i18next';
import { useQueryClient } from '@tanstack/react-query';
import {
  CheckCircle2Icon,
  CircleAlertIcon,
  PauseIcon,
  PlayIcon,
  SquareIcon,
  UnplugIcon,
} from 'lucide-react';
import { scanPhaseOf } from '@/connector/contract';
import {
  useCancelScan,
  usePauseScan,
  useResumeScan,
  useScanProgress,
  useScanState,
} from '@/queries/scan';
import { queryKeys } from '@/queries/client';
import { useScanSessionStore } from '@/stores/scan-session-store';
import { selectedCountOf, useSelectionStore } from '@/stores/selection-store';
import { formatBytes, formatDuration, formatPercent } from '@/lib/format';
import { ProgressGauge } from '@/components/ui/progress';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Hint } from '@/components/ui/tooltip';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { ResultsExplorer } from '@/features/results-explorer/results-explorer';
import { toast } from 'sonner';
import { ResultsPageFallback } from '@/features/results-explorer/page-fallback';

/**
 * Scan Progress — the 3-pane results layout live during the scan, plus the
 * corpus bottom control cluster (atlas §6, GT-scan §2/§7): circular gauge,
 * the reference status-line formats ("Quick/Advanced Scanning, Files Found:
 * N (size)" / "Remaining time: … | Reading sector: X/Y"), the full-width
 * "Searching: <path>" ticker, pause/stop, and the completion celebration.
 * The scan runs INSIDE the results surface, per the corpus state machine.
 */
export function ScanProgressPage(): React.JSX.Element {
  const [params] = useSearchParams();
  const scanIdFromUrl = params.get('scanId');
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const qc = useQueryClient();

  const active = useScanSessionStore((s) => s.active);
  const markStopped = useScanSessionStore((s) => s.markScanStopped);
  const markSeen = useScanSessionStore((s) => s.markCelebrationSeen);
  const seenFor = useScanSessionStore((s) => s.celebrationSeenFor);
  const setActiveStatus = useScanSessionStore((s) => s.setActiveStatus);
  const selection = useSelectionStore();

  const scanId = scanIdFromUrl ?? active?.scanId ?? null;

  const progress = useScanProgress(scanId);
  const scanState = useScanState(scanId);
  const pause = usePauseScan(scanId);
  const resume = useResumeScan(scanId);
  const cancel = useCancelScan(scanId);
  const [stopConfirmOpen, setStopConfirmOpen] = React.useState(false);
  const [stopStats, setStopStats] = React.useState<{ files: number; time: string } | null>(
    null,
  );

  const status = progress?.status ?? 'starting';
  const overall = progress?.overallProgress ?? 0;
  const completed = status === 'completed';
  const deviceLost = progress?.deviceConnected === false && status === 'paused';
  const phase = scanPhaseOf(progress?.stage ?? 'volume-analysis');

  const selectedCount = selectedCountOf({
    mode: selection.mode,
    ids: selection.ids,
    exclusions: selection.exclusions,
    allCount: selection.allCount,
  });

  // A completed scan is no longer 'active' — the sidebar stops pulsing.
  React.useEffect(() => {
    if (
      progress?.status === 'completed' ||
      progress?.status === 'cancelled' ||
      progress?.status === 'failed'
    ) {
      setActiveStatus(scanId ?? '', progress.status);
    }
  }, [progress?.status, scanId, setActiveStatus]);

  /** Elapsed since the engine recorded the scan start (real, not derived).
   *  A 1 s ticker keeps it live while the scan runs; completion freezes it
   *  at the engine's own timestamps (pure render — no Date.now() inline). */
  const [nowMs, setNowMs] = React.useState(() => Date.now());
  React.useEffect(() => {
    if (completed) return;
    const id = window.setInterval(() => setNowMs(Date.now()), 1_000);
    return () => window.clearInterval(id);
  }, [completed]);
  const usedTime = React.useMemo(() => {
    const startedAt = scanState.data?.startedAt;
    if (!startedAt) return null;
    const ref = scanState.data?.completedAt ? Date.parse(scanState.data.completedAt) : nowMs;
    const secs = Math.max(0, (ref - Date.parse(startedAt)) / 1000);
    return formatDuration(secs, i18n.language);
  }, [scanState.data, nowMs, i18n.language]);

  if (!scanId) {
    return <ResultsPageFallback />;
  }

  const sectorsScanned = Math.floor((progress?.bytesScanned ?? 0) / 512);
  const sectorsTotal = Math.floor((progress?.bytesTotal ?? 0) / 512);

  const onRecover = (): void => {
    if (selection.mode === 'all' || selection.ids.size === 0) {
      void navigate(`/recover/${scanId}?all=1`);
      return;
    }
    void navigate(`/recover/${scanId}?ids=1`);
  };

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      {/* Live results (partial) — browse-while-scanning (GT-scan §3.2) */}
      <div className="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
        <ResultsExplorer
          scanId={scanId}
          liveMode
          recoverableDuringScan={progress?.recoverableDuringScan ?? false}
        />
      </div>

      {/* "Searching: <path>" ticker — full-width light-blue strip above the
          control cluster (matching scan-midscan-210s.png; hidden when completed). */}
      {!completed && progress?.currentPath ? (
        <div
          className="flex h-7 shrink-0 items-center gap-2 border-t border-[#9FC1F7] bg-[#BBD5FF] px-4 text-xs font-medium text-[#0B4F9E] dark:border-border dark:bg-primary-muted dark:text-primary"
          role="status"
        >
          <span className="truncate">
            {t('scan.searchingFor', { path: progress.currentPath })}
          </span>
        </div>
      ) : null}

      {/* Unified bottom scan status cluster (matching scan-midscan-210s.png, scan-paused.png, and complete-celebration-modal.png) */}
      <div className="flex h-[68px] shrink-0 items-center gap-4 border-t border-[#C8DAF5] bg-[#E2ECFA] px-4 dark:border-border dark:bg-surface">
        <ProgressGauge
          value={overall}
          completed={completed}
          size={52}
          label={t('a11y.gauge')}
        />
        <div className="min-w-0 flex-1 space-y-1 text-xs">
          {/* Line 1: "Quick/Advanced Scanning , Files Found: N (size)" */}
          <div
            className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-sm"
            role="status"
            aria-live="polite"
          >
            <span className="font-bold text-foreground">
              {status === 'paused'
                ? `${t('scan.pausedTitle')} ,`
                : completed
                  ? `${t('scan.completedStatus')},`
                  : `${t(phase === 'quick' ? 'scan.quickScanning' : 'scan.advancedScanning', {
                      percent: formatPercent(overall, i18n.language),
                    })} ,`}
            </span>
            <span className="font-semibold tabular text-[#1677FF] dark:text-primary">
              {t('scan.filesFoundWithSize', {
                count: (progress?.filesFound ?? 0).toLocaleString(i18n.language),
                size: formatBytes(progress?.dataFoundBytes ?? 0, i18n.language),
              })}
            </span>
            {(progress?.errors ?? 0) > 0 ? (
              <Hint text={t('scan.badSectorMap')}>
                <Badge variant="danger">
                  <CircleAlertIcon aria-hidden />
                  {t('scan.errorsCount', { count: progress!.errors })}
                </Badge>
              </Hint>
            ) : null}
          </div>

          {/* Line 2: Inline Pause/Resume + Stop buttons + "Remaining time: NNmNNs | Reading sector: X/Y" */}
          {!completed ? (
            <div className="flex flex-wrap items-center gap-x-2.5 gap-y-0.5 text-foreground-secondary tabular">
              <div className="flex items-center gap-1">
                {status === 'running' ? (
                  <button
                    type="button"
                    aria-label={t('scan.pause')}
                    title={t('scan.pause')}
                    onClick={() => void pause.mutate()}
                    disabled={pause.isPending}
                    className="inline-flex size-5 items-center justify-center rounded-xs text-slate-700 transition-colors hover:bg-black/10 focus-visible:ring-2 focus-visible:ring-ring dark:text-foreground"
                  >
                    <PauseIcon className="size-3.5 fill-current" aria-hidden />
                  </button>
                ) : status === 'paused' && !deviceLost ? (
                  <button
                    type="button"
                    aria-label={t('scan.resume')}
                    title={t('scan.resume')}
                    onClick={() => void resume.mutate()}
                    disabled={resume.isPending}
                    className="inline-flex size-5 items-center justify-center rounded-xs text-[#1677FF] transition-colors hover:bg-black/10 focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <PlayIcon className="size-3.5 fill-current" aria-hidden />
                  </button>
                ) : null}
                <button
                  type="button"
                  aria-label={t('scan.stop')}
                  title={t('scan.stop')}
                  onClick={() => {
                    setStopStats({
                      files: progress?.filesFound ?? 0,
                      time:
                        usedTime ?? formatDuration(progress?.etaSeconds ?? 0, i18n.language),
                    });
                    setStopConfirmOpen(true);
                  }}
                  className="inline-flex size-5 items-center justify-center rounded-xs text-slate-700 transition-colors hover:bg-black/10 focus-visible:ring-2 focus-visible:ring-ring dark:text-foreground"
                >
                  <SquareIcon className="size-3.5 fill-current" aria-hidden />
                </button>
              </div>

              {status === 'paused' ? (
                <span className="font-medium text-danger">{t('scan.pausedTitle')}.</span>
              ) : progress?.etaSeconds != null && progress.etaSeconds > 0 ? (
                <span>
                  {t('scan.remaining', {
                    time: formatDuration(progress.etaSeconds, i18n.language),
                  })}
                </span>
              ) : null}

              <span className="text-border-strong" aria-hidden>
                |
              </span>
              <span>
                {t('scan.readingSector', {
                  current: sectorsScanned.toLocaleString(i18n.language),
                  total: sectorsTotal.toLocaleString(i18n.language),
                })}
              </span>
            </div>
          ) : usedTime ? (
            <div className="text-foreground-secondary tabular">
              <span>{t('scan.usedTime', { time: usedTime })}</span>
            </div>
          ) : null}
        </div>

        {/* Right primary CTA: "Recover All ..." */}
        <div className="flex items-center gap-2">
          <Button
            variant="primary"
            size="lg"
            className="min-w-[164px] [background-image:none] bg-[#1677FF] font-semibold text-white shadow-none hover:bg-[#0958D9]"
            onClick={() => onRecover()}
            disabled={!completed && !(progress?.recoverableDuringScan ?? false)}
            title={
              !completed && !(progress?.recoverableDuringScan ?? false)
                ? t('scan.recoverNowUnsafe')
                : undefined
            }
          >
            {selectedCount > 0
              ? t('results.status.recoverSelected', { count: selectedCount })
              : t('results.status.recoverAll')}
          </Button>
        </div>
      </div>

      {/* Device-lost reconnect prompt — modal, non-dismissable: the scan
          stays paused until the source device comes back. */}
      {deviceLost ? (
        <Dialog open onOpenChange={() => undefined}>
          <DialogContent
            showCloseButton={false}
            className="max-w-md"
            onEscapeKeyDown={(e) => e.preventDefault()}
            onInteractOutside={(e) => e.preventDefault()}
          >
            <div className="flex items-start gap-3">
              <UnplugIcon className="mt-0.5 size-5 shrink-0 text-warning" aria-hidden />
              <DialogHeader className="min-w-0">
                <DialogTitle className="text-base">{t('scan.deviceLostTitle')}</DialogTitle>
                <DialogDescription>{t('scan.deviceLostBody')}</DialogDescription>
              </DialogHeader>
            </div>
            <DialogFooter>
              <Button
                variant="primary"
                onClick={() => {
                  void resume.mutate(undefined, {
                    onSuccess: () => toast.success(t('scan.resume')),
                    onError: (err) => toast.error(err.message),
                  });
                }}
              >
                {t('scan.reconnect')}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      ) : null}

      {/* Stop confirmation (matching scan-stop-confirm-dialog.png & scan-after-stop-attempt.png) */}
      <Dialog open={stopConfirmOpen} onOpenChange={setStopConfirmOpen}>
        <DialogContent
          className="w-[500px] max-w-[92vw] rounded-xl p-6 text-center"
          aria-label={t('scan.stopTitle')}
        >
          <StopFolderIllustration />
          <DialogHeader className="items-center text-center">
            <DialogTitle className="text-base font-bold">{t('scan.stopTitle')}</DialogTitle>
          </DialogHeader>
          <div className="mt-3 grid grid-cols-2 items-center rounded-lg bg-[#F7F8FA] px-6 py-4 dark:bg-surface-muted">
            <div className="border-r border-border pr-4 text-center">
              <div className="text-xl font-bold tabular text-foreground">
                {(stopStats?.files ?? progress?.filesFound ?? 0).toLocaleString(i18n.language)}
              </div>
              <div className="mt-1 text-xs text-foreground-secondary">
                {t('scan.statsFiles')}
              </div>
            </div>
            <div className="pl-4 text-center">
              <div className="text-xl font-bold tabular text-foreground">
                {stopStats?.time ?? '00:00'}
              </div>
              <div className="mt-1 text-xs text-foreground-secondary">
                {t('scan.statsTime')}
              </div>
            </div>
          </div>
          <DialogDescription className="mt-3 text-left text-xs leading-relaxed text-foreground-secondary">
            {t('scan.stopBody')}
          </DialogDescription>
          <div className="mt-5 flex items-center justify-center gap-3">
            <Button
              variant="danger"
              className="min-w-[110px] [background-image:none] bg-[#FDE2E2] text-[#C92A2A] shadow-none hover:bg-[#F8C2C2] dark:bg-danger-muted dark:text-danger dark:hover:bg-danger/25"
              onClick={() => {
                setStopConfirmOpen(false);
                void cancel.mutate(undefined, {
                  onSuccess: () => {
                    markStopped();
                    void qc.invalidateQueries({ queryKey: queryKeys.sessions });
                    toast(t('common.stop'));
                    void navigate(`/results/${scanId}`);
                  },
                });
              }}
            >
              {t('scan.stopConfirm')}
            </Button>
            <Button
              variant="primary"
              className="min-w-[110px] [background-image:none] bg-[#0066FF] text-white shadow-none hover:bg-[#0052CC]"
              onClick={() => setStopConfirmOpen(false)}
            >
              {t('scan.keepGoing')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {/* Celebration modal (matching complete-celebration-modal.png: 520×375 card,
          trophy illustration, 2-column Files found | Size gray box, green-check
          recoverable callout banner, and centered "Go to Recover" button). */}
      <Dialog
        open={completed && seenFor !== scanId}
        onOpenChange={(open) => {
          if (!open && scanId) {
            markSeen(scanId);
            void navigate(`/results/${scanId}`);
          }
        }}
      >
        <DialogContent
          showCloseButton
          className="w-[520px] max-w-[92vw] rounded-xl p-6 text-center"
          aria-label={t('scan.completedTitle')}
          onEscapeKeyDown={(e) => e.preventDefault()}
        >
          <TrophyCelebrationIllustration />
          <DialogTitle className="mt-2 text-base font-bold">
            {t('scan.congratulations')} {t('scan.completedStatus')}!
            <span className="sr-only"> ({t('scan.completedTitle')})</span>
          </DialogTitle>
          <div className="mt-4 grid grid-cols-2 items-center rounded-lg bg-[#F7F8FA] px-6 py-4 dark:bg-surface-muted">
            <div className="border-r border-border pr-4 text-center">
              <div className="text-xl font-bold tabular text-foreground">
                {(progress?.filesFound ?? 0).toLocaleString(i18n.language)}
              </div>
              <div className="mt-1 text-xs text-foreground-secondary">
                {t('scan.statsFiles')}
              </div>
            </div>
            <div className="pl-4 text-center">
              <div className="text-xl font-bold tabular text-foreground">
                {formatBytes(progress?.dataFoundBytes ?? 0, i18n.language)}
              </div>
              <div className="mt-1 text-xs text-foreground-secondary">
                {t('scan.statsSize')}
              </div>
            </div>
          </div>
          <div className="mt-4 flex items-center gap-2.5 rounded-md bg-[#EBF3FF] px-4 py-2.5 text-left dark:bg-primary-muted">
            <CheckCircle2Icon className="size-4 shrink-0 text-[#16A34A]" aria-hidden />
            <DialogDescription className="text-xs font-semibold text-[#0066FF] dark:text-primary">
              {t('scan.completedVerdict')}
            </DialogDescription>
          </div>
          <div className="mt-5 flex justify-center">
            <Button
              variant="primary"
              size="lg"
              className="min-w-[140px] [background-image:none] bg-[#0066FF] px-6 text-white shadow-none hover:bg-[#0052CC]"
              onClick={() => {
                markSeen(scanId);
                void navigate(`/results/${scanId}`);
              }}
            >
              {t('scan.goToRecover')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** Reference folder-with-files + amber pause badge illustration (scan-stop-confirm-dialog.png). */
function StopFolderIllustration(): React.JSX.Element {
  return (
    <div className="mx-auto mb-2 flex h-16 w-20 items-center justify-center" aria-hidden>
      <svg width="76" height="64" viewBox="0 0 76 64" fill="none">
        <rect
          x="14"
          y="10"
          width="20"
          height="24"
          rx="3"
          transform="rotate(-10 14 10)"
          fill="#7C5CFC"
        />
        <rect x="26" y="7" width="22" height="26" rx="3" fill="#FF9F43" />
        <rect
          x="38"
          y="11"
          width="20"
          height="24"
          rx="3"
          transform="rotate(10 38 11)"
          fill="#FF5B79"
        />
        <path
          d="M12 22C12 19.7909 13.7909 18 16 18H29L34 23H56C58.2091 23 60 24.7909 60 27V49C60 51.2091 58.2091 53 56 53H16C13.7909 53 12 51.2091 12 49V22Z"
          fill="#CED9EA"
        />
        <circle cx="53" cy="45" r="11" fill="#FFB830" />
        <rect x="49.5" y="40" width="2.6" height="10" rx="1.3" fill="white" />
        <rect x="54" y="40" width="2.6" height="10" rx="1.3" fill="white" />
      </svg>
    </div>
  );
}

/** Reference silver-and-blue trophy on pedestal with confetti illustration (complete-celebration-modal.png). */
function TrophyCelebrationIllustration(): React.JSX.Element {
  return (
    <div className="mx-auto mb-1 flex h-20 w-36 items-center justify-center" aria-hidden>
      <svg width="144" height="80" viewBox="0 0 144 80" fill="none">
        {/* Confetti accents */}
        <circle cx="30" cy="22" r="2.5" fill="#C084FC" opacity="0.6" />
        <circle cx="116" cy="26" r="3" fill="#FBBF24" opacity="0.6" />
        <path d="M18 42L24 32" stroke="#818CF8" strokeWidth="2.5" strokeLinecap="round" />
        <path d="M98 16L104 26" stroke="#818CF8" strokeWidth="2.5" strokeLinecap="round" />
        <circle cx="94" cy="48" r="2" fill="#60A5FA" />
        {/* Pedestal */}
        <path d="M46 54L72 68L98 54L72 42L46 54Z" fill="#60A5FA" />
        <path d="M46 54V60L72 74L98 60V54L72 68L46 54Z" fill="#2563EB" />
        {/* Trophy handles */}
        <path
          d="M55 24H49C46.5 24 45 26 45 28.5C45 33 49 37 54 38"
          stroke="#93C5FD"
          strokeWidth="3"
          strokeLinecap="round"
        />
        <path
          d="M89 24H95C97.5 24 99 26 99 28.5C99 33 95 37 90 38"
          stroke="#93C5FD"
          strokeWidth="3"
          strokeLinecap="round"
        />
        {/* Trophy cup body */}
        <path
          d="M54 18H90V32C90 41.9411 81.9411 50 72 50C62.0589 50 54 41.9411 54 32V18Z"
          fill="#DBEAFE"
          stroke="#60A5FA"
          strokeWidth="2"
        />
        <rect x="68" y="49" width="8" height="7" rx="1" fill="#93C5FD" />
        <rect x="61" y="55" width="22" height="5" rx="2" fill="#3B82F6" />
      </svg>
    </div>
  );
}
