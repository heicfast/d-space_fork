/**
 * ADR-007 token-surface snapshot test.
 *
 * The adopted theme files (src/theme/adopted/tokens.css + themes.ts) are
 * verbatim copies of the two designated theme source files. This test
 * fails if the adopted token surface drifts — anyone editing the adopted
 * files (or a dependency reshaping them) must regenerate the fixture ONLY
 * after re-fetching and re-verifying against the upstream sources.
 *
 * Also validates that every adopted token referenced by the app's semantic
 * mapping layer (src/styles/tokens.css) actually exists in the adopted
 * file, so a rename upstream cannot silently produce var() dead-ends.
 */
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  THEME_IDS,
  THEME_LIST,
  THEMES,
  applyThemeAttributes,
  familyOf,
  validateThemeId,
} from '@/theme/adopted/themes';

const here = dirname(fileURLToPath(import.meta.url));

interface TokenBlock {
  selector: string;
  props: Record<string, string>;
}

function parseTokenBlocks(css: string): TokenBlock[] {
  const blocks: TokenBlock[] = [];
  const re = /([^{}]+)\{([^{}]*)\}/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(css)) !== null) {
    const sel = m[1]
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .trim()
      .replace(/\s+/g, ' ');
    if (!sel || sel.startsWith('@')) continue;
    const props: Record<string, string> = {};
    const pm = /--([a-z0-9-]+)\s*:\s*([^;]+);/g;
    let p: RegExpExecArray | null;
    while ((p = pm.exec(m[2])) !== null) props[`--${p[1]}`] = p[2].trim();
    if (Object.keys(props).length > 0) blocks.push({ selector: sel, props });
  }
  return blocks;
}

const adoptedCss = readFileSync(join(here, '../src/theme/adopted/tokens.css'), 'utf8');
const fixture = JSON.parse(
  readFileSync(join(here, 'fixtures/adopted-tokens.snapshot.json'), 'utf8'),
) as TokenBlock[];

describe('adopted theme token surface (ADR-007)', () => {
  it('tokens.css block/selector structure matches the snapshot', () => {
    const blocks = parseTokenBlocks(adoptedCss);
    expect(blocks.map((b) => b.selector)).toEqual(fixture.map((b) => b.selector));
  });

  it('every adopted token value matches the snapshot exactly', () => {
    const blocks = parseTokenBlocks(adoptedCss);
    expect(blocks).toEqual(fixture);
  });

  it('the canonical five palettes and families are intact', () => {
    expect(THEME_IDS).toEqual(['light', 'dark', 'ember', 'tide', 'blossom']);
    expect(Object.fromEntries(THEME_LIST.map((t) => [t.id, t.label]))).toEqual({
      light: 'Light',
      dark: 'Dark',
      ember: 'Ember',
      tide: 'Tide',
      blossom: 'Blossom',
    });
    expect(THEMES.ember.family).toBe('dark');
    expect(THEMES.tide.family).toBe('dark');
    expect(THEMES.blossom.family).toBe('light');
    expect(familyOf('dark')).toBe('dark');
  });

  it('unknown theme ids fall back to light', () => {
    expect(validateThemeId('violet-ai-gradient')).toBe('light');
    expect(validateThemeId('ember')).toBe('ember');
    expect(validateThemeId(null)).toBe('light');
  });

  it('applyThemeAttributes writes BOTH attributes together', () => {
    const root = document.documentElement;
    applyThemeAttributes('tide');
    expect(root.getAttribute('data-theme')).toBe('tide');
    expect(root.getAttribute('data-scheme')).toBe('dark');
    applyThemeAttributes('blossom');
    expect(root.getAttribute('data-theme')).toBe('blossom');
    expect(root.getAttribute('data-scheme')).toBe('light');
    root.removeAttribute('data-theme');
    root.removeAttribute('data-scheme');
  });

  it('the semantic layer references only tokens the adopted file defines', () => {
    const defined = new Set<string>();
    for (const block of parseTokenBlocks(adoptedCss)) {
      for (const name of Object.keys(block.props)) defined.add(name);
    }
    // App-semantic tokens defined by the mapping layer itself.
    const semantic = readFileSync(join(here, '../src/styles/tokens.css'), 'utf8');
    const semanticDefs = new Set<string>();
    for (const m of semantic.matchAll(/--([a-z0-9-]+)\s*:/g)) semanticDefs.add(`--${m[1]}`);
    const adoptedRefs = new Set<string>();
    for (const m of semantic.matchAll(/var\((--[a-z0-9-]+)\s*(?:,|\))/g)) {
      const name = m[1];
      if (semanticDefs.has(name) || name === '--focus-offset' || name === '--focus-w') continue;
      adoptedRefs.add(name);
    }
    expect(adoptedRefs.size).toBeGreaterThan(20);
    const missing = [...adoptedRefs].filter((name) => !defined.has(name));
    expect(missing).toEqual([]);
  });
});
