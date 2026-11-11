import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { connector } from '@/connector';
import { queryKeys } from './client';

export function useSessions() {
  return useQuery({
    queryKey: queryKeys.sessions,
    queryFn: () => connector.sessions.listSessions(),
  });
}

export function useSaveSession(scanId: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (path: string | null) => connector.sessions.saveSession(scanId!, path),
    onSuccess: () => void qc.invalidateQueries({ queryKey: queryKeys.sessions }),
  });
}

export function useLoadSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (path: string) => connector.sessions.loadSession(path),
    onSuccess: ({ scanId }) => {
      void qc.invalidateQueries({ queryKey: ['scan', scanId] });
      void qc.invalidateQueries({ queryKey: queryKeys.sessions });
    },
  });
}
