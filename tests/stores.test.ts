import { beforeEach, describe, expect, it } from 'vitest';
import { selectedCountOf, useSelectionStore } from '@/stores/selection-store';
import {
  dateRangeOf,
  resolveFilter,
  sizeRangeOf,
  useFilterStore,
  useSortStore,
} from '@/stores/filter-store';

describe('selection store', () => {
  beforeEach(() => {
    useSelectionStore.getState().clear();
  });

  it('toggles membership and tracks anchor', () => {
    const s = useSelectionStore.getState();
    s.toggle('a');
    s.toggle('b');
    expect(useSelectionStore.getState().ids.size).toBe(2);
    expect(useSelectionStore.getState().anchor).toBe('b');
    s.toggle('b');
    expect(useSelectionStore.getState().ids.has('b')).toBe(false);
  });

  it('setOnly replaces the selection and resets the anchor', () => {
    useSelectionStore.getState().setOnly(['x', 'y', 'z']);
    expect(useSelectionStore.getState().ids.size).toBe(3);
    expect(useSelectionStore.getState().anchor).toBeNull();
  });

  it('select-all switches to all-mode with a filter snapshot + count', () => {
    const filter = resolveFilter();
    useSelectionStore.getState().setAll(filter, 1_000);
    let s = useSelectionStore.getState();
    expect(s.mode).toBe('all');
    expect(s.allCount).toBe(1_000);
    expect(s.allFilter).toBe(filter);
    // Row toggles become exclusions, not membership.
    s.toggle('row-1');
    s = useSelectionStore.getState();
    expect(s.exclusions.has('row-1')).toBe(true);
    expect(s.isSelected('row-1')).toBe(false);
    expect(s.isSelected('row-2')).toBe(true);
    expect(selectedCountOf(s)).toBe(999);
    // Untoggle restores membership.
    s.toggle('row-1');
    expect(selectedCountOf(useSelectionStore.getState())).toBe(1_000);
  });

  it('clear returns to empty ids-mode', () => {
    useSelectionStore.getState().setAll(resolveFilter(), 10);
    useSelectionStore.getState().clear();
    const s = useSelectionStore.getState();
    expect(s.mode).toBe('ids');
    expect(s.ids.size).toBe(0);
    expect(selectedCountOf(s)).toBe(0);
  });
});

describe('filter store', () => {
  beforeEach(() => {
    useFilterStore.getState().reset();
  });

  it('tracks active filters', () => {
    const f = useFilterStore.getState();
    expect(f.hasActiveFilters()).toBe(false);
    f.setSearch('report');
    f.commitSearch();
    expect(useSelectionStore.getState().mode).toBe('ids');
    expect(useFilterStore.getState().hasActiveFilters()).toBe(true);
    useFilterStore.getState().reset();
    expect(useFilterStore.getState().hasActiveFilters()).toBe(false);
  });

  it('toggles categories without duplicates', () => {
    const f = useFilterStore.getState();
    f.toggleCategory('graphics');
    f.toggleCategory('video');
    expect(useFilterStore.getState().categories).toEqual(['graphics', 'video']);
    useFilterStore.getState().toggleCategory('graphics');
    expect(useFilterStore.getState().categories).toEqual(['video']);
  });

  it('parses custom extension lists', () => {
    const f = useFilterStore.getState();
    f.setExtensionInput('*.doc; *.xlsx, ppt');
    f.commitExtensions();
    expect(useFilterStore.getState().extensions).toEqual(['doc', 'xlsx', 'ppt']);
  });

  it('carries the corpus filter toggles into the payload', () => {
    const f = useFilterStore.getState();
    f.setExistingOnly(true);
    f.setHideEmptyFiles(true);
    const payload = resolveFilter();
    expect(payload.origins).toEqual(['existing']);
    expect(payload.hideEmptyFiles).toBe(true);
    expect(useFilterStore.getState().hasActiveFilters()).toBe(true);
  });
});

describe('date/size range resolution', () => {
  it('maps presets to ISO bounds', () => {
    const any = dateRangeOf('any', null, null);
    expect(any.from).toBeNull();
    const custom = dateRangeOf('custom', '2024-02-01', '2024-03-01');
    expect(custom.from!.startsWith('2024-02-01')).toBe(true);
    expect(custom.to!.startsWith('2024-03-01')).toBe(true);
  });

  it('maps size presets to byte bounds', () => {
    expect(sizeRangeOf('tiny', null, null).maxBytes).toBe(512 * 1024);
    expect(sizeRangeOf('huge', null, null).minBytes).toBe(1024 ** 3);
    const custom = sizeRangeOf('custom', '10', '20');
    expect(custom.minBytes).toBe(10 * 1024 ** 2);
    expect(custom.maxBytes).toBe(20 * 1024 ** 2);
  });
});

describe('sort store', () => {
  it('flips direction when the same field is chosen twice', () => {
    useSortStore.setState({ field: 'name', direction: 'asc' });
    useSortStore.getState().setSort('name');
    expect(useSortStore.getState().direction).toBe('desc');
  });

  it('switches fields with sensible defaults', () => {
    useSortStore.setState({ field: 'name', direction: 'asc' });
    useSortStore.getState().setSort('size');
    expect(useSortStore.getState().field).toBe('size');
    expect(useSortStore.getState().direction).toBe('desc');
  });
});
