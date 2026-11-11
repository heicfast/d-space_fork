import * as React from 'react';
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  HelpCircleIcon,
  KeyRoundIcon,
  LanguagesIcon,
  MenuIcon,
  MoonIcon,
  PanelLeftCloseIcon,
  PanelLeftOpenIcon,
  RefreshCcwIcon,
  SettingsIcon,
  ShoppingCartIcon,
  SunIcon,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { useTheme } from './theme';
import { useScanSessionStore } from '@/stores/scan-session-store';
import { useUiStore } from '@/stores/ui-store';
import { DevPanel, PerfOverlay } from '@/features/dev-panel/dev-panel';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/menus';
import { AboutDialog } from '@/features/settings-about/about-dialog';
import { ActivationDialog } from '@/features/settings-about/activation-dialog';
import { setLocale, SUPPORTED_LOCALES } from '@/i18n';
import { WindowControls } from '@/components/ui/window-controls';
import {
  BrandBriefcaseLogo,
  SidebarChronoSnapIcon,
  SidebarFileRepairIcon,
  SidebarHardwareDiskIcon,
  SidebarLostPartitionIcon,
  SidebarMoreToolsIcon,
  SidebarSdCardIcon,
} from '@/components/ui/brand-icons';
import { isTauriRuntime } from '@/lib/runtime';

/**
 * App shell (reference: landing-baseline_53c354a9.png) — #EBF2FF ice-blue
 * outer window frame, custom title bar with briefcase-plus emblem, robot
 * mascot helper popover ("Hi! Guess you need:"), orange Buy Now pill, key
 * activation modal trigger, 3-group left sidebar (Data Recovery, Advanced
 * Features, File Backup), and white rounded-tl-2xl content card.
 */

const NAV_DATA_RECOVERY = [
  {
    to: '/',
    icon: SidebarHardwareDiskIcon,
    labelKey: 'sidebar.hardwareDisk',
    ariaLabel: 'Hardware Disk (Internal Drives)',
    end: true,
  },
  {
    to: '/sd-card',
    icon: SidebarSdCardIcon,
    labelKey: 'sidebar.sdCard',
    end: false,
  },
  {
    to: '/health',
    icon: SidebarMoreToolsIcon,
    labelKey: 'sidebar.diskHealth',
    end: false,
  },
] as const;

const NAV_ADVANCED = [
  {
    to: '/repair',
    icon: SidebarFileRepairIcon,
    labelKey: 'sidebar.fileRepair',
    end: false,
  },
  {
    to: '/lost-partition',
    icon: SidebarLostPartitionIcon,
    labelKey: 'sidebar.lostPartition',
    end: false,
  },
] as const;

const NAV_BACKUP = [
  {
    to: '/backup',
    icon: SidebarChronoSnapIcon,
    labelKey: 'sidebar.chronoSnap',
    end: false,
  },
] as const;

