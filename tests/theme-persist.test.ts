import { beforeEach, describe, expect, it } from 'vitest';
import {
  THEME_IDS,
  currentDomTheme,
  nativeThemeFor,
  persistTheme,
  readSavedTheme,
} from '@/theme/adopted/themes';

describe('theme persistence helpers', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
  });

  it('persists and reads back the saved choice', () => {
    expect(readSavedTheme()).toBe('light');
    persistTheme('ember');
    expect(localStorage.getItem('diskgenie.theme')).toBe('ember');
    expect(readSavedTheme()).toBe('ember');
  });

  it('falls back to light for unknown or corrupt stored values', () => {
    localStorage.setItem('diskgenie.theme', 'not-a-theme');
    expect(readSavedTheme()).toBe('light');
    localStorage.setItem('diskgenie.theme', 'dark?');
    expect(readSavedTheme()).toBe('light');
  });

  it('currentDomTheme validates the pre-mounted root attribute', () => {
    expect(currentDomTheme()).toBe('light');
    document.documentElement.setAttribute('data-theme', 'tide');
    expect(currentDomTheme()).toBe('tide');
    document.documentElement.setAttribute('data-theme', 'bogus');
    expect(currentDomTheme()).toBe('light');
  });

  it('nativeThemeFor maps every theme onto the two native families', () => {
    for (const id of THEME_IDS) {
      const native = nativeThemeFor(id);
      expect(native === 'light' || native === 'dark').toBe(true);
    }
    expect(nativeThemeFor('dark')).toBe('dark');
    expect(nativeThemeFor('blossom')).toBe('light');
  });
});
