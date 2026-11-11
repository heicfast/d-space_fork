import { describe, expect, it } from 'vitest';
import {
  formatBytes,
  formatBytesCompact,
  formatCountCompact,
  formatDate,
  formatDateTime,
  formatDuration,
  formatInteger,
  formatPercent,
  formatThroughput,
} from '@/lib/format';
import { formatHexRows } from '@/lib/hex-format';
import { mulberry32, randInt } from '@/connector/mock/rng';

describe('formatting (grid #49 — Intl-based)', () => {
  it('formats bytes with binary steps (Windows convention, corpus parity)', () => {
    expect(formatBytes(0, 'en')).toBe('0 B');
    expect(formatBytes(1536, 'en')).toBe('1.5 KB');
    expect(formatBytes(1536, 'en', { inputUnit: 'KB' })).toBe('1.5 MB');
    expect(formatBytes(5 * 1024 ** 3, 'en')).toBe('5 GB');
    expect(formatBytes(149.45 * 1024 ** 3, 'en')).toBe('149.5 GB');
    expect(formatBytes(-1, 'en')).toBe('—');
  });

  it('formats compact bytes and integers', () => {
    expect(formatBytesCompact(0, 'en')).toBe('0B');
    expect(formatBytesCompact(2048, 'en')).toBe('2K');
    expect(formatBytesCompact(-1, 'en')).toBe('—');
    expect(formatInteger(1234567, 'en')).toBe('1,234,567');
  });

  it('formats dates and date-times with fallback on invalid values', () => {
    expect(formatDate(null, 'en')).toBe('—');
    expect(formatDate('invalid-date', 'en')).toBe('—');
    expect(formatDate('2025-01-15T10:30:00Z', 'en')).toMatch(/2025/);
    expect(formatDateTime(null, 'en')).toBe('—');
    expect(formatDateTime('invalid-date', 'en')).toBe('—');
    expect(formatDateTime(new Date('2025-01-15T10:30:00Z'), 'en')).toMatch(/2025/);
  });

  it('formats durations as mm:ss / hh:mm:ss', () => {
    expect(formatDuration(65, 'en')).toBe('01:05');
    expect(formatDuration(3671, 'en')).toBe('01:01:11');
    expect(formatDuration(0, 'en')).toBe('00:00');
  });

  it('formats percentages clamped to [0,1]', () => {
    expect(formatPercent(0.42, 'en')).toBe('42%');
    expect(formatPercent(1.4, 'en')).toBe('100%');
    expect(formatPercent(-0.2, 'en')).toBe('0%');
  });

  it('formats throughput with units', () => {
    expect(formatThroughput(2 * 1024 ** 2, 'en')).toBe('2 MB/s');
    expect(formatThroughput(0, 'en')).toBe('—');
  });

  it('compacts large counts', () => {
    expect(formatCountCompact(1_200_000, 'en')).toMatch(/1\.2M/);
  });

  it('formats 16-byte aligned hex rows with ASCII substitution', () => {
    const bytes = new Uint8Array([0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x00, 0xff, 0x20, 0x41]);
    const rows = formatHexRows(bytes, 0x100);
    expect(rows).toHaveLength(1);
    expect(rows[0].offset).toBe(0x100);
    expect(rows[0].ascii).toBe('Hello·· A');
    expect(rows[0].hex.startsWith('48 65 6c 6c 6f 00 ff 20  41')).toBe(true);
  });
});

describe('mulberry32 RNG', () => {
  it('produces identical sequences for identical seeds', () => {
    const a = mulberry32(42);
    const b = mulberry32(42);
    for (let i = 0; i < 100; i++) {
      expect(a()).toBe(b());
    }
  });

  it('randInt stays within bounds', () => {
    const rng = mulberry32(7);
    for (let i = 0; i < 200; i++) {
      const v = randInt(rng, 3, 9);
      expect(v).toBeGreaterThanOrEqual(3);
      expect(v).toBeLessThanOrEqual(9);
    }
  });
});
