import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { useTheme, type ThemeMode } from '@/app/theme';
import { useAppInfo, useDriverStatus } from '@/queries/system';
import { exportLogText, getLogBuffer } from '@/lib/logger';
import { setLocale, SUPPORTED_LOCALES, type AppLocale } from '@/i18n';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Segmented } from '@/components/ui/stepper';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/menus';
import { Table, TableBody, TableCell, TableRow } from '@/components/ui/table';
import { toast } from 'sonner';
import { PaletteIcon } from 'lucide-react';
import { AboutDialog } from './about-dialog';

/** Picker order: app modes wrapping the adopted palettes (ADR-007). */
const THEME_MODES: readonly ThemeMode[] = [
  'system',
  'light',
  'dark',
  'ember',
  'tide',
  'blossom',
  'high-contrast',
];

const THEME_LABEL_KEY: Record<ThemeMode, string> = {
  system: 'settings.themeSystem',
  light: 'settings.themeLight',
  dark: 'settings.themeDark',
  ember: 'settings.themeEmber',
  tide: 'settings.themeTide',
  blossom: 'settings.themeBlossom',
  'high-contrast': 'settings.themeHc',
};

const SCALE_KEY = 'rw.ui.scale';
const SCALES = [1, 1.5, 2] as const;

function readStoredScale(): number {
  try {
    const v = Number(localStorage.getItem(SCALE_KEY));
    if (v === 1 || v === 1.5 || v === 2) return v;
  } catch {
    /* storage unavailable */
  }
  return 1;
}

function isAppLocale(value: string): value is AppLocale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(value);
}

export function SettingsPage(): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const { mode, setMode, density, setDensity } = useTheme();
  const appInfo = useAppInfo();
  const driver = useDriverStatus();
  const locale = isAppLocale(i18n.language) ? i18n.language : 'en';
  const [scale, setScaleState] = React.useState<number>(() => readStoredScale());
  const [aboutOpen, setAboutOpen] = React.useState(false);

  const setScale = (next: number): void => {
    setScaleState(next);
    document.documentElement.style.zoom = next === 1 ? '' : String(next);
    try {
      localStorage.setItem(SCALE_KEY, String(next));
    } catch {
      /* storage unavailable */
    }
  };

  React.useEffect(() => {
    // Re-apply a persisted non-default scale on mount.
    const stored = readStoredScale();
    if (stored !== 1) document.documentElement.style.zoom = String(stored);
  }, []);

  return (
    <div className="mx-auto w-full max-w-3xl overflow-y-auto p-6">
      <h1 className="mb-6 text-xl font-semibold">{t('settings.title')}</h1>

      <section className="mb-8" aria-labelledby="settings-appearance">
        <h2 id="settings-appearance" className="mb-3 text-base font-semibold">
          {t('settings.appearance')}
        </h2>
        <div className="space-y-4 rounded-lg border border-border bg-surface p-4">
          <div className="flex items-center justify-between gap-4">
            <Label>{t('settings.theme')}</Label>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="secondary" size="sm" className="gap-2">
                  <PaletteIcon className="size-4" aria-hidden />
                  {t(THEME_LABEL_KEY[mode])}
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {THEME_MODES.map((m) => (
                  <React.Fragment key={m}>
                    {m === 'high-contrast' ? <DropdownMenuSeparator /> : null}
                    <DropdownMenuItem onClick={() => setMode(m)} aria-checked={mode === m}>
                      {t(THEME_LABEL_KEY[m])}
                    </DropdownMenuItem>
                  </React.Fragment>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          <div className="flex items-center justify-between gap-4">
            <Label>{t('settings.scale')}</Label>
            <Segmented
              ariaLabel={t('settings.scale')}
              value={String(scale)}
              onChange={(v) => setScale(Number(v))}
              options={SCALES.map((s) => ({ value: String(s), label: `${s * 100}%` }))}
            />
          </div>
          <div className="flex items-center justify-between gap-4">
            <Label>{t('settings.density')}</Label>
            <Segmented
              ariaLabel={t('settings.density')}
              value={density}
              onChange={(v) => setDensity(v)}
              options={[
                { value: 'comfortable', label: t('settings.densityComfortable') },
                { value: 'compact', label: t('settings.densityCompact') },
              ]}
            />
          </div>
          <div className="flex items-center justify-between gap-4">
            <Label>{t('settings.language')}</Label>
            <Segmented
              ariaLabel={t('settings.language')}
              value={locale}
              onChange={(v) => setLocale(v)}
              options={[
                { value: 'en', label: 'English' },
                { value: 'zh-CN', label: '简体中文' },
              ]}
            />
          </div>
        </div>
      </section>

      <section className="mb-8" aria-labelledby="settings-diagnostics">
        <h2 id="settings-diagnostics" className="mb-3 text-base font-semibold">
          {t('settings.diagnostics')}
        </h2>
        <div className="rounded-lg border border-border bg-surface p-4">
          <p className="mb-2 text-xs text-foreground-secondary">
            {t('settings.diagnosticsIntro', { count: getLogBuffer().length })}
          </p>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => {
              void navigator.clipboard
                ?.writeText(exportLogText())
                .then(() => toast.success(t('common.copied')))
                .catch(() => toast.error(t('errors.unknown')));
            }}
          >
            {t('settings.copyLog')}
          </Button>
        </div>
      </section>

      <section aria-labelledby="settings-about">
        <div className="mb-3 flex items-center justify-between">
          <h2 id="settings-about" className="text-base font-semibold">
            {t('settings.aboutTitle')}
          </h2>
          <Button variant="secondary" size="sm" onClick={() => setAboutOpen(true)}>
            {t('nav.about')}
          </Button>
        </div>
        <div className="overflow-hidden rounded-lg border border-border bg-surface shadow-card">
          <Table>
            <TableBody>
              <TableRow>
                <TableCell className="w-40 text-foreground-secondary">
                  {t('settings.version')}
                </TableCell>
                <TableCell className="tabular">{appInfo.data?.version ?? '—'}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-foreground-secondary">
                  {t('settings.connectorMode')}
                </TableCell>
                <TableCell>
                  <Badge
                    variant={appInfo.data?.connectorMode === 'mock' ? 'warning' : 'success'}
                  >
                    {appInfo.data?.connectorMode === 'mock'
                      ? t('settings.connectorMock')
                      : t('settings.connectorTauri')}
                  </Badge>
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-foreground-secondary">
                  {t('settings.engineStatus')}
                </TableCell>
                <TableCell className="tabular">
                  {driver.data
                    ? `${driver.data.installed ? '✓' : '✕'} ${driver.data.version ?? '—'}${driver.data.running ? '' : ' (idle)'}`
                    : '—'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-foreground-secondary">
                  {t('settings.licenses')}
                </TableCell>
                <TableCell className="text-sm text-foreground-secondary">
                  {t('settings.licensesBody')}
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </div>
      </section>
      <AboutDialog open={aboutOpen} onOpenChange={setAboutOpen} />
    </div>
  );
}
