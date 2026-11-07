<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useI18n } from "vue-i18n";
import { setLocale, type SupportedLocale } from "./i18n";

const { t, locale } = useI18n();
import Sunburst from "./components/Sunburst.vue";
import Sunburst3D from "./components/Sunburst3D.vue";
import Treemap from "./components/Treemap.vue";
import Bubble from "./components/Bubble.vue";
import Timeline from "./components/Timeline.vue";
import SnapshotPanel from "./components/SnapshotPanel.vue";
import DuplicatePanel from "./components/DuplicatePanel.vue";
import Onboarding from "./components/Onboarding.vue";
import UserRulesPanel from "./components/UserRulesPanel.vue";
import ScanProgress from "./components/ScanProgress.vue";
import VolumeSidebar from "./components/VolumeSidebar.vue";
import IndexSearchBar from "./components/IndexSearchBar.vue";
import UpdateNotification from "./components/UpdateNotification.vue";
import InfoButton from "./components/InfoButton.vue";

type ViewMode = "sunburst" | "sunburst3d" | "treemap" | "bubble" | "timeline";

interface AppInfo {
  name: string;
  version: string;
  spec_version: string;
  spec_status: string;
  license: string;
  platform: string;
}

type ScanStrategy =
  | "MftService"
  | "DirectRawVolume"
  | "FindFirstFileFallback";

interface PrivilegeStatus {
  elevated: boolean;
  strategy: ScanStrategy;
}

interface MftProbe {
  drive: string;
  volume_path: string;
  volume_serial: number;
  cluster_size: number;
  sector_size: number;
  file_record_size: number;
  elapsed_ms: number;
}

type VolumeStatusKind =
  | "Ready"
  | "BitLockerLocked"
  | "BitLockerSuspended"
  | "Encrypted"
  | "NotMounted"
  | "AccessDenied"
  | "NotFormatted"
  | "UnsupportedDriveType";

interface VolumeStatus {
  kind: VolumeStatusKind;
  data?: { method?: string; drive_type?: number };
}

type DriveKind =
  | "Unknown"
  | "NoRootDir"
  | "Removable"
  | "Fixed"
  | "Remote"
  | "CdRom"
  | "RamDisk";

interface VolumeInfo {
  drive_letter: string;
  root_path: string;
  file_system: string;
  volume_label: string;
  volume_serial: number;
  drive_kind: DriveKind;
  total_bytes: number;
  free_bytes: number;
  status: VolumeStatus;
  elapsed_ms: number;
}

interface DspaceError {
  kind: string;
  message: string;
}

interface DbInfo {
  path: string;
  schema_version: number;
  journal_mode: string;
  page_size: number;
  table_count: number;
  spec_version: string;
}

interface StagedItem {
  id: number;
  original_path: string;
  staged_path: string;
  size_bytes: number;
  staged_at_unix: number;
  expires_at_unix: number;
  is_dir: boolean;
  fallback_tier: string;
}

interface MftWalkStats {
  drive: string;
  volume_path: string;
  total_records_estimate: number;
  records_visited: number;
  in_use_records: number;
  directory_records: number;
  file_records: number;
  skipped_errors: number;
  bytes_aggregate: number;
  sample_names: string[];
  elapsed_ms: number;
}

interface ScanSummary {
  drive: string;
  volume_id: string;
  strategy: ScanStrategy;
  root_id: number;
  node_count: number;
  file_count: number;
  dir_count: number;
  total_bytes: number;
  collect_elapsed_ms: number;
  build_elapsed_ms: number;
}

interface TreeNode {
  id: number;
  parent: number | null;
  name: string;
  data_size: number;
  aggregate_size: number;
  is_dir: boolean;
  score: number | null;
  /** Bölüm 6.4 — "user" eşleşmesi kullanıcı kuralından, diğerleri built-in id'si */
  score_rule: string | null;
  score_reason: string | null;
  modified_unix: number;
}

type SortKey = "size_desc" | "name_asc" | "data_size_desc";

type LockState =
  | "free"
  | "locked"
  | "access_denied"
  | "not_found"
  | { other_error: number };

type LockedFileAction =
  | "proceed"
  | "skip_content"
  | "snapshot_read"
  | "user_drill_down"
  | "hard_pass";

interface LockOwner {
  pid: number;
  process_name: string;
  service_short_name: string | null;
  restartable: boolean;
}

interface LockedFileProbe {
  path: string;
  state: LockState;
  owners: LockOwner[];
  recommended_action: LockedFileAction;
  vss_available: boolean;
  probe_elapsed_ms: number;
}

interface WindowResult {
  parent_id: number;
  parent_name: string;
  parent_aggregate_size: number;
  total_children: number;
  returned: number;
  nodes: TreeNode[];
}

const info = ref<AppInfo | null>(null);
const ipcError = ref<string | null>(null);

const privilege = ref<PrivilegeStatus | null>(null);
const drive = ref<string>("C");
const probe = ref<MftProbe | null>(null);
const probeError = ref<string | null>(null);
const probing = ref(false);

const volumeInfo = ref<VolumeInfo | null>(null);
const volumeError = ref<string | null>(null);
const preFlighting = ref(false);

const dbInfo = ref<DbInfo | null>(null);
const dbError = ref<string | null>(null);

const walkStats = ref<MftWalkStats | null>(null);
const walkError = ref<string | null>(null);
const walking = ref(false);

const scanSummary = ref<ScanSummary | null>(null);
const scanError = ref<string | null>(null);
const scanning = ref(false);

const viewWindow = ref<WindowResult | null>(null);
const breadcrumb = ref<TreeNode[]>([]);
const sortKey = ref<SortKey>("size_desc");
const windowError = ref<string | null>(null);
const windowLoading = ref(false);

const stagingList = ref<StagedItem[]>([]);
const stagingError = ref<string | null>(null);
const stagingBusyId = ref<number | null>(null);
const stagePendingPath = ref<string | null>(null);

const lockProbes = ref<Map<number, LockedFileProbe>>(new Map());
const lockProbeBusyId = ref<number | null>(null);
const lockProbeErrors = ref<Map<number, string>>(new Map());

type CloudPlaceholderState =
  | "local_only"
  | "online_only"
  | "always_available"
  | "other_reparse"
  | "not_found"
  | { other_error: number };

interface CloudProbe {
  path: string;
  state: CloudPlaceholderState;
  raw_attributes: number;
  probe_elapsed_ms: number;
}

const cloudProbes = ref<Map<number, CloudProbe>>(new Map());
const cloudProbeBusyId = ref<number | null>(null);

// Bölüm 9.6.5 + 15.4 — canlı tarama progress.
interface ScanProgressEvent {
  phase: string;
  visited: number;
  total_estimate: number;
  in_use: number;
  last_name: string;
  elapsed_ms: number;
}
const scanProgress = ref<ScanProgressEvent | null>(null);

// Bölüm 15.1 — sağ panel için seçili düğüm. drillInto handler'ı set eder.
const selectedNode = ref<TreeNode | null>(null);

function scoreTierLabelFor(score: number | null): string {
  if (score === null) return t("scoreTier.none");
  if (score <= 30) return t("scoreTier.danger");
  if (score <= 60) return t("scoreTier.caution");
  if (score <= 85) return t("scoreTier.likely");
  return t("scoreTier.cache");
}

async function probeCloud(node: TreeNode) {
  if (node.is_dir) return;
  const path = nodeFullPath(node);
  cloudProbeBusyId.value = node.id;
  try {
    const probe = await invoke<CloudProbe>("probe_cloud_state_cmd", { path });
    const next = new Map(cloudProbes.value);
    next.set(node.id, probe);
    cloudProbes.value = next;
  } catch (err) {
    console.warn("cloud probe error", err);
  } finally {
    cloudProbeBusyId.value = null;
  }
}

function cloudStateLabel(s: CloudPlaceholderState): string {
  // Gemini review — sabit TR metinler i18n'e taşındı.
  if (typeof s === "string") {
    switch (s) {
      case "local_only":
        return t("cloud.localOnly");
      case "online_only":
        return t("cloud.onlineOnly");
      case "always_available":
        return t("cloud.alwaysAvailable");
      case "other_reparse":
        return t("cloud.otherReparse");
      case "not_found":
        return t("cloud.notFound");
    }
  }
  return t("cloud.win32Error", { code: s.other_error });
}

function cloudStateClass(s: CloudPlaceholderState): string {
  if (typeof s === "string") {
    if (s === "online_only") return "lock-busy";
    if (s === "always_available") return "lock-warn";
    if (s === "local_only") return "lock-free";
  }
  return "lock-warn";
}

const viewMode = ref<ViewMode>("sunburst");

const permDeletePendingId = ref<number | null>(null);
const permDeletePhrase = ref<string>("");
const permDeleteSecure = ref<boolean>(false);

interface NetworkScanResult {
  unc_path: string;
  file_count: number;
  dir_count: number;
  total_bytes: number;
  bandwidth_used_bytes: number;
  elapsed_ms: number;
  partial: boolean;
}

const uncDialogOpen = ref<boolean>(false);
const uncInput = ref<string>("\\\\");
const uncBusy = ref<boolean>(false);
const uncError = ref<string | null>(null);
const uncResult = ref<NetworkScanResult | null>(null);

function closeUncDialog() {
  uncDialogOpen.value = false;
  uncError.value = null;
  uncResult.value = null;
}

async function runUncScan() {
  if (uncBusy.value) return;
  const path = uncInput.value.trim();
  if (!path.startsWith("\\\\") || path.length < 4) {
    uncError.value = "UNC formatı: \\\\sunucu\\paylaşım[\\altpath]";
    return;
  }
  uncBusy.value = true;
  uncError.value = null;
  uncResult.value = null;
  try {
    uncResult.value = await invoke<NetworkScanResult>("scan_unc_path", {
      uncPath: path,
      bandwidthCapBps: 0,
    });
  } catch (err) {
    uncError.value = formatIpcError(err);
  } finally {
    uncBusy.value = false;
  }
}
const permDeleteBusyId = ref<number | null>(null);
const permDeleteError = ref<string | null>(null);

interface PermanentDeleteResult {
  id: number;
  original_path: string;
  staged_path: string;
  size_bytes: number;
  deleted_at_unix: number;
  blake3_first4kb_hex: string | null;
  is_dir: boolean;
}

interface ExpiredItem {
  id: number;
  original_path: string;
  size_bytes: number;
  expired_at_unix: number;
  is_dir: boolean;
  age_secs: number;
}

interface CleanupReport {
  deleted: number;
  failed: number;
  total_bytes: number;
  elapsed_ms: number;
  aborted_threshold: boolean;
}

const expiredList = ref<ExpiredItem[]>([]);
const expiredCleanupBusy = ref<boolean>(false);
const expiredCleanupReport = ref<CleanupReport | null>(null);
const expiredCleanupError = ref<string | null>(null);
const expiredConfirmOpen = ref<boolean>(false);

// Bölüm 17.1 — yerel performans telemetrisi (gönderim YOK, sadece UI).
interface PerfSample {
  ts: number;
  collect_ms: number;
  build_ms: number;
  total_ms: number;
  node_count: number;
  strategy: string;
}
const perfHistory = ref<PerfSample[]>([]);
const MAX_PERF_HISTORY = 20;

function recordPerfSample(summary: ScanSummary) {
  const sample: PerfSample = {
    ts: Date.now(),
    collect_ms: summary.collect_elapsed_ms,
    build_ms: summary.build_elapsed_ms,
    total_ms: summary.collect_elapsed_ms + summary.build_elapsed_ms,
    node_count: summary.node_count,
    strategy: summary.strategy,
  };
  perfHistory.value = [sample, ...perfHistory.value].slice(0, MAX_PERF_HISTORY);
}

const perfStats = computed(() => {
  if (perfHistory.value.length === 0) return null;
  const totals = perfHistory.value.map((s) => s.total_ms);
  const sum = totals.reduce((a, b) => a + b, 0);
  return {
    count: perfHistory.value.length,
    avg: Math.round(sum / totals.length),
    min: Math.min(...totals),
    max: Math.max(...totals),
    last: totals[0],
  };
});

// Bölüm 18.3 — telemetry opt-in flag. Gerçek event endpoint YOK (v0.2);
// bu sprint sadece kullanıcı tercihini settings'e kaydeder.
const telemetryOptIn = ref<boolean>(false);

async function setTelemetryOptIn(v: boolean) {
  telemetryOptIn.value = v;
  try {
    await invoke("set_setting_cmd", {
      key: "telemetry_opt_in",
      value: v ? "1" : "0",
    });
  } catch (err) {
    console.warn("telemetry pref kaydedilemedi", err);
  }
}

async function refreshExpired() {
  try {
    expiredList.value = await invoke<ExpiredItem[]>("list_expired_staging_cmd");
  } catch (err) {
    expiredCleanupError.value = formatIpcError(err);
  }
}

async function runExpiredCleanup(force: boolean) {
  expiredCleanupBusy.value = true;
  expiredCleanupError.value = null;
  try {
    const report = await invoke<CleanupReport>("cleanup_expired_staging_cmd", {
      ratePerSec: 20,
      force,
    });
    expiredCleanupReport.value = report;
    expiredConfirmOpen.value = false;
    await refreshExpired();
    await refreshStaging();
  } catch (err) {
    expiredCleanupError.value = formatIpcError(err);
  } finally {
    expiredCleanupBusy.value = false;
  }
}

interface FileSnapshot {
  path: string;
  size_bytes: number;
  modified_unix: number;
  blake3_first4kb_hex: string | null;
  is_dir: boolean;
}

type UndoOutcome =
  | { kind: "restored"; original_path: string }
  | { kind: "idempotent"; original_path: string }
  | {
      kind: "conflict";
      original_path: string;
      staged: FileSnapshot;
      target: FileSnapshot;
    };

type UndoResolution = "overwrite" | "rename" | "keep_both" | "cancel";

interface ConflictDialogState {
  id: number;
  original_path: string;
  staged: FileSnapshot;
  target: FileSnapshot;
}

const conflictDialog = ref<ConflictDialogState | null>(null);
const conflictBusy = ref<boolean>(false);
const conflictError = ref<string | null>(null);

// Bölüm 15.1.1-15.1.2 Progressive Disclosure — Üç seviye görünüm.
// Default summary (Seviye 1+2): ana akış + drilldown. Advanced (Seviye 3):
// + tüm tanı kartları (Volume Pre-Flight, MFT Probe, Walk, raw ScanTree, DB).
const showAdvanced = ref<boolean>(false);

// Bölüm 15.3 + 37 — ilk açılış. settings.onboarding_done = "1" flag.
const onboardingVisible = ref<boolean>(false);

// Bölüm 19 — language toggle. Persisted in settings.language_preference.
async function switchLocale(next: SupportedLocale) {
  setLocale(next);
  // Gemini review — index.html lang attribute'u dil değişinde güncellenir
  // (ekran okuyucular / tarayıcı çeviri ipuçları doğru çalışır).
  document.documentElement.lang = next === "tr" ? "tr" : "en";
  try {
    await invoke("set_setting_cmd", {
      key: "language_preference",
      value: next,
    });
  } catch (err) {
    console.warn("language pref kaydedilemedi", err);
  }
}

