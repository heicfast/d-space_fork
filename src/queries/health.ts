import { useQuery } from '@tanstack/react-query';
import { connector } from '@/connector';
import { queryKeys } from './client';

export function useHealth(deviceId: string | null) {
  return useQuery({
    queryKey: queryKeys.health(deviceId ?? 'none'),
    queryFn: () => connector.health.getHealth(deviceId!),
    enabled: deviceId !== null,
    staleTime: 60_000,
    retry: false,
  });
}
