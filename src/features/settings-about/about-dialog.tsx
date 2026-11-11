import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { AppWindowIcon } from 'lucide-react';
import { useAppInfo, useDriverStatus } from '@/queries/system';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';

/**
 * About dialog (corpus: hamburger → About; rebuilt as a title-bar dialog on
 * this app's chrome). Shows truthful build/runtime facts only — no invented
 * engine claims: every row is data from the connector.
 */
export function AboutDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}): React.JSX.Element {
  const { t } = useTranslation();
  const appInfo = useAppInfo();
  const driver = useDriverStatus();

  const info = appInfo.data;
  const rows: { label: string; value: React.ReactNode }[] = [
    { label: t('about.version'), value: info ? info.version : '…' },
    {
      label: t('about.connector'),
      value: info ? (
        <Badge variant={info.connectorMode === 'tauri' ? 'success' : 'warning'}>
          {info.connectorMode === 'tauri'
            ? t('settings.connectorTauri')
            : t('settings.connectorMock')}
        </Badge>
      ) : (
        '…'
      ),
    },
    {
      label: t('about.engine'),
      value: info?.engineVersion ?? t('about.engineNone'),
    },
    {
      label: t('about.driver'),
      value: driver.data
        ? driver.data.installed
          ? t('about.driverOk', {
              version: driver.data.version ?? '—',
              running: driver.data.running
                ? t('about.driverRunning')
                : t('about.driverStopped'),
            })
          : t('about.driverMissing')
        : '…',
    },
    { label: t('about.platform'), value: info ? info.platform : '…' },
  ];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <div className="mb-1 flex size-12 items-center justify-center self-start rounded-md bg-primary-muted text-primary">
            <AppWindowIcon className="size-6" strokeWidth={1.5} aria-hidden />
          </div>
          <DialogTitle>{t('app.name')}</DialogTitle>
          <DialogDescription>{t('about.subtitle')}</DialogDescription>
        </DialogHeader>
        <dl className="space-y-2">
          {rows.map((row) => (
            <div
              key={row.label}
              className="flex items-center justify-between gap-4 border-b border-border-inner pb-2 last:border-b-0 last:pb-0"
            >
              <dt className="text-sm text-foreground-secondary">{row.label}</dt>
              <dd className="text-sm font-medium tabular">{row.value}</dd>
            </div>
          ))}
        </dl>
      </DialogContent>
    </Dialog>
  );
}
