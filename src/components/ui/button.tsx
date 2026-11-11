import * as React from 'react';
import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-semibold transition-[background-color,border-color,color,box-shadow,transform] duration-100 outline-none active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0',
  {
    variants: {
      variant: {
        primary:
          'gradient-cta gradient-glow bg-[#1677FF] text-primary-foreground hover:bg-[#0958D9] hover:text-primary-foreground active:bg-[#003EB3] dark:bg-primary-fill dark:hover:bg-primary-fill-hover',
        secondary:
          'border border-border-strong bg-surface text-foreground shadow-2xs hover:border-[#94A3B8] hover:bg-[#E2E8F0]/80 hover:text-foreground active:bg-surface-selected dark:hover:border-border-strong dark:hover:bg-surface-hover',
        ghost:
          'text-foreground-secondary hover:bg-[#DCE8FF]/85 hover:text-foreground active:bg-surface-tint dark:hover:bg-surface-hover dark:hover:text-foreground',
        danger:
          'bg-danger text-white shadow-xs hover:bg-[#B91C1C] hover:text-white active:bg-danger-pressed',
        dangerOutline:
          'border border-danger bg-transparent text-danger hover:bg-danger-muted hover:text-danger active:bg-danger-muted/80',
        link: 'text-primary underline-offset-4 hover:underline',
      },
      size: {
        sm: 'h-7 rounded-xs px-2.5 text-xs [&_svg]:size-4',
        md: 'h-8 rounded-sm px-3.5 text-sm [&_svg]:size-4',
        lg: 'h-9.5 rounded-md px-5 text-sm [&_svg]:size-5',
        xl: 'h-11 rounded-md px-6 text-base [&_svg]:size-5',
        icon: 'size-8 rounded-sm [&_svg]:size-4',
      },
    },
    defaultVariants: { variant: 'secondary', size: 'md' },
  },
);

function Button({
  className,
  variant,
  size,
  asChild = false,
  loading = false,
  children,
  disabled,
  ...props
}: React.ComponentProps<'button'> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean;
    loading?: boolean;
  }): React.JSX.Element {
  const Comp = asChild ? Slot : 'button';
  return (
    <Comp
      data-slot="button"
      className={cn(buttonVariants({ variant, size }), className)}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      {...props}
    >
      {loading ? (
        <span
          className="size-4 animate-spin rounded-full border-2 border-current border-t-transparent"
          aria-hidden
        />
      ) : null}
      {children}
    </Comp>
  );
}

export { Button };
