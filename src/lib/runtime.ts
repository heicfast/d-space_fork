/**
 * Runtime environment detection.
 *
 * The app runs in two contexts: the Tauri 2 shell (WebView2, desktop) and a
 * plain browser (vite dev/preview, Playwright smoke). Shell-only affordances
 * (window controls, native window behaviors) render ONLY in the Tauri
 * runtime — in a browser the host chrome already provides those, and
 * rendering inert imitations would be dead buttons.
 */

declare global {
  interface Window {
    readonly __TAURI_INTERNALS__?: unknown;
  }
}

/** True when running inside the Tauri 2 shell (WebView with internals). */
export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
