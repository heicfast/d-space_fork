import * as React from 'react';
import { useTranslation } from 'react-i18next';
import type { FileCategoryId } from '@/connector/contract';
import { CalendarIcon, FilterXIcon, RulerIcon, TagIcon } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Checkbox } from '@/components/ui/checkbox';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/menus';
import { CategoryIcon } from '@/components/ui/file-icon';
import { useFilterStore } from '@/stores/filter-store';

/**
 * Filter bar — the corpus filter row (atlas §9): File Type / Date Modified /
 * File Size dropdowns (each with a Custom entry) + the "Existing" and
 * "Hide 0 KB files" toggles + Reset. All filters resolve into a connector
 * ResultsFilter (server-side in the real backend; typed-array scan in the
 * mock — never client-side over paged data).
 */

export function FilterBar(): React.JSX.Element {
  const { t } = useTranslation();
  const f = useFilterStore();

  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-border bg-surface-muted/60 px-3 py-1.5 text-sm">
      <FilterPopover icon={TagIcon} label={t('results.filterType')}>
        <CategoryPicker />
      </FilterPopover>

      <FilterPopover icon={CalendarIcon} label={t('results.filterDate')}>
        <DatePicker />
      </FilterPopover>

      <FilterPopover icon={RulerIcon} label={t('results.filterSize')}>
        <SizePicker />
      </FilterPopover>

      <label
        className="flex cursor-pointer select-none items-center gap-1.5 text-xs text-foreground-secondary"
        htmlFor="filter-existing"
      >
        <Checkbox
          id="filter-existing"
          checked={f.existingOnly}
          onCheckedChange={(v) => f.setExistingOnly(v === true)}
        />
        {t('results.filterExisting')}
      </label>

      <label
        className="flex cursor-pointer select-none items-center gap-1.5 text-xs text-foreground-secondary"
        htmlFor="filter-hide-empty"
      >
        <Checkbox
          id="filter-hide-empty"
          checked={f.hideEmptyFiles}
          onCheckedChange={(v) => f.setHideEmptyFiles(v === true)}
        />
        {t('results.filterHideEmpty')}
      </label>

      {f.hasActiveFilters() ? (
        <Button variant="ghost" size="sm" onClick={() => f.reset()} className="ml-auto">
          <FilterXIcon aria-hidden />
          {t('results.clearFilter')}
        </Button>
      ) : null}
    </div>
  );
}

function FilterPopover({
  icon: Icon,
  label,
  children,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  children: React.ReactNode;
}): React.JSX.Element {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" className="gap-1.5">
          <Icon className="size-4" aria-hidden />
          {label}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-64">{children}</PopoverContent>
    </Popover>
  );
}

const CATEGORIES: readonly FileCategoryId[] = [
  'graphics',
  'video',
  'audio',
  'document',
  'email',
  'archive',
  'bookmarks',
  'other',
];

function CategoryPicker(): React.JSX.Element {
  const { t } = useTranslation();
  const f = useFilterStore();
  const [customOpen, setCustomOpen] = React.useState(false);
  return (
    <div className="space-y-1" role="group" aria-label={t('results.filterType')}>
      {CATEGORIES.map((cat) => {
        const checked = f.categories.includes(cat);
        return (
          <label
            key={cat}
            className="flex cursor-pointer items-center gap-2.5 rounded-xs px-2 py-1.5 text-sm hover:bg-surface-hover"
          >
            <Checkbox checked={checked} onCheckedChange={() => f.toggleCategory(cat)} />
            <CategoryIcon category={cat} />
            <span className="flex-1">{t(`categories.${cat}`)}</span>
          </label>
        );
      })}
      {/* Corpus Custom entry: "Enter the file type suffix, such as *.doc; *.xlsx" (atlas §9) */}
      <div className="mt-2 border-t border-border pt-2">
        {customOpen || (f.extensions?.length ?? 0) > 0 ? (
          <div className="space-y-1.5 px-2 pb-1">
            <Label htmlFor="filter-ext-custom" className="text-2xs text-foreground-secondary">
              {t('results.typeCustomHint')}
            </Label>
            <div className="flex gap-2">
              <Input
                id="filter-ext-custom"
                className="h-7 font-mono text-xs"
                placeholder="*.doc; *.xlsx"
                value={f.extensionInput}
                onChange={(e) => f.setExtensionInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') f.commitExtensions();
                }}
              />
              <Button variant="secondary" size="sm" onClick={() => f.commitExtensions()}>
                {t('common.apply')}
              </Button>
            </div>
            {(f.extensions?.length ?? 0) > 0 ? (
              <p className="text-2xs text-primary">
                {t('results.typeCustomActive', { list: f.extensions!.join(', ') })}
              </p>
            ) : null}
          </div>
        ) : (
          <button
            type="button"
            className="w-full rounded-xs px-2 py-1.5 text-left text-sm text-primary hover:bg-surface-hover"
            onClick={() => setCustomOpen(true)}
          >
            {t('results.typeCustom')}
          </button>
        )}
      </div>
    </div>
  );
}

