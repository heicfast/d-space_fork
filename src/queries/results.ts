import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { connector } from '@/connector';
import type {
  ResultsFilter,
  ResultTreeMode,
  SortDirection,
  SortField,
} from '@/connector/contract';
import { queryKeys } from './client';

export const RESULTS_PAGE_SIZE = 200;

export function filterKeyOf(filter: ResultsFilter): string {
  return JSON.stringify(filter);
}

export function sortKeyOf(sort: { field: SortField; direction: SortDirection }): string {
  return `${sort.field}:${sort.direction}`;
}

export function useResultTree(
  scanId: string | null,
  parentId: string | null,
  mode: ResultTreeMode,
) {
  return useQuery({
    queryKey: queryKeys.tree(scanId ?? 'none', parentId, mode),
    queryFn: () => connector.results.getResultTree(scanId!, parentId, mode),
    enabled: scanId !== null,
    staleTime: 10_000,
  });
}

export function useResultsPages(
  scanId: string | null,
  filter: ResultsFilter,
  sort: { field: SortField; direction: SortDirection },
) {
  const fk = filterKeyOf(filter);
  const sk = sortKeyOf(sort);
  return useInfiniteQuery({
    queryKey: queryKeys.results(scanId ?? 'none', fk, sk),
    queryFn: ({ pageParam }) =>
      connector.results.queryResults(scanId!, filter, sort, {
        offset: pageParam,
        limit: RESULTS_PAGE_SIZE,
      }),
    initialPageParam: 0,
    getNextPageParam: (lastPage) => {
      const next = lastPage.offset + lastPage.pageSize;
      return next < lastPage.total ? next : undefined;
    },
    enabled: scanId !== null,
  });
}

/** Filtered-total count — select-all semantics + recover-all sizing. */
export function useCountResults(scanId: string | null, filter: ResultsFilter, enabled = true) {
  const fk = filterKeyOf(filter);
  return useQuery({
    queryKey: queryKeys.count(scanId ?? 'none', fk),
    queryFn: () => connector.results.countResults(scanId!, filter),
    enabled: scanId !== null && enabled,
    staleTime: 5_000,
  });
}

export function useNodeDetail(scanId: string | null, nodeId: string | null) {
  return useQuery({
    queryKey: queryKeys.nodeDetail(scanId ?? 'none', nodeId ?? 'none'),
    queryFn: () => connector.results.getNodeDetail(scanId!, nodeId!),
    enabled: scanId !== null && nodeId !== null,
  });
}

export function useThumbnail(scanId: string | null, nodeId: string | null, enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.thumbnail(scanId ?? 'none', nodeId ?? 'none'),
    queryFn: () => connector.preview.getThumbnail(scanId!, nodeId!),
    enabled: scanId !== null && nodeId !== null && enabled,
    staleTime: Infinity,
  });
}
