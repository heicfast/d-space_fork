import * as React from 'react';
import { useNavigate, useParams } from 'react-router';
import { useTranslation } from 'react-i18next';
import { RotateCcwIcon, TriangleAlertIcon } from 'lucide-react';
import { useScanState, useStartScan } from '@/queries/scan';
import { ResultsExplorer } from './results-explorer';
import { ResultsPageFallback } from './page-fallback';
import { Skeleton } from '@/components/ui/skeleton';
import { Button } from '@/components/ui/button';
import { toast } from 'sonner';

/**
 * Results route guard — verifies the scan session exists via the connector
 * (live or loaded), otherwise falls back to Home with no crash. The 3-pane
 * explorer itself renders for both live scans and saved sessions.
 *
 * A STOPPED scan is honestly marked as partial (grid #33): the banner states
 * that results may be incomplete and offers the corpus's recovery path —
 * searching again ("Remaining data can be rescanned and recovered anytime
 * later", stop-confirm copy family).
 */
export function ResultsRoute(): React.JSX.Element {
  const { scanId = '' } = useParams();
  const state = useScanState(scanId || null);

  if (!scanId) return <ResultsPageFallback />;

  if (state.isLoading) {
    return (
      <div className="space-y-2 p-4">
        <Skeleton className="h-11" />
        <div className="flex gap-2">
          <Skeleton className="h-96 w-56" />
          <Skeleton className="h-96 flex-1" />
          <Skeleton className="hidden h-96 w-80 xl:block" />
        </div>
      </div>
    );
  }

  if (state.isError || !state.data) {
    return <ResultsPageFallback />;
  }

  const partial = state.data.status === 'cancelled';

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      {partial ? (
        <PartialBanner
          targetId={state.data.params.targetId}
          pathScope={state.data.params.pathScope}
        />
      ) : null}
      <div className="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
        <ResultsExplorer scanId={scanId} liveMode={false} />
      </div>
    </div>
  );
}

function PartialBanner({
  targetId,
  pathScope,
}: {
  targetId: string;
  pathScope?: string | null;
}): React.JSX.Element {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const startScan = useStartScan();

  return (
    <div
      className="flex shrink-0 items-center gap-3 border-b border-warning/40 bg-warning-muted px-4 py-2 text-sm"
      role="status"
    >
      <TriangleAlertIcon className="size-4 shrink-0 text-warning" aria-hidden />
      <span className="min-w-0 flex-1">{t('results.partialBanner')}</span>
      <Button
        variant="secondary"
        size="sm"
        loading={startScan.isPending}
        disabled={startScan.isPending}
        onClick={() =>
          startScan.mutate(
            { targetId, ...(pathScope != null ? { pathScope } : {}) },
            {
              onSuccess: ({ scanId: nextId }) => {
                toast(t('scan.liveResults'));
                void navigate(`/scan/${targetId}/live?scanId=${nextId}`);
              },
            },
          )
        }
      >
        <RotateCcwIcon aria-hidden />
        {t('results.partialResume')}
      </Button>
    </div>
  );
}
