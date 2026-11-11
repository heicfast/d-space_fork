import { useQuery, useQueryClient } from '@tanstack/react-query';
import { connector, scenarioControl } from '@/connector';
import { queryKeys } from './client';

/** App-level info surfaced by the connector's System domain. */
export function useConnectorMode(): 'mock' | 'tauri' | 'loading' {
  const { data } = useQuery({
    queryKey: queryKeys.appInfo,
    queryFn: () => connector.system.getAppInfo(),
    staleTime: Infinity,
  });
  return data?.connectorMode ?? 'loading';
}

/**
 * Dev-panel scenario control (mock-only). Returns null when the real
 * connector is active — the panel hides these controls accordingly.
 */
export function useScenarioControl(): ReturnType<typeof scenarioControl> {
  // Not a query — control surface; re-render is driven by the panel's own
  // state. Memoized once per mount.
  const qc = useQueryClient();
  void qc;
  return scenarioControl();
}