export function WizardShell(): React.JSX.Element {
  const { t } = useTranslation();
  const { resolved, setMode } = useTheme();
  const active = useScanSessionStore((s) => s.active);
  const navigate = useNavigate();
  const location = useLocation();
  const [aboutOpen, setAboutOpen] = React.useState(false);
  const [activationOpen, setActivationOpen] = React.useState(false);
  const sidebarCollapsed = useUiStore((s) => s.sidebarCollapsed);
  const toggleSidebar = useUiStore((s) => s.toggleSidebar);
  const mainRef = React.useRef<HTMLElement | null>(null);

  React.useEffect(() => {
    mainRef.current?.focus({ preventScroll: true });
  }, [location.pathname]);

  const inExplorerFlow =
    location.pathname.startsWith('/scan/') || location.pathname.startsWith('/results');

  const inResultsFlow = inExplorerFlow || location.pathname.startsWith('/recover');

  const onTitleBarDoubleClick = (e: React.MouseEvent<HTMLElement>): void => {
    if (!isTauriRuntime()) return;
    if ((e.target as HTMLElement).closest('button, a, input, [role="menu"]')) return;
    void getCurrentWindow().toggleMaximize();
  };

  return (
    <div className="flex h-full min-h-0 flex-col bg-[#EBF2FF] text-foreground dark:bg-background">
      {/* Top Window Chrome */}
      <header
        className={cn(
          'flex shrink-0 items-center justify-between bg-[#EBF2FF] px-3.5 select-none dark:border-border dark:bg-surface',
          inExplorerFlow && 'border-b border-[#D8E5FA]',
        )}
        style={{ height: 'var(--titlebar-height)' }}
        data-tauri-drag-region
        onDoubleClick={onTitleBarDoubleClick}
      >
        <div className="flex items-center gap-2">
          <BrandBriefcaseLogo className="size-5" />
          <span className="text-[12.5px] font-semibold tracking-tight text-[#1E293B] dark:text-foreground">
            {t('app.name')}
          </span>
        </div>

        <div className="flex items-center gap-1.5">
          {/* Orange Buy Now Pill / Cart Button */}
          <button
            type="button"
            onClick={() => setActivationOpen(true)}
            className="inline-flex h-6 items-center gap-1 rounded-full bg-gradient-to-r from-[#FF7A2F] to-[#FF4D00] px-2.5 text-[11px] font-semibold text-white shadow-2xs transition-all hover:from-[#F26518] hover:to-[#E03E00] active:scale-[var(--press-scale)] focus-visible:ring-2 focus-visible:ring-ring"
          >
            <ShoppingCartIcon className="size-3" aria-hidden />
            <span>{t('activation.buyPill')}</span>
          </button>

          {/* Key Icon — Opens Activate EaseUS Data Recovery Wizard Modal */}
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('activation.open')}
            title={t('activation.open')}
            data-testid="header-activate-trigger"
            className="size-7 text-[#475569] hover:bg-[#DCE8FF] hover:text-[#0F172A] dark:text-foreground-secondary dark:hover:bg-surface-hover dark:hover:text-foreground"
            onClick={() => setActivationOpen(true)}
          >
            <KeyRoundIcon className="size-3.5" aria-hidden />
          </Button>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                aria-label={t('settings.language')}
                className="size-7 text-[#475569] hover:bg-[#DCE8FF] hover:text-[#0F172A] dark:text-foreground-secondary dark:hover:bg-surface-hover dark:hover:text-foreground"
              >
                <LanguagesIcon className="size-3.5" aria-hidden />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {SUPPORTED_LOCALES.map((loc) => (
                <DropdownMenuItem key={loc} onClick={() => setLocale(loc)}>
                  {loc === 'en' ? 'English' : '简体中文'}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                aria-label={t('menu.label')}
                className="size-7 text-[#475569] hover:bg-[#DCE8FF] hover:text-[#0F172A] dark:text-foreground-secondary dark:hover:bg-surface-hover dark:hover:text-foreground"
              >
                <MenuIcon className="size-3.5" aria-hidden />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={() => navigate('/settings')}>
                <SettingsIcon className="size-3.5" aria-hidden />
                <span>{t('nav.settings')}</span>
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setMode(resolved === 'dark' ? 'light' : 'dark')}>
                {resolved === 'dark' ? (
                  <SunIcon className="size-3.5" aria-hidden />
                ) : (
                  <MoonIcon className="size-3.5" aria-hidden />
                )}
                <span>
                  {t('settings.theme')}:{' '}
                  {resolved === 'dark' ? t('settings.themeLight') : t('settings.themeDark')}
                </span>
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={() => setAboutOpen(true)}>
                <HelpCircleIcon className="size-3.5" aria-hidden />
                <span>{t('nav.about')}</span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>

          <WindowControls />
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        {/* Left Sidebar — matches reference 3-section layout on Home/Sidebar pages */}
        {!inExplorerFlow ? (
          <nav
            id="sidebar-nav"
            aria-label={t('a11y.sidebarRegion')}
            style={{ width: sidebarCollapsed ? 52 : undefined }}
            className={cn(
              'flex shrink-0 flex-col overflow-x-hidden overflow-y-auto bg-[#EBF2FF] py-2 select-none dark:border-r dark:border-border dark:bg-surface-muted/60',
              !sidebarCollapsed && 'w-[var(--sidebar-width)]',
              'sidebar-width-transition',
            )}
          >
            {!inResultsFlow ? (
              <>
                <SidebarSection
                  label={t('sidebar.dataRecovery')}
                  collapsed={sidebarCollapsed}
                  isFirst
                />
                {NAV_DATA_RECOVERY.map((item) => (
                  <SidebarLink key={item.to} {...item} collapsed={sidebarCollapsed} />
                ))}

                <SidebarSection label={t('sidebar.advanced')} collapsed={sidebarCollapsed} />
                {NAV_ADVANCED.map((item) => (
                  <SidebarLink key={item.to} {...item} collapsed={sidebarCollapsed} />
                ))}

                <SidebarSection label={t('sidebar.fileBackup')} collapsed={sidebarCollapsed} />
                {NAV_BACKUP.map((item) => (
                  <SidebarLink key={item.to} {...item} collapsed={sidebarCollapsed} />
                ))}
              </>
            ) : (
              <>
                <SidebarSection
                  label={t('sidebar.dataRecovery')}
                  collapsed={sidebarCollapsed}
                  isFirst
                />
                <SidebarLink
                  to="/"
                  icon={SidebarHardwareDiskIcon}
                  labelKey="nav.backToDevices"
                  end
                  collapsed={sidebarCollapsed}
                />
                {active ? (
                  <button
                    type="button"
                    onClick={() => navigate(`/scan/${active.targetId}/live`)}
                    title={t('home.scanningHint')}
                    className={cn(
                      'mx-2 flex h-9 items-center rounded-[8px] pl-[9px] text-[12.5px] text-[#1677FF] transition-colors hover:bg-[#DCE8FF] dark:text-primary dark:hover:bg-surface-hover',
                      sidebarCollapsed ? 'gap-0 pr-[9px]' : 'gap-2 pr-2.5',
                    )}
                  >
                    <RefreshCcwIcon className="size-[18px] shrink-0" aria-hidden />
                    <span
                      className={cn(
                        'sidebar-label-fade truncate',
                        sidebarCollapsed && 'pointer-events-none w-0 overflow-hidden opacity-0',
                      )}
                    >
                      {t('home.scanningHint')}
                    </span>
                  </button>
                ) : null}
              </>
            )}

            <div className="mt-auto space-y-0.5 pt-2">
              <div
                className={cn(
                  'flex items-center pt-1',
                  sidebarCollapsed ? 'justify-center px-2' : 'justify-end px-2',
                )}
              >
                <button
                  type="button"
                  onClick={toggleSidebar}
                  aria-expanded={!sidebarCollapsed}
                  aria-controls="sidebar-nav"
                  aria-label={sidebarCollapsed ? t('sidebar.expand') : t('sidebar.collapse')}
                  title={sidebarCollapsed ? t('sidebar.expand') : t('sidebar.collapse')}
                  data-testid="sidebar-toggle"
                  className={cn(
                    'flex items-center justify-center rounded-[8px] text-[#64748B] outline-none transition-colors hover:bg-[#DCE8FF] hover:text-[#1E293B] focus-visible:ring-2 focus-visible:ring-ring dark:text-foreground-secondary dark:hover:bg-surface-hover dark:hover:text-foreground',
                    sidebarCollapsed ? 'size-9' : 'size-7',
                  )}
                >
                  {sidebarCollapsed ? (
                    <PanelLeftOpenIcon className="size-4" aria-hidden />
                  ) : (
                    <PanelLeftCloseIcon className="size-3.5" aria-hidden />
                  )}
                </button>
              </div>
            </div>
          </nav>
        ) : null}

        {/* Main Content Surface — rounded-tl-2xl white card inside #EBF2FF frame on sidebar pages */}
        <main
          ref={mainRef}
          id="main-content"
          tabIndex={-1}
          aria-label={t('a11y.mainRegion')}
          className={cn(
            'flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-white outline-none dark:bg-background',
            !inExplorerFlow &&
              'rounded-tl-[16px] shadow-[-2px_-2px_12px_rgba(22,119,255,0.04)]',
          )}
        >
          <div
            key={location.pathname}
            className="route-fade flex h-full min-h-0 flex-1 flex-col overflow-hidden"
          >
            <Outlet />
          </div>
        </main>
      </div>

      {import.meta.env.DEV ? <DevSurfaces /> : null}
      <AboutDialog open={aboutOpen} onOpenChange={setAboutOpen} />
      <ActivationDialog open={activationOpen} onOpenChange={setActivationOpen} />
    </div>
  );
}

function DevSurfaces(): React.JSX.Element {
  return (
    <>
      <DevPanel />
      <PerfOverlay />
    </>
  );
}

function SidebarSection({
  label,
  collapsed,
  isFirst = false,
}: {
  label: string;
  collapsed: boolean;
  isFirst?: boolean;
}): React.JSX.Element {
  return (
    <div
      className={cn(
        'relative overflow-hidden whitespace-nowrap text-[11px] font-medium text-[#7A8BA8] dark:text-foreground-disabled',
        'sidebar-label-fade',
        collapsed
          ? isFirst
            ? 'h-1 p-0 opacity-0'
            : 'mx-auto my-2 h-px w-6 bg-[#CBD9F0] p-0 text-transparent dark:bg-border'
          : 'px-4 pt-3.5 pb-1 opacity-100',
      )}
    >
      <span className={cn(collapsed && 'opacity-0')}>{label}</span>
    </div>
  );
}

function SidebarLink({
  to,
  icon: Icon,
  labelKey,
  ariaLabel,
  end,
  collapsed,
}: {
  to: string;
  icon: React.ComponentType<{ className?: string }>;
  labelKey: string;
  ariaLabel?: string;
  end?: boolean;
  collapsed: boolean;
}): React.JSX.Element {
  const { t } = useTranslation();
  return (
    <NavLink
      to={to}
      end={end}
      aria-label={ariaLabel}
      title={collapsed ? t(labelKey) : undefined}
      className={({ isActive }) =>
        cn(
          'relative mx-2 my-0.5 flex h-9 items-center rounded-[8px] pl-[9px] text-[12.5px] tracking-[-0.01em] outline-none transition-colors',
          collapsed ? 'gap-0 pr-[9px]' : 'gap-2.5 pr-2.5',
          'focus-visible:ring-2 focus-visible:ring-ring',
          isActive
            ? 'bg-[#DCE8FF] font-semibold text-[#1E293B] before:absolute before:top-2 before:bottom-2 before:left-0 before:w-[3.5px] before:rounded-full before:bg-[#1677FF] dark:bg-primary-muted dark:text-primary dark:before:bg-primary'
            : 'font-medium text-[#334155] hover:bg-[#DCE8FF]/70 hover:text-[#0F172A] dark:text-foreground-secondary dark:hover:bg-surface-hover dark:hover:text-foreground',
        )
      }
    >
      <Icon className="size-[18px] shrink-0" aria-hidden />
      <span
        className={cn(
          'sidebar-label-fade truncate',
          collapsed && 'pointer-events-none w-0 overflow-hidden opacity-0',
        )}
      >
        {t(labelKey)}
      </span>
    </NavLink>
  );
}
