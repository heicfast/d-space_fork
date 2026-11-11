import * as React from 'react';
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import '@/i18n';

/**
 * Dialog choreography (EXPERIENCE-SPEC §3 / MOTION-SPEC §3.2):
 * - enter: fade-in on data-[state=open] (overlay + content)
 * - exit: fade-out on data-[state=closed] (Radix keeps the node mounted
 *   until the animation ends — the class must be present)
 * - corpus rule 10: outside clicks are swallowed by every modal
 * - ESC still dismisses dismissible dialogs (explicit-control parity)
 */

function ProbeDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}): React.JSX.Element {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-label="Probe">
        <DialogTitle>Probe dialog</DialogTitle>
      </DialogContent>
    </Dialog>
  );
}

afterEach(() => {
  cleanup();
});

describe('Dialog choreography', () => {
  it('binds the enter AND exit animations on overlay and content', async () => {
    const onOpenChange = vi.fn();
    const { rerender } = render(<ProbeDialog open={false} onOpenChange={onOpenChange} />);
    rerender(<ProbeDialog open onOpenChange={onOpenChange} />);

    const content = await screen.findByRole('dialog');
    expect(content.className).toContain('data-[state=open]:animate-fade-in');
    expect(content.className).toContain('data-[state=closed]:animate-fade-out');

    const overlay = document.querySelector('[data-slot="dialog-overlay"]') as HTMLElement;
    expect(overlay.className).toContain('data-[state=closed]:animate-fade-out');

    rerender(<ProbeDialog open={false} onOpenChange={onOpenChange} />);
    // The content stays mounted with the exit class until the animation
    // completes (Radix Presence).
    expect(content.getAttribute('data-state')).toBe('closed');
    expect(content.className).toContain('data-[state=closed]:animate-fade-out');
  });

  it('swallows outside clicks (corpus atlas §12 rule 10 — no click-through close)', async () => {
    const onOpenChange = vi.fn();
    const { rerender } = render(<ProbeDialog open={false} onOpenChange={onOpenChange} />);
    rerender(<ProbeDialog open onOpenChange={onOpenChange} />);
    await screen.findByRole('dialog');

    // Radix fires interact-outside for pointerdown on the overlay.
    const overlay = document.querySelector('[data-slot="dialog-overlay"]') as HTMLElement;
    expect(overlay).toBeTruthy();
    fireEvent.pointerDown(overlay, { button: 0, clientX: 2, clientY: 2 });
    fireEvent.pointerUp(overlay, { button: 0, clientX: 2, clientY: 2 });
    fireEvent.click(overlay);

    // The dialog never asked to close — outside clicks were swallowed.
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    expect(screen.getByRole('dialog')).toBeTruthy();
  });

  it('ESC still dismisses a dismissible dialog', async () => {
    const onOpenChange = vi.fn();
    const { rerender } = render(<ProbeDialog open={false} onOpenChange={onOpenChange} />);
    rerender(<ProbeDialog open onOpenChange={onOpenChange} />);
    const content = await screen.findByRole('dialog');
    fireEvent.keyDown(content, { key: 'Escape' });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('returns focus to the invoking element on close (controlled dialogs)', async () => {
    const onOpenChange = vi.fn();
    const opener = document.createElement('button');
    opener.textContent = 'Open the probe';
    document.body.appendChild(opener);
    opener.focus();

    const { rerender } = render(<ProbeDialog open={false} onOpenChange={onOpenChange} />);
    rerender(<ProbeDialog open onOpenChange={onOpenChange} />);
    await screen.findByRole('dialog');

    rerender(<ProbeDialog open={false} onOpenChange={onOpenChange} />);
    // Radix restores focus in a post-unmount timeout.
    await vi.waitFor(() => {
      expect(document.activeElement).toBe(opener);
    });
    opener.remove();
  });
});
