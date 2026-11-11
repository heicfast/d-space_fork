/**
 * Deterministic fixture definitions for the Mock Connector.
 *
 * Scenarios (per contract MockScenarioId):
 *  - standard:        internal 512 GB NTFS (C:) + 2 TB NTFS data disk + 64 GB SD
 *  - large-ntfs:      2 TB NTFS with ~1.2 M result nodes (procedural index)
 *  - sd-card:         single 64 GB exFAT card, photo-heavy content
 *  - btrfs-disk:      4 TB Btrfs disk, linux-flavored layout
 *  - lost-partition:  disk with a candidate lost partition entry point
 *  - permission-denied / disconnect-mid-scan / bad-sectors / out-of-space /
 *    corrupt-session: behavior-modifying injections
 */

import type { Device, FileCategoryId, MockScenarioId, Volume } from '../contract';

export interface DeviceFixture {
  readonly device: Omit<Device, 'volumes' | 'isSourceOfActiveScan'>;
  readonly volumes: readonly Volume[];
  /** Seed for this device's node space. */
  readonly nodeSeed: number;
  /** Approximate file count for the primary volume. */
  readonly fileCount: number;
  readonly directoryCount: number;
}

export const CATEGORY_BY_EXT: Record<string, FileCategoryId> = {};
function register(category: FileCategoryId, exts: string[]): void {
  for (const e of exts) CATEGORY_BY_EXT[e] = category;
}

