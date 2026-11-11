import { describe, expect, it } from 'vitest';
import { analyze } from '../scripts/i18n-sweep.mjs';

/**
 * T12 governance: the i18n surface must be exactly the referenced surface.
 *  - missing → a component renders a raw key at runtime (a real bug);
 *  - unused  → dead surface (provisional strings are not shipped — wire the
 *    feature or drop the key via the sweep CLI report).
 * Plural pairs (X / X_other) count as one entry.
 */
describe('i18n surface (T12)', () => {
  const { unused, missing } = analyze();

  it('references only keys that exist (no raw-key renders)', () => {
    expect(missing).toEqual([]);
  });

  it('ships no unused keys (dead surface purge)', () => {
    expect(unused).toEqual([]);
  });
});
