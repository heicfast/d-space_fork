import { Toaster as Sonner } from 'sonner';
import { useTheme } from '@/app/theme';

/**
 * Toast surface bound to the token themes.
 *
 * Choreography (MOTION-SPEC §3.3): enter fade + slight rise, stack (gap 8),
 * auto-dismiss at 4 s, hover pauses the timer (sonner built-in), exit fade.
 * Positioned top-right BELOW the custom title bar (corpus VLM readings of
 * the results-phase toast; keeps the bottom-right clear of the persistent
 * Recover CTA — no overlap with persistent surfaces).
 */
function Toaster(): React.JSX.Element {
  const { resolved } = useTheme();
  return (
    <Sonner
      theme={resolved}
      position="top-right"
      offset={{ top: 48, right: 16, bottom: 16, left: 16 }}
      gap={8}
      duration={4000}
      toastOptions={{
        classNames: {
          toast:
            '!rounded-md !border !border-border !bg-surface !text-foreground !shadow-overlay !text-sm',
          description: '!text-foreground-secondary',
          actionButton: '!bg-primary !text-primary-foreground !rounded-xs',
          cancelButton:
            '!bg-surface-muted !text-foreground-secondary !rounded-xs !border !border-border',
        },
      }}
    />
  );
}

export { Toaster };
