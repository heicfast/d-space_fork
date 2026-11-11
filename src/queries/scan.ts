import { useRef, useSyncExternalStore } from 'react';
import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { connector } from '@/connector';
import type { ScanParams, ScanProgressEvent, ScanState } from '@/connector/contract';
import { queryKeys } from './client';
import { useScanSessionStore } from '@/stores/scan-session-store';

// ---------------------------------------------------------------------------
// Progress store — external-store pattern over the connector event stream.
// One store per scanId; subscribers share the raw event; the store also
// drives query-cache invalidations on status changes.
// ---------------------------------------------------------------------------

interface ScanProgressStore {
  getSnapshot: () => ScanProgressEvent | null;
  subscribe: (listener: () => void) => () => void;
}

const scanStores = new Map<string, ScanProgressStore>();

function getScanStore(scanId: string, qc: () => QueryClient): ScanProgressStore {
  const existing = scanStores.get(scanId);
  if (existing) return existing;
  let current: ScanProgressEvent | null = null;
  const listeners = new Set<() => void>();
  let lastStatus: string | null = null;
  let lastBucket = -1;
  const emit = (): void => {
    for (const l of listeners) l();
  };
  const unsubscribe = connector.scan.onScanProgress(scanId, (event) => {
    current = event;
    const invalidate = qc() as {
      invalidateQueries: (q: { queryKey: readonly unknown[] }) => Promise<unknown>;
    };
    const bucket = Math.floor(event.overallProgress * 10);
    if (event.status !== lastStatus) {
      lastStatus = event.status;
      void invalidate.invalidateQueries({ queryKey: queryKeys.scanState(scanId) });
      if (
        event.status === 'completed' ||
        event.status === 'cancelled' ||
        event.status === 'failed'
      ) {
        void invalidate.invalidateQueries({ queryKey: ['scan', scanId, 'tree'] });
        void invalidate.invalidateQueries({ queryKey: ['scan', scanId, 'results'] });
        void invalidate.invalidateQueries({ queryKey: ['scan', scanId, 'count'] });
        void invalidate.invalidateQueries({ queryKey: queryKeys.sessions });
      }
    } else if (event.status === 'running' && bucket > lastBucket) {
      lastBucket = bucket;
      void invalidate.invalidateQueries({ queryKey: ['scan', scanId, 'tree'] });
      void invalidate.invalidateQueries({ queryKey: ['scan', scanId, 'results'] });
      void invalidate.invalidateQueries({ queryKey: ['scan', scanId, 'count'] });
    }
    emit();
  });
  // prime from state asynchronously (arrival triggers a re-render via emit)
  void connector.scan
    .getScanState(scanId)
    .then((state: ScanState) => {
      if (current === null) {
        current = state.progress;
        emit();
      }
    })
    .catch(() => undefined);
  const store: ScanProgressStore = {
    getSnapshot: () => current,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
        if (listeners.size === 0) {
          unsubscribe();
          scanStores.delete(scanId);
        }
      };
    },
  };
  scanStores.set(scanId, store);
  return store;
}

const NULL_SNAPSHOT: null = null;

export function useStartScan() {
  const qc = useQueryClient();
  const setActive = useScanSessionStore((s) => s.setActiveScan);
  return useMutation({
    mutationFn: (params: ScanParams) => connector.scan.startScan(params),
    onSuccess: ({ scanId }, params) => {
      setActive({ scanId, targetId: params.targetId });
      void qc.invalidateQueries({ queryKey: ['scan', scanId] });
    },
  });
}

export function useScanState(scanId: string | null) {
  return useQuery({
    queryKey: queryKeys.scanState(scanId ?? 'none'),
    queryFn: () => connector.scan.getScanState(scanId!),
    enabled: scanId !== null,
    refetchInterval: (query) => {
      const status = query.state.data?.status;
      return status === 'running' || status === 'starting' ? 500 : false;
    },
  });
}

/**
 * Live progress via the connector event stream (external store — no polling,
 * no setState-in-effect). The 100–250 ms mock ticks render directly.
 */
export function useScanProgress(scanId: string | null): ScanProgressEvent | null {
  const qc = useQueryClient();
  const getQueryClient = useRef(() => qc); // ref: stable getter; call via .current()
  const store = useSyncExternalStore(
    scanId
      ? (listener) => getScanStore(scanId, () => getQueryClient.current()).subscribe(listener)
      : noopSubscribe,
    scanId
      ? () => getScanStore(scanId, () => getQueryClient.current()).getSnapshot()
      : () => NULL_SNAPSHOT,
    () => NULL_SNAPSHOT,
  );
  return store;
}

function noopSubscribe(): () => void {
  return () => undefined;
}

export function usePauseScan(scanId: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => connector.scan.pauseScan(scanId!),
    onSuccess: () => void qc.invalidateQueries({ queryKey: queryKeys.scanState(scanId!) }),
  });
}

export function useResumeScan(scanId: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => connector.scan.resumeScan(scanId!),
    onSuccess: () => void qc.invalidateQueries({ queryKey: queryKeys.scanState(scanId!) }),
  });
}

export function useCancelScan(scanId: string | null) {
  const qc = useQueryClient();
  const markStopped = useScanSessionStore((s) => s.markScanStopped);
  return useMutation({
    mutationFn: () => connector.scan.cancelScan(scanId!),
    onSuccess: () => {
      markStopped();
      void qc.invalidateQueries({ queryKey: queryKeys.scanState(scanId!) });
      void qc.invalidateQueries({ queryKey: queryKeys.sessions });
    },
  });
}