function DatePicker(): React.JSX.Element {
  const { t } = useTranslation();
  const f = useFilterStore();
  return (
    <div className="space-y-2">
      <RadioGroup
        value={f.datePreset}
        onValueChange={(v) => f.setDatePreset(v as typeof f.datePreset)}
        className="space-y-1"
      >
        {(['any', 'yesterday', 'week', 'month'] as const).map((preset) => (
          <label
            key={preset}
            className="flex cursor-pointer items-center gap-2 rounded-xs px-2 py-1 text-sm hover:bg-surface-hover"
          >
            <RadioGroupItem value={preset} />
            {t(
              `results.date${preset === 'any' ? 'Any' : preset === 'yesterday' ? 'Yesterday' : preset === 'week' ? 'Week' : 'Month'}`,
            )}
          </label>
        ))}
      </RadioGroup>
      <div className="rounded-sm border border-border p-2">
        <p className="mb-1.5 text-xs font-medium text-foreground-secondary">
          {t('results.dateCustom')}
        </p>
        <div className="grid grid-cols-2 gap-2">
          <div>
            <Label htmlFor="date-from" className="mb-1 text-2xs text-foreground-disabled">
              {t('results.dateFrom')}
            </Label>
            <Input
              id="date-from"
              type="date"
              value={f.dateFrom ?? ''}
              onChange={(e) => f.setDatePreset('custom', e.target.value || null, undefined)}
              className="h-7 text-xs"
            />
          </div>
          <div>
            <Label htmlFor="date-to" className="mb-1 text-2xs text-foreground-disabled">
              {t('results.dateTo')}
            </Label>
            <Input
              id="date-to"
              type="date"
              value={f.dateTo ?? ''}
              onChange={(e) => f.setDatePreset('custom', undefined, e.target.value || null)}
              className="h-7 text-xs"
            />
          </div>
        </div>
      </div>
    </div>
  );
}

function SizePicker(): React.JSX.Element {
  const { t } = useTranslation();
  const f = useFilterStore();
  return (
    <div className="space-y-2">
      <RadioGroup
        value={f.sizePreset}
        onValueChange={(v) => f.setSizePreset(v as typeof f.sizePreset)}
        className="space-y-1"
      >
        {(['any', 'tiny', 'small', 'medium', 'large', 'huge'] as const).map((preset) => (
          <label
            key={preset}
            className="flex cursor-pointer items-center gap-2 rounded-xs px-2 py-1 text-sm hover:bg-surface-hover"
          >
            <RadioGroupItem value={preset} />
            {t(
              `results.size${preset === 'any' ? 'Any' : preset === 'tiny' ? 'Tiny' : preset === 'small' ? 'Small' : preset === 'medium' ? 'Medium' : preset === 'large' ? 'Large' : 'Huge'}`,
            )}
          </label>
        ))}
      </RadioGroup>
      <div className="rounded-sm border border-border p-2">
        <p className="mb-1.5 text-xs font-medium text-foreground-secondary">
          {t('results.sizeCustom')}
        </p>
        <div className="flex items-center gap-2">
          <Input
            aria-label={t('results.sizeMinLabel')}
            type="number"
            min={0}
            placeholder="0"
            className="h-7 w-20 text-xs tabular"
            value={f.sizeMin ?? ''}
            onChange={(e) => f.setSizePreset('custom', e.target.value, undefined)}
          />
          <span className="text-xs text-foreground-disabled">MB —</span>
          <Input
            aria-label={t('results.sizeMaxLabel')}
            type="number"
            min={0}
            placeholder="∞"
            className="h-7 w-20 text-xs tabular"
            value={f.sizeMax ?? ''}
            onChange={(e) => f.setSizePreset('custom', undefined, e.target.value)}
          />
          <span className="text-xs text-foreground-disabled">MB</span>
        </div>
      </div>
    </div>
  );
}
