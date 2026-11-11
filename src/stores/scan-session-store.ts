import { create } from 'zustand';

interface ActiveScan {
  scanId: string;
  targetId: string;
}

interface ScanSessionState {
  active: ActiveScan | null;
  /** Scan ids known to this app session (live or finished). */
  known: Record<string, ActiveScan>;
  /** Route-intent helpers for the celebration → results handoff. */
  celebrationSeenFor: string | null;
  setActiveScan: (scan: ActiveScan) => void;
  markScanStopped: () => void;
  setActiveStatus: (scanId: string, status: 'completed' | 'cancelled' | 'failed') => void;
  markCelebrationSeen: (scanId: string) => void;
}

export const useScanSessionStore = create<ScanSessionState>((set, get) => ({
  active: null,
  known: {},
  celebrationSeenFor: null,
  setActiveScan: (scan) =>
    set((state) => ({
      active: scan,
      known: { ...state.known, [scan.scanId]: scan },
      celebrationSeenFor: null,
    })),
  markScanStopped: () => {
    const active = get().active;
    if (!active) return;
    set((state) => ({
      active: null,
      known: { ...state.known, [active.scanId]: { ...active } },
    }));
  },
  setActiveStatus: (scanId, status) => {
    if (status !== 'completed') {
      get().markScanStopped();
      return;
    }
    const scan = get().known[scanId];
    if (scan) {
      set({ active: null, known: { ...get().known, [scanId]: { ...scan } } });
    }
  },
  markCelebrationSeen: (scanId) => set({ celebrationSeenFor: scanId }),
}));
