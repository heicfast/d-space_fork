import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { FileIcon, FolderIcon, FolderOpenIcon, SearchIcon } from 'lucide-react';
import { useIndexSearch } from '@/queries/cleanup';
import { useOpenPath } from '@/queries/system';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { Input } from '@/components/ui/input';
import { Skeleton } from '@/components/ui/skeleton';
import { toast } from 'sonner';

/**
 * Quick Search — the USN journal index (chapter 5.6) surfaced as an
 * Everything-style name finder. The backend keeps the index warm for
 * scanned volumes (`dw_index_search`); this page is a thin, honest
 * surface over it: type a fragment, get matching file/folder names with
 * their full paths.
 *
 * Scope notes (deliberate):
 *  - substring search only; the engine owns the matching semantics
 *  - results carry the owning volume id; reveal opens the parent folder
 *  - when no index exists yet (volume never scanned / no admin), the
 *    connector degrades honestly and the page says so
 */
export function QuickSearchPage(): React.JSX.Element {
  const { t } = useTranslation();
  const [raw, setRaw] = React.useState('');
  const [query, setQuery] = React.useState('');
  const openPath = useOpenPath();

  // Debounced commit (250 ms) — the query hook only fires for >= 2 chars.
  React.useEffect(() => {
    const id = window.setTimeout(() => setQuery(raw.trim()), 250);
    return () => window.clearTimeout(id);
  }, [raw]);

  const search = useIndexSearch(query.length >= 2 ? query : null);

  const hits = search.data ?? [];
  const reveal = (path: string | null): void => {
    if (!path) return;
    const cut = path.lastIndexOf('\\');
    const folder = cut > 2 ? path.slice(0, cut) : path;
    openPath.mutate(folder, { onError: (err) => toast.error(err.message) });
  };

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto">
      <header className="px-8 pt-7 pb-4">
        <h1 className="text-xl font-semibold tracking-tight">{t('quickSearch.title')}</h1>
        <p className="mt-1 max-w-3xl text-sm text-foreground-secondary">
          {t('quickSearch.subtitle')}
        </p>
      </header>

      <div className="mx-8 mb-2 flex items-center gap-2">
        <div className="relative flex-1">
          <SearchIcon
            className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-foreground-secondary"
            aria-hidden
          />
          <Input
            autoFocus
            value={raw}
            onChange={(e) => setRaw(e.target.value)}
            placeholder={t('quickSearch.placeholder')}
            aria-label={t('quickSearch.placeholder')}
            className="pl-9"
          />
        </div>
      </div>

      <div className="mx-8 mb-4 min-h-6 text-xs text-foreground-secondary" role="status">
        {search.isFetching
          ? t('quickSearch.searching')
          : query.length >= 2 && !search.isError
            ? t('quickSearch.resultCount', { count: hits.length })
            : ''}
      </div>

      <div className="mx-8 mb-8 flex-1">
        {query.length < 2 ? (
          <EmptyState
            icon={SearchIcon}
            title={t('quickSearch.emptyTitle')}
            description={t('quickSearch.emptyBody')}
          />
        ) : search.isLoading ? (
          <div className="space-y-2" aria-hidden>
            {Array.from({ length: 5 }, (_, i) => (
              <Skeleton key={i} className="h-12" />
            ))}
          </div>
        ) : search.isError ? (
          <EmptyState
            icon={SearchIcon}
            title={t('quickSearch.unavailableTitle')}
            description={search.error.message}
          />
        ) : hits.length === 0 ? (
          <EmptyState
            icon={SearchIcon}
            title={t('quickSearch.noResults')}
            description={t('quickSearch.noResultsBody')}
          />
        ) : (
          <ul className="divide-y divide-border rounded-lg border border-border">
            {hits.map((hit, i) => (
              <li
                key={`${hit.volumeId}:${hit.fullPath ?? hit.name}:${i}`}
                className="flex items-center gap-3 px-4 py-2.5"
              >
                {hit.isDir ? (
                  <FolderIcon
                    className="size-4 shrink-0 text-foreground-secondary"
                    aria-hidden
                  />
                ) : (
                  <FileIcon className="size-4 shrink-0 text-foreground-secondary" aria-hidden />
                )}
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium" title={hit.name}>
                      {hit.name}
                    </span>
                    {hit.isDir ? (
                      <Badge variant="outline">{t('quickSearch.folderBadge')}</Badge>
                    ) : null}
                  </div>
                  {hit.fullPath ? (
                    <div
                      className="truncate text-xs text-foreground-secondary"
                      title={hit.fullPath}
                    >
                      {hit.fullPath}
                    </div>
                  ) : null}
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={!hit.fullPath || openPath.isPending}
                  onClick={() => reveal(hit.fullPath)}
                >
                  <FolderOpenIcon className="size-4" aria-hidden />
                  {t('quickSearch.reveal')}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="sr-only" aria-live="polite">
        {search.isFetching
          ? t('quickSearch.searching')
          : query.length >= 2
            ? t('quickSearch.resultCount', { count: hits.length })
            : ''}
      </div>
      <div className="px-8 pb-6 text-xs text-foreground-secondary">{t('quickSearch.hint')}</div>
    </div>
  );
}
