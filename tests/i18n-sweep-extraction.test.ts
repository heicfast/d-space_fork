import { describe, expect, it } from 'vitest';
import { firstArgKeyLiterals, tCalls } from '../scripts/i18n-sweep.mjs';

/**
 * Regression pin for the T12 purge incident: the historical regex-only
 * extractor could not see ternary key literals inside t() calls that also
 * contain nested parens (e.g. t(cond ? 'a' : 'b', f(x))). Both keys looked
 * "unused", the one-shot prune deleted them from the catalogs, and the scan
 * status line rendered raw keys on Windows CI. The extractor must resolve
 * every key literal in the first argument, parens or not.
 */
describe('i18n sweep extraction (ternary blind-spot pin)', () => {
  const nested = `t(phase === 'quick' ? 'scan.quickScanning' : 'scan.advancedScanning', {
    percent: formatPercent(overall, i18n.language),
  })`;

  it('finds every t() call even with nested parens and multiline calls', () => {
    const calls = tCalls(
      `const a = ${nested};\nconst b = t('plain.key');\nconst c = formatDuration(x, y);`,
    );
    expect(calls).toHaveLength(2);
    expect(calls[0].trim().startsWith('phase')).toBe(true);
    expect(calls[1].trim()).toBe("'plain.key'");
  });

  it('extracts both ternary key literals from the first argument only', () => {
    expect(firstArgKeyLiterals(nested).sort()).toEqual([
      'scan.advancedScanning',
      'scan.quickScanning',
    ]);
  });

  it('does not treat the interpolation argument as key-bearing', () => {
    const call = `t('scan.quickScanning', {
      percent: formatPercent(overall, i18n.language),
    })`;
    expect(firstArgKeyLiterals(call)).toEqual(['scan.quickScanning']);
  });
});
