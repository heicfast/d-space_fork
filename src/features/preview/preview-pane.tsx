import * as React from 'react';
import { useTranslation } from 'react-i18next';
import type { ConnectorError, PreviewPayload } from '@/connector/contract';
import {
  FileWarningIcon,
  ImageIcon,
  Maximize2Icon,
  MinusIcon,
  PlusIcon,
  TextIcon,
  XIcon,
} from 'lucide-react';
import { useNodeDetail, useThumbnail } from '@/queries/results';
import { usePreview } from '@/queries/preview';
import { formatBytes, formatDateTime, formatDuration } from '@/lib/format';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { ConfidenceBadge } from '@/components/ui/confidence-badge';
import { Hint } from '@/components/ui/tooltip';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { hexWorker } from '@/workers/hex-worker-client';

/**
 * Preview pane — bounded per-kind rendering with connector-driven failure
 * diagnostics. Hex/text parse in a worker (never on the main thread);
 * image/pdf/av/office render connector-provided bounded artifacts.
 */

export function PreviewPane({
  scanId,
  nodeId,
  onClose,
}: {
  scanId: string;
  nodeId: string | null;
  onClose?: () => void;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const detail = useNodeDetail(scanId, nodeId);
  const node = detail.data?.node ?? null;
  const preview = usePreview(scanId, nodeId);
  const [modalOpen, setModalOpen] = React.useState(false);

  return (
    <div
      className="relative flex h-full min-h-0 flex-col overflow-y-auto bg-white dark:bg-surface"
      onClick={(e) => e.stopPropagation()}
    >
      {node ? (
        <>
          {/* Reference top thumbnail card + filename + Preview button (preview-panel-live_343e85c9.png) */}
          <div className="flex flex-col px-4 pt-3.5 pb-3">
            {onClose ? (
              <div className="mb-1.5 flex items-center justify-end">
                <button
                  type="button"
                  onClick={onClose}
                  aria-label={t('common.close')}
                  className="inline-flex size-6 items-center justify-center rounded-md text-[#64748B] transition-colors hover:bg-[#F1F5F9] hover:text-[#1E293B] dark:text-foreground-secondary dark:hover:bg-surface-hover dark:hover:text-foreground"
                >
                  <XIcon className="size-3.5" aria-hidden />
                </button>
              </div>
            ) : null}

            <div className="relative flex h-[132px] w-full items-center justify-center overflow-hidden rounded-[8px] border border-[#E2ECF9] bg-[#F2F6FC] dark:border-border dark:bg-surface-tint">
              <Thumbnail scanId={scanId} nodeId={node.id} available={node.thumbnailAvailable} />
            </div>

            {/* Filename directly below thumbnail */}
            <h3
              className="mt-2.5 text-center break-all text-[12.5px] leading-snug font-medium text-[#1E293B] dark:text-foreground"
              title={node.name}
            >
              {node.name}
            </h3>

            {/* Light blue "Preview" button below filename matching preview-panel-live_343e85c9.png */}
            <button
              type="button"
              onClick={() => setModalOpen(true)}
              className="mt-2.5 inline-flex h-8 w-full items-center justify-center rounded-[6px] bg-[#DCEBFF] px-4 text-[12.5px] font-semibold text-[#1677FF] transition-colors hover:bg-[#C7E0FF] focus-visible:ring-2 focus-visible:ring-ring dark:bg-surface-elevated dark:text-primary dark:hover:bg-surface-hover"
            >
              {t('preview.title')}
            </button>
          </div>

          {/* Stacked metadata ending at Recovery Chances (Size, Date Modified, Path, Status, Recovery Chances) */}
          <div className="space-y-3 px-4 pt-1 pb-4 text-[12px]">
            <div>
              <div className="text-[#94A3B8] dark:text-foreground-secondary">
                {t('results.column.size')}
              </div>
              <div className="mt-0.5 font-medium text-[#1E293B] tabular dark:text-foreground">
                {formatBytes(node.sizeBytes, i18n.language)}
              </div>
            </div>
            <div>
              <div className="text-[#94A3B8] dark:text-foreground-secondary">
                {t('results.column.modified')}
              </div>
              <div className="mt-0.5 font-medium text-[#1E293B] tabular dark:text-foreground">
                {formatDateTime(node.modifiedAt, i18n.language)}
              </div>
            </div>
            <div>
              <div className="text-[#94A3B8] dark:text-foreground-secondary">
                {t('results.pathTab')}
              </div>
              <div
                className="mt-0.5 truncate font-medium text-[#1E293B] dark:text-foreground"
                title={node.path ?? 'C:\\'}
              >
                {node.path ?? 'C:\\'}
              </div>
            </div>
            <div>
              <div className="text-[#94A3B8] dark:text-foreground-secondary">
                {t('results.column.type')}
              </div>
              <div className="mt-0.5 flex items-center gap-2 font-medium text-[#1E293B] dark:text-foreground">
                <span>
                  {node.origin === 'existing'
                    ? t('preview.statusExisting')
                    : node.origin === 'raw'
                      ? t('preview.statusRaw')
                      : t('preview.statusDeleted')}
                </span>
                <span className="text-[#64748B] uppercase dark:text-foreground-secondary">
                  {node.extension}
                </span>
              </div>
            </div>
            <div>
              <div className="text-[#94A3B8] dark:text-foreground-secondary">
                {t('results.column.condition')}
              </div>
              <div className="mt-1">
                <ConfidenceBadge grade={node.confidence} scanId={scanId} nodeId={node.id} />
              </div>
            </div>
          </div>

          <Dialog open={modalOpen} onOpenChange={setModalOpen}>
            <DialogContent
              className="max-w-2xl rounded-[12px] border border-[#DCE6F5] bg-white p-5 shadow-2xl dark:border-border dark:bg-surface-elevated"
              aria-label={node.name}
            >
              <DialogHeader>
                <DialogTitle className="truncate text-[15px] font-bold text-[#1E293B] dark:text-foreground">
                  {node.name}
                </DialogTitle>
                <DialogDescription className="text-[12px] text-[#64748B] dark:text-foreground-secondary">
                  {detail.data?.confidenceReason ?? formatBytes(node.sizeBytes, i18n.language)}
                </DialogDescription>
              </DialogHeader>
              <div className="mt-2 min-h-[260px] max-h-[65vh] overflow-y-auto">
                <PreviewBody previewQuery={preview} />
              </div>
            </DialogContent>
          </Dialog>
        </>
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-foreground-secondary">
          <div className="flex h-[128px] w-[152px] items-center justify-center rounded-lg border border-[#E2ECF9] bg-[#F2F6FC] dark:border-border dark:bg-surface-tint">
            <ImageIcon className="size-10 text-[#4B8BF5]" strokeWidth={1.5} aria-hidden />
          </div>
          <p className="mt-2 text-xs">{t('preview.title')}</p>
        </div>
      )}
    </div>
  );
}

function Thumbnail({
  scanId,
  nodeId,
  available,
}: {
  scanId: string;
  nodeId: string;
  available: boolean;
}): React.JSX.Element {
  const { data } = useThumbnail(scanId, nodeId, available);
  if (data?.dataUrl) {
    return <img src={data.dataUrl} alt="" className="size-full object-cover" />;
  }
  return (
    <div className="flex size-full items-center justify-center">
      <ImageIcon className="size-12 text-[#4B8BF5]" strokeWidth={1.5} aria-hidden />
    </div>
  );
}

function PreviewBody({
  previewQuery,
}: {
  previewQuery: ReturnType<typeof usePreview>;
}): React.JSX.Element | null {
  const { t } = useTranslation();

  if (previewQuery.isLoading) {
    return (
      <div className="space-y-2">
        <Skeleton className="h-40" />
        <Skeleton className="h-4 w-2/3" />
        <Skeleton className="h-4 w-1/2" />
      </div>
    );
  }

  if (previewQuery.isError) {
    const err = previewQuery.error as Partial<ConnectorError> & { message?: string };
    const reasonKey = err?.reasonKey;
    return (
      <div className="flex flex-col items-center justify-center gap-2 py-10 text-center">
        <FileWarningIcon className="size-8 text-warning" strokeWidth={1.5} aria-hidden />
        <p className="text-sm font-semibold">{t('preview.failedTitle')}</p>
        <p className="max-w-56 text-xs text-foreground-secondary">
          {reasonKey ? t(reasonKey) : (err?.message ?? t('errors.unknown'))}
        </p>
      </div>
    );
  }

  const payload = previewQuery.data;
  if (!payload) return null;

  switch (payload.kind) {
    case 'image':
      return <ImagePreview payload={payload} />;
    case 'hex':
      // key forces a fresh subcomponent per payload — state resets by
      // remount instead of synchronous setState in an effect.
      return <HexPreview key={`${previewQuery.dataUpdatedAt}`} payload={payload} />;
    case 'text':
      return <TextPreview payload={payload} />;
    case 'pdf':
      return <PdfPreview payload={payload} />;
    case 'av':
      return <AvPreview payload={payload} />;
    case 'office':
      return <OfficePreview payload={payload} />;
  }
  return null;
}

// --------------------------------------------------------------------- kinds

function ImagePreview({
  payload,
}: {
  payload: Extract<PreviewPayload, { kind: 'image' }>;
}): React.JSX.Element {
  const { t } = useTranslation();
  const [zoom, setZoom] = React.useState(1);
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-end gap-1 pb-2">
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          aria-label={t('preview.zoomOut')}
          onClick={() => setZoom((z) => Math.max(0.25, z / 1.25))}
        >
          <MinusIcon aria-hidden />
        </Button>
        <span className="w-12 text-center text-2xs tabular text-foreground-secondary">
          {Math.round(zoom * 100)}%
        </span>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          aria-label={t('preview.zoomIn')}
          onClick={() => setZoom((z) => Math.min(8, z * 1.25))}
        >
          <PlusIcon aria-hidden />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          aria-label={t('preview.fit')}
          onClick={() => setZoom(1)}
        >
          <Maximize2Icon aria-hidden />
        </Button>
      </div>
      <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto rounded-md bg-surface-tint p-2">
        <img
          src={payload.dataUrl}
          alt=""
          style={{ transform: `scale(${zoom})`, transformOrigin: 'center' }}
          className="max-h-full transition-transform duration-150"
        />
      </div>
    </div>
  );
}

