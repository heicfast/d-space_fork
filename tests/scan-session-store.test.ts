import { beforeEach, describe, expect, it } from 'vitest';
import { useScanSessionStore } from '@/stores/scan-session-store';

/* The celebration → results handoff store: the active scan, the known-scan
 * registry and the one-shot celebration flag (route-intent contract). */
describe('scan session store', () => {
  beforeEach(() => {
    useScanSessionStore.setState({ active: null, known: {}, celebrationSeenFor: null });
  });

  it('setActiveScan registers the scan and resets the celebration flag', () => {
    useScanSessionStore.getState().markCelebrationSeen('stale-scan');
    useScanSessionStore.getState().setActiveScan({ scanId: 's1', targetId: 'disk-0' });
    const s = useScanSessionStore.getState();
    expect(s.active).toEqual({ scanId: 's1', targetId: 'disk-0' });
    expect(s.known['s1']).toEqual({ scanId: 's1', targetId: 'disk-0' });
    expect(s.celebrationSeenFor).toBeNull();
  });

  it('markScanStopped retires the active scan but keeps it known', () => {
    useScanSessionStore.getState().setActiveScan({ scanId: 's1', targetId: 'disk-0' });
    useScanSessionStore.getState().markScanStopped();
    const s = useScanSessionStore.getState();
    expect(s.active).toBeNull();
    expect(s.known['s1']).toEqual({ scanId: 's1', targetId: 'disk-0' });
  });

  it('markScanStopped without an active scan is a no-op', () => {
    useScanSessionStore.getState().setActiveScan({ scanId: 's1', targetId: 'disk-0' });
    useScanSessionStore.getState().markScanStopped();
    const known = { ...useScanSessionStore.getState().known };
    useScanSessionStore.getState().markScanStopped();
    expect(useScanSessionStore.getState().active).toBeNull();
    expect(useScanSessionStore.getState().known).toEqual(known);
  });

  it('setActiveStatus(completed) retires a known scan for the results route', () => {
    useScanSessionStore.getState().setActiveScan({ scanId: 's1', targetId: 'disk-0' });
    useScanSessionStore.getState().setActiveStatus('s1', 'completed');
    const s = useScanSessionStore.getState();
    expect(s.active).toBeNull();
    expect(s.known['s1']).toEqual({ scanId: 's1', targetId: 'disk-0' });
  });

  it('setActiveStatus(completed) for an unknown id changes nothing', () => {
    useScanSessionStore.getState().setActiveScan({ scanId: 's1', targetId: 'disk-0' });
    useScanSessionStore.getState().setActiveStatus('ghost', 'completed');
    const s = useScanSessionStore.getState();
    expect(s.active).toEqual({ scanId: 's1', targetId: 'disk-0' });
    expect(s.known['ghost']).toBeUndefined();
  });

  it('setActiveStatus(cancelled|failed) routes through markScanStopped', () => {
    useScanSessionStore.getState().setActiveScan({ scanId: 's1', targetId: 'disk-0' });
    useScanSessionStore.getState().setActiveStatus('s1', 'cancelled');
    expect(useScanSessionStore.getState().active).toBeNull();
    useScanSessionStore.getState().setActiveScan({ scanId: 's2', targetId: 'disk-1' });
    useScanSessionStore.getState().setActiveStatus('s2', 'failed');
    expect(useScanSessionStore.getState().active).toBeNull();
  });

  it('markCelebrationSeen pins the scan id exactly once', () => {
    useScanSessionStore.getState().markCelebrationSeen('s1');
    expect(useScanSessionStore.getState().celebrationSeenFor).toBe('s1');
    useScanSessionStore.getState().markCelebrationSeen('s2');
    expect(useScanSessionStore.getState().celebrationSeenFor).toBe('s2');
  });
});
