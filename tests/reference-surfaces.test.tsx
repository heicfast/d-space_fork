import { describe, expect, it, afterEach } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider } from '@/app/theme';
import { TooltipProvider } from '@/components/ui/tooltip';
import { WizardShell } from '@/app/shell';
import { SdCardPage } from '@/features/device-selection/sd-card-page';
import { HealthPage } from '@/features/health/health-page';
import { BackupPage } from '@/features/backup/backup-page';
import { RepairPage } from '@/features/repair/repair-page';
import { LostPartitionPage } from '@/features/lost-partition/lost-partition-page';
import '@/i18n';

function wrap(ui: React.ReactElement, initialPath = '/'): ReturnType<typeof render> {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  return render(
    <ThemeProvider>
      <QueryClientProvider client={qc}>
        <TooltipProvider>
          <MemoryRouter initialEntries={[initialPath]}>{ui}</MemoryRouter>
        </TooltipProvider>
      </QueryClientProvider>
    </ThemeProvider>,
  );
}

afterEach(() => {
  cleanup();
});

describe('reference UI surfaces and sidebar', () => {
  it('renders the 3-section sidebar and header without any simulator badge', () => {
    wrap(<WizardShell />);
    expect(screen.getByText('DiskWizard')).toBeVisible();
    expect(screen.queryByText(/built-in simulator/i)).toBeNull();
    expect(screen.getByText('Hardware Disk')).toBeVisible();
    expect(screen.getByText('SD Card')).toBeVisible();
    expect(screen.getByText('Disk Health')).toBeVisible();
    expect(screen.getByText('File Repair')).toBeVisible();
    expect(screen.getByText('Lost Partition Rescue')).toBeVisible();
    expect(screen.getByText('Scan Snapshot')).toBeVisible();
  });

  it('does not render any robot/AI mascot icon in the title bar', () => {
    wrap(<WizardShell />);
    expect(screen.queryByRole('button', { name: /recovery assistant/i })).toBeNull();
  });

  it('renders no activation or licensing surface anywhere in the shell', () => {
    wrap(<WizardShell />);
    expect(screen.queryByText(/buy now/i)).toBeNull();
    expect(screen.queryByTestId('header-activate-trigger')).toBeNull();
  });

  it('renders the SD Card page with SD/MMC/MS PRO row, SDR/SSR/DVR columns, and Search for Lost Data CTA', () => {
    wrap(<SdCardPage />, '/sd-card');
    expect(
      screen.getByRole('heading', {
        name: /connect your memory card to recover lost photos & videos/i,
      }),
    ).toBeVisible();
    expect(screen.getByText('SD/MMC/MS PRO')).toBeVisible();
    expect(screen.getByText('Fix SD card access')).toBeVisible();
    expect(screen.getByText('Photos that open')).toBeVisible();
    expect(screen.getByText('Video that play')).toBeVisible();
    expect(screen.getByRole('button', { name: /show all external devices/i })).toBeVisible();
    expect(screen.getByRole('button', { name: /search for lost data/i })).toBeVisible();
  });

  it('renders the redesigned Disk Health page directly', async () => {
    wrap(<HealthPage />, '/health');
    expect(await screen.findByRole('heading', { name: /disk health/i })).toBeVisible();
    expect(await screen.findByText(/s\.m\.a\.r\.t\. attributes/i)).toBeVisible();
  });

  it('renders Volume Repair (the real workbench; the mock per-file dropzone is gone)', () => {
    wrap(<RepairPage />, '/repair');
    expect(screen.getByRole('heading', { name: /volume repair/i })).toBeVisible();
    expect(
      screen.getByText(/read-only checks run against the volume structures/i),
    ).toBeVisible();
    // No mock queue surfaces anywhere.
    expect(screen.queryByText(/add files/i)).toBeNull();
    expect(screen.queryByRole('button', { name: /repair all/i })).toBeNull();
  });

  it('renders Lost Partition Rescue from real disk layouts (no canned fixtures)', async () => {
    wrap(<LostPartitionPage />, '/lost-partition');
    expect(
      await screen.findByRole('heading', { name: /lost partition rescue/i }),
    ).toBeVisible();
    // The mock connector serves Disk 2 with one lost NTFS volume — the
    // card renders it with a scan button targeting the lp: id.
    expect(await screen.findByText(/^Disk 2$/)).toBeVisible();
    const scanBtn = screen.getByRole('button', { name: /try data recovery/i });
    expect(scanBtn).toBeVisible();
    // The canned step-2 table is gone.
    expect(screen.queryByText(/smart partition scan completed/i)).toBeNull();
  });

  it('renders Scan Snapshot page', () => {
    wrap(<BackupPage />, '/backup');
    expect(screen.getByRole('heading', { name: /scan snapshot/i })).toBeVisible();
    expect(screen.getByText('Saved Recovery Checkpoints')).toBeVisible();
  });
});
