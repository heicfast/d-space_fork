import * as React from 'react';
import type { LucideIcon } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from './button';

/** Designed empty state: illustration slot + guidance + primary action (grid #31). */
function EmptyState({
  icon: Icon,
  title,
  description,
  actionLabel,
  onAction,
  secondaryLabel,
  onSecondary,
  className,
}: {
  icon: LucideIcon;
  title: string;
  description?: string;
  actionLabel?: string;
  onAction?: () => void;
  secondaryLabel?: string;
  onSecondary?: () => void;
  className?: string;
}): React.JSX.Element {
  return (
    <div
      className={cn(
        'flex h-full min-h-48 flex-col items-center justify-center gap-3 p-8 text-center',
        className,
      )}
    >
      <div className="flex size-16 items-center justify-center rounded-xl bg-surface-tint text-foreground-secondary">
        <Icon className="size-8" strokeWidth={1.5} aria-hidden />
      </div>
      <div className="space-y-1">
        <p className="text-base font-semibold">{title}</p>
        {description ? (
          <p className="max-w-sm text-sm text-foreground-secondary">{description}</p>
        ) : null}
      </div>
      {actionLabel || secondaryLabel ? (
        <div className="mt-2 flex items-center gap-2">
          {secondaryLabel ? (
            <Button variant="secondary" onClick={onSecondary}>
              {secondaryLabel}
            </Button>
          ) : null}
          {actionLabel ? (
            <Button variant="primary" onClick={onAction}>
              {actionLabel}
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/** Compact stat tile used in scan-complete celebration and health cards. */
function StatTile({
  label,
  value,
  icon: Icon,
  tone = 'default',
  className,
}: {
  label: string;
  value: string;
  icon?: LucideIcon;
  tone?: 'default' | 'success' | 'warning' | 'danger';
  className?: string;
}): React.JSX.Element {
  return (
    <div
      className={cn(
        'flex min-w-28 flex-col gap-1 rounded-md border border-border bg-surface-muted px-4 py-3 text-left',
        className,
      )}
    >
      <span className="flex items-center gap-1.5 text-xs text-foreground-secondary">
        {Icon ? <Icon className="size-3.5" aria-hidden /> : null}
        {label}
      </span>
      <span
        className={cn(
          'text-xl font-semibold tabular',
          tone === 'success' && 'text-success',
          tone === 'warning' && 'text-warning',
          tone === 'danger' && 'text-danger',
        )}
      >
        {value}
      </span>
    </div>
  );
}

/** Used/total capacity bar for device cards and destination rows. */
function CapacityBar({
  usedBytes,
  totalBytes,
  label,
  tone = 'default',
  className,
}: {
  usedBytes: number;
  totalBytes: number;
  label?: string;
  tone?: 'default' | 'warning' | 'danger';
  className?: string;
}): React.JSX.Element {
  const ratio = totalBytes > 0 ? Math.min(1, usedBytes / totalBytes) : 0;
  const pct = ratio * 100;
  return (
    <div className={cn('w-full', className)}>
      {label ? (
        <div className="mb-1 text-xs text-foreground-secondary tabular">{label}</div>
      ) : null}
      <div
        className="h-1.5 w-full overflow-hidden rounded-full bg-surface-tint"
        role="progressbar"
        aria-valuenow={Math.round(pct)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={label ?? 'capacity'}
      >
        <div
          className={cn(
            'h-full rounded-full transition-[width] duration-300',
            tone === 'danger' || ratio > 0.92
              ? 'bg-danger'
              : tone === 'warning' || ratio > 0.8
                ? 'bg-warning'
                : 'bg-primary',
          )}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}

export { CapacityBar, EmptyState, StatTile };
