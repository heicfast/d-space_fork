import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ThemeProvider } from '@/app/theme';
import { EmptyState, StatTile, CapacityBar } from '@/components/ui/empty-state';
import { ProgressGauge, Progress } from '@/components/ui/progress';
import { Badge } from '@/components/ui/badge';
import { ConfidenceBadge } from '@/components/ui/confidence-badge';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { HardDriveIcon } from 'lucide-react';
import * as React from 'react';

function withProviders(ui: React.ReactElement): React.ReactElement {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <ThemeProvider>{ui}</ThemeProvider>
    </QueryClientProvider>
  );
}

describe('ThemeProvider (ADR-007 two-layer attributes)', () => {
  it('applies the light palette by default (system → light)', () => {
    render(withProviders(<div>probe</div>));
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    expect(document.documentElement.getAttribute('data-scheme')).toBe('light');
    expect(document.documentElement.classList.contains('hc')).toBe(false);
  });

  it('applies data-theme/data-scheme for each adopted palette', () => {
    for (const [mode, theme, scheme] of [
      ['dark', 'dark', 'dark'],
      ['ember', 'ember', 'dark'],
      ['tide', 'tide', 'dark'],
      ['blossom', 'blossom', 'light'],
    ] as const) {
      localStorage.setItem('rw.ui.theme', mode);
      const { unmount } = render(withProviders(<div />));
      expect(document.documentElement.getAttribute('data-theme')).toBe(theme);
      expect(document.documentElement.getAttribute('data-scheme')).toBe(scheme);
      unmount();
    }
    localStorage.removeItem('rw.ui.theme');
  });

  it('high-contrast keeps the family attributes and adds the derived .hc layer', () => {
    localStorage.setItem('rw.ui.theme', 'high-contrast');
    render(withProviders(<div />));
    expect(document.documentElement.classList.contains('hc')).toBe(true);
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    localStorage.removeItem('rw.ui.theme');
  });
});

describe('EmptyState (grid #31)', () => {
  it('renders title, description and a working action', () => {
    const onAction = vi.fn();
    render(
      withProviders(
        <EmptyState
          icon={HardDriveIcon}
          title="No drives"
          description="Plug one in"
          actionLabel="Refresh"
          onAction={onAction}
        />,
      ),
    );
    expect(screen.getByText('No drives')).toBeInTheDocument();
    expect(screen.getByText('Plug one in')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    expect(onAction).toHaveBeenCalledOnce();
  });
});

describe('StatTile / CapacityBar', () => {
  it('renders values with tabular styling', () => {
    render(withProviders(<StatTile label="Files" value="1,234" icon={HardDriveIcon} />));
    expect(screen.getByText('1,234')).toBeInTheDocument();
  });

  it('renders an accessible progressbar with the used ratio', () => {
    render(withProviders(<CapacityBar usedBytes={75} totalBytes={100} label="Disk" />));
    const bar = screen.getByRole('progressbar', { name: 'Disk' });
    expect(bar).toHaveAttribute('aria-valuenow', '75');
  });
});

describe('Progress primitives (grid #24)', () => {
  it('exposes gauge as role=progressbar with percentage', () => {
    render(withProviders(<ProgressGauge value={0.42} label="Overall" />));
    const gauge = screen.getByRole('progressbar', { name: 'Overall' });
    expect(gauge).toHaveAttribute('aria-valuenow', '42');
    expect(gauge).toHaveAttribute('aria-valuemin', '0');
    expect(gauge).toHaveAttribute('aria-valuemax', '100');
  });

  it('renders determinate and indeterminate linear progress', () => {
    const { rerender } = render(withProviders(<Progress value={40} />));
    expect(document.querySelector('[data-slot="progress"]')).toBeInTheDocument();
    rerender(withProviders(<Progress indeterminate />));
    expect(document.querySelector('[data-slot="progress"]')).toBeInTheDocument();
  });
});

describe('ConfidenceBadge (grid #14 — never color alone)', () => {
  it.each(['excellent', 'good', 'fair', 'poor', 'unknown'] as const)(
    'pairs %s grade with a label',
    (grade) => {
      render(withProviders(<ConfidenceBadge grade={grade} />));
      // The badge label comes from the i18n table (default en in tests).
      expect(document.querySelector('[data-slot="badge"]')?.textContent).toBeTruthy();
    },
  );
});

describe('Badge variants', () => {
  it('renders semantic variants', () => {
    const { container } = render(
      withProviders(
        <div>
          <Badge variant="success">ok</Badge>
          <Badge variant="warning">warn</Badge>
          <Badge variant="danger">bad</Badge>
        </div>,
      ),
    );
    expect(container.textContent).toContain('ok');
    expect(container.textContent).toContain('warn');
  });
});
