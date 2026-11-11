import * as React from 'react';
import type { ConfidenceGrade } from '@/connector/contract';
import { BadgeCheckIcon, CircleAlertIcon, CircleHelpIcon, ThumbsUpIcon } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Badge } from './badge';
import { Hint } from './tooltip';
import { useNodeDetail } from '@/queries/results';
import { cn } from '@/lib/utils';

/**
 * Confidence badge renders grades exactly as the connector returns them —
 * never re-graded in the UI. Icon + label (never color alone, grid #14).
 * When detail is available the tooltip shows the connector's reason string.
 */
const TONE: Record<ConfidenceGrade, 'success' | 'primary' | 'warning' | 'danger' | 'neutral'> =
  {
    excellent: 'success',
    good: 'primary',
    fair: 'warning',
    poor: 'danger',
    unknown: 'neutral',
  };

const ICON: Record<ConfidenceGrade, React.ComponentType<{ className?: string }>> = {
  excellent: BadgeCheckIcon,
  good: ThumbsUpIcon,
  fair: CircleAlertIcon,
  poor: CircleAlertIcon,
  unknown: CircleHelpIcon,
};

function ConfidenceBadge({
  grade,
  scanId,
  nodeId,
  className,
}: {
  grade: ConfidenceGrade;
  scanId?: string;
  nodeId?: string;
  className?: string;
}): React.JSX.Element {
  const { t } = useTranslation();
  const label = t(`results.confidence.${grade}`);
  const Icon = ICON[grade];

  // Optional reason from node detail (only for the preview pane; the table
  // skips this to avoid per-row queries).
  const detail = useNodeDetail(
    grade !== 'unknown' && nodeId?.startsWith('f') ? (scanId ?? null) : null,
    nodeId ?? null,
  );

  const badge = (
    <Badge variant={TONE[grade]} className={cn('gap-1', className)}>
      <Icon aria-hidden />
      {label}
    </Badge>
  );

  const reason = detail.data?.confidenceReason;
  if (reason) {
    return <Hint text={reason}>{badge}</Hint>;
  }
  return badge;
}

function ConfidencePillInline({ grade }: { grade: ConfidenceGrade }): React.JSX.Element {
  const { t } = useTranslation();
  const Icon = ICON[grade];
  return (
    <span className={cn('inline-flex items-center gap-1 text-xs font-medium tabular')}>
      <Icon className="size-3.5" aria-hidden />
      {t(`results.confidence.${grade}`)}
    </span>
  );
}

export { ConfidenceBadge, ConfidencePillInline };
