import { create } from 'zustand';
import type {
  FileCategoryId,
  ResultTreeMode,
  ResultsFilter,
  SortDirection,
  SortField,
} from '@/connector/contract';

export type DateRangePreset = 'any' | 'yesterday' | 'week' | 'month' | 'custom';
export type SizeRangePreset = 'any' | 'tiny' | 'small' | 'medium' | 'large' | 'huge' | 'custom';

export interface ResultsFilterUi {
  treeMode: ResultTreeMode;
  treeNodeId: string | null;
  search: string;
  /** Debounced search applied to queries. */
  searchApplied: string;
  categories: FileCategoryId[];
  datePreset: DateRangePreset;
  dateFrom: string | null; // yyyy-MM-dd
  dateTo: string | null;
  sizePreset: SizeRangePreset;
  sizeMin: string | null; // user-entered number, MB
  sizeMax: string | null;
  extensionInput: string;
  extensions: string[] | null;
  /** Corpus filter toggle: "Existing" (existing-files filter, atlas §9). */
  existingOnly: boolean;
  /** Corpus filter toggle: "Hide 0KB Files" (atlas §9). */
  hideEmptyFiles: boolean;
}

interface ResultsUiState extends ResultsFilterUi {
  setTreeMode: (mode: ResultTreeMode) => void;
  setTreeNode: (nodeId: string | null) => void;
  setSearch: (q: string) => void;
  commitSearch: () => void;
  toggleCategory: (c: FileCategoryId) => void;
  setCategories: (cs: FileCategoryId[]) => void;
  setDatePreset: (p: DateRangePreset, from?: string | null, to?: string | null) => void;
  setSizePreset: (p: SizeRangePreset, min?: string | null, max?: string | null) => void;
  setExtensionInput: (v: string) => void;
  commitExtensions: () => void;
  setExistingOnly: (v: boolean) => void;
  setHideEmptyFiles: (v: boolean) => void;
  reset: () => void;
  hasActiveFilters: () => boolean;
}

const INITIAL: ResultsFilterUi = {
  treeMode: 'path',
  treeNodeId: null,
  search: '',
  searchApplied: '',
  categories: [],
  datePreset: 'any',
  dateFrom: null,
  dateTo: null,
  sizePreset: 'any',
  sizeMin: null,
  sizeMax: null,
  extensionInput: '',
  extensions: null,
  existingOnly: false,
  hideEmptyFiles: false,
};

const DAY_MS = 86_400_000;

export function dateRangeOf(
  preset: DateRangePreset,
  from: string | null,
  to: string | null,
): { from: string | null; to: string | null } {
  const now = Date.now();
  switch (preset) {
    case 'yesterday':
      return { from: new Date(now - 1 * DAY_MS).toISOString(), to: null };
    case 'week':
      return { from: new Date(now - 7 * DAY_MS).toISOString(), to: null };
    case 'month':
      return { from: new Date(now - 30 * DAY_MS).toISOString(), to: null };
    case 'custom':
      return {
        from: from ? new Date(`${from}T00:00:00`).toISOString() : null,
        to: to ? new Date(`${to}T23:59:59`).toISOString() : null,
      };
    default:
      return { from: null, to: null };
  }
}

export function sizeRangeOf(
  preset: SizeRangePreset,
  min: string | null,
  max: string | null,
): { minBytes: number | null; maxBytes: number | null } {
  const MB = 1024 ** 2;
  switch (preset) {
    case 'tiny':
      return { minBytes: null, maxBytes: 512 * 1024 };
    case 'small':
      return { minBytes: 512 * 1024, maxBytes: 1 * MB };
    case 'medium':
      return { minBytes: 1 * MB, maxBytes: 512 * MB };
    case 'large':
      return { minBytes: 512 * MB, maxBytes: 1 * 1024 ** 3 };
    case 'huge':
      return { minBytes: 1 * 1024 ** 3, maxBytes: null };
    case 'custom': {
      const minBytes =
        min !== null && min !== '' && Number.isFinite(Number(min)) ? Number(min) * MB : null;
      const maxBytes =
        max !== null && max !== '' && Number.isFinite(Number(max)) ? Number(max) * MB : null;
      return { minBytes, maxBytes };
    }
    default:
      return { minBytes: null, maxBytes: null };
  }
}

