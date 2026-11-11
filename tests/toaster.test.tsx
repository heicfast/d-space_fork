import { describe, expect, it, afterEach } from 'vitest';
import { render, screen, cleanup, act } from '@testing-library/react';
import { ThemeProvider } from '@/app/theme';
import { Toaster } from '@/components/ui/toaster';
import { toast } from 'sonner';
import '@/i18n';

/**
 * Toast choreography (MOTION-SPEC §3.3 / EXPERIENCE-SPEC §4):
 * - the surface renders top-right below the custom title bar (offset top
 *   48px = 36px title bar + 12px gap), clear of the bottom-right Recover CTA
 * - toasts announce politely (aria-live region, sonner default)
 * - hover pauses dismissal (sonner built-in), 4 s auto-dismiss
 */

afterEach(() => {
  cleanup();
});

describe('Toaster', () => {
  it('mounts the stack top-right below the title bar with polite semantics', async () => {
    render(
      <ThemeProvider>
        <Toaster />
      </ThemeProvider>,
    );
    // Sonner mounts its container lazily — publish a toast first. The
    // awaited microtask flush lets its promise-based render settle.
    await act(async () => {
      toast.success('Session saved');
      await Promise.resolve();
    });
    expect(await screen.findByText('Session saved')).toBeTruthy();

    const holder = document.querySelector('[data-sonner-toaster]') as HTMLElement;
    expect(holder).toBeTruthy();
    const styles = getComputedStyle(holder);
    expect(styles.position).toBe('fixed');
    // Corpus placement: top-right below the custom title bar. Sonner anchors
    // via --offset-* custom properties on the inline style (jsdom does not
    // resolve var() in computed styles, so read the properties directly).
    expect(holder.style.getPropertyValue('--offset-top')).toBe('48px');
    expect(holder.style.getPropertyValue('--offset-right')).toBe('16px');
    // Polite announcement region (not assertive).
    const live = document.querySelector('[aria-live]');
    expect(live?.getAttribute('aria-live')).toBe('polite');
  });
});
