import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { ConfidenceGrade } from '@/connector/contract';
import { ConfidencePillInline } from '@/components/ui/confidence-badge';

const GRADES: ConfidenceGrade[] = ['excellent', 'good', 'fair', 'poor', 'unknown'];

describe('ConfidencePillInline (compact confidence surface)', () => {
  it('renders the localized label for every grade', () => {
    for (const grade of GRADES) {
      const { unmount } = render(<ConfidencePillInline grade={grade} />);
      const expected = {
        excellent: 'Excellent',
        good: 'Good',
        fair: 'Fair',
        poor: 'Poor',
        unknown: 'Unknown',
      }[grade];
      expect(screen.getByText(expected)).toBeInTheDocument();
      unmount();
    }
  });

  it('never renders a bare icon — the label is always present (grid #14)', () => {
    const { container } = render(<ConfidencePillInline grade="good" />);
    const pill = container.firstElementChild as HTMLElement;
    expect(pill.textContent).toBe('Good');
    expect(pill.querySelector('svg')).not.toBeNull();
  });
});
