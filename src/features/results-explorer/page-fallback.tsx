import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { HardDriveIcon } from 'lucide-react';
import { EmptyState } from '@/components/ui/empty-state';

/** Shown when a results/scan route is reached without a live or loaded session. */
export function ResultsPageFallback(): React.JSX.Element {
  const { t } = useTranslation();
  const navigate = useNavigate();
  return (
    <div className="h-full">
      <EmptyState
        icon={HardDriveIcon}
        title={t('common.empty')}
        description={t('errors.retryOrHome')}
        actionLabel={t('nav.backToDevices')}
        onAction={() => navigate('/')}
      />
    </div>
  );
}
