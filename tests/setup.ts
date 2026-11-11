import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { afterEach, beforeAll, vi } from 'vitest';
import '@/i18n';

beforeAll(() => {
  // jsdom lacks matchMedia; the theme provider needs it.
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }),
  });
  // performance.now exists in jsdom; keep a deterministic-enough clock for the
  // mock connector (timers are real but short).
});

afterEach(() => {
  cleanup();
});
