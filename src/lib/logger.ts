/**
 * Structured logger — the ONLY logging surface in shipped code
 * (enforced by eslint no-console). Console output exists in dev only;
 * entries are always mirrored into a bounded in-memory ring buffer that the
 * Settings → Diagnostics surface renders, so end users can inspect and export
 * logs without touching the filesystem.
 */

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
  readonly t: number;
  readonly level: LogLevel;
  readonly scope: string;
  readonly message: string;
  readonly data?: Record<string, unknown>;
}

const RING_CAPACITY = 500;
const ring: LogEntry[] = [];

const LEVEL_ORDER: Record<LogLevel, number> = {
  debug: 10,
  info: 20,
  warn: 30,
  error: 40,
};

const minLevel: LogLevel = import.meta.env.DEV ? 'debug' : 'warn';

function push(entry: LogEntry): void {
  ring.push(entry);
  if (ring.length > RING_CAPACITY) {
    ring.shift();
  }
}

function emit(
  level: LogLevel,
  scope: string,
  message: string,
  data?: Record<string, unknown>,
): void {
  if (LEVEL_ORDER[level] < LEVEL_ORDER[minLevel]) {
    return;
  }
  const entry: LogEntry = { t: Date.now(), level, scope, message, data };
  push(entry);
  if (import.meta.env.DEV && import.meta.env.MODE !== 'test') {
    const fn =
      level === 'debug'
        ? console.debug
        : level === 'info'
          ? console.info
          : level === 'warn'
            ? console.warn
            : console.error;
    if (data) fn(`[${scope}] ${message}`, data);
    else fn(`[${scope}] ${message}`);
  }
}

export function createLogger(scope: string) {
  return {
    debug: (message: string, data?: Record<string, unknown>) =>
      emit('debug', scope, message, data),
    info: (message: string, data?: Record<string, unknown>) =>
      emit('info', scope, message, data),
    warn: (message: string, data?: Record<string, unknown>) =>
      emit('warn', scope, message, data),
    error: (message: string, data?: Record<string, unknown>) =>
      emit('error', scope, message, data),
  };
}

export function getLogBuffer(): readonly LogEntry[] {
  return ring;
}

export function exportLogText(): string {
  return ring
    .map(
      (e) =>
        `${new Date(e.t).toISOString()} ${e.level.toUpperCase().padEnd(5)} [${e.scope}] ${e.message}${e.data ? ` ${JSON.stringify(e.data)}` : ''}`,
    )
    .join('\n');
}

/** Dev watchdog: long-task observer feeding the performance overlay (grid #38). */
export function installMainThreadWatchdog(): () => void {
  if (typeof PerformanceObserver === 'undefined') return () => undefined;
  const logger = createLogger('perf');
  const observer = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      if (entry.duration > 50) {
        logger.warn('long-task', { duration: Math.round(entry.duration) });
      }
    }
  });
  try {
    observer.observe({ type: 'longtask', buffered: true });
  } catch {
    // longtask observation unsupported — watchdog is best-effort only
  }
  return () => observer.disconnect();
}
