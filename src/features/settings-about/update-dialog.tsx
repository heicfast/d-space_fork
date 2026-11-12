import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { DownloadIcon, RefreshCwIcon } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Progress } from '@/components/ui/progress';
import { isTauriRuntime } from '@/lib/runtime';
import { useAppInfo } from '@/queries/system';
import { cn } from '@/lib/utils';

/**
 * Auto-updater dialog (D-Space feature carried into the DiskWizard shell).
 * Ed25519-signed updates over the GitHub releases feed; install is passive
 * and relaunches when ready. In browser/mock mode the check fails honestly
 * with a network-style message (the updater only exists in the desktop
 * shell).
 */
type Phase =
  | { readonly kind: 'idle' }
  | { readonly kind: 'checking' }
  | { readonly kind: 'upToDate' }
  | { readonly kind: 'available'; readonly update: Update }
  | { readonly kind: 'installing'; readonly received: number; readonly total: number | null }
  | { readonly kind: 'installed' }
  | { readonly kind: 'error'; readonly message: string };

export function UpdateDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}): React.JSX.Element {
  const { t } = useTranslation();
  const [phase, setPhase] = React.useState<Phase>({ kind: 'idle' });
  const { data: appInfo } = useAppInfo();

  const runCheck = React.useCallback(async () => {
    if (!isTauriRuntime()) {
      setPhase({
        kind: 'error',
        message: t('update.errorNetwork', { detail: 'browser' }),
      });
      return;
    }
    setPhase({ kind: 'checking' });
    try {
      const update = await check();
      if (update) {
        setPhase({ kind: 'available', update });
      } else {
        setPhase({ kind: 'upToDate' });
      }
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      const message = /signature|verify|key/i.test(detail)
        ? t('update.errorVerify')
        : /network|dns|connect|timeout|fetch|offline/i.test(detail)
          ? t('update.errorNetwork', { detail })
          : t('update.errorUnknown', { detail });
      setPhase({ kind: 'error', message });
    }
  }, [t]);

  const install = React.useCallback(
    async (update: Update) => {
      setPhase({ kind: 'installing', received: 0, total: null });
      try {
        let total: number | null = null;
        await update.downloadAndInstall((event) => {
          switch (event.event) {
            case 'Started':
              total = event.data.contentLength ?? null;
              setPhase({ kind: 'installing', received: 0, total });
              break;
            case 'Progress':
              setPhase((prev) => ({
                kind: 'installing',
                received:
                  prev.kind === 'installing'
                    ? prev.received + event.data.chunkLength
                    : event.data.chunkLength,
                total,
              }));
              break;
            case 'Finished':
              setPhase({ kind: 'installed' });
              break;
          }
        });
        await relaunch();
      } catch (error) {
        const detail = error instanceof Error ? error.message : String(error);
        setPhase({ kind: 'error', message: t('update.errorUnknown', { detail }) });
      }
    },
    [t],
  );

  const resetOnClose = (next: boolean): void => {
    if (!next) setPhase({ kind: 'idle' });
    onOpenChange(next);
  };

  const busy = phase.kind === 'checking' || phase.kind === 'installing';

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => (busy ? resetOnClose(false) : resetOnClose(next))}
    >
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>
            {phase.kind === 'available' ? t('update.availableTitle') : t('update.check')}
          </DialogTitle>
          {phase.kind === 'available' ? (
            <DialogDescription className="whitespace-pre-line">
              {t('update.availableBody', { version: phase.update.version })}
              {'\n'}
              {t('update.currentVersion', { version: appInfo?.version ?? '?' })}
            </DialogDescription>
          ) : null}
        </DialogHeader>

        <div className="min-h-16 space-y-3">
          {phase.kind === 'checking' || phase.kind === 'idle' ? (
            <p className="flex items-center gap-2 text-sm text-foreground-secondary">
              <RefreshCwIcon className="size-4 animate-spin" aria-hidden />
              {t('update.checking')}
            </p>
          ) : null}
          {phase.kind === 'upToDate' ? (
            <p className="text-sm text-foreground-secondary">
              {t('update.upToDate', { version: appInfo?.version ?? '?' })}
            </p>
          ) : null}
          {phase.kind === 'error' ? (
            <p className="text-sm text-status-error" role="alert">
              {phase.message}
            </p>
          ) : null}
          {phase.kind === 'installing' ? (
            <div className="space-y-2">
              <Progress
                value={
                  phase.total && phase.total > 0
                    ? Math.min(100, Math.round((phase.received / phase.total) * 100))
                    : undefined
                }
                aria-label={t('update.installing')}
              />
              <p className="text-xs text-foreground-secondary">{t('update.installing')}</p>
            </div>
          ) : null}
          {phase.kind === 'installed' ? (
            <p className="text-sm text-foreground-secondary">{t('update.installed')}</p>
          ) : null}
        </div>

        <DialogFooter>
          {phase.kind === 'available' ? (
            <>
              <Button variant="ghost" onClick={() => resetOnClose(false)} disabled={busy}>
                {t('update.dismiss')}
              </Button>
              <Button onClick={() => void install(phase.update)} disabled={busy}>
                <DownloadIcon className="size-3.5" aria-hidden />
                {t('update.install')}
              </Button>
            </>
          ) : (
            <>
              <Button
                variant="ghost"
                onClick={() => resetOnClose(false)}
                disabled={busy}
                className={cn(busy && 'invisible')}
              >
                {t('update.close')}
              </Button>
              <Button onClick={() => void runCheck()} disabled={busy}>
                <RefreshCwIcon
                  className={cn('size-3.5', phase.kind === 'checking' && 'animate-spin')}
                  aria-hidden
                />
                {t('update.check')}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