function TextPreview({
  payload,
}: {
  payload: Extract<PreviewPayload, { kind: 'text' }>;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  return (
    <div className="flex h-full flex-col gap-2">
      {payload.truncated ? (
        <p className="text-xs text-foreground-secondary">
          {t('preview.truncated', {
            size: formatBytes(payload.text.length, i18n.language),
            total: formatBytes(payload.totalSize, i18n.language),
          })}
        </p>
      ) : null}
      <div className="flex items-center gap-2 text-xs">
        <TextIcon className="size-3.5" aria-hidden />
        <Badge variant="outline">
          {t('preview.encoding')}: {payload.encoding}
        </Badge>
      </div>
      <pre className="min-h-0 flex-1 overflow-auto rounded-md bg-surface-muted p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap">
        {payload.text}
      </pre>
    </div>
  );
}

function PdfPreview({
  payload,
}: {
  payload: Extract<PreviewPayload, { kind: 'pdf' }>;
}): React.JSX.Element {
  const { t } = useTranslation();
  const [page, setPage] = React.useState(0);
  const current = payload.pages[page];
  return (
    <div className="flex h-full flex-col gap-2">
      <img
        src={current?.dataUrl}
        alt={`page ${page + 1}`}
        className="mx-auto max-h-full rounded-md border border-border shadow-card"
      />
      <div className="flex items-center justify-center gap-2 pt-1 text-xs tabular">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setPage((p) => Math.max(0, p - 1))}
          disabled={page === 0}
          aria-label={t('preview.prevPage')}
        >
          ‹
        </Button>
        <span>{t('preview.pageOf', { page: page + 1, total: payload.pageCount })}</span>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setPage((p) => Math.min(payload.pageCount - 1, p + 1))}
          disabled={page >= payload.pageCount - 1}
          aria-label={t('preview.nextPage')}
        >
          ›
        </Button>
      </div>
    </div>
  );
}

