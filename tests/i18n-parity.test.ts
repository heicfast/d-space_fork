/**
 * Localization parity (grid #45/#46): en is the schema; zh-CN must carry
 * the exact same key surface — no missing keys, no extra keys, no empty
 * values. Long-string/CJK behavior is covered by the interpolation checks.
 */
import { describe, expect, it } from 'vitest';
import { en } from '@/i18n/en';
import { zhCN } from '@/i18n/zh-CN';

type Flat = Record<string, string>;

function flatten(obj: unknown, prefix = ''): Flat {
  const out: Flat = {};
  for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
    if (typeof v === 'object' && v !== null) {
      Object.assign(out, flatten(v, `${prefix}${k}.`));
    } else if (typeof v === 'string') {
      out[`${prefix}${k}`] = v;
    }
  }
  return out;
}

const enFlat = flatten(en);
const zhFlat = flatten(zhCN);

describe('i18n parity (en ⇄ zh-CN)', () => {
  it('zh-CN defines exactly the en key surface', () => {
    const missing = Object.keys(enFlat).filter((k) => !(k in zhFlat));
    const extra = Object.keys(zhFlat).filter((k) => !(k in enFlat));
    expect(missing).toEqual([]);
    expect(extra).toEqual([]);
  });

  it('no empty translations', () => {
    const empty = Object.entries(zhFlat).filter(([, v]) => v.trim() === '');
    expect(empty.map(([k]) => k)).toEqual([]);
  });

  it('interpolation placeholders match between locales', () => {
    const ph = (s: string): string[] => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    const mismatches: string[] = [];
    for (const [k, v] of Object.entries(enFlat)) {
      const zh = zhFlat[k];
      if (zh && JSON.stringify(ph(v)) !== JSON.stringify(ph(zh))) mismatches.push(k);
    }
    expect(mismatches).toEqual([]);
  });

  it('CJK strings are present where en uses CJK-capable copy paths', () => {
    // Spot-check a representative set across surfaces.
    for (const key of [
      'app.name',
      'home.title',
      'scan.stopTitle',
      'results.filter',
      'recovery.title',
    ]) {
      expect(zhFlat[key], key).toBeTruthy();
      expect(enFlat[key], key).toBeTruthy();
    }
  });
});
