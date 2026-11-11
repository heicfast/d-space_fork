import * as React from 'react';
import { useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';
import { RefreshCcwIcon } from 'lucide-react';
import { useDevices, useRefreshDevices } from '@/queries/devices';
import { useStartScan } from '@/queries/scan';
import { formatBytes } from '@/lib/format';
import { cn } from '@/lib/utils';
import { toast } from 'sonner';

/**
 * SD Card Recovery Page (references: Screenshot_2026-10-25_015201_2a9dc07f.png &
 * landing-sidebar-sdcard-page_bb0f35e7.png) —
 * header with inline circular refresh icon, selectable SD/MMC/MS PRO card row
 * ("Others  58 GB", "Disk2"), bottom rounded feature card with SDR / SSR / DVR
 * colorful icons + badges, and footer bar with "SD card connected but not
 * detected? Show all external devices or Get Help" on the left and
 * "Search for Lost Data" primary blue CTA button on the right.
 */
export function SdCardPage(): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const devicesQuery = useDevices();
  const refresh = useRefreshDevices();
  const startScan = useStartScan();
  const [showExternal, setShowExternal] = React.useState(false);
  const [selectedCardId, setSelectedCardId] = React.useState<string>('dev-sd');

  const devices = devicesQuery.data ?? [];
  const externalDevices = devices.filter((d) => d.kind === 'sd' || d.kind === 'usb');
  const sdDevice =
    externalDevices.find((d) => d.id === selectedCardId) ??
    externalDevices.find((d) => d.kind === 'sd') ??
    externalDevices[0] ??
    devices[0];

  const beginScan = (targetId?: string): void => {
    const resolvedId = targetId ?? sdDevice?.id ?? 'dev-sd';
    startScan.mutate(
      { targetId: resolvedId },
      {
        onSuccess: ({ scanId }) => {
          void navigate(`/scan/${resolvedId}/live?scanId=${scanId}`);
        },
        onError: (err) => toast.error(err.message),
      },
    );
  };

  return (
    <div className="flex h-full min-h-0 flex-col justify-between overflow-y-auto px-8 py-6">
      {/* Top Section: Header + Selectable SD Card Row */}
      <div>
        <div className="flex items-center gap-2.5">
          <h1 className="text-[20px] font-bold tracking-tight text-[#1E293B] dark:text-foreground">
            {t('sdCard.title')}
          </h1>
          <button
            type="button"
            onClick={() => void refresh.mutate()}
            disabled={refresh.isPending}
            aria-label={t('sdCard.refresh')}
            title={t('sdCard.refresh')}
            className="inline-flex size-7 shrink-0 items-center justify-center rounded-full text-[#1E293B] transition-colors hover:bg-[#E2E8F0]/70 hover:text-[#1677FF] focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 dark:text-foreground dark:hover:bg-surface-hover"
          >
            <RefreshCcwIcon
              className={cn('size-4', refresh.isPending && 'animate-spin')}
              aria-hidden
            />
          </button>
        </div>
        <p className="mt-1.5 max-w-4xl text-[13px] leading-relaxed text-[#64748B] dark:text-foreground-secondary">
          {t('sdCard.subtitle')}
        </p>

        {/* Connected SD/MMC/MS PRO Card Row (matching Screenshot_2026-10-25_015201_2a9dc07f.png) */}
        <div className="mt-5 space-y-2.5">
          <div
            role="button"
            tabIndex={0}
            onClick={() => {
              if (sdDevice) setSelectedCardId(sdDevice.id);
            }}
            onDoubleClick={() => beginScan(sdDevice?.id)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                beginScan(sdDevice?.id);
              }
            }}
            className="group flex w-full cursor-pointer items-center justify-between rounded-[10px] border-2 border-[#1677FF] bg-[#EDF4FF] px-5 py-3.5 transition-all hover:bg-[#E4EFFE] focus-visible:ring-2 focus-visible:ring-ring dark:border-primary dark:bg-surface-selected"
          >
            <div className="flex items-center gap-4">
              <SdCardSlotIcon />
              <div>
                <div className="text-[14px] font-bold text-[#1E293B] dark:text-foreground">
                  {t('sdCard.cardName')}
                </div>
                <div className="mt-0.5 flex items-center gap-4 text-[12.5px] text-[#64748B] tabular dark:text-foreground-secondary">
                  <span>{t('sdCard.cardPartition')}</span>
                  <span>{t('sdCard.cardSize')}</span>
                </div>
              </div>
            </div>

            <div className="flex items-center gap-4">
              <span className="text-[13px] font-medium text-[#64748B] dark:text-foreground-secondary">
                {t('sdCard.cardDiskNo')}
              </span>
            </div>
          </div>

          {/* Optional expanded external devices list when user clicks "Show all external devices" */}
          {showExternal && externalDevices.length > 0 ? (
            <div className="rounded-[10px] border border-[#DCE8FF] bg-[#F8FAFD] p-3.5 dark:border-border dark:bg-surface-elevated">
              <div className="mb-2 text-[12px] font-semibold text-[#475569] dark:text-foreground-secondary">
                {t('home.removableGroup')} ({externalDevices.length})
              </div>
              <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
                {externalDevices.map((dev) => (
                  <div
                    key={dev.id}
                    className="flex items-center justify-between gap-3 rounded-[8px] border border-[#E2E8F0] bg-white px-3.5 py-2.5 dark:border-border dark:bg-surface"
                  >
                    <div className="min-w-0">
                      <div className="truncate text-[13px] font-semibold text-[#1E293B] dark:text-foreground">
                        {dev.label}
                      </div>
                      <div className="text-[11px] text-[#64748B] tabular dark:text-foreground-secondary">
                        {dev.model} · {formatBytes(dev.capacity.totalBytes, i18n.language)}
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={() => beginScan(dev.id)}
                      className="shrink-0 rounded-[6px] bg-[#1677FF] px-3 py-1.5 text-[12px] font-semibold text-white transition-colors hover:bg-[#0958D9] dark:bg-primary dark:text-primary-foreground dark:hover:bg-primary-hover"
                    >
                      {t('sdCard.start')}
                    </button>
                  </div>
                ))}
              </div>
            </div>
          ) : null}
        </div>
      </div>

      {/* Bottom Section: Rounded SDR/SSR/DVR Feature Card + Footer Bar (Screenshot_2026-10-25_015201_2a9dc07f.png) */}
      <div className="mt-6 space-y-4">
        <div className="rounded-[12px] border border-[#E5EEFA] bg-[#F8FAFF] px-7 py-5 shadow-2xs dark:border-border dark:bg-surface-elevated">
          <div className="grid grid-cols-1 gap-6 sm:grid-cols-3">
            <FeatureBadgeItem
              badge="SDR"
              title={t('sdCard.sdrTitle')}
              body={t('sdCard.sdrBody')}
            />
            <FeatureBadgeItem
              badge="SSR"
              title={t('sdCard.ssrTitle')}
              body={t('sdCard.ssrBody')}
            />
            <FeatureBadgeItem
              badge="DVR"
              title={t('sdCard.dvrTitle')}
              body={t('sdCard.dvrBody')}
            />
          </div>
        </div>

        {/* Footer Bar: Left detection links + Right "Search for Lost Data" primary blue CTA */}
        <div className="flex flex-wrap items-center justify-between gap-4 pt-1">
          <div className="text-[13px] text-[#64748B] dark:text-foreground-secondary">
            <span>{t('sdCard.bannerPrefix')} </span>
            <button
              type="button"
              onClick={() => setShowExternal((v) => !v)}
              className="font-medium text-[#1677FF] underline decoration-[#1677FF]/60 underline-offset-2 hover:text-[#0958D9] dark:text-primary dark:hover:text-primary-hover"
            >
              {t('sdCard.showExternal')}
            </button>
            <span> {t('sdCard.bannerOr')} </span>
            <button
              type="button"
              onClick={() => toast.info(t('sdCard.helpToast'))}
              className="font-medium text-[#1677FF] underline decoration-[#1677FF]/60 underline-offset-2 hover:text-[#0958D9] dark:text-primary dark:hover:text-primary-hover"
            >
              {t('sdCard.getHelp')}
            </button>
          </div>

          <button
            type="button"
            disabled={startScan.isPending}
            onClick={() => beginScan()}
            className="inline-flex h-10 min-w-[200px] items-center justify-center rounded-[8px] bg-[#1677FF] px-6 text-[14px] font-semibold text-white shadow-xs transition-all hover:bg-[#0958D9] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 dark:bg-primary dark:text-primary-foreground dark:hover:bg-primary-hover"
          >
            {t('home.scanAction')}
          </button>
        </div>
      </div>
    </div>
  );
}

function SdCardSlotIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 40 44" fill="none" className="size-10 shrink-0" aria-hidden>
      <path
        d="M6 2H26L34 10V38C34 40.2 32.2 42 30 42H6C3.8 42 2 40.2 2 38V6C2 3.8 3.8 2 6 2Z"
        fill="url(#sd-card-blue)"
      />
      <rect x="7" y="6" width="3" height="7" rx="1" fill="#FBBF24" />
      <rect x="12" y="6" width="3" height="7" rx="1" fill="#FBBF24" />
      <rect x="17" y="6" width="3" height="7" rx="1" fill="#FBBF24" />
      <rect x="22" y="6" width="3" height="7" rx="1" fill="#FBBF24" />
      <rect x="7" y="25" width="20" height="11" rx="2" fill="white" fillOpacity="0.22" />
      <defs>
        <linearGradient
          id="sd-card-blue"
          x1="2"
          y1="2"
          x2="34"
          y2="42"
          gradientUnits="userSpaceOnUse"
        >
          <stop stopColor="#3B82F6" />
          <stop offset="1" stopColor="#1D4ED8" />
        </linearGradient>
      </defs>
    </svg>
  );
}

function FeatureBadgeItem({
  badge,
  title,
  body,
}: {
  badge: 'SDR' | 'SSR' | 'DVR';
  title: string;
  body: string;
}): React.JSX.Element {
  return (
    <div className="flex items-start gap-3.5">
      <div className="relative flex size-11 shrink-0 items-center justify-center" aria-hidden>
        {badge === 'SDR' ? (
          <svg viewBox="0 0 44 44" fill="none" className="size-11">
            <rect
              x="6"
              y="4"
              width="24"
              height="32"
              rx="4"
              fill="#DBEAFE"
              stroke="#60A5FA"
              strokeWidth="1.5"
            />
            <rect x="10" y="8" width="3" height="5" rx="0.8" fill="#3B82F6" />
            <rect x="15" y="8" width="3" height="5" rx="0.8" fill="#3B82F6" />
            <rect x="20" y="8" width="3" height="5" rx="0.8" fill="#3B82F6" />
            <circle cx="28" cy="26" r="7" fill="#10B981" />
            <path
              d="M25 26L27 28L31.5 23.5"
              stroke="white"
              strokeWidth="1.8"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        ) : badge === 'SSR' ? (
          <svg viewBox="0 0 44 44" fill="none" className="size-11">
            <rect
              x="4"
              y="6"
              width="30"
              height="26"
              rx="4"
              fill="#DCFCE7"
              stroke="#34D399"
              strokeWidth="1.5"
            />
            <circle cx="13" cy="14" r="2.5" fill="#10B981" />
            <path d="M7 28L15 19L21 25L26 20L31 28H7Z" fill="#10B981" fillOpacity="0.65" />
          </svg>
        ) : (
          <svg viewBox="0 0 44 44" fill="none" className="size-11">
            <rect
              x="4"
              y="6"
              width="30"
              height="26"
              rx="4"
              fill="#EDE9FE"
              stroke="#A78BFA"
              strokeWidth="1.5"
            />
            <circle cx="19" cy="19" r="7" fill="#F97316" />
            <path d="M17.5 15.5L22.5 19L17.5 22.5V15.5Z" fill="white" />
          </svg>
        )}
        <span className="absolute -bottom-0.5 right-0 rounded-[4px] bg-[#64748B] px-1.5 py-0.5 text-[9px] font-bold tracking-wider text-white shadow-2xs">
          {badge}
        </span>
      </div>
      <div className="min-w-0">
        <div className="text-[13.5px] font-bold text-[#1E293B] dark:text-foreground">
          {title}
        </div>
        <div className="mt-0.5 text-[12px] text-[#64748B] dark:text-foreground-secondary">
          {body}
        </div>
      </div>
    </div>
  );
}
