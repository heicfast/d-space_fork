import * as React from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { TooltipProvider } from '@/components/ui/tooltip';
import { Toaster } from '@/components/ui/toaster';
import { ThemeProvider } from './theme';
import { queryClient } from '@/queries/client';
import { installMainThreadWatchdog } from '@/lib/logger';

/**
 * Root providers: theme → query → tooltips → toaster. The watchdog installs
 * once in dev for the performance overlay (grid #38).
 */
export function Providers({ children }: { children: React.ReactNode }): React.JSX.Element {
  React.useEffect(() => {
    if (import.meta.env.DEV) {
      return installMainThreadWatchdog();
    }
    return undefined;
  }, []);

  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <TooltipProvider delayDuration={300}>{children}</TooltipProvider>
        <Toaster />
      </QueryClientProvider>
    </ThemeProvider>
  );
}