register('graphics', [
  'jpg',
  'jpeg',
  'png',
  'gif',
  'bmp',
  'tif',
  'tiff',
  'webp',
  'heic',
  'svg',
  'psd',
  'cr2',
  'cr3',
  'nef',
  'arw',
  'dng',
  'raw',
  'ico',
]);
register('video', [
  'mp4',
  'mov',
  'avi',
  'mkv',
  'wmv',
  'flv',
  'm4v',
  'webm',
  'mts',
  'm2ts',
  '3gp',
]);
register('audio', ['mp3', 'wav', 'flac', 'aac', 'ogg', 'wma', 'm4a', 'aiff', 'opus']);
register('document', [
  'doc',
  'docx',
  'xls',
  'xlsx',
  'ppt',
  'pptx',
  'pdf',
  'txt',
  'rtf',
  'odt',
  'md',
  'csv',
  'tsv',
]);
register('email', ['pst', 'ost', 'eml', 'msg', 'mbox']);
register('archive', ['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'cab', 'iso']);
register('bookmarks', ['url', 'webloc']);
register('other', [
  'exe',
  'dll',
  'sys',
  'ini',
  'log',
  'dat',
  'db',
  'sqlite',
  'xml',
  'json',
  'bin',
  'tmp',
  'crypt',
]);

/** Weighted extension pools so generated content looks realistic. */
export const EXT_WEIGHTS: readonly { readonly ext: string; readonly weight: number }[] = [
  { ext: 'jpg', weight: 18 },
  { ext: 'png', weight: 12 },
  { ext: 'mp4', weight: 6 },
  { ext: 'pdf', weight: 8 },
  { ext: 'docx', weight: 6 },
  { ext: 'xlsx', weight: 4 },
  { ext: 'pptx', weight: 3 },
  { ext: 'txt', weight: 8 },
  { ext: 'zip', weight: 4 },
  { ext: 'rar', weight: 2 },
  { ext: '7z', weight: 1 },
  { ext: 'mp3', weight: 5 },
  { ext: 'wav', weight: 2 },
  { ext: 'flac', weight: 2 },
  { ext: 'heic', weight: 3 },
  { ext: 'raw', weight: 2 },
  { ext: 'cr3', weight: 1 },
  { ext: 'nef', weight: 1 },
  { ext: 'svg', weight: 2 },
  { ext: 'psd', weight: 1 },
  { ext: 'mov', weight: 2 },
  { ext: 'mkv', weight: 2 },
  { ext: 'avi', weight: 2 },
  { ext: 'eml', weight: 2 },
  { ext: 'pst', weight: 1 },
  { ext: 'exe', weight: 3 },
  { ext: 'dll', weight: 2 },
  { ext: 'ini', weight: 2 },
  { ext: 'log', weight: 3 },
  { ext: 'db', weight: 1 },
  { ext: 'sqlite', weight: 1 },
  { ext: 'json', weight: 2 },
  { ext: 'xml', weight: 2 },
  { ext: 'csv', weight: 2 },
  { ext: 'tmp', weight: 2 },
];

export const NAME_STEMS: readonly string[] = [
  'report',
  'notes',
  'photo',
  'img',
  'document',
  'draft',
  'backup',
  'project',
  'invoice',
  'resume',
  'budget',
  'plan',
  'summary',
  'screenshot',
  'capture',
  'render',
  'scan',
  'export',
  'data',
  'log',
  'config',
  'archive',
  'video',
  'clip',
  'track',
  'song',
  'album',
  'picture',
  'image',
  'final',
  'copy',
  'old',
  'new',
  'test',
  'work',
  'home',
  'family',
  'trip',
  'holiday',
  'event',
  'meeting',
  'presentation',
  'design',
  'asset',
  'source',
  'readme',
  'manual',
  'guide',
  'worksheet',
  'ledger',
  'statement',
  'receipt',
  'contract',
  'letter',
];

export const DIR_NAMES: readonly string[] = [
  'Documents',
  'Downloads',
  'Pictures',
  'Photos',
  'Videos',
  'Music',
  'Desktop',
  'Projects',
  'Work',
  'Archive',
  'Backups',
  'Camera',
  'Screenshots',
  'Exports',
  'Data',
  'Logs',
  'Config',
  'Temp',
  'Shared',
  'Media',
  'Movies',
  'Records',
  '2023',
  '2024',
  '2025',
  'Q1',
  'Q2',
  'Q3',
  'Q4',
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
  'vacation',
  'birthday',
  'wedding',
  'office',
  'personal',
  'client',
];

function capacity(total: number, usedRatio: number) {
  const totalBytes = total;
  const usedBytes = Math.round(totalBytes * usedRatio);
  return { totalBytes, usedBytes, freeBytes: totalBytes - usedBytes };
}

function mkVolume(
  v: Omit<Volume, 'lostPartition' | 'quickScanAvailable' | 'sectorSize'> &
    Partial<Pick<Volume, 'lostPartition' | 'quickScanAvailable' | 'sectorSize'>>,
): Volume {
  return {
    ...v,
    quickScanAvailable: v.quickScanAvailable ?? true,
    sectorSize: v.sectorSize ?? 512,
  };
}

function deviceBase(p: {
  id: string;
  kind: Device['kind'];
  model: string;
  label: string;
  serial: string;
  cap: { totalBytes: number; usedBytes: number; freeBytes: number };
  status?: Device['status'];
  canScanWholeDisk?: boolean;
}): Omit<Device, 'volumes' | 'isSourceOfActiveScan'> {
  return {
    id: p.id,
    kind: p.kind,
    model: p.model,
    label: p.label,
    serial: p.serial,
    capacity: p.cap,
    status: p.status ?? 'ok',
    canScanWholeDisk: p.canScanWholeDisk ?? true,
  };
}

/** Shared system-volume base (differs only by node seed + counts). */
function sysVolumeFixture(
  nodeSeed: number,
  fileCount: number,
  directoryCount: number,
): DeviceFixture {
  const totalBytes = Math.round(149.45 * GB);
  const freeBytes = Math.round(31.16 * GB);
  const cap = { totalBytes, usedBytes: totalBytes - freeBytes, freeBytes };
  return {
    device: deviceBase({
      id: 'dev-sys',
      kind: 'internal',
      model: 'Samsung 970 EVO 500GB',
      label: 'Windows (C:)',
      serial: 'S4EWN0-500-33892',
      cap,
    }),
    volumes: [
      mkVolume({
        id: 'vol-sys',
        deviceId: 'dev-sys',
        label: 'Windows (C:)',
        letter: 'C',
        fs: 'ntfs',
        capacity: cap,
        status: 'ok',
        bitlocker: 'unlocked',
      }),
    ],
    nodeSeed,
    fileCount,
    directoryCount,
  };
}

const GB = 1024 ** 3;
const TB = 1024 ** 4;

/**
 * The "Specify a location" entries (atlas §5 / GT-landing §4): Recycle Bin
 * and Desktop are virtual scan targets, not drives — no letter, no mount
 * point, excluded from health/repair pickers, scanned directly from the
 * landing page's quick-target row.
 */
const LOCATION_FIXTURES: readonly DeviceFixture[] = [
  {
    device: deviceBase({
      id: 'dev-recycle-bin',
      kind: 'virtual',
      model: 'Shell location',
      label: 'Recycle Bin',
      serial: 'LOC-RECYCLE',
      cap: capacity(4 * GB, 0.35),
      canScanWholeDisk: false,
    }),
    volumes: [
      mkVolume({
        id: 'vol-recycle-bin',
        deviceId: 'dev-recycle-bin',
        label: 'Recycle Bin',
        letter: null,
        fs: 'ntfs',
        capacity: capacity(4 * GB, 0.35),
        status: 'ok',
      }),
    ],
    nodeSeed: 5501,
    fileCount: 900,
    directoryCount: 24,
  },
  {
    device: deviceBase({
      id: 'dev-desktop',
      kind: 'virtual',
      model: 'Shell location',
      label: 'Desktop',
      serial: 'LOC-DESKTOP',
      cap: capacity(2 * GB, 0.5),
      canScanWholeDisk: false,
    }),
    volumes: [
      mkVolume({
        id: 'vol-desktop',
        deviceId: 'dev-desktop',
        label: 'Desktop',
        letter: null,
        fs: 'ntfs',
        capacity: capacity(2 * GB, 0.5),
        status: 'ok',
      }),
    ],
    nodeSeed: 6602,
    fileCount: 420,
    directoryCount: 12,
  },
];

export function fixturesFor(scenario: MockScenarioId): readonly DeviceFixture[] {
  const base = fixturesForScenario(scenario);
  return [...base, ...LOCATION_FIXTURES];
}

function fixturesForScenario(scenario: MockScenarioId): readonly DeviceFixture[] {
  switch (scenario) {
    case 'sd-card':
      return [
        {
          device: deviceBase({
            id: 'dev-sdcard',
            kind: 'sd',
            model: 'SDXC Card',
            label: 'Memory Card (E:)',
            serial: 'SD-9042-C',
            cap: capacity(64 * GB, 0.71),
          }),
          volumes: [
            mkVolume({
              id: 'vol-sdcard',
              deviceId: 'dev-sdcard',
              label: 'Memory Card (E:)',
              letter: 'E',
              fs: 'exfat',
              capacity: capacity(64 * GB, 0.71),
              status: 'ok',
            }),
          ],
          nodeSeed: 1102,
          fileCount: 24_000,
          directoryCount: 120,
        },
      ];
    case 'btrfs-disk':
      return [
        {
          device: deviceBase({
            id: 'dev-btrfs',
            kind: 'internal',
            model: 'WD Red Pro 4TB',
            label: 'Storage Array',
            serial: 'WD-4TB-88210',
            cap: capacity(4 * TB, 0.58),
          }),
          volumes: [
            mkVolume({
              id: 'vol-btrfs',
              deviceId: 'dev-btrfs',
              label: 'Storage Array',
              letter: null,
              fs: 'btrfs',
              capacity: capacity(4 * TB, 0.58),
              status: 'ok',
            }),
          ],
          nodeSeed: 2203,
          fileCount: 480_000,
          directoryCount: 2_600,
        },
      ];
    case 'lost-partition':
      return [
        {
          device: deviceBase({
            id: 'dev-lost',
            kind: 'internal',
            model: 'Seagate Barracuda 1TB',
            label: 'Hard Disk 1',
            serial: 'ST-1TB-55107',
            cap: capacity(1 * TB, 0.42),
          }),
          volumes: [
            mkVolume({
              id: 'vol-lost-main',
              deviceId: 'dev-lost',
              label: 'Data (D:)',
              letter: 'D',
              fs: 'ntfs',
              capacity: capacity(600 * GB, 0.42),
              status: 'ok',
              lostPartition: {
                id: 'lost-part-1',
                startByte: 660 * GB,
                sizeBytes: 330 * GB,
                probableFs: 'ntfs',
                note: 'Unallocated area with a valid NTFS boot sector',
              },
            }),
          ],
          nodeSeed: 3304,
          fileCount: 96_000,
          directoryCount: 900,
        },
      ];
    case 'large-ntfs':
      return [
        {
          device: deviceBase({
            id: 'dev-large',
            kind: 'internal',
            model: 'Samsung 970 EVO 2TB',
            label: 'Data Disk (D:)',
            serial: 'S4EWN0-2TB-33891',
            cap: capacity(2 * TB, 0.64),
          }),
          volumes: [
            mkVolume({
              id: 'vol-large',
              deviceId: 'dev-large',
              label: 'Data Disk (D:)',
              letter: 'D',
              fs: 'ntfs',
              capacity: capacity(2 * TB, 0.64),
              status: 'ok',
            }),
          ],
          nodeSeed: 4405,
          fileCount: 1_200_000,
          directoryCount: 5_400,
        },
        sysVolumeFixture(5506, 210_000, 1_800),
      ];
    default:
      // standard
      return [
        sysVolumeFixture(6607, 180_000, 1_600),
        {
          device: deviceBase({
            id: 'dev-data',
            kind: 'internal',
            model: 'WD Blue 2TB',
            label: 'Temporary Storage (D:)',
            serial: 'WD-2TB-71042',
            cap: {
              totalBytes: 150 * GB,
              freeBytes: Math.round(146.12 * GB),
              usedBytes: Math.round((150 - 146.12) * GB),
            },
          }),
          volumes: [
            mkVolume({
              id: 'vol-data',
              deviceId: 'dev-data',
              label: 'Temporary Storage (D:)',
              letter: 'D',
              fs: 'ntfs',
              capacity: {
                totalBytes: 150 * GB,
                freeBytes: Math.round(146.12 * GB),
                usedBytes: Math.round((150 - 146.12) * GB),
              },
              status: 'ok',
              bitlocker: 'locked',
            }),
          ],
          nodeSeed: 7708,
          fileCount: 340_000,
          directoryCount: 2_400,
        },
        {
          device: deviceBase({
            id: 'dev-usb',
            kind: 'usb',
            model: 'SanDisk Ultra 64GB',
            label: 'USB Drive (F:)',
            serial: 'SD-64GB-22140',
            cap: capacity(60 * GB, 0.83),
          }),
          volumes: [
            mkVolume({
              id: 'vol-usb',
              deviceId: 'dev-usb',
              label: 'USB Drive (F:)',
              letter: 'F',
              fs: 'exfat',
              capacity: capacity(60 * GB, 0.83),
              status: 'ok',
            }),
          ],
          nodeSeed: 8809,
          fileCount: 21_000,
          directoryCount: 260,
        },
      ];
  }
}
