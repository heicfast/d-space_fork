/**
 * Localized formatting helpers (grid #49 — number/date/byte formatting via Intl).
 * Locale is taken from i18next at call time; formatters are memoized per locale.
 */

const dateFormatters = new Map<string, Intl.DateTimeFormat>();
const timeFormatters = new Map<string, Intl.DateTimeFormat>();
const numberFormatters = new Map<string, Intl.NumberFormat>();

/** Human byte size, e.g. 1.5 GB — binary (1024) steps with decimal-style
 *  labels, matching Windows/Qt convention the corpus volumes use
 *  (GT-scan §8: "147.12 GB" found on a 149.45 GB volume = binary GiB). */
export function formatBytes(
  bytes: number,
  locale: string,
  opts?: { inputUnit?: 'B' | 'KB' },
): string {
  const value = opts?.inputUnit === 'KB' ? bytes * 1024 : bytes;
  if (!Number.isFinite(value) || value < 0) return '—';
  const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  let v = value;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const nf = new Intl.NumberFormat(locale, {
    maximumFractionDigits: i === 0 ? 0 : 1,
  });
  return `${nf.format(v)} ${units[i]}`;
}

/** Bytes with unit variants per unit for compact display. */
export function formatBytesCompact(bytes: number, locale: string): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—';
  const units = ['B', 'K', 'M', 'G', 'T'];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const nf = new Intl.NumberFormat(locale, { maximumFractionDigits: v >= 10 ? 0 : 1 });
  return `${nf.format(v)}${units[i]}`;
}

/** 2-decimal GB label matching the reference drive cards ("31.16 GB", "149.45 GB", "150 GB"). */
export function formatDriveGb(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—';
  const gb = bytes / 1024 ** 3;
  const rounded = Math.round(gb * 100) / 100;
  return `${Number(rounded.toFixed(2))} GB`;
}

export function formatDate(
  iso: string | number | Date | null | undefined,
  locale: string,
): string {
  if (iso === null || iso === undefined) return '—';
  const d = iso instanceof Date ? iso : new Date(iso);
  if (Number.isNaN(d.getTime())) return '—';
  let f = dateFormatters.get(locale);
  if (!f) {
    f = new Intl.DateTimeFormat(locale, { year: 'numeric', month: '2-digit', day: '2-digit' });
    dateFormatters.set(locale, f);
  }
  return f.format(d);
}

export function formatDateTime(
  iso: string | number | Date | null | undefined,
  locale: string,
): string {
  if (iso === null || iso === undefined) return '—';
  const d = iso instanceof Date ? iso : new Date(iso);
  if (Number.isNaN(d.getTime())) return '—';
  const key = `${locale}:dt`;
  let f = timeFormatters.get(key);
  if (!f) {
    f = new Intl.DateTimeFormat(locale, {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    });
    timeFormatters.set(key, f);
  }
  return f.format(d);
}

export function formatInteger(value: number, locale: string): string {
  let f = numberFormatters.get(locale);
  if (!f) {
    f = new Intl.NumberFormat(locale, { useGrouping: true });
    numberFormatters.set(locale, f);
  }
  return f.format(value);
}

/** Compact "1.2M"-style counts for live counters. */
export function formatCountCompact(value: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(value);
}

/** Duration in seconds → localized mm:ss / hh:mm:ss. */
export function formatDuration(totalSeconds: number, locale: string): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) return '—';
  const s = Math.floor(totalSeconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const nf = new Intl.NumberFormat(locale, { minimumIntegerDigits: 2 });
  if (h > 0) return `${nf.format(h)}:${nf.format(m)}:${nf.format(sec)}`;
  return `${nf.format(m)}:${nf.format(sec)}`;
}

/** Throughput, bytes/s. */
export function formatThroughput(bytesPerSecond: number, locale: string): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '—';
  return `${formatBytes(bytesPerSecond, locale)}/s`;
}

export function formatPercent(fraction: number, locale: string): string {
  if (!Number.isFinite(fraction)) return '—';
  const nf = new Intl.NumberFormat(locale, { style: 'percent', maximumFractionDigits: 1 });
  return nf.format(Math.min(1, Math.max(0, fraction)));
}
