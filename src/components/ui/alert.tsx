import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { AlertTriangleIcon, CheckCircle2Icon, InfoIcon, XCircleIcon } from 'lucide-react';
import { cn } from '@/lib/utils';

const alertVariants = cva('relative flex w-full gap-3 rounded-md border px-4 py-3 text-sm', {
  variants: {
    variant: {
      info: 'border-primary-muted-border bg-primary-muted text-foreground [&_svg]:text-primary',
      success: 'border-transparent bg-success-muted text-foreground [&_svg]:text-success',
      warning: 'border-transparent bg-warning-muted text-foreground [&_svg]:text-warning',
      danger: 'border-transparent bg-danger-muted text-foreground [&_svg]:text-danger',
    },
  },
  defaultVariants: { variant: 'info' },
});

const ICONS: Record<
  NonNullable<VariantProps<typeof alertVariants>['variant']>,
  React.ComponentType<{ className?: string }>
> = {
  info: InfoIcon,
  success: CheckCircle2Icon,
  warning: AlertTriangleIcon,
  danger: XCircleIcon,
};

function Alert({
  className,
  variant,
  children,
  ...props
}: React.ComponentProps<'div'> & VariantProps<typeof alertVariants>): React.JSX.Element {
  const Icon = ICONS[variant ?? 'info'];
  return (
    <div role="alert" className={cn(alertVariants({ variant }), className)} {...props}>
      <Icon className="mt-0.5 size-4 shrink-0" aria-hidden />
      <div className="min-w-0">{children}</div>
    </div>
  );
}

export { Alert };
