import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  createLogger,
  exportLogText,
  getLogBuffer,
  installMainThreadWatchdog,
} from '@/lib/logger';

describe('structured logger (ring buffer contract)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('mirrors every level into the ring with scope and data', () => {
    const log = createLogger('probe');
    const before = getLogBuffer().length;
    log.debug('dbg', { k: 1 });
    log.info('inf');
    log.warn('wrn');
    log.error('err', { reason: 'x' });
    const buf = getLogBuffer();
    expect(buf.length).toBeGreaterThanOrEqual(before + 4);
    const tail = buf.slice(-4);
    expect(tail.map((e) => [e.level, e.scope, e.message])).toEqual([
      ['debug', 'probe', 'dbg'],
      ['info', 'probe', 'inf'],
      ['warn', 'probe', 'wrn'],
      ['error', 'probe', 'err'],
    ]);
    expect(tail[0].data).toEqual({ k: 1 });
    expect(tail[3].data).toEqual({ reason: 'x' });
  });

  it('exportLogText renders ISO stamps, padded levels and JSON data', () => {
    const log = createLogger('fmt');
    log.warn('formatted entry', { n: 7 });
    const text = exportLogText();
    const last = text.split('\n').filter(Boolean).pop()!;
    expect(last).toContain('WARN ');
    expect(last).toContain('[fmt] formatted entry');
    expect(last).toContain('{"n":7}');
    expect(() => new Date(last.slice(0, 24)).toISOString()).not.toThrow();
  });

  it('never writes to the console while MODE=test', () => {
    const debug = vi.spyOn(console, 'debug');
    const error = vi.spyOn(console, 'error');
    const log = createLogger('quiet');
    log.debug('no console in test mode');
    log.error('no console in test mode either');
    expect(debug).not.toHaveBeenCalled();
    expect(error).not.toHaveBeenCalled();
  });

  it('watchdog degrades to a no-op without PerformanceObserver', () => {
    const original = (globalThis as Record<string, unknown>).PerformanceObserver;
    vi.stubGlobal('PerformanceObserver', undefined);
    const stop = installMainThreadWatchdog();
    expect(typeof stop).toBe('function');
    stop();
    vi.stubGlobal('PerformanceObserver', original);
  });

  it('watchdog surfaces long tasks above 50ms as perf warnings', () => {
    const observed: Array<{ type: string; buffered: boolean }> = [];
    const disconnect = vi.fn();
    class FakeObserver {
      constructor(
        private readonly cb: (list: { getEntries: () => Array<{ duration: number }> }) => void,
      ) {}
      observe(opts: { type: string; buffered: boolean }) {
        observed.push(opts);
        this.cb({ getEntries: () => [{ duration: 80 }, { duration: 12 }] });
      }
      disconnect = disconnect;
    }
    vi.stubGlobal('PerformanceObserver', FakeObserver);
    const warn = vi.spyOn(console, 'warn');
    const stop = installMainThreadWatchdog();
    expect(observed).toEqual([{ type: 'longtask', buffered: true }]);
    const buf = getLogBuffer();
    const longTask = buf[buf.length - 1];
    expect(longTask.level).toBe('warn');
    expect(longTask.scope).toBe('perf');
    expect(longTask.message).toBe('long-task');
    expect(longTask.data).toEqual({ duration: 80 });
    expect(warn).not.toHaveBeenCalled(); // test mode: ring only
    stop();
    expect(disconnect).toHaveBeenCalled();
  });
});
