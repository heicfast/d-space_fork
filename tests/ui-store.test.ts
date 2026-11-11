import { beforeEach, describe, expect, it } from 'vitest';
import { useUiStore } from '@/stores/ui-store';

describe('ui store', () => {
  beforeEach(() => {
    localStorage.clear();
    useUiStore.setState({
      viewMode: 'details',
      previewPaneWidth: 320,
      sidebarCollapsed: false,
      devPanelOpen: false,
      perfOverlayVisible: false,
    });
  });

  it('setViewMode switches the results presentation', () => {
    useUiStore.getState().setViewMode('icons');
    expect(useUiStore.getState().viewMode).toBe('icons');
    useUiStore.getState().setViewMode('details');
    expect(useUiStore.getState().viewMode).toBe('details');
  });

  it('setPreviewPaneWidth clamps + rounds into the 240..560 envelope', () => {
    useUiStore.getState().setPreviewPaneWidth(100);
    expect(useUiStore.getState().previewPaneWidth).toBe(240);
    useUiStore.getState().setPreviewPaneWidth(999.6);
    expect(useUiStore.getState().previewPaneWidth).toBe(560);
    useUiStore.getState().setPreviewPaneWidth(400.49);
    expect(useUiStore.getState().previewPaneWidth).toBe(400);
  });

  it('setSidebarCollapsed persists the choice across sessions', () => {
    useUiStore.getState().setSidebarCollapsed(true);
    expect(useUiStore.getState().sidebarCollapsed).toBe(true);
    expect(localStorage.getItem('rw.ui.sidebarCollapsed')).toBe('1');
    useUiStore.getState().setSidebarCollapsed(false);
    expect(localStorage.getItem('rw.ui.sidebarCollapsed')).toBe('0');
  });

  it('toggleSidebar flips the current state and persists it', () => {
    useUiStore.getState().toggleSidebar();
    expect(useUiStore.getState().sidebarCollapsed).toBe(true);
    expect(localStorage.getItem('rw.ui.sidebarCollapsed')).toBe('1');
    useUiStore.getState().toggleSidebar();
    expect(useUiStore.getState().sidebarCollapsed).toBe(false);
    expect(localStorage.getItem('rw.ui.sidebarCollapsed')).toBe('0');
  });

  it('dev panel setters and toggle drive the dev-build panel', () => {
    useUiStore.getState().setDevPanelOpen(true);
    expect(useUiStore.getState().devPanelOpen).toBe(true);
    useUiStore.getState().toggleDevPanel();
    expect(useUiStore.getState().devPanelOpen).toBe(false);
    useUiStore.getState().toggleDevPanel();
    expect(useUiStore.getState().devPanelOpen).toBe(true);
  });

  it('setPerfOverlay toggles the performance overlay visibility', () => {
    useUiStore.getState().setPerfOverlay(true);
    expect(useUiStore.getState().perfOverlayVisible).toBe(true);
    useUiStore.getState().setPerfOverlay(false);
    expect(useUiStore.getState().perfOverlayVisible).toBe(false);
  });
});
