import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import {
  THEME_IDS,
  applyThemeAttributes,
  familyOf,
  validateThemeId,
  type ThemeId,
} from '@/theme/adopted/themes';

/**
 * App theme state, composed on the adopted two-layer theme system
 * (ADR-007): `data-theme="<palette>"` + `data-scheme="light|dark"` are
 * written by the adopted module's applyThemeAttributes(). The app adds:
 *  - 'system' — resolves to the canonical light/dark pair via matchMedia;
 *  - 'high-contrast' — a DERIVED accessibility layer (the adopted sources
 *    define no HC palette): keeps the light/dark family attributes and
 *    adds the `.hc` class, which the semantic token layer overrides.
 */
export type ThemeMode = 'system' | ThemeId | 'high-contrast';
export type Density = 'comfortable' | 'compact';

interface ThemeContextValue {
  mode: ThemeMode;
  /** The active palette (system resolved to light/dark). */
  themeId: ThemeId;
  /** Effective scheme family ('dark' when system resolves dark). */
  resolved: 'light' | 'dark';
  setMode: (mode: ThemeMode) => void;
  density: Density;
  setDensity: (density: Density) => void;
}

const STORAGE_KEY = 'rw.ui.theme';
const DENSITY_KEY = 'rw.ui.density';
const THEME_MODES: readonly string[] = ['system', 'high-contrast', ...THEME_IDS];

const ThemeContext = createContext<ThemeContextValue | null>(null);

function readStoredMode(): ThemeMode {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v != null && THEME_MODES.includes(v)) return v as ThemeMode;
  } catch {
    /* storage unavailable — fall through */
  }
  return 'system';
}

function readStoredDensity(): Density {
  try {
    const v = localStorage.getItem(DENSITY_KEY);
    if (v === 'comfortable' || v === 'compact') return v;
  } catch {
    /* storage unavailable */
  }
  return 'comfortable';
}

function systemPrefersDark(): boolean {
  return (
    typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches
  );
}

/** Resolve a mode to the concrete palette id + apply all root attributes. */
function applyTheme(
  mode: ThemeMode,
  systemDark: boolean,
): { themeId: ThemeId; resolved: 'light' | 'dark' } {
  const root = document.documentElement;
  root.classList.remove('hc');
  let themeId: ThemeId;
  if (mode === 'system') {
    themeId = systemDark ? 'dark' : 'light';
  } else if (mode === 'high-contrast') {
    themeId = systemDark ? 'dark' : 'light';
    root.classList.add('hc');
  } else {
    themeId = validateThemeId(mode);
  }
  applyThemeAttributes(themeId);
  // Track Windows forced-colors for diagnostics (the .hc layer harmonizes
  // with it; forced-colors itself overrides all author colors).
  if (typeof window !== 'undefined' && window.matchMedia('(forced-colors: active)').matches) {
    root.dataset.forcedColors = 'active';
  } else {
    delete root.dataset.forcedColors;
  }
  return { themeId, resolved: familyOf(themeId) };
}

export function ThemeProvider({ children }: { children: ReactNode }): React.JSX.Element {
  const [mode, setModeState] = useState<ThemeMode>(() => readStoredMode());
  const [density, setDensityState] = useState<Density>(() => readStoredDensity());
  const [systemDark, setSystemDark] = useState<boolean>(() => systemPrefersDark());

  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = (e: MediaQueryListEvent): void => setSystemDark(e.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);

  useEffect(() => {
    const fc = window.matchMedia('(forced-colors: active)');
    const onFc = () => {
      applyTheme(mode, systemDark);
    };
    fc.addEventListener('change', onFc);
    return () => fc.removeEventListener('change', onFc);
  }, [mode, systemDark]);

  const { themeId, resolved } = useMemo(
    () => applyTheme(mode, systemDark),
    // applyTheme touches the DOM; run per mode/system change only.
    [mode, systemDark],
  );

  useEffect(() => {
    document.documentElement.dataset.density = density;
  }, [density]);

  const setMode = useCallback((next: ThemeMode) => {
    setModeState(next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      /* storage unavailable */
    }
  }, []);

  const setDensity = useCallback((next: Density) => {
    setDensityState(next);
    try {
      localStorage.setItem(DENSITY_KEY, next);
    } catch {
      /* storage unavailable */
    }
  }, []);

  const value = useMemo<ThemeContextValue>(
    () => ({ mode, themeId, resolved, setMode, density, setDensity }),
    [mode, themeId, resolved, setMode, density, setDensity],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error('useTheme must be used within ThemeProvider');
  return ctx;
}
