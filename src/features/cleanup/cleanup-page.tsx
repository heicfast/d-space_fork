import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import {
  AlertTriangleIcon,
  FileIcon,
  FolderIcon,
  ShieldCheckIcon,
  Trash2Icon,
  Undo2Icon,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { EmptyState } from '@/components/ui/empty-state';
import { Hint } from '@/components/ui/tooltip';
import { formatDate, formatBytes } from '@/lib/format';
import {
  useCleanupExpired,
  useExpiredStaging,
  usePermanentDelete,
  useStaging,
  useUndoResolve,
  useUndoStaging,
} from '@/queries/cleanup';
import type {
  CleanupReport,
  CleanupStagedItem,
  CleanupUndoOutcome,
} from '@/connector/contract';
import { cn } from '@/lib/utils';

/**
 * Cleanup Bin — the D-Space deletion safety net in the DiskWizard design
 * language: everything staged is restorable for 24 h; permanent deletion
 * demands a typed phrase; the batch cleanup honors the engine's threshold
 * abort. The whole page is connector-driven (real staging engine).
 */
export function CleanupPage(): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const staging = useStaging();
  const expired = useExpiredStaging();
  const undo = useUndoStaging();
  const [conflict, setConflict] = React.useState<{
    id: number;
    outcome: Extract<CleanupUndoOutcome, { kind: 'conflict' }>;
  } | null>(null);
  const [permTarget, setPermTarget] = React.useState<CleanupStagedItem | null>(null);
  const [thresholdOpen, setThresholdOpen] = React.useState(false);

  const onUndo = (id: number): void => {
    undo.mutate(id, {
      onSuccess: (outcome) => {
        if (outcome.kind === 'conflict') {
          setConflict({ id, outcome });
        } else if (outcome.kind === 'idempotent') {
          toast.info(t('cleanup.undoIdempotent'));
        } else {
          toast.success(t('cleanup.undoSuccess', { path: outcome.originalPath }));
        }
      },
      onError: (err) => toast.error(err.message),
    });
  };

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto">
      <header className="px-8 pt-7 pb-4">
        <h1 className="text-xl font-semibold tracking-tight">{t('cleanup.binTitle')}</h1>
        <p className="mt-1 text-sm text-foreground-secondary">{t('cleanup.binSubtitle')}</p>
      </header>

      <div className="mx-8 mb-8 space-y-4">
        {staging.isLoading ? (
          <div className="h-28 animate-pulse rounded-[10px] bg-surface-muted" />
        ) : (staging.data ?? []).length === 0 ? (
          <EmptyState
            icon={ShieldCheckIcon}
            title={t('cleanup.emptyTitle')}
            description={t('cleanup.emptyBody')}
          />
        ) : (
          <ul className="divide-y divide-border overflow-hidden rounded-[10px] border border-border bg-white dark:bg-surface">
            {(staging.data ?? []).map((item) => (
              <li
                key={item.id}
                className="flex items-center gap-3 px-4 py-3 hover:bg-[#F5F8FF] dark:hover:bg-surface-hover/50"
              >
                {item.isDir ? (
                  <FolderIcon className="size-4.5 shrink-0 text-[#1677FF]" aria-hidden />
                ) : (
                  <FileIcon
                    className="size-4.5 shrink-0 text-foreground-secondary"
                    aria-hidden
                  />
                )}
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-xs" title={item.originalPath}>
                    {item.originalPath}
                  </p>
                  <p className="mt-0.5 flex flex-wrap items-center gap-2 text-[11px] text-foreground-secondary">
                    <span className="tabular">
                      {formatBytes(item.sizeBytes, i18n.language)}
                    </span>
                    <span
                      className={cn(
                        'inline-flex items-center gap-1 rounded-full px-1.5 py-px text-[10px] font-medium',
                        item.fallbackTier === 'cross_volume'
                          ? 'bg-[#EBF2FF] text-[#0958D9] dark:bg-primary/15 dark:text-primary'
                          : 'bg-emerald-50 text-emerald-700 dark:bg-emerald-500/10 dark:text-emerald-400',
                      )}
                    >
                      <Hint
                        text={
                          item.fallbackTier === 'cross_volume'
                            ? t('cleanup.tierCrossVolumeHint')
                            : t('cleanup.tierNormalHint')
                        }
                      >
                        {item.fallbackTier === 'cross_volume'
                          ? t('cleanup.tierCrossVolume')
                          : t('cleanup.tierNormal')}
                      </Hint>
                    </span>
                    <span>
                      {t('cleanup.expires', { time: remainingHours(item.expiresAt) })}
                    </span>
                  </p>
                </div>
                <Button variant="secondary" size="sm" onClick={() => onUndo(item.id)}>
                  <Undo2Icon aria-hidden />
                  {undo.isPending && undo.variables === item.id
                    ? t('cleanup.undoBusy')
                    : t('cleanup.undo')}
                </Button>
                <Button
                  variant="dangerOutline"
                  size="sm"
                  aria-label={t('cleanup.permanentDelete')}
                  onClick={() => setPermTarget(item)}
                >
                  <Trash2Icon aria-hidden />
                </Button>
              </li>
            ))}
          </ul>
        )}

        {(expired.data ?? []).length > 0 ? (
          <section className="rounded-[10px] border border-border bg-white p-4 dark:bg-surface">
            <h2 className="flex items-center gap-2 text-sm font-semibold">
              <AlertTriangleIcon className="size-4 text-amber-500" aria-hidden />
              {t('cleanup.expiredSection')}
            </h2>
            <ul className="mt-2 space-y-1">
              {(expired.data ?? []).slice(0, 6).map((item) => (
                <li
                  key={item.id}
                  className="flex items-center justify-between gap-3 text-xs text-foreground-secondary"
                >
                  <span className="truncate font-mono" title={item.originalPath}>
                    {item.originalPath}
                  </span>
                  <span className="tabular shrink-0">
                    {formatBytes(item.sizeBytes, i18n.language)}
                  </span>
                </li>
              ))}
              {(expired.data ?? []).length > 6 ? (
                <li className="text-[11px] text-foreground-disabled">
                  {t('cleanup.expiredCount', { count: (expired.data ?? []).length - 6 })}
                </li>
              ) : null}
            </ul>
            <ExpiredCleanupButton
              count={(expired.data ?? []).length}
              onThreshold={() => setThresholdOpen(true)}
            />
          </section>
        ) : null}
      </div>

      <UndoConflictDialog conflict={conflict} onClose={() => setConflict(null)} />
      <PermanentDeleteDialog item={permTarget} onClose={() => setPermTarget(null)} />
      <ThresholdConfirmDialog
        open={thresholdOpen}
        count={(expired.data ?? []).length}
        onClose={() => setThresholdOpen(false)}
      />
    </div>
  );
}

