import * as React from 'react';
import * as DialogPrimitive from '@radix-ui/react-dialog';
import { XIcon } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';

const Dialog = DialogPrimitive.Root;
const DialogPortal = DialogPrimitive.Portal;

function DialogOverlay({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Overlay>): React.JSX.Element {
  return (
    <DialogPrimitive.Overlay
      data-slot="dialog-overlay"
      className={cn(
        'fixed inset-0 z-50 bg-[var(--overlay-scrim)]',
        'data-[state=open]:animate-fade-in data-[state=closed]:animate-fade-out',
        className,
      )}
      {...props}
    />
  );
}

/**
 * The app's modal surface. Three corpus rules are baked in as defaults
 * (EXPERIENCE-SPEC §3):
 * - outside clicks are SWALLOWED — the reference's modal family dims the
 *   main window and blocks click-through (atlas §12 rule 10; the
 *   stop-dialog capture shows the dialog persisting after a body click);
 *   dismissal is ESC or an explicit control only
 * - enter/exit choreography: fade-in on open, fade-out on close
 *   (MOTION-SPEC §3.2 — Radix keeps the tree mounted until the exit
 *   animation ends)
 * - focus choreography: focus moves into the dialog on open and RETURNS to
 *   the invoking element on close. Radix's own default returns focus to
 *   `triggerRef.current`, which is null for controlled dialogs that open
 *   without `DialogTrigger` — so we capture the invoker ourselves at
 *   open-auto-focus time (before Radix moves focus inside) and restore it
 *   at close-auto-focus time.
 */
function DialogContent({
  className,
  children,
  showCloseButton = true,
  onInteractOutside,
  onOpenAutoFocus,
  onCloseAutoFocus,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Content> & {
  showCloseButton?: boolean;
}): React.JSX.Element {
  const { t } = useTranslation();
  const contentRef = React.useRef<HTMLDivElement | null>(null);
  const invokerRef = React.useRef<HTMLElement | null>(null);

  const handleOpenAutoFocus = React.useCallback(
    (event: Event) => {
      // Capture the invoking element while focus is still outside the
      // content (Radix moves focus inside right after this event). The
      // containment guard keeps the capture StrictMode-safe: on a second
      // effect pass focus is already inside and must not overwrite it.
      const active = document.activeElement;
      if (
        active instanceof HTMLElement &&
        contentRef.current &&
        !contentRef.current.contains(active)
      ) {
        invokerRef.current = active;
      }
      onOpenAutoFocus?.(event);
    },
    [onOpenAutoFocus],
  );

  const handleCloseAutoFocus = React.useCallback(
    (event: Event) => {
      // Prevent Radix's default (focus the DialogTrigger we do not use).
      event.preventDefault();
      invokerRef.current?.focus({ preventScroll: true });
      onCloseAutoFocus?.(event);
    },
    [onCloseAutoFocus],
  );

  return (
    <DialogPortal>
      <DialogOverlay />
      <DialogPrimitive.Content
        ref={contentRef}
        data-slot="dialog-content"
        onInteractOutside={(e) => {
          // Corpus default: modals swallow outside clicks.
          e.preventDefault();
          onInteractOutside?.(e);
        }}
        onOpenAutoFocus={handleOpenAutoFocus}
        onCloseAutoFocus={handleCloseAutoFocus}
        className={cn(
          'fixed left-1/2 top-1/2 z-50 grid w-full max-w-lg -translate-x-1/2 -translate-y-1/2 gap-4',
          'rounded-lg border border-border bg-surface p-6 shadow-dialog',
          'data-[state=open]:animate-fade-in data-[state=closed]:animate-fade-out',
          className,
        )}
        {...props}
      >
        {children}
        {showCloseButton ? (
          <DialogPrimitive.Close
            className="absolute right-4 top-4 rounded-xs p-1 text-foreground-secondary transition-colors hover:bg-surface-hover hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
            aria-label={t('common.close')}
          >
            <XIcon className="size-4" aria-hidden />
          </DialogPrimitive.Close>
        ) : null}
      </DialogPrimitive.Content>
    </DialogPortal>
  );
}

function DialogHeader({ className, ...props }: React.ComponentProps<'div'>): React.JSX.Element {
  return <div className={cn('flex flex-col gap-1.5 text-left', className)} {...props} />;
}

function DialogFooter({ className, ...props }: React.ComponentProps<'div'>): React.JSX.Element {
  return (
    <div
      className={cn('flex flex-col-reverse gap-2 sm:flex-row sm:justify-end', className)}
      {...props}
    />
  );
}

function DialogTitle({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Title>): React.JSX.Element {
  return (
    <DialogPrimitive.Title
      className={cn('text-lg font-semibold leading-none', className)}
      {...props}
    />
  );
}

function DialogDescription({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Description>): React.JSX.Element {
  return (
    <DialogPrimitive.Description
      className={cn('text-sm text-foreground-secondary', className)}
      {...props}
    />
  );
}

export {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
};
