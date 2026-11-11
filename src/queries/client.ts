import { QueryClient } from '@tanstack/react-query';

/** Single shared client. Defaults tuned for a local mock backend. */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 5_000,
      gcTime: 5 * 60_000,
      refetchOnWindowFocus: false,
    },
    mutations: {
      retry: 0,
    },
  },
});

export const queryKeys = {
  devices: ['devices'] as const,
  device: (id: string) => ['devices', id] as const,
  scanState: (scanId: string) => ['scan', scanId, 'state'] as const,
  scanProgress: (scanId: string) => ['scan', scanId, 'progress'] as const,
  tree: (scanId: string, parentId: string | null, mode: string) =>
    ['scan', scanId, 'tree', mode, parentId] as const,
  results: (scanId: string, filterKey: string, sortKey: string) =>
    ['scan', scanId, 'results', filterKey, sortKey] as const,
  count: (scanId: string, filterKey: string) => ['scan', scanId, 'count', filterKey] as const,
  nodeDetail: (scanId: string, nodeId: string) => ['scan', scanId, 'node', nodeId] as const,
  thumbnail: (scanId: string, nodeId: string) => ['scan', scanId, 'thumb', nodeId] as const,
  preview: (scanId: string, nodeId: string, kind: string) =>
    ['scan', scanId, 'preview', nodeId, kind] as const,
  sessions: ['sessions'] as const,
  job: (jobId: string) => ['recovery', jobId] as const,
  jobItems: (jobId: string) => ['recovery', jobId, 'items'] as const,
  report: (jobId: string) => ['recovery', jobId, 'report'] as const,
  health: (deviceId: string) => ['health', deviceId] as const,
  appInfo: ['system', 'app'] as const,
  driverStatus: ['system', 'driver'] as const,
  mounts: ['system', 'mounts'] as const,
  repairDryRun: (deviceId: string) => ['repair', deviceId, 'dry-run'] as const,
  undoLog: (deviceId: string) => ['repair', deviceId, 'undo'] as const,
};