function remainingHours(iso: string): string {
  const ms = Date.parse(iso) - Date.now();
  if (ms <= 0) return '0 h';
  return `${Math.max(1, Math.round(ms / 3_600_000))} h`;
}

function ExpiredCleanupButton({
  count,
  onThreshold,
}: {
  count: number;
  onThreshold: (report: CleanupReport) => void;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const cleanup = useCleanupExpired();
  return (
    <Button
      variant="danger"
      size="sm"
      className="mt-3"
      disabled={cleanup.isPending}
      onClick={() =>
        cleanup.mutate(
          { ratePerSec: 20, force: false },
          {
            onSuccess: (report) => {
              if (report.abortedThreshold) {
                onThreshold(report);
              } else {
                toast.success(
                  t('cleanup.cleanupReport', {
                    deleted: report.deleted,
                    bytes: formatBytes(report.totalBytes, i18n.language),
                    ms: report.elapsedMs,
                  }),
                );
              }
            },
          },
        )
      }
    >
      <Trash2Icon aria-hidden />
      {t('cleanup.cleanupExpired', { count })}
    </Button>
  );
}

function ThresholdConfirmDialog({
  open,
  count,
  onClose,
}: {
  open: boolean;
  count: number;
  onClose: () => void;
}): React.JSX.Element | null {
  const { t, i18n } = useTranslation();
  const cleanup = useCleanupExpired();
  if (!open) return null;
  return (
    <Dialog open onOpenChange={(next) => (next ? undefined : onClose())}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('cleanup.expiredSection')}</DialogTitle>
          <DialogDescription>{t('cleanup.cleanupThreshold', { count })}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            {t('common.close')}
          </Button>
          <Button
            variant="danger"
            loading={cleanup.isPending}
            onClick={() =>
              cleanup.mutate(
                { ratePerSec: 20, force: true },
                {
                  onSuccess: (result) => {
                    toast.success(
                      t('cleanup.cleanupReport', {
                        deleted: result.deleted,
                        bytes: formatBytes(result.totalBytes, i18n.language),
                        ms: result.elapsedMs,
                      }),
                    );
                    onClose();
                  },
                },
              )
            }
          >
            {t('cleanup.cleanupExpired', { count })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function UndoConflictDialog({
  conflict,
  onClose,
}: {
  conflict: { id: number; outcome: Extract<CleanupUndoOutcome, { kind: 'conflict' }> } | null;
  onClose: () => void;
}): React.JSX.Element | null {
  const { t, i18n } = useTranslation();
  const resolve = useUndoResolve();
  if (conflict === null) return null;
  const outcome = conflict.outcome;
  const snapshotColumn = (
    label: string,
    snap: {
      path: string;
      sizeBytes: number;
      modifiedAt: string;
      blake3First4kbHex: string | null;
      isDir: boolean;
    },
  ) => (
    <div className="min-w-0 flex-1 rounded-[8px] border border-border p-3">
      <p className="text-xs font-semibold">{label}</p>
      <p className="mt-1 truncate font-mono text-[11px]" title={snap.path}>
        {snap.path}
      </p>
      <p className="mt-1 text-[11px] text-foreground-secondary tabular">
        {formatBytes(snap.sizeBytes, i18n.language)} ·{' '}
        {formatDate(snap.modifiedAt, i18n.language)}
      </p>
      {snap.blake3First4kbHex ? (
        <p
          className="mt-1 truncate font-mono text-[10px] text-foreground-disabled"
          title={t('cleanup.conflictHashHint')}
        >
          {snap.blake3First4kbHex.slice(0, 12)}…
        </p>
      ) : null}
    </div>
  );
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('cleanup.undoConflictTitle')}</DialogTitle>
          <DialogDescription>{outcome.originalPath}</DialogDescription>
        </DialogHeader>
        <div className="flex gap-3">
          {snapshotColumn(t('cleanup.undoConflictStaged'), outcome.staged)}
          {snapshotColumn(t('cleanup.undoConflictTarget'), outcome.target)}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={() => onClose()}>
            {t('cleanup.conflictCancel')}
          </Button>
          <Button
            variant="secondary"
            loading={resolve.isPending}
            onClick={() =>
              resolve.mutate(
                { id: conflict.id, resolution: 'overwrite' },
                { onSuccess: () => onClose() },
              )
            }
          >
            {t('cleanup.conflictOverwrite')}
          </Button>
          <Button
            variant="primary"
            loading={resolve.isPending}
            onClick={() =>
              resolve.mutate(
                { id: conflict.id, resolution: 'keep_both' },
                { onSuccess: () => onClose() },
              )
            }
          >
            {t('cleanup.conflictKeepBoth')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function PermanentDeleteDialog({
  item,
  onClose,
}: {
  item: CleanupStagedItem | null;
  onClose: () => void;
}): React.JSX.Element | null {
  if (!item) return null;
  return <PermanentDeleteForm key={item.id} item={item} onClose={onClose} />;
}

function PermanentDeleteForm({
  item,
  onClose,
}: {
  item: CleanupStagedItem;
  onClose: () => void;
}): React.JSX.Element {
  const { t } = useTranslation();
  const permanent = usePermanentDelete();
  const [phrase, setPhrase] = React.useState('');
  const [wipe, setWipe] = React.useState(false);
  const fileName = item.originalPath.split('\\').pop() ?? '';
  const matches = phrase.trim().toLowerCase() === fileName.toLowerCase();
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('cleanup.permanentTitle')}</DialogTitle>
          <DialogDescription>{t('cleanup.permanentBody')}</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div className="space-y-1.5">
            <Label htmlFor="perm-phrase">{t('cleanup.permanentPhrase')}</Label>
            <Input
              id="perm-phrase"
              value={phrase}
              onChange={(e) => setPhrase(e.target.value)}
              placeholder={fileName}
              autoComplete="off"
              aria-invalid={!matches && phrase.length > 0}
            />
          </div>
          <div className="flex items-start gap-2">
            <Checkbox
              id="perm-wipe"
              checked={wipe}
              onCheckedChange={(v) => setWipe(v === true)}
              disabled={item.isDir}
              aria-label={t('cleanup.permanentWipe')}
            />
            <div className="text-xs">
              <Label htmlFor="perm-wipe" className="font-normal">
                {t('cleanup.permanentWipe')}
              </Label>
              <p className="mt-0.5 text-foreground-disabled">
                {t('cleanup.permanentWipeHint')}
              </p>
            </div>
          </div>
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            {t('common.close')}
          </Button>
          <Button
            variant="danger"
            disabled={!matches || permanent.isPending}
            onClick={() =>
              permanent.mutate(
                { id: item.id, confirmPhrase: phrase, secureWipe: wipe },
                { onSuccess: () => onClose() },
              )
            }
          >
            <Trash2Icon aria-hidden />
            {t('cleanup.permanentDelete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export { ThresholdConfirmDialog };
