import * as React from 'react';
import { AlertTriangleIcon, HomeIcon, RotateCcwIcon } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { Button } from '@/components/ui/button';
import { createLogger } from '@/lib/logger';

const logger = createLogger('error-boundary');

interface State {
  error: Error | null;
}

export class ErrorBoundary extends React.Component<{ children: React.ReactNode }, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: React.ErrorInfo): void {
    logger.error('ui-crash', {
      message: error.message,
      stack: info.componentStack?.slice(0, 400),
    });
  }

  override render(): React.ReactNode {
    if (this.state.error) {
      return <ErrorScreen error={this.state.error} />;
    }
    return this.props.children;
  }
}

/** Designed error surface (grid #32): reason + recovery action, never a raw stack. */
export function ErrorScreen({
  error,
  onRetry,
}: {
  error: Error;
  onRetry?: () => void;
}): React.JSX.Element {
  const { t } = useTranslation();
  const navigate = useNavigate();
  return (
    <div
      role="alert"
      className="flex h-full flex-col items-center justify-center gap-4 p-10 text-center"
    >
      <div className="flex size-16 items-center justify-center rounded-xl bg-danger-muted text-danger">
        <AlertTriangleIcon className="size-8" strokeWidth={1.5} aria-hidden />
      </div>
      <div className="space-y-1">
        <h1 className="text-lg font-semibold">{t('errors.title')}</h1>
        <p className="max-w-md text-sm text-foreground-secondary">
          {error.message || t('errors.unknown')}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <Button variant="secondary" onClick={() => navigate('/')}>
          <HomeIcon aria-hidden />
          {t('nav.backToDevices')}
        </Button>
        <Button
          variant="primary"
          onClick={() => (onRetry ? onRetry() : window.location.reload())}
        >
          <RotateCcwIcon aria-hidden />
          {t('common.retry')}
        </Button>
      </div>
    </div>
  );
}
