import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { useTheme, type ThemeMode } from '@/app/theme';
import { useAppInfo, useDriverStatus } from '@/queries/system';
import { useAddRule, useDeleteRule, useRules, useToggleRule } from '@/queries/cleanup';
import { exportLogText, getLogBuffer } from '@/lib/logger';
import { setLocale, SUPPORTED_LOCALES, type AppLocale } from '@/i18n';
import { cn } from '@/lib/utils';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Segmented } from '@/components/ui/stepper';
import { Switch } from '@/components/ui/switch';
import { Hint } from '@/components/ui/tooltip';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/menus';
import { Table, TableBody, TableCell, TableRow } from '@/components/ui/table';
import { toast } from 'sonner';
import { FolderIcon, InfoIcon, PaletteIcon, TagIcon, Trash2Icon } from 'lucide-react';
import type { CleanupRule } from '@/connector/contract';
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

      <section className="mb-8" aria-labelledby="settings-cleanup-rules">
        <h2 id="settings-cleanup-rules" className="mb-3 text-base font-semibold">
          {t('rulesSection.title')}
        </h2>
        <div className="space-y-4 rounded-lg border border-border bg-surface p-4">
          <p className="text-xs text-foreground-secondary">{t('rulesSection.intro')}</p>
          <CleanupRulesSection />
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

/** Cleanup-score tiers shown by the preview pill and on every rule row. */
type RuleTier = 'danger' | 'caution' | 'safe' | 'cache';

/** Tier pill washes per the cleanup score legend (hexes from the spec). */
const RULE_TIER_PILL: Record<RuleTier, string> = {
  danger: 'bg-[#DC2626]/10 text-[#DC2626] dark:bg-[#DC2626]/25 dark:text-[#FCA5A5]',
  caution: 'bg-[#D97706]/10 text-[#D97706] dark:bg-[#D97706]/25 dark:text-[#FCD34D]',
  safe: 'bg-[#16A34A]/10 text-[#16A34A] dark:bg-[#16A34A]/25 dark:text-[#86EFAC]',
  cache: 'bg-[#1677FF]/10 text-[#1677FF] dark:bg-[#1677FF]/20 dark:text-[#93C5FD]',
};

/** Tier boundaries from rulesSection.scoreHint: 0–30 / 31–60 / 61–85 / 86–100. */
function tierForScore(score: number): RuleTier {
  if (score <= 30) return 'danger';
  if (score <= 60) return 'caution';
  if (score <= 85) return 'safe';
  return 'cache';
}

function RuleTierPill({ score }: { score: number }): React.JSX.Element {
  const { t } = useTranslation();
  const tier = tierForScore(score);
  // Tier label via ternary literals INSIDE t() — a Record indirection would
  // hide the four rulesSection.tier* keys from the T12 i18n sweep.
  const label = t(
    tier === 'danger'
      ? 'rulesSection.tierDanger'
      : tier === 'caution'
        ? 'rulesSection.tierCaution'
        : tier === 'safe'
          ? 'rulesSection.tierSafe'
          : 'rulesSection.tierCache',
  );
  return (
    <span
      className={cn(
        'inline-flex h-7 shrink-0 items-center rounded-full px-2.5 text-2xs font-semibold leading-none',
        RULE_TIER_PILL[tier],
      )}
    >
      {label}
    </span>
  );
}

