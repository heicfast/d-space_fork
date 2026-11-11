import * as React from 'react';
import * as ProgressPrimitive from '@radix-ui/react-progress';
import { cn } from '@/lib/utils';

function Progress({
  className,
  value,
  indeterminate = false,
  ...props
}: React.ComponentProps<typeof ProgressPrimitive.Root> & {
  indeterminate?: boolean;
}): React.JSX.Element {
  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      className={cn(
        'relative h-1.5 w-full overflow-hidden rounded-full bg-surface-tint',
        className,
      )}
      {...props}
    >
      <ProgressPrimitive.Indicator
        className={cn(
          'h-full rounded-full bg-primary transition-transform duration-200 ease-linear',
          indeterminate &&
            'w-1/3 animate-progress-stripes bg-[repeating-linear-gradient(45deg,var(--accent)_0_14px,var(--accent-pressed)_14px_28px)]',
        )}
        style={indeterminate ? undefined : { transform: `translateX(-${100 - (value ?? 0)}%)` }}
      />
    </ProgressPrimitive.Root>
  );
}

/** Circular overall-progress gauge (scan screen, preview condition). */
function ProgressGauge({
  value,
  size = 56,
  thickness = 5,
  label,
  completed = false,
  className,
  ...props
}: Omit<React.ComponentProps<'svg'>, 'children' | 'strokeWidth' | 'width' | 'height'> & {
  value: number;
  size?: number;
  thickness?: number;
  label?: string;
  completed?: boolean;
}): React.JSX.Element {
  const clamped = Math.min(1, Math.max(0, Number.isFinite(value) ? value : 0));
  const pct = Math.round(clamped * 100);
  const r = (size - thickness) / 2;
  const c = 2 * Math.PI * r;
  return (
    <svg
      role="progressbar"
      aria-valuenow={pct}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={label}
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      className={cn('shrink-0', className)}
      {...props}
    >
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="var(--surface)"
        stroke="#D0E2FF"
        strokeWidth={thickness}
      />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        stroke="#1677FF"
        strokeWidth={thickness}
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={completed ? 0 : c * (1 - clamped)}
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
        style={{ transition: 'stroke-dashoffset 200ms linear' }}
      />
      {completed ? (
        <path
          d={`M ${size * 0.32} ${size * 0.52} L ${size * 0.45} ${size * 0.65} L ${size * 0.7} ${size * 0.38}`}
          fill="none"
          stroke="#1677FF"
          strokeWidth={2.75}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      ) : (
        <text
          x="50%"
          y="50%"
          dominantBaseline="central"
          textAnchor="middle"
          fill="#1677FF"
          fontSize={size >= 50 ? 13 : 11}
          fontWeight={700}
        >
          {pct}%
        </text>
      )}
    </svg>
  );
}

export { Progress, ProgressGauge };
