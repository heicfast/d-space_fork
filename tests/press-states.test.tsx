import { describe, expect, it, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { Checkbox } from '@/components/ui/checkbox';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Segmented } from '@/components/ui/stepper';

/**
 * Microinteraction press states (MOTION-SPEC §3.7 / §2 — adopted press tokens
 * --press-scale 0.985 / --press-opacity 0.75):
 * - checkbox + radio: active press scales by the token; color transitions
 * - segmented options: active press dims by the token
 * - the corpus treatment (instant QSS swaps) remains the base: transitions
 *   stay at the micro duration
 */

afterEach(() => {
  cleanup();
});

describe('press microinteractions', () => {
  it('checkbox carries the press-scale treatment', () => {
    render(<Checkbox aria-label="pick" />);
    const box = screen.getByRole('checkbox', { name: 'pick' });
    expect(box.className).toContain('active:scale-[var(--press-scale)]');
    expect(box.className).toContain('transition-[colors,transform]');
    // state change still flips the data-state immediately (Qt-swap parity)
    fireEvent.click(box);
    expect(box.getAttribute('data-state')).toBe('checked');
  });

  it('radio item carries the press-scale treatment', () => {
    render(
      <RadioGroup defaultValue="a" aria-label="group">
        <RadioGroupItem value="a" aria-label="A" />
        <RadioGroupItem value="b" aria-label="B" />
      </RadioGroup>,
    );
    const radio = screen.getByRole('radio', { name: 'A' });
    expect(radio.className).toContain('active:scale-[var(--press-scale)]');
  });

  it('segmented options carry the press-opacity treatment', () => {
    render(
      <Segmented
        ariaLabel="view"
        value="details"
        onChange={() => undefined}
        options={[
          { value: 'details', label: 'Details' },
          { value: 'icons', label: 'Icons' },
        ]}
      />,
    );
    const opt = screen.getByRole('radio', { name: 'Details' });
    expect(opt.className).toContain('active:opacity-[var(--press-opacity)]');
    fireEvent.click(screen.getByRole('radio', { name: 'Icons' }));
  });
});