/** Add-rule form + user rules list (runs before built-in scoring on scans). */
function CleanupRulesSection(): React.JSX.Element {
  const { t } = useTranslation();
  const rules = useRules();
  const addRule = useAddRule();
  const toggleRule = useToggleRule();
  const deleteRule = useDeleteRule();

  const [pattern, setPattern] = React.useState('');
  const [patternType, setPatternType] = React.useState<'name' | 'extension'>('name');
  const [score, setScore] = React.useState('70');
  const [explanation, setExplanation] = React.useState('');

  const scoreValue = Number(score);
  const patternValid = pattern.trim().length > 0;
  const scoreValid =
    score.trim() !== '' && Number.isFinite(scoreValue) && scoreValue >= 0 && scoreValue <= 100;
  const canAdd = patternValid && scoreValid && !addRule.isPending;

  const handleAdd = (): void => {
    if (!canAdd) return;
    addRule.mutate(
      {
        pattern: pattern.trim(),
        patternType,
        score: Math.round(scoreValue),
        explanation: explanation.trim(),
      },
      {
        onSuccess: () => {
          setPattern('');
          setExplanation('');
        },
        onError: (err) => toast.error(err instanceof Error ? err.message : t('errors.unknown')),
      },
    );
  };

  const ruleItems = rules.data ?? [];

  return (
    <div className="space-y-4">
      {/* Add-rule form */}
      <div className="space-y-3">
        <div className="flex flex-wrap items-end gap-3">
          <div className="min-w-48 flex-1">
            <Label htmlFor="rule-pattern">{t('rulesSection.pattern')}</Label>
            <Input
              id="rule-pattern"
              className="mt-1.5"
              value={pattern}
              autoComplete="off"
              placeholder={patternType === 'extension' ? '.tmp' : 'Thumbs'}
              onChange={(e) => setPattern(e.target.value)}
            />
          </div>
          <div>
            <Segmented
              ariaLabel={t('rulesSection.pattern')}
              value={patternType}
              onChange={(v) => setPatternType(v)}
              options={[
                { value: 'name', label: t('rulesSection.patternName') },
                { value: 'extension', label: t('rulesSection.patternExtension') },
              ]}
            />
          </div>
          <div className="w-24">
            <Label htmlFor="rule-score" className="gap-1">
              {t('rulesSection.score')}
              <Hint text={t('rulesSection.scoreHint')}>
                <span
                  className="inline-flex cursor-help items-center text-foreground-secondary"
                  aria-label={t('rulesSection.scoreHint')}
                >
                  <InfoIcon className="size-3.5" aria-hidden />
                </span>
              </Hint>
            </Label>
            <div className="mt-1.5 flex items-center gap-2">
              <Input
                id="rule-score"
                type="number"
                min={0}
                max={100}
                step={1}
                value={score}
                aria-invalid={!scoreValid}
                onChange={(e) => setScore(e.target.value)}
              />
              {scoreValid ? <RuleTierPill score={scoreValue} /> : null}
            </div>
          </div>
        </div>
        <div className="flex flex-wrap items-end gap-3">
          <div className="min-w-48 flex-1">
            <Label htmlFor="rule-explanation">{t('rulesSection.explanation')}</Label>
            <Input
              id="rule-explanation"
              className="mt-1.5"
              value={explanation}
              autoComplete="off"
              onChange={(e) => setExplanation(e.target.value)}
            />
          </div>
          <Button
            variant="primary"
            loading={addRule.isPending}
            disabled={!canAdd}
            onClick={handleAdd}
          >
            {t('rulesSection.add')}
          </Button>
        </div>
      </div>

      {/* Rules list */}
      {rules.isLoading ? (
        <p className="text-sm text-foreground-secondary">{t('common.loading')}</p>
      ) : rules.isError ? (
        <p className="text-sm text-danger">
          {rules.error instanceof Error ? rules.error.message : t('errors.unknown')}
        </p>
      ) : ruleItems.length === 0 ? (
        <p className="text-sm text-foreground-secondary">{t('rulesSection.empty')}</p>
      ) : (
        <ul className="space-y-2">
          {ruleItems.map((rule) => (
            <li key={rule.id}>
              <RuleRow
                rule={rule}
                onToggle={(enabled) =>
                  toggleRule.mutate(
                    { id: rule.id, enabled },
                    {
                      onError: (err) =>
                        toast.error(err instanceof Error ? err.message : t('errors.unknown')),
                    },
                  )
                }
                onDelete={() =>
                  deleteRule.mutate(rule.id, {
                    onError: (err) =>
                      toast.error(err instanceof Error ? err.message : t('errors.unknown')),
                  })
                }
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function RuleRow({
  rule,
  onToggle,
  onDelete,
}: {
  rule: CleanupRule;
  onToggle: (enabled: boolean) => void;
  onDelete: () => void;
}): React.JSX.Element {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-3 rounded-md border border-border bg-surface-muted px-3.5 py-2.5">
      {rule.patternType === 'extension' ? (
        <TagIcon className="size-4 shrink-0 text-foreground-secondary" aria-hidden />
      ) : (
        <FolderIcon className="size-4 shrink-0 text-foreground-secondary" aria-hidden />
      )}
      <span className="min-w-0 flex-1">
        <span className="block break-all font-mono text-[13px] font-medium text-foreground">
          {rule.pattern}
        </span>
        {rule.explanation ? (
          <span
            className="mt-0.5 block truncate text-xs text-foreground-secondary"
            title={rule.explanation}
          >
            {rule.explanation}
          </span>
        ) : null}
      </span>
      <span className="shrink-0 font-mono text-sm font-semibold tabular text-foreground">
        {rule.score}
      </span>
      <RuleTierPill score={rule.score} />
      <Switch
        checked={rule.enabled}
        onCheckedChange={(v) => onToggle(v === true)}
        aria-label={`${t('rulesSection.toggle')}: ${rule.pattern}`}
      />
      <Button
        variant="ghost"
        size="icon"
        aria-label={`${t('rulesSection.delete')}: ${rule.pattern}`}
        onClick={onDelete}
      >
        <Trash2Icon aria-hidden />
      </Button>
    </div>
  );
}
