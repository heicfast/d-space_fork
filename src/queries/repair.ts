import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { connector } from '@/connector';
import { queryKeys } from './client';

export function useRepairDryRun(deviceId: string | null) {
  return useQuery({
    queryKey: queryKeys.repairDryRun(deviceId ?? 'none'),
    queryFn: () => connector.repair.dryRunRepair(deviceId!),
    enabled: deviceId !== null,
    retry: false,
  });
}

export function useApplyRepair(deviceId: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (consentToken: string) =>
      connector.repair.applyRepair(deviceId!, { consentToken }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.undoLog(deviceId ?? 'none') });
    },
  });
}

export function useUndoLog(deviceId: string | null) {
  return useQuery({
    queryKey: queryKeys.undoLog(deviceId ?? 'none'),
    queryFn: () => connector.repair.getUndoLog(deviceId!),
    enabled: deviceId !== null,
  });
}

export function useUndoRepairs(deviceId: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => connector.repair.undoRepairs(deviceId!),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.undoLog(deviceId ?? 'none') });
    },
  });
}
