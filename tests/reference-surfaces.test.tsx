import { describe, expect, it, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
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
import { ActivationDialog } from '@/features/settings-about/activation-dialog';
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

  it('renders the Activate dialog with Paste bubble, Activate CTA, and Buy a License Now button', async () => {
    wrap(<ActivationDialog open onOpenChange={() => undefined} />);
    expect(
      await screen.findByRole('dialog', { name: /activate easeus data recovery wizard/i }),
    ).toBeVisible();
    expect(screen.getByText('Paste')).toBeVisible();
    expect(screen.getByRole('button', { name: /^activate$/i })).toBeVisible();
    expect(screen.getByRole('button', { name: /buy a license now/i })).toBeVisible();
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

  it('renders File Repair directly on Add Files Dropzone -> Queue & Repair All', () => {
    wrap(<RepairPage />, '/repair');
    expect(
      screen.getByRole('heading', {
        name: /repair corrupted videos, photos and documents/i,
      }),
    ).toBeVisible();
    expect(
      screen.getByText(/click or drag files from your local disk to start the repair/i),
    ).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: /add files/i }));
    expect(screen.getByText('WinMemoryCleaner (1).exe')).toBeVisible();
    expect(screen.getByRole('button', { name: /repair all/i })).toBeVisible();
  });

  it('renders Lost Partition Rescue directly on Disk Selection -> Smart Partition Scan Completed', async () => {
    wrap(<LostPartitionPage />, '/lost-partition');
    expect(
      await screen.findByRole('heading', { name: /lost partition rescue/i }),
    ).toBeVisible();
    expect(
      screen.getByText(/choose a disk to start searching for lost partition\(s\)/i),
    ).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: /search for lost partition/i }));
    expect(
      screen.getByRole('heading', { name: /smart partition scan completed/i }),
    ).toBeVisible();
    expect(screen.getByRole('button', { name: /try data recovery/i })).toBeVisible();
  });

  it('renders Scan Snapshot page', () => {
    wrap(<BackupPage />, '/backup');
    expect(screen.getByRole('heading', { name: /scan snapshot/i })).toBeVisible();
    expect(screen.getByText('Saved Recovery Checkpoints')).toBeVisible();
  });
});
