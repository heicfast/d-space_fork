import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider } from '@/app/theme';
import { WizardShell } from '@/app/shell';
import '@/i18n';

/**
 * Route transitions (MOTION-SPEC §3.4):
 * - the incoming page fades in at --dur-fast (opacity only — zero CLS by
 *   construction; the corpus QStackedWidget swaps instantly, the fade is
 *   the documented premium refinement)
 * - keyed by pathname: navigating remounts the surface; query-string changes
 *   on the same path do not
 */

function renderShellAt(path: string): ReturnType<typeof render> {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <ThemeProvider>
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={[path]}>
          <WizardShell />
        </MemoryRouter>
      </QueryClientProvider>
    </ThemeProvider>,
  );
}

beforeEach(() => {
  localStorage.removeItem('rw.ui.sidebarCollapsed');
});

afterEach(() => {
  cleanup();
});

describe('route fade', () => {
  it('wraps the routed surface in the fade container', () => {
    renderShellAt('/');
    const main = document.getElementById('main-content') as HTMLElement;
    const fadeWrap = main.firstElementChild as HTMLElement;
    expect(fadeWrap.className).toContain('route-fade');
    expect(fadeWrap.className).toContain('h-full');
  });

  it('remounts the fade wrapper on pathname change', () => {
    renderShellAt('/');
    const main = document.getElementById('main-content') as HTMLElement;
    const first = main.firstElementChild as HTMLElement;

    // Navigating to SD Card remounts the faded surface.
    fireEvent.click(screen.getByRole('link', { name: /sd card/i }));
    const afterSd = document.getElementById('main-content')?.firstElementChild as HTMLElement;
    expect(afterSd).not.toBe(first);

    // Navigating to Disk Health remounts the faded surface again.
    fireEvent.click(screen.getAllByRole('link', { name: /disk health/i })[0]);
    const afterPath = document.getElementById('main-content')?.firstElementChild as HTMLElement;
    expect(afterPath).not.toBe(afterSd);
    expect(afterPath.className).toContain('route-fade');
  });
});
