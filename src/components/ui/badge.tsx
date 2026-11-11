import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const badgeVariants = cva(
  'inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-2xs font-semibold leading-none transition-colors [&_svg]:size-3',
  {
    variants: {
      variant: {
        neutral: 'border-border bg-surface-muted text-foreground-secondary',
        primary: 'border-primary-muted-border bg-primary-muted text-primary',
        success: 'border-transparent bg-success-muted text-success',
        warning: 'border-transparent bg-warning-muted text-warning',
        danger: 'border-transparent bg-danger-muted text-danger',
        outline: 'border-border-strong bg-transparent text-foreground-secondary',
      },
    },
    defaultVariants: { variant: 'neutral' },
  },
);

function Badge({
  className,
  variant,
  ...props
}: React.ComponentProps<'span'> & VariantProps<typeof badgeVariants>): React.JSX.Element {
  return (
    <span data-slot="badge" className={cn(badgeVariants({ variant }), className)} {...props} />
  );
}

export { Badge };
