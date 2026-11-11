import * as React from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTranslation } from 'react-i18next';
import { isTauriRuntime } from '@/lib/runtime';
import { useTheme } from '@/app/theme';

import minimizeNorL from '@/assets/interim/chrome/light/minimize-nor.svg';
import minimizeHovL from '@/assets/interim/chrome/light/minimize-hov.svg';
import minimizePreL from '@/assets/interim/chrome/light/minimize-pre.svg';
import maximizeNorL from '@/assets/interim/chrome/light/maximize-nor.svg';
import maximizeHovL from '@/assets/interim/chrome/light/maximize-hov.svg';
import maximizePreL from '@/assets/interim/chrome/light/maximize-pre.svg';
import restoreNorL from '@/assets/interim/chrome/light/restore-nor.svg';
import restoreHovL from '@/assets/interim/chrome/light/restore-hov.svg';
import restorePreL from '@/assets/interim/chrome/light/restore-pre.svg';
import closeNorL from '@/assets/interim/chrome/light/close-nor.svg';
import closeHovL from '@/assets/interim/chrome/light/close-hov.svg';
import closePreL from '@/assets/interim/chrome/light/close-pre.svg';

import minimizeNorD from '@/assets/interim/chrome/dark/minimize-nor.svg';
import minimizeHovD from '@/assets/interim/chrome/dark/minimize-hov.svg';
import minimizePreD from '@/assets/interim/chrome/dark/minimize-pre.svg';
import maximizeNorD from '@/assets/interim/chrome/dark/maximize-nor.svg';
import maximizeHovD from '@/assets/interim/chrome/dark/maximize-hov.svg';
import maximizePreD from '@/assets/interim/chrome/dark/maximize-pre.svg';
import restoreNorD from '@/assets/interim/chrome/dark/restore-nor.svg';
import restoreHovD from '@/assets/interim/chrome/dark/restore-hov.svg';
import restorePreD from '@/assets/interim/chrome/dark/restore-pre.svg';
import closeNorD from '@/assets/interim/chrome/dark/close-nor.svg';
import closeHovD from '@/assets/interim/chrome/dark/close-hov.svg';
import closePreD from '@/assets/interim/chrome/dark/close-pre.svg';

/**
 * WindowControls — the custom window chrome cluster (mandate §2.1;
 * EXPERIENCE-SPEC §1, corpus ATL35 §2).
 *
 * Corpus facts implemented here:
 * - Order: minimize → maximize/restore → close (Windows convention; the
 *   corpus census places min ~x959, max ~x994, close ~x1015).
 * - 24×24 glyphs; hover adds the rounded-4 plate (`#D3DCE7` light /
 *   `#323D4C` dark); hov glyph brand blue, pre near-black — the 4-state
 *   SVGs are the corpus's own (interim assets, manifest under
 *   src/assets/interim/chrome/).
 * - State swaps are instant (Qt QSS has no transitions — ART21 §3).
 * - The maximize glyph ↔ restore glyph follows the live window state.
 *
 * Browser context: the cluster is NOT rendered (the host provides window
 * controls; inert imitations would be dead buttons). Tests mock the tauri
 * window API; the runtime gate is `isTauriRuntime()`.
 */

type ChromeState = 'nor' | 'hov' | 'pre';
type GlyphSet = readonly [string, string, string];

const LIGHT: Record<'minimize' | 'maximize' | 'restore' | 'close', GlyphSet> = {
  minimize: [minimizeNorL, minimizeHovL, minimizePreL],
  maximize: [maximizeNorL, maximizeHovL, maximizePreL],
  restore: [restoreNorL, restoreHovL, restorePreL],
  close: [closeNorL, closeHovL, closePreL],
};
const DARK: Record<'minimize' | 'maximize' | 'restore' | 'close', GlyphSet> = {
  minimize: [minimizeNorD, minimizeHovD, minimizePreD],
  maximize: [maximizeNorD, maximizeHovD, maximizePreD],
  restore: [restoreNorD, restoreHovD, restorePreD],
  close: [closeNorD, closeHovD, closePreD],
};

function useGlyphState(): {
  state: ChromeState;
  buttonProps: {
    onMouseEnter: () => void;
    onMouseLeave: () => void;
    onPointerDown: () => void;
    onPointerUp: () => void;
  };
} {
  const [state, setState] = React.useState<ChromeState>('nor');
  const buttonProps = React.useMemo(
    () => ({
      onMouseEnter: () => setState('hov'),
      onMouseLeave: () => setState('nor'),
      onPointerDown: () => setState('pre'),
      onPointerUp: () => setState('hov'),
    }),
    [],
  );
  return { state, buttonProps };
}

function ChromeButton({
  glyphs,
  label,
  onClick,
}: {
  glyphs: GlyphSet;
  label: string;
  onClick: () => void;
}): React.JSX.Element {
  const { state, buttonProps } = useGlyphState();
  // Corpus hover plate + state glyph arrive baked into the per-state SVG
  // (interim assets). `draggable=false` keeps pointer drags off the image.
  const src = state === 'hov' ? glyphs[1] : state === 'pre' ? glyphs[2] : glyphs[0];
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className="flex size-8 items-center justify-center rounded-xs outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1"
      {...buttonProps}
    >
      <img src={src} alt="" width={24} height={24} draggable={false} />
    </button>
  );
}

function WindowControls(): React.JSX.Element | null {
  const { t } = useTranslation();
  const { resolved } = useTheme();
  const [maximized, setMaximized] = React.useState(false);

  // Live maximize state: initial query + resize-event follow (debounced —
  // interactive resize fires rapidly and every isMaximized() is an IPC hop).
  React.useEffect(() => {
    if (!isTauriRuntime()) return;
    const win = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const sync = (): void => {
      void win.isMaximized().then((v) => {
        if (!disposed) setMaximized(v);
      });
    };
    sync();
    void win
      .onResized(() => {
        clearTimeout(timer);
        timer = setTimeout(sync, 120);
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => {
      disposed = true;
      clearTimeout(timer);
      unlisten?.();
    };
  }, []);

  if (!isTauriRuntime()) return null;

  const set = resolved === 'dark' ? DARK : LIGHT;

  return (
    <div className="flex items-center gap-0.5 pl-1" data-testid="window-controls">
      <ChromeButton
        glyphs={set.minimize}
        label={t('chrome.minimize')}
        onClick={() => void getCurrentWindow().minimize()}
      />
      <ChromeButton
        glyphs={maximized ? set.restore : set.maximize}
        label={maximized ? t('chrome.restore') : t('chrome.maximize')}
        onClick={() => void getCurrentWindow().toggleMaximize()}
      />
      <ChromeButton
        glyphs={set.close}
        label={t('chrome.close')}
        onClick={() => void getCurrentWindow().close()}
      />
    </div>
  );
}

export { WindowControls };
