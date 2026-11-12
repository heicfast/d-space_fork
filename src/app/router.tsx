import * as React from 'react';
import { createHashRouter, RouterProvider } from 'react-router';
import { WizardShell } from './shell';
import { ErrorBoundary } from './error-boundary';
import { HomePage } from '@/features/device-selection/home-page';
import { SdCardPage } from '@/features/device-selection/sd-card-page';
import { BackupPage } from '@/features/backup/backup-page';
import { QuickSearchPage } from '@/features/quick-search/quick-search-page';
import { ScanProgressPage } from '@/features/scan-progress/scan-progress-page';
import { ResultsPageFallback } from '@/features/results-explorer/page-fallback';
import { RecoveryPage } from '@/features/recovery-queue/recovery-page';
import { HealthPage } from '@/features/health/health-page';
import { RepairPage } from '@/features/repair/repair-page';
import { SettingsPage } from '@/features/settings-about/settings-page';
import { LostPartitionPage } from '@/features/lost-partition/lost-partition-page';
import { ResultsRoute } from '@/features/results-explorer/results-route';
import { CleanupPage } from '@/features/cleanup/cleanup-page';
import { DuplicatesPage } from '@/features/duplicates/duplicates-page';
import { SnapshotsPage } from '@/features/snapshots/snapshots-page';

/**
 * Router (ADR-001): React Router v7 data mode with a HASH history — hash
 * routing behaves identically in WebView2's custom protocol, plain browser
 * dev/preview, and any static host.
 */
export const router = createHashRouter([
  {
    path: '/',
    element: (
      <ErrorBoundary>
        <WizardShell />
      </ErrorBoundary>
    ),
    children: [
      { index: true, element: <HomePage /> },
      { path: 'sd-card', element: <SdCardPage /> },
      { path: 'tools', element: <HealthPage /> },
      { path: 'backup', element: <BackupPage /> },
      { path: 'quick-search', element: <QuickSearchPage /> },
      { path: 'scan/:deviceId/live', element: <ScanProgressPage /> },
      { path: 'results/:scanId', element: <ResultsRoute /> },
      { path: 'recover/:scanId', element: <RecoveryPage /> },
      { path: 'health', element: <HealthPage /> },
      { path: 'repair', element: <RepairPage /> },
      { path: 'settings', element: <SettingsPage /> },
      { path: 'lost-partition', element: <LostPartitionPage /> },
      { path: 'cleanup', element: <CleanupPage /> },
      { path: 'duplicates', element: <DuplicatesPage /> },
      { path: 'snapshots', element: <SnapshotsPage /> },
      { path: '*', element: <ResultsPageFallback /> },
    ],
  },
]);

export function App(): React.JSX.Element {
  return <RouterProvider router={router} />;
}
