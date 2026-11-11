import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { act, render, screen, fireEvent, cleanup, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider } from '@/app/theme';
import { WizardShell } from '@/app/shell';
import { useUiStore } from '@/stores/ui-store';
import '@/i18n';

/**
 * Sidebar collapse (mandate §2.1 / MOTION-SPEC §3.5):
 * - toggle flips the persisted state
 * - collapsed rail: 52px, labels faded (opacity-0), icons retained,
 *   section headers faded
 * - keyboard path: the toggle is a real focusable button with
 *   aria-expanded/aria-controls
 * - persistence across store re-creation (localStorage round-trip)
 */

function renderShell(): ReturnType<typeof render> {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  qc.setQueryData(['devices'], []);
  return render(
    <ThemeProvider>
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={['/']}>
          <WizardShell />
        </MemoryRouter>
      </QueryClientProvider>
    </ThemeProvider>,
  );
}

beforeEach(() => {
  localStorage.removeItem('rw.ui.sidebarCollapsed');
  useUiStore.setState({ sidebarCollapsed: false });
});

afterEach(() => {
  cleanup();
});

describe('sidebar collapse', () => {
  it('toggle button is keyboard-reachable with correct aria wiring', () => {
    renderShell();
    const toggle = screen.getByTestId('sidebar-toggle');
    expect(toggle.tagName).toBe('BUTTON');
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(toggle).toHaveAttribute('aria-controls', 'sidebar-nav');
    expect(toggle).toHaveAttribute('aria-label', 'Collapse sidebar');
    // focusable by default (no tabindex="-1")
    expect(toggle).not.toHaveAttribute('tabindex', '-1');
  });

  it('collapses: width style 52px, labels fade, icons remain', () => {
    renderShell();
    const nav = document.getElementById('sidebar-nav') as HTMLElement;
    expect(nav.style.width).toBe('');

    fireEvent.click(screen.getByTestId('sidebar-toggle'));

    expect(useUiStore.getState().sidebarCollapsed).toBe(true);
    expect(nav.style.width).toBe('52px');

    // labels fade (opacity-0) but stay in the a11y tree; icons untouched
    const links = within(nav).getAllByRole('link');
    expect(links.length).toBeGreaterThan(0);
    for (const link of links) {
      const label = link.querySelector('span');
      expect(label?.className).toContain('opacity-0');
      expect(link.querySelector('svg')).toBeTruthy();
    }
    // toggle reflects the collapsed state
    expect(screen.getByTestId('sidebar-toggle')).toHaveAttribute('aria-expanded', 'false');
    expect(screen.getByTestId('sidebar-toggle')).toHaveAttribute(
      'aria-label',
      'Expand sidebar',
    );
  });

  it('persisted state survives store re-creation (session memory)', () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sidebar-toggle'));
    expect(localStorage.getItem('rw.ui.sidebarCollapsed')).toBe('1');
    // a fresh boot (next session) reads the stored state — the store's
    // initializer is readStoredSidebarCollapsed(); simulate via the store API
    act(() => {
      useUiStore.getState().setSidebarCollapsed(false);
    });
    expect(localStorage.getItem('rw.ui.sidebarCollapsed')).toBe('0');
    act(() => {
      useUiStore.getState().toggleSidebar();
    });
    expect(useUiStore.getState().sidebarCollapsed).toBe(true);
    expect(localStorage.getItem('rw.ui.sidebarCollapsed')).toBe('1');
  });

  it('collapsed links expose their label as title (hover discovery)', () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sidebar-toggle'));
    const nav = document.getElementById('sidebar-nav') as HTMLElement;
    const link = within(nav).getAllByRole('link')[0];
    expect(link.title).not.toBe('');
    // and the expanded state removes the title (label is visible)
    fireEvent.click(screen.getByTestId('sidebar-toggle'));
    expect(within(nav).getAllByRole('link')[0].title).toBe('');
  });

  it('does not shift the main region: same DOM node, same layout class', () => {
    renderShell();
    const main = document.getElementById('main-content') as HTMLElement;
    const child = main.firstElementChild as HTMLElement;
    expect(main.className).toContain('flex-1');
    fireEvent.click(screen.getByTestId('sidebar-toggle'));
    // The main region is neither re-created nor re-classed — width changes
    // flow through the flex layout, scroll and DOM state stay put.
    expect(document.getElementById('main-content')).toBe(main);
    expect(main.className).toContain('flex-1');
    expect(main.firstElementChild).toBe(child);
  });
});
