import * as React from 'react';
import { cn } from '@/lib/utils';

/** Minimal table primitives with our token styling (no external dep). */

function Table({ className, ...props }: React.ComponentProps<'table'>): React.JSX.Element {
  return (
    <div className="relative w-full overflow-auto">
      <table className={cn('w-full caption-bottom text-sm tabular', className)} {...props} />
    </div>
  );
}

function TableHeader({
  className,
  ...props
}: React.ComponentProps<'thead'>): React.JSX.Element {
  return <thead className={cn('[&_tr]:border-b [&_tr]:border-border', className)} {...props} />;
}

function TableBody({ className, ...props }: React.ComponentProps<'tbody'>): React.JSX.Element {
  return <tbody className={cn('[&_tr:last-child]:border-0', className)} {...props} />;
}

function TableRow({ className, ...props }: React.ComponentProps<'tr'>): React.JSX.Element {
  return (
    <tr
      className={cn(
        'border-b border-border transition-colors hover:bg-surface-hover/60 data-[state=selected]:bg-surface-selected/40',
        className,
      )}
      {...props}
    />
  );
}

function TableHead({ className, ...props }: React.ComponentProps<'th'>): React.JSX.Element {
  return (
    <th
      className={cn(
        'h-8 px-3 text-left align-middle text-xs font-semibold tracking-wide text-foreground-secondary select-none',
        className,
      )}
      {...props}
    />
  );
}

function TableCell({ className, ...props }: React.ComponentProps<'td'>): React.JSX.Element {
  return <td className={cn('px-3 py-1.5 align-middle', className)} {...props} />;
}

export { Table, TableBody, TableCell, TableHead, TableHeader, TableRow };
