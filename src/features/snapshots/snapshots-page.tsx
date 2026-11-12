import * as React from 'react';
import { useTranslation } from 'react-i18next';

/** Placeholder route — the full page lands with the cleanup feature set. */
export function SnapshotsPage(): React.JSX.Element {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <p className="text-sm text-foreground-secondary">{t('sidebar.sizeSnapshots')}</p>
    </div>
  );
}
