import * as React from 'react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { ThemeProvider } from '@/app/theme';
import { WindowControls } from '@/components/ui/window-controls';
// Side-effect import: initializes react-i18next with the app catalogs so
// aria-labels resolve to their English values.
import '@/i18n';

/**
 * WindowControls (mandate §2.1 / EXPERIENCE-SPEC §1):
 * - renders ONLY in the Tauri runtime (browser gets host chrome — no dead
 *   buttons, no lies)
 * - order: minimize → maximize/restore → close (corpus census)
 * - clicking dispatches the window API
 * - the max/restore glyph follows the live maximize state (onResized sync)
 * - state swap nor/hov/pre (corpus 4-state treatment; dis unused at runtime)
 */

const minimize = vi.fn();
const toggleMaximize = vi.fn();
const close = vi.fn();
const onResizedListeners: Array<() => void> = [];
let maximizedNow = false;

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    minimize,
    toggleMaximize,
    close,
    isMaximized: () => Promise.resolve(maximizedNow),
    onResized: (cb: () => void) => {
      onResizedListeners.push(cb);
      return Promise.resolve(() => undefined);
    },
  }),
}));

function withTheme(ui: React.ReactElement): React.ReactElement {
  return <ThemeProvider>{ui}</ThemeProvider>;
}

function setTauriRuntime(on: boolean): void {
  if (on) {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
  } else {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  }
}

beforeEach(() => {
  vi.clearAllMocks();
  onResizedListeners.length = 0;
  maximizedNow = false;
});

afterEach(() => {
  cleanup();
  setTauriRuntime(false);
});

describe('WindowControls', () => {
  it('renders nothing in a plain browser (host chrome provides controls)', () => {
    setTauriRuntime(false);
    const { container } = render(withTheme(<WindowControls />));
    expect(container.textContent).toBe('');
    expect(screen.queryByTestId('window-controls')).toBeNull();
  });

  it('renders the corpus order: minimize → maximize → close', async () => {
    setTauriRuntime(true);
    render(withTheme(<WindowControls />));
    const cluster = await screen.findByTestId('window-controls');
    const labels = cluster.querySelectorAll('button[aria-label]');
    expect(labels).toHaveLength(3);
    expect(labels[0]).toHaveAttribute('aria-label', 'Minimize');
    expect(labels[1]).toHaveAttribute('aria-label', 'Maximize');
    expect(labels[2]).toHaveAttribute('aria-label', 'Close');
  });

  it('dispatches the window API on click', async () => {
    setTauriRuntime(true);
    render(withTheme(<WindowControls />));
    fireEvent.click(await screen.findByRole('button', { name: 'Minimize' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    fireEvent.click(screen.getByRole('button', { name: 'Maximize' }));
    expect(minimize).toHaveBeenCalledTimes(1);
    expect(close).toHaveBeenCalledTimes(1);
    expect(toggleMaximize).toHaveBeenCalledTimes(1);
  });

  it('swaps maximize ↔ restore glyph and label with the live window state', async () => {
    setTauriRuntime(true);
    render(withTheme(<WindowControls />));
    // initial sync (isMaximized=false) → Maximize
    await screen.findByRole('button', { name: 'Maximize' });
    // simulate the window being maximized → label flips to Restore
    maximizedNow = true;
    onResizedListeners.forEach((cb) => cb());
    const restoreBtn = await screen.findByRole('button', { name: 'Restore' });
    expect(restoreBtn).toBeDefined();
    // Vite inlines SVGs as data URIs in tests; the corpus SVGs embed their
    // own <title> (e.g. btn_restore_nor) — assert on that identity.
    const img = restoreBtn.querySelector('img');
    expect(img?.getAttribute('src')).toContain('btn_restore_nor');
  });

  it('swaps glyph state on hover and press (corpus 4-state treatment)', async () => {
    setTauriRuntime(true);
    render(withTheme(<WindowControls />));
    const btn = await screen.findByRole('button', { name: 'Close' });
    const img = btn.querySelector('img') as HTMLImageElement;
    expect(img.getAttribute('src')).toContain('btn_close_nor');
    fireEvent.mouseEnter(btn);
    expect(img.getAttribute('src')).toContain('btn_close_hov');
    fireEvent.pointerDown(btn);
    expect(img.getAttribute('src')).toContain('btn_close_pre');
    fireEvent.pointerUp(btn);
    expect(img.getAttribute('src')).toContain('btn_close_hov');
    fireEvent.mouseLeave(btn);
    expect(img.getAttribute('src')).toContain('btn_close_nor');
  });
});
