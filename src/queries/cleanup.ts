import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { connector } from '@/connector';
import type { CleanupUndoResolution, SnapshotDelta } from '@/connector/contract';

/**
 * Cleanup domain queries — the D-Space safety features (staging/undo,
 * rules, duplicates, size snapshots, USN quick search, engine settings).
 * Mirrors the query-key conventions of the recovery domains.
 */

export const cleanupKeys = {
  all: ['cleanup'] as const,
  staging: () => [...cleanupKeys.all, 'staging'] as const,
  expired: () => [...cleanupKeys.all, 'expired'] as const,
  rules: () => [...cleanupKeys.all, 'rules'] as const,
  duplicates: (scanId: string) => [...cleanupKeys.all, 'duplicates', scanId] as const,
  snapshots: () => [...cleanupKeys.all, 'snapshots'] as const,
  snapshotDelta: (from: number, to: number) =>
    [...cleanupKeys.all, 'snapshot-delta', from, to] as const,
  indexSearch: (query: string) => [...cleanupKeys.all, 'index', query] as const,
  setting: (key: string) => [...cleanupKeys.all, 'setting', key] as const,
};

function invalidateCleanup(qc: QueryClient): void {
  void qc.invalidateQueries({ queryKey: cleanupKeys.staging() });
  void qc.invalidateQueries({ queryKey: cleanupKeys.expired() });
}

export function useStaging() {
  return useQuery({
    queryKey: cleanupKeys.staging(),
    queryFn: () => connector.cleanup.listStaging(),
  });
}

export function useStagePath() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (path: string) => connector.cleanup.stagePath(path),
    onSuccess: () => invalidateCleanup(qc),
  });
}

export function useUndoStaging() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => connector.cleanup.undoStaging(id),
    onSuccess: () => invalidateCleanup(qc),
  });
}

export function useUndoResolve() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: number; resolution: CleanupUndoResolution }) =>
      connector.cleanup.undoResolve(args.id, args.resolution),
    onSuccess: () => invalidateCleanup(qc),
  });
}

export function usePermanentDelete() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: number; confirmPhrase: string; secureWipe: boolean }) =>
      connector.cleanup.permanentDelete(args.id, args.confirmPhrase, args.secureWipe),
    onSuccess: () => invalidateCleanup(qc),
  });
}

export function useExpiredStaging() {
  return useQuery({
    queryKey: cleanupKeys.expired(),
    queryFn: () => connector.cleanup.listExpired(),
  });
}

export function useCleanupExpired() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { ratePerSec: number; force: boolean }) =>
      connector.cleanup.cleanupExpired(args.ratePerSec, args.force),
    onSuccess: () => invalidateCleanup(qc),
  });
}

export function useRules() {
  return useQuery({
    queryKey: cleanupKeys.rules(),
    queryFn: () => connector.cleanup.listRules(),
  });
}

export function useAddRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: {
      pattern: string;
      patternType: 'name' | 'extension';
      score: number;
      explanation: string;
    }) =>
      connector.cleanup.addRule(args.pattern, args.patternType, args.score, args.explanation),
    onSuccess: () => void qc.invalidateQueries({ queryKey: cleanupKeys.rules() }),
  });
}

export function useDeleteRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => connector.cleanup.deleteRule(id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: cleanupKeys.rules() }),
  });
}

export function useToggleRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: number; enabled: boolean }) =>
      connector.cleanup.toggleRule(args.id, args.enabled),
    onSuccess: () => void qc.invalidateQueries({ queryKey: cleanupKeys.rules() }),
  });
}

export function useFindDuplicates() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { scanId: string; minSizeBytes?: number; sizeOnly?: boolean }) =>
      connector.cleanup.findDuplicates(args.scanId, {
        minSizeBytes: args.minSizeBytes,
        sizeOnly: args.sizeOnly,
      }),
    onSuccess: (result, args) => {
      qc.setQueryData(cleanupKeys.duplicates(args.scanId), result);
    },
  });
}

export function useSnapshots() {
  return useQuery({
    queryKey: cleanupKeys.snapshots(),
    queryFn: () => connector.cleanup.listSnapshots(),
  });
}

export function useCaptureSnapshot() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (scanId: string) => connector.cleanup.captureSnapshot(scanId),
    onSuccess: () => void qc.invalidateQueries({ queryKey: cleanupKeys.snapshots() }),
  });
}

export function useSnapshotDelta(from: number | null, to: number | null) {
  return useQuery<SnapshotDelta>({
    queryKey: cleanupKeys.snapshotDelta(from ?? -1, to ?? -1),
    queryFn: () => connector.cleanup.snapshotDelta(from!, to!),
    enabled: from !== null && to !== null && from !== to,
    staleTime: Infinity,
  });
}
