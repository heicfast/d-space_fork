import * as React from 'react';
import { cn } from '@/lib/utils';

/** Small segmented control (view toggle, path/type). Icon-only options
 * MUST pass srLabel — every radio needs a discernible accessible name. */
function Segmented<T extends string>({
  options,
  value,
  onChange,
  ariaLabel,
  className,
}: {
  options: readonly {
    value: T;
    label: string;
    icon?: React.ComponentType<{ className?: string }>;
    /** Accessible name for icon-only options (required when label is empty). */
    srLabel?: string;
  }[];
  value: T;
  onChange: (v: T) => void;
  ariaLabel: string;
  className?: string;
}): React.JSX.Element {
  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className={cn(
        'inline-flex h-8 items-center gap-0.5 rounded-md bg-surface-muted p-0.5',
        className,
      )}
    >
      {options.map((opt) => {
        const Icon = opt.icon;
        const active = opt.value === value;
        return (
          <button
            key={opt.value}
            type="button"
            role="radio"
            aria-checked={active}
            aria-label={opt.label || opt.srLabel}
            title={opt.srLabel}
            onClick={() => onChange(opt.value)}
            className={cn(
              'inline-flex h-7 flex-1 items-center justify-center gap-1.5 rounded-sm px-2.5 text-xs font-medium outline-none',
              'transition-[colors,opacity] active:opacity-[var(--press-opacity)]',
              'focus-visible:ring-2 focus-visible:ring-ring',
              active
                ? 'bg-surface text-foreground shadow-sm'
                : 'text-foreground-secondary hover:text-foreground',
            )}
          >
            {Icon ? <Icon className="size-4" aria-hidden /> : null}
            {opt.label ? <span>{opt.label}</span> : null}
          </button>
        );
      })}
    </div>
  );
}

export { Segmented };