// Bölüm 9.5 — tema sistemi (dark/light/auto). settings.theme persist edilir.
type Theme = "dark" | "light" | "auto";
const theme = ref<Theme>("auto");

function applyTheme(t: Theme) {
  const root = document.documentElement;
  if (t === "auto") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", t);
  }
}

async function cycleTheme() {
  // auto → dark → light → auto
  const order: Theme[] = ["auto", "dark", "light"];
  const idx = order.indexOf(theme.value);
  const next = order[(idx + 1) % order.length];
  theme.value = next;
  applyTheme(next);
  try {
    await invoke("set_setting_cmd", { key: "theme", value: next });
  } catch (err) {
    console.warn("theme pref kaydedilemedi", err);
  }
}

function toggleAdvanced() {
  showAdvanced.value = !showAdvanced.value;
}

// Bölüm 15.2 — Klavye-First kısayollar.
const showShortcuts = ref<boolean>(false);

function isTypingTarget(t: EventTarget | null): boolean {
  if (!t) return false;
  const el = t as HTMLElement;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    el.isContentEditable
  );
}

function navigateUp() {
  if (breadcrumb.value.length >= 2) {
    const parent = breadcrumb.value[breadcrumb.value.length - 2];
    loadWindow(parent.id);
  }
}

async function lastUndo() {
  // En son staging item'ını undo et (staged_at DESC, en üst).
  if (stagingList.value.length === 0) return;
  const top = stagingList.value[0];
  await runUndo(top.id);
}

function focusFirstFileForDelete(): TreeNode | null {
  if (!viewWindow.value) return null;
  return viewWindow.value.nodes.find((n) => !n.is_dir) ?? null;
}

function handleShortcut(e: KeyboardEvent) {
  // Gemini review — kısayollar yalnızca ilk iki dialog değil, AÇIK TÜM
  // modal/confirm'lerde bastırılır (uncDialog, expiredConfirm, shortcuts,
  // onboarding); Esc açık olanı kapatır.
  const anyModalOpen =
    conflictDialog.value !== null ||
    permDeletePendingId.value !== null ||
    uncDialogOpen.value ||
    expiredConfirmOpen.value ||
    showShortcuts.value ||
    onboardingVisible.value;
  if (anyModalOpen) {
    if (e.key === "Escape") {
      if (conflictDialog.value) {
        resolveConflict("cancel");
      } else if (permDeletePendingId.value !== null) {
        cancelPermDelete();
      } else if (uncDialogOpen.value) {
        closeUncDialog();
      } else if (expiredConfirmOpen.value) {
        expiredConfirmOpen.value = false;
      } else if (showShortcuts.value) {
        showShortcuts.value = false;
      }
      e.preventDefault();
    }
    return;
  }
  if (isTypingTarget(e.target)) return;

  // Ctrl+R — Yeniden tara
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "r") {
    e.preventDefault();
    if (!scanning.value) runFullScan();
    return;
  }
  // Ctrl+1/2/3/4/5 — Görüntü modu (Gemini review: 3D mod da
  // klavyeden seçilebilir olmalı — eskiden yalnız butonla ulaşılıyordu).
  if ((e.ctrlKey || e.metaKey) && ["1", "2", "3", "4", "5"].includes(e.key)) {
    e.preventDefault();
    const modes: ViewMode[] = [
      "sunburst",
      "treemap",
      "bubble",
      "timeline",
      "sunburst3d",
    ];
    const idx = parseInt(e.key, 10) - 1;
    if (viewWindow.value && modes[idx]) {
      viewMode.value = modes[idx];
    }
    return;
  }
  // Ctrl+Z — Son undo
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
    e.preventDefault();
    lastUndo();
    return;
  }
  // Ctrl+F — Arama (placeholder toggle, gerçek arama v0.2)
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
    e.preventDefault();
    // Sprint 3.8 — Ctrl+F IndexSearchBar input'una odaklanır.
    // (Gemini review: ölü `searchOpen` placeholder toggle'ı kaldırıldı.)
    const input = document.querySelector<HTMLInputElement>(
      ".index-search-bar input",
    );
    if (input) {
      input.focus();
      input.select();
    }
    return;
  }
  // Ctrl+? — Kısayol yardımı
  if ((e.ctrlKey || e.metaKey) && e.key === "?") {
    e.preventDefault();
    showShortcuts.value = !showShortcuts.value;
    return;
  }
  // Backspace — bir seviye yukarı (drilldown breadcrumb)
  if (e.key === "Backspace") {
    e.preventDefault();
    navigateUp();
    return;
  }
  // Delete — viewport'taki ilk dosyayı staging'e gönder
  if (e.key === "Delete") {
    const target = focusFirstFileForDelete();
    if (target) {
      e.preventDefault();
      confirmStage(target);
    }
    return;
  }
}

onMounted(() => {
  window.addEventListener("keydown", handleShortcut);
});

let trayUnlisten: UnlistenFn | null = null;
let progressUnlisten: UnlistenFn | null = null;
let trayMonitorUnlisten: UnlistenFn | null = null;

interface TrayMonitorAlert {
  drive_letter: string;
  volume_label: string;
  usage_percent: number;
  total_bytes: number;
  free_bytes: number;
}

const trayMonitorAlert = ref<TrayMonitorAlert | null>(null);
let trayAlertTimer: ReturnType<typeof setTimeout> | null = null;

function dismissTrayAlert(): void {
  trayMonitorAlert.value = null;
  if (trayAlertTimer) {
    clearTimeout(trayAlertTimer);
    trayAlertTimer = null;
  }
}

onMounted(async () => {
  // Bölüm 13.1 — tray "Tara C:" menüsünden gelen event'i yakala.
  try {
    trayUnlisten = await listen<string>("tray-scan-request", (event) => {
      const requested = (event.payload || "C").toString();
      drive.value = requested;
      if (!scanning.value) {
        runFullScan();
      }
    });
  } catch (err) {
    console.warn("tray listener kurulamadı", err);
  }
  // Bölüm 9.6.5 — canlı tarama progress event listener.
  try {
    progressUnlisten = await listen<ScanProgressEvent>(
      "scan-progress",
      (event) => {
        scanProgress.value = event.payload;
      },
    );
  } catch (err) {
    console.warn("scan-progress listener kurulamadı", err);
  }

  // Sprint 3.8 (Bölüm 5.6) — USN index baseline build arka planda.
  // Admin yoksa command "needs_admin" status döner, watcher başlatılmaz.
  // Hata kullanıcıyı bloklamaz — fire-and-forget.
  invoke("index_build", { drive: drive.value, force: false }).catch((err) => {
    console.info("[index] build atlandı:", err);
  });

  // Sprint 5.1 (Bölüm 13.2) — tray live monitor alert listener.
  try {
    trayMonitorUnlisten = await listen<TrayMonitorAlert>(
      "tray-monitor-alert",
      (event) => {
        trayMonitorAlert.value = event.payload;
        // 20 sn sonra otomatik kapat (kullanıcı zaten görür).
        if (trayAlertTimer) clearTimeout(trayAlertTimer);
        trayAlertTimer = setTimeout(() => {
          trayMonitorAlert.value = null;
        }, 20_000);
      },
    );
  } catch (err) {
    console.warn("tray-monitor listener kurulamadı", err);
  }
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleShortcut);
  if (trayUnlisten) {
    trayUnlisten();
    trayUnlisten = null;
  }
  if (progressUnlisten) {
    progressUnlisten();
    progressUnlisten = null;
  }
  if (trayMonitorUnlisten) {
    trayMonitorUnlisten();
    trayMonitorUnlisten = null;
  }
  if (trayAlertTimer) {
    clearTimeout(trayAlertTimer);
    trayAlertTimer = null;
  }
});

function formatHex(n: number): string {
  return "0x" + n.toString(16).toUpperCase().padStart(8, "0");
}

