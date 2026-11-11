import { create } from 'zustand';
import type { ResultsFilter } from '@/connector/contract';

/**
 * Selection model (corpus: tri-state CSelAllCheckBox + per-row checkboxes).
 * Two modes:
 *  - 'ids': an explicit node-id set (individual toggles / ranges);
 *  - 'all': everything matching a filter snapshot (select-all over the full
 *    filtered total, not just loaded rows — audit item 52), minus a set of
 *    per-row exclusions.
 */
interface SelectionState {
  mode: 'ids' | 'all';
  ids: Set<string>;
  exclusions: Set<string>;
  /** Filter snapshot + count captured when select-all fired. */
  allFilter: ResultsFilter | null;
  allCount: number;
  anchor: string | null;
  setOnly: (ids: readonly string[]) => void;
  setAll: (filter: ResultsFilter, count: number) => void;
  toggle: (id: string) => void;
  /** Effective membership test for a row checkbox. */
  isSelected: (id: string) => boolean;
  clear: () => void;
  setAnchor: (id: string | null) => void;
}

export const useSelectionStore = create<SelectionState>((set, get) => ({
  mode: 'ids',
  ids: new Set<string>(),
  exclusions: new Set<string>(),
  allFilter: null,
  allCount: 0,
  anchor: null,
  setOnly: (ids) =>
    set({
      mode: 'ids',
      ids: new Set(ids),
      exclusions: new Set(),
      allFilter: null,
      allCount: 0,
      anchor: null,
    }),
  setAll: (filter, count) =>
    set({
      mode: 'all',
      ids: new Set(),
      exclusions: new Set(),
      allFilter: filter,
      allCount: count,
      anchor: null,
    }),
  toggle: (id) => {
    const s = get();
    if (s.mode === 'all') {
      const next = new Set(s.exclusions);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      set({ exclusions: next, anchor: id });
      return;
    }
    const next = new Set(s.ids);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    set({ ids: next, anchor: id });
  },
  isSelected: (id) => {
    const s = get();
    return s.mode === 'all' ? !s.exclusions.has(id) : s.ids.has(id);
  },
  clear: () =>
    set({
      mode: 'ids',
      ids: new Set<string>(),
      exclusions: new Set<string>(),
      allFilter: null,
      allCount: 0,
      anchor: null,
    }),
  setAnchor: (id) => set({ anchor: id }),
}));

/** The user-visible selected count for the current mode. */
export function selectedCountOf(s: {
  mode: 'ids' | 'all';
  ids: Set<string>;
  exclusions: Set<string>;
  allCount: number;
}): number {
  return s.mode === 'all' ? Math.max(0, s.allCount - s.exclusions.size) : s.ids.size;
}