/** Key/value metadata list shared by the AV and Office preview kinds. */
function MetaList({ metadata }: { metadata: Readonly<Record<string, string>> }) {
  return (
    <dl className="space-y-1 text-xs">
      {Object.entries(metadata).map(([k, v]) => (
        <div key={k} className="flex justify-between gap-3">
          <dt className="text-foreground-secondary">{k}</dt>
          <dd className="font-medium tabular">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function AvPreview({
  payload,
}: {
  payload: Extract<PreviewPayload, { kind: 'av' }>;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  return (
    <div className="space-y-3">
      {payload.thumbnailDataUrl ? (
        <img
          src={payload.thumbnailDataUrl}
          alt=""
          className="w-full rounded-md border border-border"
        />
      ) : null}
      <MetaList metadata={payload.metadata} />
      {payload.durationSeconds ? (
        <p className="text-xs text-foreground-secondary tabular">
          {formatDuration(payload.durationSeconds ?? 0, i18n.language)}
        </p>
      ) : null}
      {!payload.thumbnailDataUrl ? (
        <Hint text={t('preview.reason.unsupportedKind')}>
          <Badge variant="neutral">{t('preview.unsupportedTitle')}</Badge>
        </Hint>
      ) : null}
    </div>
  );
}

function OfficePreview({
  payload,
}: {
  payload: Extract<PreviewPayload, { kind: 'office' }>;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  return (
    <div className="space-y-3">
      <MetaList metadata={payload.metadata} />
      {payload.extractedText ? (
        <>
          {payload.truncated ? (
            <p className="text-xs text-foreground-secondary">
              {t('preview.truncated', {
                size: formatBytes(payload.extractedText.length, i18n.language),
                total: formatBytes(Number(payload.metadata.pages ?? 1) * 24_000, i18n.language),
              })}
            </p>
          ) : null}
          <pre className="max-h-64 overflow-auto rounded-md bg-surface-muted p-3 text-xs whitespace-pre-wrap">
            {payload.extractedText}
          </pre>
        </>
      ) : null}
    </div>
  );
}

function HexPreview({
  payload,
}: {
  payload: Extract<PreviewPayload, { kind: 'hex' }>;
}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const [rows, setRows] = React.useState<
    { offset: number; hex: string; ascii: string }[] | null
  >(null);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let cancelled = false;
    hexWorker(payload.bytes, payload.baseOffset)
      .then((r) => {
        if (!cancelled) setRows(r);
      })
      .catch(() => {
        if (!cancelled) setError(t('errors.unknown'));
      });
    return () => {
      cancelled = true;
    };
  }, [payload.bytes, payload.baseOffset, t]);

  if (error) {
    return <p className="text-xs text-danger">{error}</p>;
  }
  if (!rows) {
    return <Skeleton className="h-48" />;
  }

  return (
    <div className="space-y-2">
      <p className="text-xs text-foreground-secondary tabular">
        {t('preview.firstBytes', {
          size: formatBytes(payload.bytes.byteLength, i18n.language),
        })}
        {payload.totalSize > payload.bytes.byteLength
          ? ` ${t('preview.truncated', { size: formatBytes(payload.bytes.byteLength, i18n.language), total: formatBytes(payload.totalSize, i18n.language) })}`
          : ''}
      </p>
      <div className="overflow-x-auto rounded-md bg-surface-muted p-2">
        <table className="w-full font-mono text-2xs leading-relaxed tabular">
          <tbody>
            {rows.map((r) => (
              <tr key={r.offset}>
                <td className="pr-3 text-foreground-disabled">
                  {r.offset.toString(16).padStart(8, '0')}
                </td>
                <td className="whitespace-pre pr-3">{r.hex}</td>
                <td className="whitespace-pre text-foreground-secondary">{r.ascii}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
