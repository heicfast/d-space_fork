import { useQuery } from '@tanstack/react-query';
import { connector } from '@/connector';
import { queryKeys } from './client';

/** The connector owns kind inference (ADR-008 contract change); the UI
 *  renders whatever payload kind comes back. */
export function usePreview(scanId: string | null, nodeId: string | null) {
  return useQuery({
    queryKey: queryKeys.preview(scanId ?? 'none', nodeId ?? 'none', 'auto'),
    queryFn: () => connector.preview.requestPreview(scanId!, nodeId!),
    enabled: scanId !== null && nodeId !== null,
    staleTime: 60_000,
    retry: false, // preview failures are terminal diagnostics, not transient
  });
}