function formatBytes(b: number): string {
  if (b <= 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = b;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i >= 3 ? 2 : 0)} ${units[i]}`;
}

function volumeStatusLabel(s: VolumeStatus): string {
  // Gemini review — i18n.
  switch (s.kind) {
    case "Ready":
      return t("volumeStatus.Ready");
    case "BitLockerLocked":
      return t("volumeStatus.BitLockerLocked");
    case "BitLockerSuspended":
      return t("volumeStatus.BitLockerSuspended");
    case "Encrypted":
      return t("volumeStatus.Encrypted", { method: s.data?.method ?? "?" });
    case "NotMounted":
      return t("volumeStatus.NotMounted");
    case "AccessDenied":
      return t("volumeStatus.AccessDenied");
    case "NotFormatted":
      return t("volumeStatus.NotFormatted");
    case "UnsupportedDriveType":
      return t("volumeStatus.UnsupportedDriveType", {
        type: s.data?.drive_type ?? "?",
      });
  }
}

function statusPillClass(s: VolumeStatus): string {
  return s.kind === "Ready" ? "pill-ok" : "pill-warn";
}

function strategyLabel(s: ScanStrategy): string {
  switch (s) {
    case "MftService":
      return "MFT Service (Katman 3)";
    case "DirectRawVolume":
      return "Hızlı Mod (Katman 1)";
    case "FindFirstFileFallback":
      return "Standart Mod (Katman 2)";
  }
}

onMounted(async () => {
  try {
    info.value = await invoke<AppInfo>("get_app_info");
    privilege.value = await invoke<PrivilegeStatus>("check_privilege");
  } catch (err) {
    ipcError.value = formatIpcError(err);
  }
  try {
    dbInfo.value = await invoke<DbInfo>("get_db_info");
  } catch (err) {
    dbError.value = formatIpcError(err);
  }
  await refreshStaging();
  await refreshExpired();
  // Bölüm 18.3 — telemetry tercih
  try {
    const t = await invoke<string | null>("get_setting_cmd", {
      key: "telemetry_opt_in",
    });
    telemetryOptIn.value = t === "1";
  } catch {
    /* settings yoksa default false */
  }
  // Bölüm 19 — kayıtlı dil tercihi
  try {
    const pref = await invoke<string | null>("get_setting_cmd", {
      key: "language_preference",
    });
    if (pref === "en" || pref === "tr") {
      setLocale(pref);
      document.documentElement.lang = pref;
    }
  } catch (err) {
    console.warn("language pref okunamadı", err);
  }
  // Bölüm 9.5 — kayıtlı tema tercihi
  try {
    const t = await invoke<string | null>("get_setting_cmd", { key: "theme" });
    if (t === "dark" || t === "light" || t === "auto") {
      theme.value = t;
      applyTheme(t);
    }
  } catch {
    /* sessiz fallback — auto */
  }
  // Bölüm 15.3 — ilk açılış kontrolü.
  try {
    const done = await invoke<string | null>("get_setting_cmd", {
      key: "onboarding_done",
    });
    if (done !== "1") {
      onboardingVisible.value = true;
    }
  } catch (err) {
    // Settings okunamazsa onboarding'i atla — ilk açılışı bozma.
    console.warn("settings get error", err);
  }
});

async function finishOnboarding(mode: "fast" | "standard") {
  try {
    await invoke("set_setting_cmd", {
      key: "onboarding_done",
      value: "1",
    });
    await invoke("set_setting_cmd", {
      key: "scan_strategy_preference",
      value: mode,
    });
  } catch (err) {
    console.warn("settings set error", err);
  }
  onboardingVisible.value = false;
  // Hızlı mod → otomatik tarama başlat (Bölüm 15.3 adım 3).
  if (mode === "fast" && !scanning.value) {
    runFullScan();
  }
}

function formatIpcError(err: unknown): string {
  if (typeof err === "string") return err;
  const e = err as DspaceError;
  if (e && e.kind && e.message) return `[${e.kind}] ${e.message}`;
  return JSON.stringify(err);
}

async function runPreFlight() {
  preFlighting.value = true;
  volumeError.value = null;
  volumeInfo.value = null;
  try {
    volumeInfo.value = await invoke<VolumeInfo>("pre_flight_volume", {
      drive: drive.value,
    });
  } catch (err) {
    volumeError.value = formatIpcError(err);
  } finally {
    preFlighting.value = false;
  }
}

/// Sprint 3.8 (Bölüm 5.6) — IndexSearchBar'dan gelen tıklama. v0.1: sonucu
/// `selectedNode` benzeri minimal panele eşle (tam path varsa) + konsola log.
/// Gerçek breadcrumb navigation (scan tree node id ↔ usn file_ref eşleme)
/// v0.2'de gelecek; şu an arama → detay panel paylaşımı yeterli.
interface IndexSearchResult {
  volume_id: string;
  file_ref: number;
  parent_ref: number;
  name: string;
  full_path: string | null;
  attrs: number;
}

function onIndexResultSelect(r: IndexSearchResult) {
  console.info("[index] selected:", r.full_path ?? r.name);
  // Detay paneline minimal pseudo-node bilgisi (scan tree id'si yok).
  selectedNode.value = {
    id: -(r.file_ref >>> 0),
    parent: null,
    name: r.name,
    data_size: 0,
    aggregate_size: 0,
    is_dir: (r.attrs & 0x10) !== 0,
    score: null,
    score_rule: null,
    score_reason: r.full_path ?? null,
    modified_unix: 0,
  };
}

/// Bölüm 15.1 v0.2 — VolumeSidebar'dan gelen sürücü seçimi. Aktif tarama
/// veya kuyruktaki staging durumu varken yeni sürücü tıklamasını kabul
/// etmiyoruz (kullanıcı kafa karışıklığı). Pre-flight arka planda yenilenir.
function onSidebarDriveSelect(letter: string) {
  if (!letter) return;
  if (scanning.value || walking.value || probing.value) return;
  if (letter === drive.value) return;
  drive.value = letter;
  // Bölüm 33.2 — yeni sürücü için pre-flight (admin gerekmez).
  void runPreFlight();
}

async function runProbe() {
  probing.value = true;
  probeError.value = null;
  probe.value = null;
  try {
    probe.value = await invoke<MftProbe>("probe_volume", {
      drive: drive.value,
    });
  } catch (err) {
    probeError.value = formatIpcError(err);
  } finally {
    probing.value = false;
  }
}

async function runWalk() {
  walking.value = true;
  walkError.value = null;
  walkStats.value = null;
  try {
    walkStats.value = await invoke<MftWalkStats>("walk_volume", {
      drive: drive.value,
    });
  } catch (err) {
    walkError.value = formatIpcError(err);
  } finally {
    walking.value = false;
  }
}

/// Bölüm 9.3 LOD threshold. Çocuk sayısı + parent aggregate'e bakarak
/// "viewport'ta 1px'den küçük render edilecek" item'ları eler. 200
/// node'luk render bütçesini hedefler; aggregate'in ~0.05%'i altı
/// çocukları gizler. 0 dönerse threshold uygulanmaz.
function computeLodThreshold(
  totalChildren: number,
  parentAggregate: number,
): number {
  if (totalChildren <= 200) return 0;
  if (parentAggregate <= 0) return 0;
  // ~%0.05 = 1/2000 → 1 GB parent → 500 KB altı item'ler gizlenir.
  // Daha yoğun bucket'larda eşik artar.
  const factor = Math.max(1, Math.floor(totalChildren / 200));
  return Math.max(1024, Math.floor(parentAggregate / (2000 * factor)));
}

const lodHiddenCount = ref<number>(0);

// Gemini review — hızlı drill tıklamalarında eski yanıtların geç gelip
// yeni pencereyi ezmesini önleyen istek sıra numarası.
let windowReqSeq = 0;

async function loadWindow(parentId: number) {
  // Gemini review — eski pencereden kalan selectedNode yeni breadcrumb ile
  // yanlış yol üretip detay panelinde gösteriliyordu; parent değiştiğinde
  // temizle (drillInto'nun hedefi olan düğüm hariç — o kasıtlı seçim).
  if (selectedNode.value && selectedNode.value.id !== parentId) {
    selectedNode.value = null;
  }
  const seq = ++windowReqSeq;
  windowLoading.value = true;
  windowError.value = null;
  lodHiddenCount.value = 0;
  try {
    let w = await invoke<WindowResult>("tree_window", {
      parent: parentId,
      sort: sortKey.value,
      limit: 200,
      offset: 0,
    });
    if (seq !== windowReqSeq) return; // eski yanıt — atla
    // LOD: çocuk sayısı 200'ü geçiyorsa dinamik threshold ile yeniden sor.
    if (w.total_children > 200) {
      const threshold = computeLodThreshold(
        w.total_children,
        w.parent_aggregate_size,
      );
      if (threshold > 0) {
        const w2 = await invoke<WindowResult>("tree_window", {
          parent: parentId,
          sort: sortKey.value,
          limit: 200,
          offset: 0,
          minSizeBytes: threshold,
        });
        if (seq !== windowReqSeq) return; // eski yanıt — atla
        lodHiddenCount.value = w.total_children - w2.total_children;
        w = w2;
      }
    }
    const path = await invoke<TreeNode[]>("tree_path", { id: parentId });
    if (seq !== windowReqSeq) return; // eski yanıt — atla
    viewWindow.value = w;
    breadcrumb.value = path;
  } catch (err) {
    if (seq !== windowReqSeq) return;
    windowError.value = formatIpcError(err);
  } finally {
    if (seq === windowReqSeq) {
      windowLoading.value = false;
    }
  }
}

// Bölüm 5.5 + 33.3 + 33.4 — uyarı bayrakları (volume bağlamına göre).
const driveWarning = ref<string | null>(null);

function computeDriveWarning(info: VolumeInfo | null): string | null {
  // Gemini review — i18n.
  if (!info) return null;
  const fs = (info.file_system || "").toUpperCase();
  if (fs === "REFS") {
    return t("driveWarning.refs");
  }
  if (fs && !["NTFS", "REFS", "FAT32", "EXFAT"].includes(fs)) {
    return t("driveWarning.unknownFs", { fs });
  }
  if (info.drive_kind === "Remote") {
    return t("driveWarning.remote");
  }
  if (info.drive_kind === "Removable") {
    return t("driveWarning.removable");
  }
  if (info.drive_kind === "CdRom") {
    return t("driveWarning.cdrom");
  }
  return null;
}

async function runFullScan() {
  scanning.value = true;
  scanError.value = null;
  scanSummary.value = null;
  viewWindow.value = null;
  breadcrumb.value = [];
  driveWarning.value = null;
  scanProgress.value = null;
  selectedNode.value = null;
  // Bölüm 33.2 — taramadan önce volume preflight, file_system + drive_kind
  // bilgisinden uyarı çıkar.
  try {
    const info = await invoke<VolumeInfo>("pre_flight_volume", {
      drive: drive.value,
    });
    volumeInfo.value = info;
    driveWarning.value = computeDriveWarning(info);
  } catch (err) {
    // Preflight başarısızsa scan yine de denenir — örn. admin gerektirebilir.
    console.warn("preflight error", err);
  }
  try {
    scanSummary.value = await invoke<ScanSummary>("scan_full", {
      drive: drive.value,
    });
    recordPerfSample(scanSummary.value);
    await loadWindow(scanSummary.value.root_id);
  } catch (err) {
    const msg = formatIpcError(err);
    // Kullanıcı iptal ettiyse "hata" gibi göstermeyelim.
    if (msg.includes("scan-cancelled")) {
      scanError.value = null;
    } else {
      scanError.value = msg;
    }
  } finally {
    scanning.value = false;
  }
}

async function cancelScan() {
  try {
    await invoke("scan_cancel");
  } catch (err) {
    console.warn("scan_cancel hatası", err);
  }
}

function drillInto(node: TreeNode) {
  selectedNode.value = node;
  if (node.is_dir) {
    loadWindow(node.id);
  }
}

function goUp(id: number) {
  loadWindow(id);
}

watch(sortKey, () => {
  if (viewWindow.value) {
    loadWindow(viewWindow.value.parent_id);
  }
});

function percentOf(part: number, whole: number): string {
  if (whole <= 0) return "—";
  const pct = (part / whole) * 100;
  if (pct < 0.1) return "<0.1%";
  return `${pct.toFixed(1)}%`;
}

function scoreTierClass(score: number | null): string {
  if (score === null) return "";
  if (score <= 30) return "score-danger";
  if (score <= 60) return "score-caution";
  if (score <= 85) return "score-likely";
  return "score-cache";
}

function driveLetter(): string {
  const m = drive.value.toUpperCase().match(/[A-Z]/);
  return m ? m[0] : "C";
}

function nodeFullPath(n: TreeNode): string {
  // Gemini review — USN index arama pseudo-node'u (negatif id): gerçek tam
  // yol backend'den çözüldü (score_reason alanında taşınır); uydurma
  // "C:\\<name>" üretme — yanlış dosyaya stage/probe gönderme riski.
  if (n.id < 0) {
    return n.score_reason ?? `${driveLetter()}:\\${n.name}`;
  }
  const letter = driveLetter();
  const parts = breadcrumb.value.slice(1).map((b) => b.name);
  // Gemini review — drillInto sonrası breadcrumb zaten bu düğümü içerir;
  // tekrar push etmek "C:\\a\\b\\b" üretiyordu.
  const last = breadcrumb.value[breadcrumb.value.length - 1];
  if (!last || last.id !== n.id) {
    parts.push(n.name);
  }
  return `${letter}:\\${parts.join("\\")}`;
}

function localeTag(): string {
  return locale.value === "tr" ? "tr-TR" : "en-US";
}

function formatTime(unix: number): string {
  if (!unix) return "—";
  const d = new Date(unix * 1000);
  return d.toLocaleString(localeTag());
}

function formatNumber(n: number): string {
  return n.toLocaleString(localeTag());
}

async function refreshStaging() {
  try {
    stagingList.value = await invoke<StagedItem[]>("list_staging");
  } catch (err) {
    stagingError.value = formatIpcError(err);
  }
}

async function confirmStage(node: TreeNode) {
  const path = nodeFullPath(node);
  if (stagePendingPath.value === path) {
    // Confirmed
    stagePendingPath.value = null;
    stagingBusyId.value = node.id;
    stagingError.value = null;
    try {
      await invoke<StagedItem>("stage_path", { path });
      await refreshStaging();
      if (viewWindow.value) {
        await loadWindow(viewWindow.value.parent_id);
      }
    } catch (err) {
      stagingError.value = formatIpcError(err);
    } finally {
      stagingBusyId.value = null;
    }
  } else {
    stagePendingPath.value = path;
  }
}

function cancelStage() {
  stagePendingPath.value = null;
}

async function runUndo(id: number) {
  stagingBusyId.value = id;
  stagingError.value = null;
  try {
    const outcome = await invoke<UndoOutcome>("undo_staging", { id });
    if (outcome.kind === "conflict") {
      conflictDialog.value = {
        id,
        original_path: outcome.original_path,
        staged: outcome.staged,
        target: outcome.target,
      };
      conflictError.value = null;
    } else {
      await refreshStaging();
      if (viewWindow.value) {
        await loadWindow(viewWindow.value.parent_id);
      }
    }
  } catch (err) {
    stagingError.value = formatIpcError(err);
  } finally {
    stagingBusyId.value = null;
  }
}

async function resolveConflict(resolution: UndoResolution) {
  if (!conflictDialog.value) return;
  if (resolution === "cancel") {
    conflictDialog.value = null;
    conflictError.value = null;
    return;
  }
  conflictBusy.value = true;
  conflictError.value = null;
  try {
    await invoke<UndoOutcome>("undo_resolve_staging", {
      id: conflictDialog.value.id,
      resolution,
    });
    conflictDialog.value = null;
    await refreshStaging();
    if (viewWindow.value) {
      await loadWindow(viewWindow.value.parent_id);
    }
  } catch (err) {
    conflictError.value = formatIpcError(err);
  } finally {
    conflictBusy.value = false;
  }
}

function scoreTierLabel(score: number | null): string {
  // Gemini review — i18n versiyonuna delege (iki ayrı impl aynı etiketi
  // farklı dillerde basıyordu).
  return scoreTierLabelFor(score);
}

function lockStateLabel(s: LockState): string {
  // Gemini review — i18n.
  if (typeof s === "string") {
    switch (s) {
      case "free":
        return t("lock.stateFree");
      case "locked":
        return t("lock.stateLocked");
      case "access_denied":
        return t("lock.stateAccessDenied");
      case "not_found":
        return t("lock.stateNotFound");
    }
  }
  return t("lock.stateOtherError", { code: s.other_error });
}

function lockStateClass(s: LockState): string {
  if (typeof s === "string") {
    if (s === "free") return "lock-free";
    if (s === "locked") return "lock-busy";
  }
  return "lock-warn";
}

function lockActionLabel(a: LockedFileAction): string {
  // Gemini review — i18n.
  switch (a) {
    case "proceed":
      return t("lock.actionProceed");
    case "skip_content":
      return t("lock.actionSkip");
    case "snapshot_read":
      return t("lock.actionSnapshot");
    case "user_drill_down":
      return t("lock.actionDrillDown");
    case "hard_pass":
      return t("lock.actionHardPass");
  }
}

async function probeLock(node: TreeNode) {
  if (node.is_dir) return;
  const path = nodeFullPath(node);
  lockProbeBusyId.value = node.id;
  const nextErrs = new Map(lockProbeErrors.value);
  nextErrs.delete(node.id);
  lockProbeErrors.value = nextErrs;
  try {
    const probe = await invoke<LockedFileProbe>("probe_locked_file_cmd", {
      path,
    });
    const next = new Map(lockProbes.value);
    next.set(node.id, probe);
    lockProbes.value = next;
  } catch (err) {
    const errs = new Map(lockProbeErrors.value);
    errs.set(node.id, formatIpcError(err));
    lockProbeErrors.value = errs;
  } finally {
    lockProbeBusyId.value = null;
  }
}

function closeLockProbe(id: number) {
  const next = new Map(lockProbes.value);
  next.delete(id);
  lockProbes.value = next;
  const errs = new Map(lockProbeErrors.value);
  errs.delete(id);
  lockProbeErrors.value = errs;
}

function fileNameOf(p: string): string {
  const ix = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return ix >= 0 ? p.slice(ix + 1) : p;
}

function startPermDelete(item: StagedItem) {
  permDeletePendingId.value = item.id;
  permDeletePhrase.value = "";
  permDeleteError.value = null;
}

function cancelPermDelete() {
  permDeletePendingId.value = null;
  permDeletePhrase.value = "";
  permDeleteSecure.value = false;
  permDeleteError.value = null;
}

async function confirmPermDelete(item: StagedItem) {
  if (permDeletePendingId.value !== item.id) return;
  permDeleteBusyId.value = item.id;
  permDeleteError.value = null;
  try {
    await invoke<PermanentDeleteResult>("permanent_delete_cmd", {
      id: item.id,
      confirmPhrase: permDeletePhrase.value,
      secureWipe: permDeleteSecure.value,
    });
    permDeletePendingId.value = null;
    permDeletePhrase.value = "";
    permDeleteSecure.value = false;
    await refreshStaging();
  } catch (err) {
    permDeleteError.value = formatIpcError(err);
  } finally {
    permDeleteBusyId.value = null;
  }
}
</script>

<template>
  <div class="app-frame">
    <IndexSearchBar
      class="app-search"
      @update:selected-result="onIndexResultSelect"
    />
    <VolumeSidebar
      class="app-sidebar"
      :selected="drive"
      @update:selected="onSidebarDriveSelect"
    />
    <main class="shell">
    <header class="hero">
      <div class="brand">
        <span class="logo-dot"></span>
        <h1>{{ info?.name ?? "D-Space" }}</h1>
        <button
          type="button"
          class="adv-toggle"
          :class="{ 'adv-toggle-active': showAdvanced }"
          @click="toggleAdvanced"
        >
          {{ showAdvanced ? t("header.advancedOn") : t("header.advancedOff") }}
        </button>
        <button
          type="button"
          class="adv-toggle"
          :class="{ 'adv-toggle-active': showShortcuts }"
          @click="showShortcuts = !showShortcuts"
        >
          {{ t("header.shortcuts") }}
        </button>
        <button
          type="button"
          class="adv-toggle"
          :class="{ 'adv-toggle-active': locale === 'tr' }"
          @click="switchLocale(locale === 'tr' ? 'en' : 'tr')"
          :title="locale === 'tr' ? 'Switch to English' : 'Türkçe'"
        >
          🌐 {{ locale === "tr" ? "TR" : "EN" }}
        </button>
        <button
          type="button"
          class="adv-toggle"
          :class="{ 'adv-toggle-active': theme !== 'auto' }"
          :title="
            theme === 'auto'
              ? 'Sistem teması · tıkla: koyu'
              : theme === 'dark'
                ? 'Koyu tema · tıkla: açık'
                : 'Açık tema · tıkla: sistem'
          "
          @click="cycleTheme"
        >
          {{
            theme === "auto"
              ? t("header.themeAuto")
              : theme === "dark"
                ? t("header.themeDark")
                : t("header.themeLight")
          }}
        </button>
        <UpdateNotification />
      </div>
      <p class="tagline">{{ t("app.tagline") }}</p>

      <!-- Bölüm 15.1.3 Badge group: summary metrikler (scan_summary doluysa) -->
      <div v-if="scanSummary" class="badge-group">
        <span
          class="badge badge-blue"
          :title="
            t('badge.totalTitle', { total: formatBytes(scanSummary.total_bytes) })
          "
        >
          📊 {{ formatBytes(scanSummary.total_bytes) }}
        </span>
        <span
          class="badge badge-blue"
          :title="
            t('badge.dirsFilesTitle', {
              dirs: formatNumber(scanSummary.dir_count),
              files: formatNumber(scanSummary.file_count),
            })
          "
        >
          📁 {{ formatNumber(scanSummary.dir_count) }} ·
          📄 {{ formatNumber(scanSummary.file_count) }}
        </span>
        <span
          v-if="walkStats && walkStats.skipped_errors > 0"
          class="badge badge-warn"
        >
          ⚠ {{ t("badge.skipped", { count: walkStats.skipped_errors }) }}
        </span>
        <span v-if="stagingList.length > 0" class="badge badge-amber">
          📥 {{ t("badge.staging", { count: stagingList.length }) }}
        </span>
        <span v-if="dbInfo" class="badge badge-ghost">
          ℹ {{ t("badge.dbSchema", { version: dbInfo.schema_version }) }}
        </span>
      </div>
    </header>

    <section v-if="showAdvanced" class="card">
      <h2>Durum</h2>
      <div v-if="info" class="grid">
        <div class="row">
          <span class="key">Sürüm</span>
          <span class="val mono">v{{ info.version }}</span>
        </div>
        <div class="row">
          <span class="key">Spec</span>
          <span class="val mono">{{ info.spec_version }}</span>
          <span class="pill pill-frozen">{{ info.spec_status }}</span>
        </div>
        <div class="row">
          <span class="key">Lisans</span>
          <span class="val mono">{{ info.license }}</span>
        </div>
        <div class="row">
          <span class="key">Platform</span>
          <span class="val mono">{{ info.platform }}</span>
        </div>
      </div>
      <p v-else-if="ipcError" class="err">IPC hatası: {{ ipcError }}</p>
      <p v-else class="muted">Sürüm bilgisi alınıyor…</p>
    </section>

    <section v-if="showAdvanced" class="card">
      <h2>Yetki Durumu</h2>
      <div v-if="privilege" class="grid">
        <div class="row">
          <span class="key">Elevation</span>
          <span class="val mono">{{ privilege.elevated ? "evet" : "hayır" }}</span>
          <span class="pill" :class="privilege.elevated ? 'pill-ok' : 'pill-warn'">
            {{ privilege.elevated ? "admin" : "standart" }}
          </span>
        </div>
        <div class="row">
          <span class="key">Strateji</span>
          <span class="val">{{ strategyLabel(privilege.strategy) }}</span>
        </div>
      </div>
    </section>

    <section v-if="showAdvanced" class="card">
      <h2>Volume Pre-Flight (Bölüm 33.2 Katman 0)</h2>
      <div class="probe-bar">
        <input
          v-model="drive"
          maxlength="3"
          spellcheck="false"
          class="drive-input mono"
          aria-label="Sürücü harfi"
        />
        <button
          type="button"
          class="probe-btn"
          :disabled="preFlighting"
          @click="runPreFlight"
        >
          {{ preFlighting ? "Sorgulanıyor…" : "Pre-flight" }}
        </button>
      </div>
      <div v-if="volumeInfo" class="grid">
        <div class="row">
          <span class="key">Durum</span>
          <span class="val">{{ volumeStatusLabel(volumeInfo.status) }}</span>
          <span class="pill" :class="statusPillClass(volumeInfo.status)">
            {{ volumeInfo.status.kind }}
          </span>
        </div>
        <div class="row">
          <span class="key">Sürücü</span>
          <span class="val mono">{{ volumeInfo.root_path }}</span>
        </div>
        <div class="row">
          <span class="key">FS</span>
          <span class="val mono">{{ volumeInfo.file_system || "—" }}</span>
        </div>
        <div class="row">
          <span class="key">Etiket</span>
          <span class="val">{{ volumeInfo.volume_label || "—" }}</span>
        </div>
        <div class="row">
          <span class="key">Serial</span>
          <span class="val mono">{{ formatHex(volumeInfo.volume_serial) }}</span>
        </div>
        <div class="row">
          <span class="key">Tip</span>
          <span class="val">{{ volumeInfo.drive_kind }}</span>
        </div>
        <div class="row">
          <span class="key">Toplam</span>
          <span class="val mono">{{ formatBytes(volumeInfo.total_bytes) }}</span>
        </div>
        <div class="row">
          <span class="key">Boş</span>
          <span class="val mono">{{ formatBytes(volumeInfo.free_bytes) }}</span>
        </div>
        <div class="row">
          <span class="key">Süre</span>
          <span class="val mono">{{ volumeInfo.elapsed_ms }} ms</span>
        </div>
      </div>
      <p v-if="volumeError" class="err">{{ volumeError }}</p>
    </section>

    <section v-if="showAdvanced" class="card">
      <h2>MFT Probe (Bölüm 5)</h2>
      <div class="probe-bar">
        <input
          v-model="drive"
          maxlength="3"
          spellcheck="false"
          class="drive-input mono"
          aria-label="Sürücü harfi"
        />
        <button
          type="button"
          class="probe-btn"
          :disabled="probing"
          @click="runProbe"
        >
          {{ probing ? "Probe çalışıyor…" : "Probe çalıştır" }}
        </button>
      </div>
      <div v-if="probe" class="grid">
        <div class="row">
          <span class="key">Volume</span>
          <span class="val mono">{{ probe.volume_path }}</span>
        </div>
        <div class="row">
          <span class="key">Serial</span>
          <span class="val mono">{{ formatHex(probe.volume_serial) }}</span>
        </div>
        <div class="row">
          <span class="key">Cluster</span>
          <span class="val mono">{{ probe.cluster_size }} B</span>
        </div>
        <div class="row">
          <span class="key">Sector</span>
          <span class="val mono">{{ probe.sector_size }} B</span>
        </div>
        <div class="row">
          <span class="key">MFT rec.</span>
          <span class="val mono">{{ probe.file_record_size }} B</span>
        </div>
        <div class="row">
          <span class="key">Süre</span>
          <span class="val mono">{{ probe.elapsed_ms }} ms</span>
        </div>
      </div>
      <p v-if="probeError" class="err">{{ probeError }}</p>
    </section>

    <section v-if="showAdvanced" class="card">
      <h2>MFT Full Walk (Bölüm 5.1 + 4.3 Adım 2)</h2>
      <div class="probe-bar">
        <button
          type="button"
          class="probe-btn"
          :disabled="walking"
          @click="runWalk"
        >
          {{ walking ? `MFT taranıyor… (${drive})` : `MFT walk: ${drive}` }}
        </button>
      </div>
      <div v-if="walkStats" class="grid">
        <div class="row">
          <span class="key">Tahmin</span>
          <span class="val mono">
            {{ walkStats.total_records_estimate.toLocaleString(localeTag()) }} record
          </span>
        </div>
        <div class="row">
          <span class="key">Gezildi</span>
          <span class="val mono">
            {{ walkStats.records_visited.toLocaleString(localeTag()) }}
          </span>
          <span class="pill pill-ok">
            {{ walkStats.in_use_records.toLocaleString(localeTag()) }} in-use
          </span>
        </div>
        <div class="row">
          <span class="key">Klasör</span>
          <span class="val mono">
            {{ walkStats.directory_records.toLocaleString(localeTag()) }}
          </span>
        </div>
        <div class="row">
          <span class="key">Dosya</span>
          <span class="val mono">
            {{ walkStats.file_records.toLocaleString(localeTag()) }}
          </span>
        </div>
        <div class="row">
          <span class="key">Toplam</span>
          <span class="val mono">{{ formatBytes(walkStats.bytes_aggregate) }}</span>
        </div>
        <div class="row">
          <span class="key">Atlanan</span>
          <span class="val mono">{{ walkStats.skipped_errors }}</span>
        </div>
        <div class="row">
          <span class="key">Süre</span>
          <span class="val mono">{{ walkStats.elapsed_ms }} ms</span>
        </div>
        <div v-if="walkStats.sample_names.length" class="samples">
          <div class="samples-title">Örnek isimler ({{ walkStats.sample_names.length }})</div>
          <ul class="samples-list mono">
            <li v-for="(n, i) in walkStats.sample_names" :key="i">{{ n }}</li>
          </ul>
        </div>
      </div>
      <p v-if="walkError" class="err">{{ walkError }}</p>
    </section>

    <section class="card">
      <h2>
        {{ t("scan.title") }}
        <InfoButton :text="t('scan.intro')" />
      </h2>
      <div class="probe-bar">
        <input
          v-model="drive"
          maxlength="3"
          spellcheck="false"
          class="drive-input mono"
          :aria-label="t('scan.driveAria')"
        />
        <button
          type="button"
          class="probe-btn"
          :disabled="scanning"
          @click="runFullScan"
        >
          {{
            scanning
              ? t("scan.buttonBusy", { drive })
              : t("scan.buttonIdle", { drive })
          }}
        </button>
        <button
          type="button"
          class="probe-btn unc-btn"
          :disabled="scanning || uncBusy"
          :title="t('scan.uncTitle')"
          @click="uncDialogOpen = true"
        >
          {{ uncBusy ? t("scan.uncScanning") : t("scan.uncBtn") }}
        </button>
        <span v-if="scanSummary" class="scan-quick mono">
          {{ strategyLabel(scanSummary.strategy) }} ·
          {{ scanSummary.collect_elapsed_ms + scanSummary.build_elapsed_ms }} ms
        </span>
      </div>
      <div v-if="driveWarning" class="drive-warning">
        ⚠ {{ driveWarning }}
      </div>
      <div v-if="scanSummary && showAdvanced" class="grid">
        <div class="row">
          <span class="key">Strateji</span>
          <span class="val">{{ strategyLabel(scanSummary.strategy) }}</span>
          <span
            class="pill"
            :class="
              scanSummary.strategy === 'DirectRawVolume'
                ? 'pill-ok'
                : 'pill-warn'
            "
          >
            {{ scanSummary.strategy === "DirectRawVolume" ? "MFT" : "FALLBACK" }}
          </span>
        </div>
        <div class="row">
          <span class="key">Düğüm</span>
          <span class="val mono">
            {{ scanSummary.node_count.toLocaleString(localeTag()) }}
          </span>
        </div>
        <div class="row">
          <span class="key">Dosya</span>
          <span class="val mono">
            {{ scanSummary.file_count.toLocaleString(localeTag()) }}
          </span>
        </div>
        <div class="row">
          <span class="key">Klasör</span>
          <span class="val mono">
            {{ scanSummary.dir_count.toLocaleString(localeTag()) }}
          </span>
        </div>
        <div class="row">
          <span class="key">Toplam</span>
          <span class="val mono">{{ formatBytes(scanSummary.total_bytes) }}</span>
        </div>
        <div class="row">
          <span class="key">Toplama</span>
          <span class="val mono">{{ scanSummary.collect_elapsed_ms }} ms</span>
        </div>
        <div class="row">
          <span class="key">Ağaç build</span>
          <span class="val mono">{{ scanSummary.build_elapsed_ms }} ms</span>
        </div>
      </div>
      <p v-if="scanError" class="err">{{ scanError }}</p>
    </section>

    <section v-if="viewWindow" class="card">
      <h2>
        {{ t("drilldown.title") }}
        <InfoButton :text="t('drilldown.intro')" />
      </h2>

      <div class="view-mode-bar">
        <span class="view-mode-label">{{ t("drilldown.viewMode") }}</span>
        <button
          type="button"
          class="view-chip"
          :class="{ 'view-chip-active': viewMode === 'sunburst' }"
          @click="viewMode = 'sunburst'"
        >
          Sunburst
        </button>
        <button
          type="button"
          class="view-chip"
          :class="{ 'view-chip-active': viewMode === 'sunburst3d' }"
          @click="viewMode = 'sunburst3d'"
          :title="t('drilldown.mode3DTitle')"
        >
          🎚 3D
        </button>
        <button
          type="button"
          class="view-chip"
          :class="{ 'view-chip-active': viewMode === 'treemap' }"
          @click="viewMode = 'treemap'"
        >
          Treemap
        </button>
        <button
          type="button"
          class="view-chip"
          :class="{ 'view-chip-active': viewMode === 'bubble' }"
          @click="viewMode = 'bubble'"
        >
          Bubble
        </button>
        <button
          type="button"
          class="view-chip"
          :class="{ 'view-chip-active': viewMode === 'timeline' }"
          @click="viewMode = 'timeline'"
        >
          Timeline
        </button>
      </div>

      <Transition name="view-swap" mode="out-in">
        <Sunburst
          v-if="viewMode === 'sunburst'"
          key="sunburst"
          :data="viewWindow"
          @drill="drillInto"
        />
        <Sunburst3D
          v-else-if="viewMode === 'sunburst3d'"
          key="sunburst3d"
          :data="viewWindow"
          @drill="drillInto"
        />
        <Treemap
          v-else-if="viewMode === 'treemap'"
          key="treemap"
          :data="viewWindow"
          @drill="drillInto"
        />
        <Bubble
          v-else-if="viewMode === 'bubble'"
          key="bubble"
          :data="viewWindow"
          @drill="drillInto"
        />
        <Timeline
          v-else-if="viewMode === 'timeline'"
          key="timeline"
          :data="viewWindow"
          @drill="drillInto"
        />
      </Transition>

      <nav class="crumbs">
        <template v-for="(c, i) in breadcrumb" :key="c.id">
          <button
            type="button"
            class="crumb"
            :class="{ 'crumb-current': i === breadcrumb.length - 1 }"
            :disabled="i === breadcrumb.length - 1"
            @click="goUp(c.id)"
          >
            {{ c.name }}
          </button>
          <span v-if="i < breadcrumb.length - 1" class="crumb-sep">›</span>
        </template>
      </nav>

      <div class="drill-bar">
        <label class="sort-label">
          {{ t("drilldown.sortLabel") }}
          <select v-model="sortKey" class="sort-select">
            <option value="size_desc">{{ t("drilldown.sortSizeDesc") }}</option>
            <option value="name_asc">{{ t("drilldown.sortNameAsc") }}</option>
            <option value="data_size_desc">
              {{ t("drilldown.sortDataDesc") }}
            </option>
          </select>
        </label>
        <span class="drill-stats mono">
          {{
            t("drilldown.stats", {
              returned: viewWindow.returned,
              total: viewWindow.total_children,
              bytes: formatBytes(viewWindow.parent_aggregate_size),
            })
          }}
        </span>
        <span
          v-if="lodHiddenCount > 0"
          class="lod-badge"
          :title="t('drilldown.lodHidden', { count: lodHiddenCount })"
        >
          🔍 LOD · −{{ lodHiddenCount }}
        </span>
      </div>

      <ul class="drill-list">
        <li
          v-for="n in viewWindow.nodes"
          :key="n.id"
          class="drill-row"
          :class="{ 'drill-dir': n.is_dir, 'drill-file': !n.is_dir }"
          @click="drillInto(n)"
        >
          <span class="drill-icon">{{ n.is_dir ? "📁" : "📄" }}</span>
          <span class="drill-name mono">{{ n.name }}</span>
          <span
            v-if="n.score !== null"
            class="score-pill"
            :class="scoreTierClass(n.score)"
            :title="n.score_reason ?? ''"
          >
            {{ n.score }} · {{ scoreTierLabel(n.score) }}
          </span>
          <span v-else class="score-pill score-none">—</span>
          <span class="drill-bar-inner">
            <span
              class="drill-fill"
              :style="{
                width: percentOf(n.aggregate_size, viewWindow.parent_aggregate_size),
              }"
            ></span>
          </span>
          <span class="drill-pct mono">
            {{ percentOf(n.aggregate_size, viewWindow.parent_aggregate_size) }}
          </span>
          <span class="drill-size mono">
            {{ formatBytes(n.aggregate_size) }}
          </span>
          <span class="drill-actions" @click.stop>
            <button
              v-if="!n.is_dir"
              type="button"
              class="stage-btn lock-probe-btn"
              :disabled="lockProbeBusyId === n.id"
              :title="t('drilldown.lockProbe')"
              @click="probeLock(n)"
            >
              {{ lockProbeBusyId === n.id ? "…" : "🔒" }}
            </button>
            <button
              v-if="!n.is_dir"
              type="button"
              class="stage-btn lock-probe-btn"
              :disabled="cloudProbeBusyId === n.id"
              :title="t('detail.cloudProbe')"
              @click="probeCloud(n)"
            >
              {{ cloudProbeBusyId === n.id ? "…" : "☁" }}
            </button>
            <template v-if="stagePendingPath === nodeFullPath(n)">
              <button
                type="button"
                class="stage-btn stage-confirm"
                :disabled="stagingBusyId === n.id"
                @click="confirmStage(n)"
              >
                ✓ {{ t("staging.confirm") }}
              </button>
              <button
                type="button"
                class="stage-btn stage-cancel"
                @click="cancelStage"
              >
                ✕
              </button>
            </template>
            <button
              v-else
              type="button"
              class="stage-btn"
              :disabled="stagingBusyId === n.id"
              :title="t('drilldown.staging')"
              @click="confirmStage(n)"
            >
              📥
            </button>
          </span>
          <div
            v-if="cloudProbes.get(n.id)"
            class="lock-detail"
            @click.stop
          >
            <div class="lock-detail-head">
              <span
                class="lock-pill"
                :class="cloudStateClass(cloudProbes.get(n.id)!.state)"
              >
                {{ cloudStateLabel(cloudProbes.get(n.id)!.state) }}
              </span>
              <span class="lock-action mono">
                attr: 0x{{ cloudProbes.get(n.id)!.raw_attributes.toString(16).padStart(8, "0").toUpperCase() }}
              </span>
              <span class="lock-elapsed mono">
                {{ cloudProbes.get(n.id)!.probe_elapsed_ms }} ms
              </span>
              <button
                type="button"
                class="lock-close"
                @click="cloudProbes.delete(n.id); cloudProbes = new Map(cloudProbes)"
              >
                ✕
              </button>
            </div>
          </div>
          <div
            v-if="lockProbes.get(n.id) || lockProbeErrors.get(n.id)"
            class="lock-detail"
            @click.stop
          >
            <template v-if="lockProbes.get(n.id)">
              <div class="lock-detail-head">
                <span
                  class="lock-pill"
                  :class="lockStateClass(lockProbes.get(n.id)!.state)"
                >
                  {{ lockStateLabel(lockProbes.get(n.id)!.state) }}
                </span>
                <span class="lock-action mono">
                  → {{ lockActionLabel(lockProbes.get(n.id)!.recommended_action) }}
                </span>
                <span class="lock-elapsed mono">
                  {{ lockProbes.get(n.id)!.probe_elapsed_ms }} ms
                </span>
                <button
                  type="button"
                  class="lock-close"
                  @click="closeLockProbe(n.id)"
                >
                  ✕
                </button>
              </div>
              <ul
                v-if="lockProbes.get(n.id)!.owners.length"
                class="lock-owners"
              >
                <li
                  v-for="o in lockProbes.get(n.id)!.owners"
                  :key="o.pid"
                  class="lock-owner-row"
                >
                  <span class="lock-pid mono">PID {{ o.pid }}</span>
                  <span class="lock-proc">{{ o.process_name }}</span>
                  <span v-if="o.service_short_name" class="lock-svc mono">
                    svc: {{ o.service_short_name }}
                  </span>
                  <span v-if="o.restartable" class="lock-restartable">
                    restart-safe
                  </span>
                </li>
              </ul>
              <p
                v-else-if="
                  typeof lockProbes.get(n.id)!.state === 'string' &&
                  lockProbes.get(n.id)!.state === 'locked'
                "
                class="muted lock-empty"
              >
                {{ t("lock.ownerEmpty") }}
              </p>
            </template>
            <p v-if="lockProbeErrors.get(n.id)" class="err lock-err">
              {{ lockProbeErrors.get(n.id) }}
            </p>
          </div>
        </li>
        <li v-if="windowLoading" class="drill-loading">{{ t("drilldown.loading") }}</li>
      </ul>
      <p v-if="windowError" class="err">{{ windowError }}</p>
    </section>

    <section class="card">
      <h2>
        {{ t("staging.title") }}
        <InfoButton :text="t('staging.intro')" />
      </h2>
      <p class="muted empty-shimmer" v-if="stagingList.length === 0 && !stagingError">
        {{ t("staging.empty") }}
      </p>
      <transition-group
        v-if="stagingList.length"
        tag="ul"
        name="staging-row"
        class="staging-list"
      >
        <li v-for="item in stagingList" :key="item.id" class="staging-row-wrap">
          <div class="staging-row">
            <span class="staging-icon">{{ item.is_dir ? "📁" : "📄" }}</span>
            <span class="staging-path mono">{{ item.original_path }}</span>
            <span
              class="tier-pill"
              :class="
                item.fallback_tier === 'cross_volume'
                  ? 'tier-cross'
                  : 'tier-normal'
              "
              :title="
                item.fallback_tier === 'cross_volume'
                  ? t('staging.tierCrossTitle')
                  : t('staging.tierNormalTitle')
              "
            >
              {{
                item.fallback_tier === "cross_volume"
                  ? t("staging.tierCross")
                  : t("staging.tierNormal")
              }}
            </span>
            <span class="staging-size mono">{{ formatBytes(item.size_bytes) }}</span>
            <span class="staging-time mono">{{ formatTime(item.staged_at_unix) }}</span>
            <button
              type="button"
              class="stage-btn"
              :disabled="stagingBusyId === item.id"
              :title="t('staging.undoTitle')"
              @click="runUndo(item.id)"
            >
              {{ t("staging.undo") }}
            </button>
            <button
              type="button"
              class="stage-btn perm-trigger"
              :disabled="permDeleteBusyId === item.id"
              :title="t('staging.permTitle')"
              @click="startPermDelete(item)"
            >
              🔥
            </button>
          </div>
          <div
            v-if="permDeletePendingId === item.id"
            class="perm-confirm"
            @click.stop
          >
            <div class="perm-warn">
              {{ t("staging.permWarn") }}
              <code class="mono">{{ fileNameOf(item.original_path) }}</code>
            </div>
            <div class="perm-row">
              <input
                v-model="permDeletePhrase"
                type="text"
                spellcheck="false"
                class="perm-input mono"
                :placeholder="fileNameOf(item.original_path)"
                @keyup.enter="confirmPermDelete(item)"
                @keyup.esc="cancelPermDelete"
              />
              <button
                type="button"
                class="stage-btn perm-go"
                :disabled="
                  permDeleteBusyId === item.id ||
                  !permDeletePhrase.trim()
                "
                @click="confirmPermDelete(item)"
              >
                {{
                  permDeleteBusyId === item.id
                    ? t("staging.permBusy")
                    : t("staging.permGo")
                }}
              </button>
              <button
                type="button"
                class="stage-btn perm-cancel"
                @click="cancelPermDelete"
              >
                {{ t("staging.permCancel") }}
              </button>
            </div>
            <label v-if="!item.is_dir" class="perm-wipe-row">
              <input
                v-model="permDeleteSecure"
                type="checkbox"
                :disabled="permDeleteBusyId === item.id"
              />
              <span class="perm-wipe-label">
                🔐 {{ t("staging.secureWipe") }}
              </span>
              <span class="perm-wipe-hint">{{ t("staging.secureWipeHint") }}</span>
            </label>
            <p v-if="permDeleteError" class="err perm-err">
              {{ permDeleteError }}
            </p>
          </div>
        </li>
      </transition-group>
      <p v-if="stagingError" class="err">{{ stagingError }}</p>
    </section>

    <div
      v-if="showShortcuts"
      class="modal-backdrop"
      @click.self="showShortcuts = false"
    >
      <div class="shortcuts-dialog">
        <h3 class="conflict-title">{{ t("shortcuts.title") }}</h3>
        <table class="shortcut-table">
          <tbody>
            <tr>
              <td><kbd>Ctrl</kbd>+<kbd>R</kbd></td>
              <td>{{ t("shortcuts.rescan") }}</td>
            </tr>
            <tr>
              <td><kbd>Ctrl</kbd>+<kbd>1</kbd>–<kbd>5</kbd></td>
              <td>{{ t("shortcuts.viewModes") }}</td>
            </tr>
            <tr>
              <td><kbd>Backspace</kbd></td>
              <td>{{ t("shortcuts.navUp") }}</td>
            </tr>
            <tr>
              <td><kbd>Delete</kbd></td>
              <td>{{ t("shortcuts.stageDelete") }}</td>
            </tr>
            <tr>
              <td><kbd>Ctrl</kbd>+<kbd>Z</kbd></td>
              <td>{{ t("shortcuts.undo") }}</td>
            </tr>
            <tr>
              <td><kbd>Ctrl</kbd>+<kbd>F</kbd></td>
              <td>{{ t("shortcuts.search") }}</td>
            </tr>
            <tr>
              <td><kbd>Esc</kbd></td>
              <td>{{ t("shortcuts.escape") }}</td>
            </tr>
            <tr>
              <td><kbd>Ctrl</kbd>+<kbd>?</kbd></td>
              <td>{{ t("shortcuts.helpToggle") }}</td>
            </tr>
          </tbody>
        </table>
        <p class="shortcut-hint">{{ t("shortcuts.hint") }}</p>
        <button
          type="button"
          class="conflict-btn conflict-cancel"
          @click="showShortcuts = false"
        >
          {{ t("shortcuts.close") }}
        </button>
      </div>
    </div>

    <div
      v-if="conflictDialog"
      class="modal-backdrop"
      @click.self="resolveConflict('cancel')"
    >
      <div class="conflict-dialog">
        <h3 class="conflict-title">
          {{ t("conflict.title", { name: fileNameOf(conflictDialog.original_path) }) }}
        </h3>
        <p class="conflict-help">{{ t("conflict.help") }}</p>

        <div class="conflict-cols">
          <div class="conflict-col">
            <div class="conflict-col-head">{{ t("conflict.colStaged") }}</div>
            <div class="conflict-meta mono">
              <div>📄 {{ formatBytes(conflictDialog.staged.size_bytes) }}</div>
              <div>{{ formatTime(conflictDialog.staged.modified_unix) }}</div>
              <div
                v-if="conflictDialog.staged.blake3_first4kb_hex"
                class="hash-hint"
                :title="conflictDialog.staged.blake3_first4kb_hex"
              >
                {{ conflictDialog.staged.blake3_first4kb_hex.slice(0, 12) }}…
              </div>
            </div>
          </div>
          <div class="conflict-vs">vs</div>
          <div class="conflict-col">
            <div class="conflict-col-head">{{ t("conflict.colTarget") }}</div>
            <div class="conflict-meta mono">
              <div>📄 {{ formatBytes(conflictDialog.target.size_bytes) }}</div>
              <div>{{ formatTime(conflictDialog.target.modified_unix) }}</div>
              <div
                v-if="conflictDialog.target.blake3_first4kb_hex"
                class="hash-hint"
                :title="conflictDialog.target.blake3_first4kb_hex"
              >
                {{ conflictDialog.target.blake3_first4kb_hex.slice(0, 12) }}…
              </div>
            </div>
          </div>
        </div>

        <div class="conflict-actions">
          <button
            type="button"
            class="conflict-btn conflict-overwrite"
            :disabled="conflictBusy"
            @click="resolveConflict('overwrite')"
          >
            {{ t("conflict.overwrite") }}
          </button>
          <button
            type="button"
            class="conflict-btn"
            :disabled="conflictBusy"
            @click="resolveConflict('rename')"
          >
            {{ t("conflict.rename", { name: fileNameOf(conflictDialog.original_path) }) }}
          </button>
          <button
            type="button"
            class="conflict-btn"
            :disabled="conflictBusy"
            @click="resolveConflict('keep_both')"
          >
            {{ t("conflict.keepBoth") }}
          </button>
          <button
            type="button"
            class="conflict-btn conflict-cancel"
            :disabled="conflictBusy"
            @click="resolveConflict('cancel')"
          >
            {{ t("conflict.cancel") }}
          </button>
        </div>

        <p v-if="conflictError" class="err">{{ conflictError }}</p>
      </div>
    </div>

    <Onboarding :visible="onboardingVisible" @finish="finishOnboarding" />

    <!-- Sprint 4.4 ek — UNC network paylaşımı tarama dialog. -->
    <div
      v-if="uncDialogOpen"
      class="modal-backdrop"
      @click.self="closeUncDialog"
    >
      <div class="modal unc-modal">
        <h3 class="unc-title">
          🌐 {{ t("scan.uncDialogTitle") }}
        </h3>
        <p class="unc-help">{{ t("scan.uncDialogHelp") }}</p>

        <input
          v-model="uncInput"
          type="text"
          class="unc-input mono"
          spellcheck="false"
          :placeholder="t('scan.uncPlaceholder')"
          :disabled="uncBusy"
          @keyup.enter="runUncScan"
        />

        <p v-if="uncError" class="err">{{ uncError }}</p>

        <div v-if="uncResult" class="unc-result">
          <div class="row">
            <span class="key">{{ t("scan.uncFiles") }}</span>
            <span class="val mono">{{ uncResult.file_count.toLocaleString(localeTag()) }}</span>
          </div>
          <div class="row">
            <span class="key">{{ t("scan.uncDirs") }}</span>
            <span class="val mono">{{ uncResult.dir_count.toLocaleString(localeTag()) }}</span>
          </div>
          <div class="row">
            <span class="key">{{ t("scan.uncTotalBytes") }}</span>
            <span class="val mono">{{ formatBytes(uncResult.total_bytes) }}</span>
          </div>
          <div class="row">
            <span class="key">{{ t("scan.uncBandwidth") }}</span>
            <span class="val mono">
              ~{{ formatBytes(uncResult.bandwidth_used_bytes) }}
            </span>
          </div>
          <div class="row">
            <span class="key">{{ t("scanStats.elapsed") }}</span>
            <span class="val mono">{{ uncResult.elapsed_ms }} ms</span>
          </div>
        </div>

        <div class="unc-actions">
          <button
            type="button"
            class="probe-btn"
            :disabled="uncBusy || !uncInput.trim()"
            @click="runUncScan"
          >
            {{ uncBusy ? t("scan.uncScanning") : t("scan.uncRunBtn") }}
          </button>
          <button
            type="button"
            class="stage-btn perm-cancel"
            @click="closeUncDialog"
          >
            {{ t("staging.permCancel") }}
          </button>
        </div>
      </div>
    </div>

    <!-- Sprint 5.1 (Bölüm 13.2) — Tray Live Monitor alert toast. -->
    <transition name="tray-toast">
      <div
        v-if="trayMonitorAlert"
        class="tray-toast"
        role="status"
        @click="dismissTrayAlert"
      >
        <span class="tray-toast-icon">⚠</span>
        <div class="tray-toast-body">
          <div class="tray-toast-title">
            {{
              t("trayMonitor.alert", {
                drive: trayMonitorAlert.drive_letter,
                pct: trayMonitorAlert.usage_percent,
              })
            }}
          </div>
          <div class="tray-toast-hint mono">
            {{
              t("trayMonitor.alertHint", {
                free: formatBytes(trayMonitorAlert.free_bytes),
                total: formatBytes(trayMonitorAlert.total_bytes),
              })
            }}
          </div>
        </div>
        <button
          type="button"
          class="tray-toast-close"
          :aria-label="t('staging.permCancel')"
          @click.stop="dismissTrayAlert"
        >
          ✕
        </button>
      </div>
    </transition>

    <ScanProgress
      :visible="scanning"
      :progress="scanProgress"
      @cancel="cancelScan"
    />

    <!-- Bölüm 15.1 — üç-kolon workspace (Snapshot | Duplicate | Detail).
         Geniş ekranda yan yana, dar ekranda alta yığılır. -->
    <div class="workspace">
      <aside class="col-snap">
        <SnapshotPanel />
      </aside>
      <aside class="col-dup">
        <DuplicatePanel :drive="drive" :has-scan="scanSummary !== null" />
      </aside>
      <aside class="col-detail">
        <section class="card detail-card">
          <h2>
            {{ t("detail.title") }}
            <InfoButton :text="t('detail.intro')" />
          </h2>
          <template v-if="selectedNode">
            <p class="detail-name">
              {{ selectedNode.is_dir ? "📁" : "📄" }} {{ selectedNode.name }}
            </p>
            <p class="detail-path">{{ nodeFullPath(selectedNode) }}</p>
            <div class="detail-row">
              <span class="detail-key">{{ t("detail.size") }}</span>
              <span class="detail-val mono">
                {{ formatBytes(selectedNode.aggregate_size) }}
              </span>
            </div>
            <div v-if="!selectedNode.is_dir" class="detail-row">
              <span class="detail-key">{{ t("detail.data") }}</span>
              <span class="detail-val mono">
                {{ formatBytes(selectedNode.data_size) }}
              </span>
            </div>
            <div class="detail-row">
              <span class="detail-key">{{ t("detail.modified") }}</span>
              <span class="detail-val mono">
                {{ formatTime(selectedNode.modified_unix) }}
              </span>
            </div>
            <div class="detail-row">
              <span class="detail-key">{{ t("detail.score") }}</span>
              <span class="detail-val">
                <span
                  v-if="selectedNode.score !== null"
                  class="score-pill"
                  :class="scoreTierClass(selectedNode.score)"
                >
                  {{ selectedNode.score }} ·
                  {{ scoreTierLabelFor(selectedNode.score) }}
                </span>
                <span v-else class="detail-key">—</span>
              </span>
            </div>
            <div v-if="selectedNode.score_rule" class="detail-row">
              <span class="detail-key">{{ t("detail.rule") }}</span>
              <span class="detail-val mono">{{ selectedNode.score_rule }}</span>
            </div>
            <p
              v-if="selectedNode.score_reason"
              class="detail-path"
              style="margin-top: 8px"
            >
              {{ selectedNode.score_reason }}
            </p>
            <div class="detail-actions">
              <button
                v-if="!selectedNode.is_dir"
                type="button"
                class="stage-btn lock-probe-btn"
                :disabled="lockProbeBusyId === selectedNode.id"
                @click="probeLock(selectedNode)"
              >
                {{ t("detail.lockProbe") }}
              </button>
              <button
                v-if="!selectedNode.is_dir"
                type="button"
                class="stage-btn lock-probe-btn"
                :disabled="cloudProbeBusyId === selectedNode.id"
                @click="probeCloud(selectedNode)"
              >
                {{ t("detail.cloudProbe") }}
              </button>
              <button
                type="button"
                class="stage-btn"
                @click="confirmStage(selectedNode)"
              >
                {{ t("detail.sendStaging") }}
              </button>
            </div>
          </template>
          <div v-else class="detail-empty">
            <span class="detail-empty-emoji">👇</span>
            {{ t("detail.empty") }}
          </div>
        </section>
      </aside>
    </div>

    <UserRulesPanel v-if="showAdvanced" />

    <section v-if="showAdvanced && expiredList.length > 0" class="card">
      <h2>{{ t("staging.expiredTitle") }}</h2>
      <p class="muted">
        {{ t("staging.expiredIntro", { count: expiredList.length }) }}
      </p>
      <ul class="expired-list">
        <li
          v-for="item in expiredList.slice(0, 6)"
          :key="item.id"
          class="expired-row mono"
        >
          <span>📄 {{ item.original_path }}</span>
          <span class="expired-meta">
            {{ formatBytes(item.size_bytes) }} ·
            {{ t("staging.expiredAgo", { hours: Math.floor(item.age_secs / 3600) }) }}
          </span>
        </li>
        <li v-if="expiredList.length > 6" class="expired-row muted">
          {{ t("staging.expiredMore", { count: expiredList.length - 6 }) }}
        </li>
      </ul>
      <div class="probe-bar">
        <button
          type="button"
          class="stage-btn perm-trigger"
          :disabled="expiredCleanupBusy"
          @click="expiredConfirmOpen = true"
        >
          {{ t("staging.expiredDeleteAll", { count: expiredList.length }) }}
        </button>
        <span v-if="expiredCleanupReport" class="scan-quick mono">
          {{ t("staging.expiredLastReport", {
            deleted: expiredCleanupReport.deleted,
            bytes: formatBytes(expiredCleanupReport.total_bytes),
            ms: expiredCleanupReport.elapsed_ms,
          }) }}
        </span>
      </div>
      <p v-if="expiredCleanupError" class="err">{{ expiredCleanupError }}</p>
    </section>

    <div
      v-if="expiredConfirmOpen"
      class="modal-backdrop"
      @click.self="expiredConfirmOpen = false"
    >
      <div class="conflict-dialog">
        <h3 class="conflict-title">
          {{ t("staging.expiredConfirmTitle", { count: expiredList.length }) }}
        </h3>
        <p class="conflict-help">{{ t("staging.expiredConfirmHelp") }}</p>
        <!-- Gemini review — AUTO_THRESHOLD abort semantiği artık yüzeye
             çıkar: rapor aborted döndüyse kullanıcı açıkça görür. -->
        <p v-if="expiredCleanupReport?.aborted_threshold" class="err">
          {{ t("staging.abortedThreshold", { count: expiredList.length }) }}
        </p>
        <div class="conflict-actions">
          <button
            type="button"
            class="conflict-btn conflict-overwrite"
            :disabled="expiredCleanupBusy"
            @click="runExpiredCleanup(true)"
          >
            {{
              expiredCleanupBusy
                ? t("staging.expiredConfirmBusy")
                : t("staging.expiredConfirmGo")
            }}
          </button>
          <button
            type="button"
            class="conflict-btn conflict-cancel"
            :disabled="expiredCleanupBusy"
            @click="expiredConfirmOpen = false"
          >
            {{ t("conflict.cancel") }}
          </button>
        </div>
      </div>
    </div>

    <section v-if="showAdvanced" class="card">
      <h2>Performans + Gizlilik (Bölüm 17.1 + 18.3)</h2>
      <div v-if="perfStats" class="grid">
        <div class="row">
          <span class="key">Son tarama</span>
          <span class="val mono">{{ perfStats.last }} ms</span>
        </div>
        <div class="row">
          <span class="key">Ortalama</span>
          <span class="val mono">
            {{ perfStats.avg }} ms ({{ perfStats.count }} örnek)
          </span>
        </div>
        <div class="row">
          <span class="key">Aralık</span>
          <span class="val mono">
            {{ perfStats.min }}–{{ perfStats.max }} ms
          </span>
        </div>
      </div>
      <p v-else class="muted">
        Henüz tarama örneği yok. Tarayınca son 20 ölçüm burada özetlenir.
      </p>
      <div class="probe-bar">
        <label class="sort-label">
          <input
            type="checkbox"
            :checked="telemetryOptIn"
            @change="
              setTelemetryOptIn(
                ($event.target as HTMLInputElement).checked,
              )
            "
          />
          Anonim performans telemetrisine katıl (default kapalı)
        </label>
        <span class="scan-quick mono">
          {{ telemetryOptIn ? "opt-in" : "opt-out" }}
        </span>
      </div>
      <p class="muted privacy-note">
        Bölüm 18.3 — gerçek telemetry endpoint v0.2'de. Şimdilik bu tercih
        yalnızca settings'e kaydedilir; hiçbir veri gönderilmez.
      </p>
    </section>

    <section v-if="showAdvanced" class="card">
      <h2>Veritabanı (Bölüm 14)</h2>
      <div v-if="dbInfo" class="grid">
        <div class="row">
          <span class="key">Dosya</span>
          <span class="val mono path">{{ dbInfo.path }}</span>
        </div>
        <div class="row">
          <span class="key">Schema</span>
          <span class="val mono">v{{ dbInfo.schema_version }}</span>
          <span class="pill pill-ok">{{ dbInfo.table_count }} tablo</span>
        </div>
        <div class="row">
          <span class="key">Journal</span>
          <span class="val mono">{{ dbInfo.journal_mode.toUpperCase() }}</span>
        </div>
        <div class="row">
          <span class="key">Page</span>
          <span class="val mono">{{ dbInfo.page_size }} B</span>
        </div>
        <div class="row">
          <span class="key">Spec</span>
          <span class="val mono">{{ dbInfo.spec_version || "—" }}</span>
        </div>
      </div>
      <p v-if="dbError" class="err">{{ dbError }}</p>
    </section>

    <section v-if="showAdvanced" class="card">
      <h2>Yol Haritası</h2>
      <ol class="roadmap">
        <li class="done">Spec v1.4 donduruldu (37 bölüm, 4 edge case kapatıldı)</li>
        <li class="done">Proje iskeleti — Tauri 2 + Vue 3 + GPL-3.0-or-later</li>
        <li class="done">MFT probe + yetki kontrolü (Bölüm 5)</li>
        <li class="done">Volume pre-flight (Bölüm 33.2 Katman 0)</li>
        <li class="done">SQLite + migrations 0001 (Bölüm 14)</li>
        <li class="done">MFT full walk v0.1 (Bölüm 5.1 + 4.3 Adım 2)</li>
        <li class="done">ScanTree builder + agregat boyutlar (Bölüm 4.3+4.4)</li>
        <li class="done">FindFirstFile fallback + auto-strategy (Bölüm 5.2A K2)</li>
        <li class="done">Lazy viewport query + drilldown (Bölüm 9.6)</li>
        <li class="done">Sunburst donut (Bölüm 9.1 Pillar 2) — SVG hand-rolled</li>
        <li class="done">Safe-to-delete kural motoru — 33 kural (Bölüm 6)</li>
        <li class="done">Staging + Undo same-volume (Bölüm 12.2)</li>
        <li class="done">Cross-volume two-phase commit + WAL (Bölüm 12.3)</li>
        <li class="done">Time Machine / Snapshot — capture + delta (Bölüm 8)</li>
        <li class="done">Duplicate Detector v0.1 — Blake3 (Bölüm 7)</li>
        <li class="done">Locked file probe v0.1 — share-violation + RestartManager (Bölüm 34.1/34.3/34.4)</li>
        <li class="done">Treemap mod 2/4 — squarified (Bölüm 9.1)</li>
        <li class="done">Bubble mod 3/4 — force-relax (Bölüm 9.1)</li>
        <li class="done">Timeline mod 4/4 — mtime ekseni + Y-relax (Bölüm 9.1)</li>
        <li class="done">Permanent delete + forensic ledger (Bölüm 12.4)</li>
        <li class="done">Undo conflict resolution dialog (Bölüm 12.2.4)</li>
        <li class="done">CI gates v0.1 — fmt + clippy + test + build (Bölüm 20.4)</li>
        <li class="done">Release workflow — MSI + NSIS tauri-action (Bölüm 21.1)</li>
        <li class="done">Bölüm 6.2 — 53 kural (50+ hedef tutuldu)</li>
        <li class="active">Kod imzalama + Tauri auto-updater (Bölüm 18.2 + 21.4)</li>
        <li>v2 ML scorer — TFLite tier'lı (Bölüm 6.5)</li>
        <li>VSS reference-counted snapshot pool — Discovery Log #002, ertelendi</li>
        <li>Permanent delete + conflict resolution dialog (Bölüm 12.4 + 12.2.4)</li>
        <li>MSI installer + GitHub Actions CI (Bölüm 21 + 20)</li>
        <li>v2 scoring rubric — TFLite tier'lı ML (Bölüm 6.5)</li>
      </ol>
    </section>

    <footer class="foot">
      <span>GPL-3.0-or-later</span>
      <span class="dot">·</span>
      <span>Master mimari: D-Space-Mimari-v1.4.docx</span>
    </footer>
    </main>
  </div>
</template>

<style scoped>
/* Bölüm 15.1 v0.2 — gerçek üç-kolon workspace.
   - Sol: VolumeSidebar (sticky, 280px sabit)
   - Orta: .shell (akıcı, max 1400px)
   - Sağ: workspace içindeki col-right (snapshot + detail)
   <960px: sidebar üste kayar, akıcı tek kolon. */
.app-frame {
  display: grid;
  grid-template-columns: 280px minmax(0, 1fr);
  grid-template-rows: auto 1fr;
  gap: 16px;
  max-width: 1720px;
  margin: 0 auto;
  padding: 16px 16px 12px;
  align-items: start;
}

/* Sprint 3.8 — IndexSearchBar üst yapışkan bar, iki kolonu yatayda kaplar. */
.app-search {
  grid-column: 1 / -1;
  position: sticky;
  top: 0;
  z-index: 50;
}

.app-sidebar {
  position: sticky;
  top: 70px;
}

@media (max-width: 960px) {
  .app-frame {
    grid-template-columns: 1fr;
    padding: 12px;
  }
  .app-sidebar {
    position: static;
  }
}

.shell {
  max-width: 1400px;
  margin: 0;
  padding: 16px 12px 24px;
  display: flex;
  flex-direction: column;
  gap: 20px;
  min-width: 0;
}

/* Üç-kolon workspace: Snapshot | Duplicate | Detail.
   Geniş ekranda yan yana → 2K monitörler dolu kullanır.
   1280-1600 arası 2-kolon (detail alta düşer).
   <960 tek kolon. */
.workspace {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}

.col-snap,
.col-dup,
.col-detail {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}

/* Advanced diagnostic kartlar workspace altında full-width. */
.diag-stack {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

@media (max-width: 1600px) {
  .workspace {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }
  .col-detail {
    grid-column: 1 / -1;
  }
}

@media (max-width: 960px) {
  .workspace {
    grid-template-columns: 1fr;
  }
}

/* Sağ panel - seçili öğe detayı placeholder */
.detail-empty {
  color: var(--muted);
  font-size: 12px;
  text-align: center;
  padding: 40px 16px;
  line-height: 1.6;
}

.detail-empty-emoji {
  font-size: 36px;
  display: block;
  margin-bottom: 12px;
  opacity: 0.65;
  animation: empty-bounce 2.4s ease-in-out infinite;
  transform-origin: 50% 100%;
}

@keyframes empty-bounce {
  0%,
  100% {
    transform: translateY(0) scale(1);
    opacity: 0.55;
  }
  50% {
    transform: translateY(-4px) scale(1.04);
    opacity: 0.85;
  }
}

@media (prefers-reduced-motion: reduce) {
  .detail-empty-emoji {
    animation: none;
  }
}

/* Bölüm 15.4 — boş listelerde subtle shimmer; kullanıcı dikkati hafifçe
   buraya çekilir, "boş ama erişilebilir" hissi. */
.empty-shimmer {
  position: relative;
  overflow: hidden;
  background: linear-gradient(
    90deg,
    transparent 0%,
    color-mix(in srgb, var(--fg) 5%, transparent) 50%,
    transparent 100%
  );
  background-size: 200% 100%;
  animation: empty-shimmer 4s linear infinite;
  border-radius: 6px;
  padding: 16px 12px !important;
}

@keyframes empty-shimmer {
  0% {
    background-position: -100% 0;
  }
  100% {
    background-position: 100% 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  .empty-shimmer {
    animation: none;
    background: transparent;
  }
}

.detail-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--fg);
  word-break: break-all;
  margin: 0 0 4px;
}

.detail-path {
  font-size: 10px;
  color: var(--muted);
  word-break: break-all;
  margin: 0 0 12px;
  font-family: ui-monospace, monospace;
}

.detail-row {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  padding: 4px 0;
  font-size: 12px;
  border-bottom: 1px dashed var(--border);
}

.detail-row:last-child {
  border-bottom: none;
}

.detail-key {
  color: var(--muted);
}

.detail-val {
  color: var(--fg);
  text-align: right;
}

.detail-actions {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 12px;
}

.detail-actions .stage-btn {
  width: 100%;
  text-align: left;
  padding: 6px 10px;
}

.hero {
  text-align: left;
}

.brand {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
}

.logo-dot {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: radial-gradient(circle at 30% 30%, #24c8db, #0a6a78);
  box-shadow: 0 0 16px #24c8db66;
}

.hero h1 {
  margin: 0;
  font-size: 32px;
  letter-spacing: -0.02em;
  font-weight: 600;
  flex: 1;
}

.tagline {
  margin: 8px 0 0;
  color: var(--muted);
  font-size: 15px;
}

.adv-toggle {
  background: var(--bg);
  border: 1px solid var(--border);
  color: var(--muted);
  padding: 6px 12px;
  border-radius: 999px;
  font-size: 11px;
  letter-spacing: 0.04em;
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s, color 0.15s;
}

.adv-toggle:hover {
  border-color: #2a8a99;
  color: var(--fg);
}

.adv-toggle-active {
  background: rgba(59, 130, 246, 0.16);
  border-color: rgba(59, 130, 246, 0.55);
  color: #2563eb;
  font-weight: 600;
}

:root[data-theme="light"] .adv-toggle-active {
  color: #1d4ed8;
}

.badge-group {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-top: 14px;
}

.badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  padding: 5px 12px;
  border-radius: 999px;
  letter-spacing: 0.02em;
  font-family: ui-monospace, monospace;
  border: 1px solid var(--border);
  background: var(--bg);
  color: var(--fg);
}

/* Badge ve pill renk paleti — light/dark her iki temada okunabilir
   mid-tone'lar. Background tint ~%18, border ~%50, text saturated
   ana renk. */
.badge-blue {
  border-color: rgba(59, 130, 246, 0.45);
  background: rgba(59, 130, 246, 0.14);
  color: #2563eb;
}

.badge-warn {
  border-color: rgba(217, 119, 6, 0.5);
  background: rgba(245, 158, 11, 0.16);
  color: #b45309;
}

.badge-amber {
  border-color: rgba(22, 163, 74, 0.5);
  background: rgba(22, 163, 74, 0.14);
  color: #15803d;
}

/* Dark temada text'lerin biraz daha açık tonu — dark bg üzerinde okunaklı. */
:root:not([data-theme="light"]) .badge-blue {
  color: #60a5fa;
}
:root:not([data-theme="light"]) .badge-warn {
  color: #fbbf24;
}
:root:not([data-theme="light"]) .badge-amber {
  color: #4ade80;
}
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) .badge-blue {
    color: #2563eb;
  }
  :root:not([data-theme]) .badge-warn {
    color: #b45309;
  }
  :root:not([data-theme]) .badge-amber {
    color: #15803d;
  }
}

.badge-ghost {
  color: var(--muted);
}

.scan-quick {
  color: var(--muted);
  font-size: 11px;
  margin-left: 8px;
}

.drive-warning {
  margin-top: 10px;
  padding: 8px 12px;
  background: #78350f22;
  border: 1px solid #78350f80;
  border-radius: 8px;
  color: #fcd34d;
  font-size: 12px;
  line-height: 1.5;
}

.expired-list {
  list-style: none;
  margin: 12px 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.expired-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  padding: 6px 10px;
  background: var(--bg);
  border-radius: 6px;
  font-size: 12px;
}

.expired-meta {
  color: var(--muted);
  font-size: 11px;
  white-space: nowrap;
}

.privacy-note {
  font-size: 11px;
  margin-top: 8px;
}

.shortcuts-dialog {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 14px;
  padding: 22px 24px;
  max-width: 520px;
  width: calc(100% - 32px);
  box-shadow: 0 12px 48px rgba(0, 0, 0, 0.5);
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.shortcut-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

.shortcut-table td {
  padding: 6px 8px;
  border-bottom: 1px dashed var(--border);
  color: var(--fg);
  vertical-align: middle;
}

.shortcut-table td:first-child {
  width: 200px;
  white-space: nowrap;
}

.shortcut-table kbd {
  display: inline-block;
  background: var(--bg);
  border: 1px solid var(--border);
  border-bottom-width: 2px;
  border-radius: 4px;
  padding: 2px 6px;
  font-family: ui-monospace, monospace;
  font-size: 11px;
  color: #93c5fd;
  margin: 0 2px;
}

.shortcut-hint {
  font-size: 11px;
  color: var(--muted);
  margin: 0;
}

.card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 14px;
  padding: 20px 22px;
  transition: border-color 0.18s ease, box-shadow 0.18s ease,
    transform 0.18s ease;
  animation: card-enter 0.22s ease-out both;
}

.card:hover {
  border-color: color-mix(in srgb, var(--fg) 20%, var(--border));
  box-shadow: 0 6px 16px var(--shadow);
}

@keyframes card-enter {
  from {
    opacity: 0;
    transform: translateY(6px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@media (prefers-reduced-motion: reduce) {
  .card {
    animation: none;
    transition: none;
  }
}

.card h2 {
  margin: 0 0 4px;
  font-size: 14px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--muted);
  font-weight: 600;
}

.section-subtitle {
  margin: 0 0 6px;
  font-size: 12px;
  font-style: italic;
  color: var(--muted);
  line-height: 1.4;
}

.section-intro {
  margin: 0 0 14px;
  font-size: 13px;
  line-height: 1.55;
  color: var(--fg);
  opacity: 0.85;
  max-width: 64ch;
}

.grid {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.row {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 14px;
}

.key {
  width: 90px;
  color: var(--muted);
}

.val {
  color: var(--fg);
}

.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 13px;
}

.pill {
  font-size: 11px;
  padding: 3px 10px;
  border-radius: 999px;
  letter-spacing: 0.04em;
  font-weight: 700;
  display: inline-flex;
  align-items: center;
}

.pill-frozen {
  background: rgba(59, 130, 246, 0.16);
  color: #2563eb;
  border: 1px solid rgba(59, 130, 246, 0.5);
}

.pill-ok {
  background: rgba(22, 163, 74, 0.16);
  color: #15803d;
  border: 1px solid rgba(22, 163, 74, 0.5);
}

.pill-warn {
  background: rgba(217, 119, 6, 0.18);
  color: #b45309;
  border: 1px solid rgba(217, 119, 6, 0.55);
}

:root:not([data-theme="light"]) .pill-frozen {
  color: #93c5fd;
}
:root:not([data-theme="light"]) .pill-ok {
  color: #6ee7b7;
}
:root:not([data-theme="light"]) .pill-warn {
  color: #fcd34d;
}
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) .pill-frozen {
    color: #1d4ed8;
  }
  :root:not([data-theme]) .pill-ok {
    color: #15803d;
  }
  :root:not([data-theme]) .pill-warn {
    color: #b45309;
  }
}

.probe-bar {
  display: flex;
  gap: 8px;
  margin-bottom: 14px;
}

.drive-input {
  width: 64px;
  padding: 8px 10px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--fg);
  text-align: center;
  text-transform: uppercase;
}

.drive-input:focus {
  outline: none;
  border-color: #24c8db;
}

.probe-btn {
  padding: 8px 16px;
  background: #2563eb;
  border: 1px solid #1d4ed8;
  border-radius: 8px;
  color: #ffffff;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease,
    transform 0.08s ease;
}

.probe-btn:hover:not(:disabled) {
  background: #1d4ed8;
  border-color: #1e40af;
}

.probe-btn:active:not(:disabled) {
  transform: scale(0.97);
}

.probe-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.roadmap {
  margin: 0;
  padding-left: 18px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 14px;
  color: var(--muted);
}

.roadmap li.done {
  color: #6ee7b7;
  text-decoration: line-through;
  text-decoration-color: #6ee7b766;
}

.path {
  font-size: 11px;
  word-break: break-all;
  color: var(--muted);
}

.samples {
  margin-top: 6px;
  padding-top: 12px;
  border-top: 1px dashed var(--border);
}

.samples-title {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--muted);
  margin-bottom: 6px;
}

.samples-list {
  margin: 0;
  padding-left: 16px;
  font-size: 12px;
  max-height: 200px;
  overflow-y: auto;
  list-style: square;
}

.samples-list li {
  color: var(--fg);
  padding: 1px 0;
}

.top-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 320px;
  overflow-y: auto;
}

.top-item {
  display: grid;
  grid-template-columns: 20px 1fr 100px;
  gap: 10px;
  align-items: center;
  padding: 4px 8px;
  border-radius: 6px;
  font-size: 13px;
}

.top-item:hover {
  background: var(--bg);
}

.top-icon {
  font-size: 14px;
  text-align: center;
}

.top-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--fg);
}

.top-size {
  text-align: right;
  color: #6ee7b7;
  font-weight: 500;
}

.crumbs {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
  margin-bottom: 12px;
  font-size: 13px;
}

.crumb {
  background: transparent;
  border: none;
  color: #93c5fd;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 6px;
  font-size: 13px;
  font-family: ui-monospace, monospace;
  transition: background 0.15s;
}

.crumb:hover:not(:disabled) {
  background: #1e3a8a33;
}

.crumb-current {
  color: var(--fg);
  cursor: default;
}

.crumb-sep {
  color: var(--muted);
  font-size: 12px;
}

.drill-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--border);
}

.sort-label {
  font-size: 12px;
  color: var(--muted);
  display: flex;
  align-items: center;
  gap: 8px;
}

.sort-select {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--fg);
  padding: 4px 8px;
  font-size: 12px;
  font-family: inherit;
  cursor: pointer;
}

.drill-stats {
  font-size: 11px;
  color: var(--muted);
}

.lod-badge {
  font-size: 10px;
  font-weight: 700;
  padding: 3px 8px;
  border-radius: 999px;
  background: rgba(217, 119, 6, 0.16);
  color: #b45309;
  border: 1px solid rgba(217, 119, 6, 0.5);
  font-family: ui-monospace, monospace;
  letter-spacing: 0.04em;
}

:root:not([data-theme="light"]) .lod-badge {
  color: #fbbf24;
}
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) .lod-badge {
    color: #b45309;
  }
}

/* Sprint 5.1 — Tray Live Monitor toast. Alt sağda sabit, 20 sn'de
   otomatik kapanır; tıklayınca da kapanır. */
.tray-toast {
  position: fixed;
  bottom: 20px;
  right: 20px;
  z-index: 1100;
  display: flex;
  align-items: center;
  gap: 12px;
  background: var(--surface);
  border: 1px solid rgba(220, 38, 38, 0.5);
  border-left: 4px solid #dc2626;
  border-radius: 10px;
  padding: 12px 14px;
  box-shadow: 0 12px 28px var(--shadow);
  max-width: 360px;
  cursor: pointer;
}

.tray-toast-icon {
  font-size: 22px;
  color: #b91c1c;
  flex-shrink: 0;
}

.tray-toast-body {
  flex: 1;
  min-width: 0;
}

.tray-toast-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--fg);
}

.tray-toast-hint {
  font-size: 11px;
  color: var(--muted);
  margin-top: 3px;
}

.tray-toast-close {
  background: transparent;
  border: none;
  color: var(--muted);
  font-size: 14px;
  cursor: pointer;
  padding: 2px 6px;
}

.tray-toast-close:hover {
  color: var(--fg);
}

.tray-toast-enter-from {
  opacity: 0;
  transform: translateX(20px);
}
.tray-toast-leave-to {
  opacity: 0;
  transform: translateX(20px);
}
.tray-toast-enter-active,
.tray-toast-leave-active {
  transition: opacity 0.25s ease, transform 0.25s ease;
}

/* Bölüm 9.4 — viz mod swap transition. Görsel sürekliliği koru. */
.view-swap-enter-from {
  opacity: 0;
  transform: scale(0.96);
}
.view-swap-leave-to {
  opacity: 0;
  transform: scale(1.04);
}
.view-swap-enter-active,
.view-swap-leave-active {
  transition: opacity 0.25s ease, transform 0.25s ease;
}

@media (prefers-reduced-motion: reduce) {
  .view-swap-enter-active,
  .view-swap-leave-active {
    transition: none;
  }
}

.drill-list {
  list-style: none;
  margin: 0;
  padding: 0;
  max-height: 500px;
  overflow-y: auto;
}

.drill-row {
  display: grid;
  grid-template-columns: 22px 1fr 130px 100px 60px 100px 130px;
  align-items: center;
  gap: 10px;
  padding: 6px 8px;
  font-size: 13px;
  border-radius: 6px;
  border: 1px solid transparent;
}

.drill-actions {
  display: flex;
  gap: 4px;
  justify-content: flex-end;
}

.stage-btn {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--fg);
  padding: 3px 8px;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s;
}

.stage-btn:hover:not(:disabled) {
  background: #1f6f7c33;
  border-color: #24c8db66;
}

.stage-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.stage-confirm {
  background: #14532d66;
  border-color: #14532d;
  color: #6ee7b7;
  font-weight: 600;
  font-size: 11px;
}

.stage-confirm:hover:not(:disabled) {
  background: #14532daa;
}

.stage-cancel {
  background: transparent;
  border-color: var(--border);
  color: var(--muted);
  font-size: 11px;
  padding: 3px 6px;
}

.lock-probe-btn {
  font-size: 13px;
}

.lock-detail {
  grid-column: 1 / -1;
  margin-top: 6px;
  padding: 10px 12px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.lock-detail-head {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.lock-pill {
  font-size: 11px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 999px;
  letter-spacing: 0.04em;
}

.lock-free {
  background: rgba(22, 163, 74, 0.16);
  color: #15803d;
  border: 1px solid rgba(22, 163, 74, 0.55);
}

.lock-busy {
  background: rgba(220, 38, 38, 0.16);
  color: #b91c1c;
  border: 1px solid rgba(220, 38, 38, 0.55);
}

.lock-warn {
  background: rgba(217, 119, 6, 0.18);
  color: #b45309;
  border: 1px solid rgba(217, 119, 6, 0.55);
}

:root:not([data-theme="light"]) .lock-free {
  color: #6ee7b7;
}
:root:not([data-theme="light"]) .lock-busy {
  color: #fca5a5;
}
:root:not([data-theme="light"]) .lock-warn {
  color: #fcd34d;
}
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) .lock-free {
    color: #15803d;
  }
  :root:not([data-theme]) .lock-busy {
    color: #b91c1c;
  }
  :root:not([data-theme]) .lock-warn {
    color: #b45309;
  }
}

.lock-action {
  color: var(--muted);
  font-size: 11px;
}

.lock-elapsed {
  margin-left: auto;
  color: var(--muted);
  font-size: 11px;
}

.lock-close {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--muted);
  padding: 2px 8px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 11px;
}

.lock-close:hover {
  background: #1f6f7c33;
}

.lock-owners {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.lock-owner-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  padding: 3px 0;
  border-top: 1px dashed var(--border);
}

.lock-owner-row:first-child {
  border-top: none;
}

.lock-pid {
  color: #93c5fd;
  font-weight: 600;
  min-width: 70px;
}

.lock-proc {
  color: var(--fg);
}

.lock-svc {
  color: var(--muted);
  font-size: 11px;
}

.lock-restartable {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  background: #14532d33;
  color: #6ee7b7;
  border: 1px solid #14532d66;
  margin-left: auto;
}

.lock-empty {
  font-size: 12px;
  margin: 0;
}

.lock-err {
  margin: 0;
  font-size: 12px;
}

.view-mode-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}

.view-mode-label {
  font-size: 11px;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  margin-right: 4px;
}

.view-chip {
  background: var(--bg);
  border: 1px solid var(--border);
  color: var(--fg);
  padding: 4px 12px;
  border-radius: 999px;
  font-size: 12px;
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s;
}

.view-chip:hover:not(.view-chip-disabled):not(.view-chip-active) {
  border-color: #2a8a99;
  background: #1f6f7c22;
}

.view-chip-active {
  background: #1f6f7c;
  border-color: #2a8a99;
  color: #e7fafe;
  font-weight: 600;
  cursor: default;
}

.view-chip-disabled {
  color: var(--muted);
  cursor: not-allowed;
  font-style: italic;
  opacity: 0.6;
}

.staging-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 360px;
  overflow-y: auto;
}

.staging-row-wrap {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

/* Bekleyen Silmeler — dosya kuyruğa girip çıkarken yumuşak geçiş.
   Enter: yandan kayarak gelir + opacity. Leave: sağa süzülerek gider
   (silindi/geri alındı hissi). Move: re-order animasyonu. */
.staging-row-enter-from {
  opacity: 0;
  transform: translateX(-14px);
}

.staging-row-leave-to {
  opacity: 0;
  transform: translateX(20px);
}

.staging-row-enter-active,
.staging-row-leave-active {
  transition: opacity 0.22s ease, transform 0.22s ease;
}

.staging-row-move {
  transition: transform 0.25s ease;
}

@media (prefers-reduced-motion: reduce) {
  .staging-row-enter-active,
  .staging-row-leave-active,
  .staging-row-move {
    transition: none;
  }
}

.staging-row {
  display: grid;
  grid-template-columns: 22px 1fr 50px 100px 150px 100px 50px;
  gap: 10px;
  align-items: center;
  padding: 6px 8px;
  font-size: 12px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg);
}

.perm-trigger {
  font-size: 13px;
}

.perm-confirm {
  padding: 10px 12px;
  background: #7f1d1d18;
  border: 1px solid #7f1d1d66;
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.perm-warn {
  font-size: 12px;
  color: #b91c1c;
  line-height: 1.45;
}

:root:not([data-theme="light"]) .perm-warn {
  color: #fca5a5;
}
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) .perm-warn {
    color: #b91c1c;
  }
}

.perm-warn code {
  background: var(--bg);
  border: 1px solid var(--border);
  padding: 1px 6px;
  border-radius: 4px;
  color: var(--fg);
  font-weight: 600;
}

/* DoD secure wipe checkbox satırı — opt-in. */
.perm-wipe-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  margin-top: 6px;
  flex-wrap: wrap;
  cursor: pointer;
}

.perm-wipe-row input[type="checkbox"] {
  cursor: pointer;
  accent-color: #b91c1c;
}

.perm-wipe-label {
  font-weight: 600;
  color: var(--fg);
}

.perm-wipe-hint {
  font-size: 10px;
  color: var(--muted);
  flex-basis: 100%;
  margin-left: 22px;
  line-height: 1.35;
}

.perm-row {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}

.perm-input {
  flex: 1;
  min-width: 220px;
  padding: 4px 10px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--fg);
  font-size: 12px;
}

.perm-input:focus {
  outline: none;
  border-color: #fca5a5;
}

.perm-go {
  background: #7f1d1d66;
  border-color: #7f1d1d;
  color: #fca5a5;
  font-weight: 600;
}

.perm-go:hover:not(:disabled) {
  background: #7f1d1daa;
}

.perm-cancel {
  background: transparent;
  border-color: var(--border);
  color: var(--muted);
}

.perm-err {
  margin: 0;
  font-size: 12px;
}

.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
  backdrop-filter: blur(3px);
}

.conflict-dialog {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 14px;
  padding: 22px 24px;
  max-width: 560px;
  width: calc(100% - 32px);
  box-shadow: 0 12px 48px rgba(0, 0, 0, 0.5);
  display: flex;
  flex-direction: column;
  gap: 14px;
}

/* Modal generic — UNC dialog için (conflict-dialog ile aynı görsel). */
.modal {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 14px;
  padding: 22px 24px;
  max-width: 560px;
  width: calc(100% - 32px);
  box-shadow: 0 12px 48px var(--shadow);
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.unc-modal {
  max-width: 600px;
}

.unc-title {
  margin: 0;
  font-size: 16px;
  color: var(--fg);
  font-weight: 600;
}

.unc-help {
  margin: 0;
  font-size: 12px;
  color: var(--muted);
  line-height: 1.5;
}

.unc-input {
  width: 100%;
  padding: 10px 12px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--fg);
  font-size: 13px;
}

.unc-input:focus {
  outline: none;
  border-color: #2563eb;
  box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.2);
}

.unc-result {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px 14px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.unc-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.unc-btn {
  background: rgba(59, 130, 246, 0.16);
  border-color: rgba(59, 130, 246, 0.55);
  color: #2563eb;
}

.unc-btn:hover:not(:disabled) {
  background: rgba(59, 130, 246, 0.24);
}

:root:not([data-theme="light"]) .unc-btn {
  color: #93c5fd;
}
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) .unc-btn {
    color: #1d4ed8;
  }
}

.conflict-title {
  margin: 0;
  font-size: 16px;
  color: #fcd34d;
  font-weight: 600;
}

.conflict-help {
  margin: 0;
  font-size: 12px;
  color: var(--muted);
  line-height: 1.5;
}

.conflict-cols {
  display: flex;
  align-items: stretch;
  gap: 10px;
}

.conflict-col {
  flex: 1;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.conflict-col-head {
  font-size: 11px;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.conflict-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--fg);
}

.hash-hint {
  font-size: 10px;
  color: var(--muted);
}

.conflict-vs {
  display: flex;
  align-items: center;
  color: var(--muted);
  font-size: 11px;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}

.conflict-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.conflict-btn {
  flex: 1 1 auto;
  min-width: 120px;
  padding: 8px 14px;
  background: var(--bg);
  border: 1px solid var(--border);
  color: var(--fg);
  border-radius: 8px;
  font-size: 12px;
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s;
}

.conflict-btn:hover:not(:disabled) {
  background: #1f6f7c33;
  border-color: #2a8a99;
}

.conflict-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.conflict-overwrite {
  background: #7f1d1d33;
  border-color: #7f1d1d;
  color: #fca5a5;
}

.conflict-overwrite:hover:not(:disabled) {
  background: #7f1d1d66;
}

.conflict-cancel {
  color: var(--muted);
}

.tier-pill {
  font-size: 10px;
  font-weight: 700;
  padding: 2px 6px;
  border-radius: 4px;
  letter-spacing: 0.04em;
  text-align: center;
  font-family: ui-monospace, monospace;
}

.tier-normal {
  background: rgba(22, 163, 74, 0.14);
  color: #15803d;
  border: 1px solid rgba(22, 163, 74, 0.5);
}

.tier-cross {
  background: rgba(59, 130, 246, 0.16);
  color: #2563eb;
  border: 1px solid rgba(59, 130, 246, 0.55);
}

:root:not([data-theme="light"]) .tier-normal {
  color: #6ee7b7;
}
:root:not([data-theme="light"]) .tier-cross {
  color: #93c5fd;
}
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) .tier-normal {
    color: #15803d;
  }
  :root:not([data-theme]) .tier-cross {
    color: #1d4ed8;
  }
}

.staging-icon {
  text-align: center;
}

.staging-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--fg);
  font-size: 11px;
}

.staging-size {
  text-align: right;
  color: var(--fg);
  font-weight: 600;
}

.staging-time {
  color: var(--muted);
  font-size: 11px;
  text-align: right;
}

.score-pill {
  font-size: 11px;
  font-weight: 700;
  padding: 3px 10px;
  border-radius: 999px;
  text-align: center;
  letter-spacing: 0.04em;
  white-space: nowrap;
  font-family: ui-monospace, monospace;
}

.score-danger {
  background: rgba(220, 38, 38, 0.16);
  color: #b91c1c;
  border: 1px solid rgba(220, 38, 38, 0.55);
}

.score-caution {
  background: rgba(217, 119, 6, 0.18);
  color: #b45309;
  border: 1px solid rgba(217, 119, 6, 0.55);
}

.score-likely {
  background: rgba(22, 163, 74, 0.16);
  color: #15803d;
  border: 1px solid rgba(22, 163, 74, 0.55);
}

.score-cache {
  background: rgba(59, 130, 246, 0.16);
  color: #2563eb;
  border: 1px solid rgba(59, 130, 246, 0.55);
}

:root:not([data-theme="light"]) .score-danger {
  color: #fca5a5;
}
:root:not([data-theme="light"]) .score-caution {
  color: #fcd34d;
}
:root:not([data-theme="light"]) .score-likely {
  color: #6ee7b7;
}
:root:not([data-theme="light"]) .score-cache {
  color: #93c5fd;
}
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) .score-danger {
    color: #b91c1c;
  }
  :root:not([data-theme]) .score-caution {
    color: #b45309;
  }
  :root:not([data-theme]) .score-likely {
    color: #15803d;
  }
  :root:not([data-theme]) .score-cache {
    color: #1d4ed8;
  }
}

.score-none {
  background: transparent;
  color: var(--muted);
  border: 1px dashed var(--border);
}

.drill-dir {
  cursor: pointer;
}

.drill-dir:hover {
  background: var(--bg);
  border-color: var(--border);
}

.drill-file {
  cursor: default;
  opacity: 0.85;
}

.drill-icon {
  text-align: center;
}

.drill-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.drill-bar-inner {
  position: relative;
  height: 8px;
  background: var(--bg);
  border-radius: 4px;
  overflow: hidden;
}

.drill-fill {
  position: absolute;
  top: 0;
  left: 0;
  height: 100%;
  background: linear-gradient(90deg, #24c8db, #1e6f7c);
  border-radius: 4px;
}

.drill-pct {
  text-align: right;
  color: var(--muted);
  font-size: 11px;
}

.drill-size {
  text-align: right;
  color: #6ee7b7;
  font-weight: 500;
}

.drill-loading {
  padding: 12px;
  text-align: center;
  color: var(--muted);
  font-size: 12px;
  list-style: none;
}

.roadmap li.active {
  color: var(--fg);
  font-weight: 600;
}

.err {
  color: #fca5a5;
  font-family: ui-monospace, monospace;
  font-size: 13px;
}

.muted {
  color: var(--muted);
  font-size: 14px;
}

.foot {
  display: flex;
  gap: 8px;
  font-size: 12px;
  color: var(--muted);
  padding-top: 8px;
}

.dot {
  opacity: 0.5;
}
</style>

<style>
/* Bölüm 9.5 — tema sistemi. data-theme="dark"|"light" body üzerinde,
   sistem (auto) ise prefers-color-scheme media query'sini kullan. */
:root {
  --fg: #e5e7eb;
  --muted: #9ca3af;
  --bg: #0b0d10;
  --surface: #14171c;
  --border: #1f242c;
  --shadow: rgba(0, 0, 0, 0.5);

  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, sans-serif;
  font-size: 14px;
  line-height: 1.5;
  color: var(--fg);
  background: var(--bg);
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  transition: background 0.2s ease, color 0.2s ease;
}

/* Light tema — kasıtlı seçim (settings.theme = "light"). */
:root[data-theme="light"] {
  --fg: #1f2937;
  --muted: #6b7280;
  --bg: #f9fafb;
  --surface: #ffffff;
  --border: #e5e7eb;
  --shadow: rgba(0, 0, 0, 0.08);
}

/* Sistem teması ("auto" / null setting): prefers-color-scheme'i izle. */
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) {
    --fg: #1f2937;
    --muted: #6b7280;
    --bg: #f9fafb;
    --surface: #ffffff;
    --border: #e5e7eb;
    --shadow: rgba(0, 0, 0, 0.08);
  }
}

* {
  box-sizing: border-box;
}

html, body, #app {
  margin: 0;
  padding: 0;
  min-height: 100vh;
}
</style>
