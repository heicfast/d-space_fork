import * as React from 'react';
import { useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';
import { ArchiveRestoreIcon, Clock3Icon, FolderSyncIcon, ShieldCheckIcon } from 'lucide-react';
import { useLoadSession, useSessions } from '@/queries/sessions';
import { formatDate, formatInteger } from '@/lib/format';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { toast } from 'sonner';

/**
 * Scan Snapshot page (renamed from ChronoSnap Backup in the v4 pass) —
 * accessible from the File Backup sidebar section. Displays the landing
 * overview and saved recovery checkpoints.
 */
export function BackupPage(): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const sessions = useSessions();
  const loadSession = useLoadSession();

  const handleOpenSession = (fileName: string): void => {
    loadSession.mutate(fileName, {
      onSuccess: ({ scanId }) => void navigate(`/results/${scanId}`),
      onError: (err) => toast.error(err instanceof Error ? err.message : t('errors.unknown')),
    });
  };

  return (
    <div className="flex h-full min-h-0 flex-col justify-between overflow-y-auto px-8 py-6">
      <div className="mx-auto w-full max-w-5xl">
        <h1 className="text-[20px] font-bold tracking-tight text-[#1E293B] dark:text-foreground">
          {t('backup.title')}
        </h1>
        <p className="mt-1.5 text-[13px] text-[#64748B] dark:text-foreground-secondary">
          {t('backup.subtitle')}
        </p>

        <div className="mt-8 flex flex-col items-center justify-between gap-8 lg:flex-row">
          <div className="max-w-lg space-y-6">
            <div className="flex items-start gap-3.5">
              <div className="flex size-10 shrink-0 items-center justify-center rounded-[8px] border border-[#DCE6F5] bg-[#F4F7FC] text-[#1677FF] dark:border-border dark:bg-surface-elevated dark:text-primary">
                <Clock3Icon className="size-5" aria-hidden />
              </div>
              <div>
                <div className="text-[14px] font-bold text-[#1E293B] dark:text-foreground">
                  {t('backup.featurePointTitle')}
                </div>
                <div className="mt-0.5 text-[12.5px] text-[#64748B] dark:text-foreground-secondary">
                  {t('backup.featurePointBody')}
                </div>
              </div>
            </div>

            <div className="flex items-start gap-3.5">
              <div className="flex size-10 shrink-0 items-center justify-center rounded-[8px] border border-[#DCE6F5] bg-[#F4F7FC] text-[#1677FF] dark:border-border dark:bg-surface-elevated dark:text-primary">
                <FolderSyncIcon className="size-5" aria-hidden />
              </div>
              <div>
                <div className="text-[14px] font-bold text-[#1E293B] dark:text-foreground">
                  {t('backup.featureSyncTitle')}
                </div>
                <div className="mt-0.5 text-[12.5px] text-[#64748B] dark:text-foreground-secondary">
                  {t('backup.featureSyncBody')}
                </div>
              </div>
            </div>

            <div className="flex items-start gap-3.5">
              <div className="flex size-10 shrink-0 items-center justify-center rounded-[8px] border border-[#DCE6F5] bg-[#F4F7FC] text-[#1677FF] dark:border-border dark:bg-surface-elevated dark:text-primary">
                <ShieldCheckIcon className="size-5" aria-hidden />
              </div>
              <div>
                <div className="text-[14px] font-bold text-[#1E293B] dark:text-foreground">
                  {t('backup.featureSafeTitle')}
                </div>
                <div className="mt-0.5 text-[12.5px] text-[#64748B] dark:text-foreground-secondary">
                  {t('backup.featureSafeBody')}
                </div>
              </div>
            </div>

            <button
              type="button"
              onClick={() => void navigate('/')}
              className="mt-2 inline-flex h-10 w-[160px] items-center justify-center rounded-[6px] bg-[#1677FF] text-[14px] font-semibold text-white shadow-xs transition-all hover:bg-[#0958D9] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring dark:bg-primary dark:text-primary-foreground dark:hover:bg-primary-hover"
            >
              {t('backup.startSearch')}
            </button>
          </div>

          {/* Saved Session Checkpoints */}
          <div className="w-full max-w-md rounded-[12px] border border-[#EBF2FF] bg-[#F8FAFF] p-5 dark:border-border dark:bg-surface-elevated">
            <h2 className="text-[13px] font-bold text-[#1E293B] dark:text-foreground">
              {t('backup.sessionsTitle')}
            </h2>
            {sessions.isLoading ? (
              <div className="mt-3 space-y-2">
                <Skeleton className="h-14" />
                <Skeleton className="h-14" />
              </div>
            ) : (sessions.data?.length ?? 0) === 0 ? (
              <div className="mt-4 flex flex-col items-center py-6 text-center">
                <ArchiveRestoreIcon
                  className="size-9 text-[#93C5FD] dark:text-primary"
                  aria-hidden
                />
                <p className="mt-2 text-[13px] font-semibold text-[#334155] dark:text-foreground">
                  {t('backup.emptyTitle')}
                </p>
                <p className="mt-1 text-[12px] text-[#64748B] dark:text-foreground-secondary">
                  {t('backup.emptyBody')}
                </p>
              </div>
            ) : (
              <div className="mt-3 space-y-2">
                {sessions.data?.map((session) => (
                  <div
                    key={session.id}
                    className="flex items-center justify-between gap-3 rounded-[8px] border border-[#E2E8F0] bg-white px-3.5 py-2.5 dark:border-border dark:bg-surface"
                  >
                    <div className="min-w-0">
                      <div className="truncate text-[13px] font-semibold text-[#1E293B] dark:text-foreground">
                        {session.targetLabel}
                      </div>
                      <div className="text-[11px] text-[#64748B] tabular dark:text-foreground-secondary">
                        {formatDate(session.savedAt, i18n.language)} ·{' '}
                        {formatInteger(session.filesFound, i18n.language)}
                      </div>
                    </div>
                    <Button
                      size="sm"
                      variant="secondary"
                      loading={loadSession.isPending}
                      onClick={() => handleOpenSession(session.fileName)}
                    >
                      {t('backup.open')}
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
