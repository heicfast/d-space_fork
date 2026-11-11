import { create } from 'zustand';
import { createLogger } from '@/lib/logger';

const logger = createLogger('ui-store');

export type ResultsViewMode = 'details' | 'icons';

const SIDEBAR_KEY = 'rw.ui.sidebarCollapsed';

function readStoredSidebarCollapsed(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_KEY) === '1';
  } catch {
    /* storage unavailable */
  }
  return false;
}

interface UiState {
  /** Results list presentation. */
  viewMode: ResultsViewMode;
  /** Preview pane width (px) in the results layout (corpus: QSplitter,
   *  always present, drag-resizable). */
  previewPaneWidth: number;
  /** Sidebar collapse (app extension per the session mandate; persisted
   *  across sessions like the theme/density preferences). */
  sidebarCollapsed: boolean;
  /** Dev panel (dev builds only). */
  devPanelOpen: boolean;
  perfOverlayVisible: boolean;
  /** Watchdog install flag (once). */
  setViewMode: (mode: ResultsViewMode) => void;
  setPreviewPaneWidth: (w: number) => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebar: () => void;
  setDevPanelOpen: (open: boolean) => void;
  toggleDevPanel: () => void;
  setPerfOverlay: (visible: boolean) => void;
}

export const useUiStore = create<UiState>((set) => ({
  viewMode: 'details',
  previewPaneWidth: 320,
  sidebarCollapsed: readStoredSidebarCollapsed(),
  devPanelOpen: false,
  perfOverlayVisible: false,
  setViewMode: (mode) => {
    logger.debug('view-mode', { mode });
    set({ viewMode: mode });
  },
  setPreviewPaneWidth: (w) =>
    set({ previewPaneWidth: Math.min(560, Math.max(240, Math.round(w))) }),
  setSidebarCollapsed: (collapsed) => {
    logger.debug('sidebar-collapsed', { collapsed });
    try {
      localStorage.setItem(SIDEBAR_KEY, collapsed ? '1' : '0');
    } catch {
      /* storage unavailable */
    }
    set({ sidebarCollapsed: collapsed });
  },
  toggleSidebar: () =>
    set((s) => {
      const collapsed = !s.sidebarCollapsed;
      try {
        localStorage.setItem(SIDEBAR_KEY, collapsed ? '1' : '0');
      } catch {
        /* storage unavailable */
      }
      return { sidebarCollapsed: collapsed };
    }),
  setDevPanelOpen: (open) => set({ devPanelOpen: open }),
  toggleDevPanel: () => set((s) => ({ devPanelOpen: !s.devPanelOpen })),
  setPerfOverlay: (visible) => set({ perfOverlayVisible: visible }),
}));
