import * as React from 'react';
import type { FileCategoryId, ResultNode } from '@/connector/contract';
import {
  ArchiveIcon,
  AudioLinesIcon,
  BookmarkIcon,
  FileIcon,
  FileTextIcon,
  ImageIcon,
  MailIcon,
  PresentationIcon,
  SheetIcon,
  VideoIcon,
} from 'lucide-react';
import { cn } from '@/lib/utils';

/**
 * Interim filetype icon mapping (Lucide base per v1 policy — see
 * INTERIM-ASSETS.md; v2 ships a purpose-drawn two-tone set).
 * Icons carry category tints so color never encodes information alone
 * (shape differs per category too, grid #14).
 */

const CATEGORY_ICON: Record<
  FileCategoryId,
  React.ComponentType<{ className?: string; strokeWidth?: number }>
> = {
  graphics: ImageIcon,
  video: VideoIcon,
  audio: AudioLinesIcon,
  document: FileTextIcon,
  email: MailIcon,
  archive: ArchiveIcon,
  bookmarks: BookmarkIcon,
  other: FileIcon,
};

const CATEGORY_TINT: Record<FileCategoryId, string> = {
  graphics: 'text-emerald-600 dark:text-emerald-400',
  video: 'text-violet-600 dark:text-violet-400',
  audio: 'text-pink-600 dark:text-pink-400',
  document: 'text-sky-700 dark:text-sky-400',
  email: 'text-amber-600 dark:text-amber-400',
  archive: 'text-orange-600 dark:text-orange-400',
  bookmarks: 'text-teal-600 dark:text-teal-400',
  other: 'text-foreground-secondary',
};

const EXT_OVERRIDE: Record<
  string,
  React.ComponentType<{ className?: string; strokeWidth?: number }>
> = {
  doc: FileTextIcon,
  docx: FileTextIcon,
  xls: SheetIcon,
  xlsx: SheetIcon,
  csv: SheetIcon,
  tsv: SheetIcon,
  ppt: PresentationIcon,
  pptx: PresentationIcon,
};

export function FileIconFor({
  node,
  className,
}: {
  node: ResultNode;
  className?: string;
}): React.JSX.Element {
  const ext = node.extension?.replace('.', '').toLowerCase() ?? '';
  const category = node.category ?? 'other';
  const Icon = EXT_OVERRIDE[ext] ?? CATEGORY_ICON[category] ?? FileIcon;
  return (
    <span
      className="relative inline-flex size-5 shrink-0 items-center justify-center"
      aria-hidden
    >
      <Icon className={cn('size-4', CATEGORY_TINT[category], className)} strokeWidth={1.75} />
    </span>
  );
}

export function CategoryIcon({
  category,
  className,
}: {
  category: FileCategoryId;
  className?: string;
}): React.JSX.Element {
  const Icon = CATEGORY_ICON[category] ?? FileIcon;
  return (
    <Icon
      className={cn('size-5', CATEGORY_TINT[category], className)}
      strokeWidth={1.75}
      aria-hidden
    />
  );
}
