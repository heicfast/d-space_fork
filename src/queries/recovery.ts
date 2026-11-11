import { useSyncExternalStore } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { connector } from '@/connector';
import type {
  RecoverySelection,
  RecoveryOptions,
  RecoveryProgressEvent,
} from '@/connector/contract';
import { queryKeys } from './client';

// ---------------------------------------------------------------------------
// Recovery progress store — external-store pattern (see scan.ts).
// ---------------------------------------------------------------------------

interface JobProgressStore {
  getSnapshot: () => RecoveryProgressEvent | null;
  subscribe: (listener: () => void) => () => void;
}

const jobStores = new Map<string, JobProgressStore>();

function getJobStore(
  jobId: string,
  qc: () => { invalidateQueries: (q: { queryKey: readonly unknown[] }) => Promise<unknown> },
): JobProgressStore {
  const existing = jobStores.get(jobId);
  if (existing) return existing;
  let current: RecoveryProgressEvent | null = null;
  const listeners = new Set<() => void>();
  let lastStatus: string | null = null;
  const emit = (): void => {
    for (const l of listeners) l();
  };
  const unsubscribe = connector.recovery.onRecoveryProgress(jobId, (event) => {
    current = event;
    if (event.status !== lastStatus) {
      lastStatus = event.status;
      void qc().invalidateQueries({ queryKey: queryKeys.job(jobId) });
      void qc().invalidateQueries({ queryKey: queryKeys.jobItems(jobId) });
      if (event.status !== 'running' && event.status !== 'validating') {
        void qc().invalidateQueries({ queryKey: queryKeys.report(jobId) });
      }
    }
    emit();
  });
  const store: JobProgressStore = {
    getSnapshot: () => current,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
        if (listeners.size === 0) {
          unsubscribe();
          jobStores.delete(jobId);
        }
      };
    },
  };
  jobStores.set(jobId, store);
  return store;
}

const NULL_SNAPSHOT: null = null;
const noopSubscribe = (): (() => void) => () => undefined;

export function useStartRecovery(scanId: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: {
      selection: RecoverySelection;
      destination: string;
      options: RecoveryOptions;
    }) =>
      connector.recovery.startRecovery(
        scanId!,
        input.selection,
        input.destination,
        input.options,
      ),
    onSuccess: ({ jobId }) => {
      void qc.invalidateQueries({ queryKey: queryKeys.job(jobId) });
    },
  });
}

export function useRecoveryProgress(jobId: string | null): RecoveryProgressEvent | null {
  const qc = useQueryClient();
  return useSyncExternalStore(
    jobId ? (listener) => getJobStore(jobId, () => qc).subscribe(listener) : noopSubscribe,
    jobId ? () => getJobStore(jobId, () => qc).getSnapshot() : () => NULL_SNAPSHOT,
    () => NULL_SNAPSHOT,
  );
}

export function useRecoveryJob(jobId: string | null) {
  return useQuery({
    queryKey: queryKeys.job(jobId ?? 'none'),
    queryFn: () => connector.recovery.getRecoveryState(jobId!),
    enabled: jobId !== null,
  });
}

export function useRecoveryItems(jobId: string | null) {
  return useQuery({
    queryKey: queryKeys.jobItems(jobId ?? 'none'),
    queryFn: () => connector.recovery.getRecoveryItems(jobId!, { offset: 0, limit: 500 }),
    enabled: jobId !== null,
  });
}

export function useCancelRecovery(jobId: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => connector.recovery.cancelRecovery(jobId!),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.job(jobId!) });
      void qc.invalidateQueries({ queryKey: queryKeys.report(jobId!) });
    },
  });
}

export function useRecoveryReport(jobId: string | null) {
  return useQuery({
    queryKey: queryKeys.report(jobId ?? 'none'),
    queryFn: () => connector.recovery.getRecoveryReport(jobId!),
    enabled: jobId !== null,
  });
}
