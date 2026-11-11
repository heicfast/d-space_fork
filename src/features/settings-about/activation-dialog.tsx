import * as React from 'react';
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { BrandBriefcaseLogo } from '@/components/ui/brand-icons';
import { toast } from 'sonner';

/**
 * Activate EaseUS Data Recovery Wizard dialog (reference:
 * landing-activate-input-focus_7f4cce64.png) — centered briefcase-plus emblem,
 * floating "Paste" tooltip bubble above the license code input, "Activate" CTA,
 * and golden-orange "Buy a License Now" footer action.
 */
export function ActivationDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}): React.JSX.Element {
  const { t } = useTranslation();
  const [licenseKey, setLicenseKey] = React.useState('');
  const [inputFocused, setInputFocused] = React.useState(true);
  const [statusMessage, setStatusMessage] = React.useState<string | null>(null);
  const inputRef = React.useRef<HTMLInputElement | null>(null);

  const handleOpenChange = (next: boolean): void => {
    if (!next) {
      setLicenseKey('');
      setStatusMessage(null);
      setInputFocused(true);
    }
    onOpenChange(next);
  };

  const handlePaste = async (): Promise<void> => {
    try {
      if (navigator.clipboard?.readText) {
        const clip = await navigator.clipboard.readText();
        if (clip.trim()) {
          setLicenseKey(clip.trim().toUpperCase());
          inputRef.current?.focus();
          return;
        }
      }
    } catch {
      // Clipboard API blocked in headless/iframe context — populate sample format
    }
    setLicenseKey('DRW26-7K9M2-P4X8L-9Q2W5-R8T3N');
    inputRef.current?.focus();
  };

  const handleActivate = (): void => {
    if (!licenseKey.trim()) {
      inputRef.current?.focus();
      return;
    }
    setStatusMessage(t('activation.providerUnavailable'));
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        className="max-w-[490px] rounded-[12px] border border-[#E2E8F0] bg-white p-8 text-center shadow-2xl dark:border-border dark:bg-surface-elevated"
        aria-label={t('activation.title')}
      >
        <DialogHeader className="items-center text-center">
          <div className="mb-2 flex justify-center">
            <BrandBriefcaseLogo className="size-14 drop-shadow-sm" />
          </div>
          <DialogTitle className="text-[18px] font-bold tracking-tight text-[#1E293B] dark:text-foreground">
            {t('activation.title')}
          </DialogTitle>
          <DialogDescription className="mt-1 text-[13px] text-[#64748B] dark:text-foreground-secondary">
            {t('activation.description')}
          </DialogDescription>
        </DialogHeader>

        <div className="mt-7 text-left">
          <div className="relative flex items-center gap-2.5">
            {inputFocused ? (
              <button
                type="button"
                onClick={() => void handlePaste()}
                className="absolute -top-9 left-16 z-10 inline-flex items-center rounded-[4px] bg-[#1677FF] px-3 py-1 text-[12px] font-semibold text-white shadow-sm transition-all hover:bg-[#0958D9] dark:bg-primary dark:text-primary-foreground dark:hover:bg-primary-hover"
              >
                {t('activation.paste')}
                <span
                  className="absolute -bottom-1 left-1/2 size-2 -translate-x-1/2 rotate-45 bg-[#1677FF] dark:bg-primary"
                  aria-hidden
                />
              </button>
            ) : null}

            <input
              ref={inputRef}
              id="activation-key"
              type="text"
              autoFocus
              aria-label={t('activation.keyLabel')}
              value={licenseKey}
              onFocus={() => setInputFocused(true)}
              onChange={(e) => {
                setLicenseKey(e.target.value.toUpperCase());
                setStatusMessage(null);
              }}
              placeholder={t('activation.placeholder')}
              className="h-10 min-w-0 flex-1 rounded-[6px] border border-[#CBD5E1] bg-white px-3.5 text-[13px] text-[#1E293B] outline-none transition-colors placeholder:text-[#94A3B8] focus:border-[#1677FF] focus:ring-2 focus:ring-[#1677FF]/15 dark:border-border dark:bg-surface dark:text-foreground dark:placeholder:text-foreground-muted dark:focus:border-primary"
            />
            <button
              type="button"
              onClick={handleActivate}
              className="h-10 shrink-0 rounded-[6px] bg-[#1677FF] px-5 text-[13px] font-semibold text-white shadow-2xs transition-colors hover:bg-[#0958D9] focus-visible:ring-2 focus-visible:ring-ring dark:bg-primary dark:text-primary-foreground dark:hover:bg-primary-hover"
            >
              {t('activation.activate')}
            </button>
          </div>

          <p
            id="activation-note"
            className="mt-2.5 text-[12px] text-[#64748B] dark:text-foreground-secondary"
          >
            {t('activation.note')}
          </p>

          {statusMessage ? (
            <p
              id="activation-error"
              role="alert"
              className="mt-3 rounded-[6px] border border-[#FDE68A] bg-[#FFFBEB] px-3 py-2 text-[12px] text-[#B45309] dark:border-warning/40 dark:bg-warning-muted dark:text-warning"
            >
              {statusMessage}
            </p>
          ) : null}
        </div>

        <div className="mt-8 flex flex-col items-center">
          <p className="text-[13px] font-semibold text-[#475569] dark:text-foreground-secondary">
            {t('activation.noLicense')}
          </p>
          <button
            type="button"
            onClick={() => toast.info(t('activation.buyToast'))}
            className="mt-3 inline-flex h-10 min-w-[208px] items-center justify-center rounded-[6px] bg-gradient-to-r from-[#FFCC33] via-[#FFB31A] to-[#FF9900] px-7 text-[13px] font-bold text-[#0F172A] shadow-xs transition-all hover:from-[#FFB800] hover:via-[#FFA000] hover:to-[#E68A00] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring"
          >
            {t('activation.buyNow')}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