export const useFilterStore = create<ResultsUiState>((set, get) => ({
  ...INITIAL,
  setTreeMode: (mode) => set({ treeMode: mode, treeNodeId: null }),
  setTreeNode: (nodeId) => set({ treeNodeId: nodeId }),
  setSearch: (q) => set({ search: q }),
  commitSearch: () => set({ searchApplied: get().search.trim() || '' }),
  toggleCategory: (c) =>
    set((state) => {
      const has = state.categories.includes(c);
      return {
        categories: has ? state.categories.filter((x) => x !== c) : [...state.categories, c],
      };
    }),
  setCategories: (cs) => set({ categories: cs }),
  setDatePreset: (p, from, to) =>
    set((state) => ({
      datePreset: p,
      dateFrom: from !== undefined ? from : state.dateFrom,
      dateTo: to !== undefined ? to : state.dateTo,
    })),
  setSizePreset: (p, min, max) =>
    set((state) => ({
      sizePreset: p,
      sizeMin: min !== undefined ? min : state.sizeMin,
      sizeMax: max !== undefined ? max : state.sizeMax,
    })),
  setExtensionInput: (v) => set({ extensionInput: v }),
  commitExtensions: () => {
    const raw = get().extensionInput;
    const list = raw
      .split(/[;, ]+/)
      .map((s) => s.trim().replace(/^\*?\.?/, ''))
      .filter((s) => s.length > 0);
    set({ extensions: list.length > 0 ? list : null });
  },
  setExistingOnly: (v) => set({ existingOnly: v }),
  setHideEmptyFiles: (v) => set({ hideEmptyFiles: v }),
  reset: () => set({ ...INITIAL }),
  hasActiveFilters: () => {
    const s = get();
    return (
      s.searchApplied !== '' ||
      s.categories.length > 0 ||
      s.datePreset !== 'any' ||
      s.sizePreset !== 'any' ||
      (s.extensions?.length ?? 0) > 0 ||
      s.existingOnly ||
      s.hideEmptyFiles
    );
  },
}));

// ---------------------------------------------------------------- sort store

interface SortState {
  field: SortField;
  direction: SortDirection;
  setSort: (field: SortField) => void;
  setDirection: (direction: SortDirection) => void;
}

export const useSortStore = create<SortState>((set, get) => ({
  field: 'modified',
  direction: 'desc',
  setSort: (field) => {
    const current = get();
    if (current.field === field) {
      set({ direction: current.direction === 'asc' ? 'desc' : 'asc' });
    } else {
      set({ field, direction: field === 'name' || field === 'type' ? 'asc' : 'desc' });
    }
  },
  setDirection: (direction) => set({ direction }),
}));

/** Resolves the UI filter state into the connector filter payload. */
export function resolveFilter(): ResultsFilter {
  const f = useFilterStore.getState();
  const date = dateRangeOf(f.datePreset, f.dateFrom, f.dateTo);
  const size = sizeRangeOf(f.sizePreset, f.sizeMin, f.sizeMax);
  return {
    nodeId: f.treeNodeId,
    treeMode: f.treeMode,
    search: f.searchApplied || null,
    categories: f.categories.length > 0 ? f.categories : null,
    dateFrom: date.from,
    dateTo: date.to,
    sizeMinBytes: size.minBytes,
    sizeMaxBytes: size.maxBytes,
    extensions: f.extensions,
    origins: f.existingOnly ? ['existing'] : null,
    hideEmptyFiles: f.hideEmptyFiles,
  };
}
