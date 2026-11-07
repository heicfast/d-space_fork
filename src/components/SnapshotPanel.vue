<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import InfoButton from "./InfoButton.vue";

const { t, locale } = useI18n();

// Gemini review — locale-aware tarih/sayı formatı.
function localeTag(): string {
  return locale.value === "tr" ? "tr-TR" : "en-US";
}

interface SnapshotMeta {
  id: number;
  volume_id: string;
  captured_at_unix: number;
  total_size_bytes: number;
  file_count: number;
  dir_count: number;
  entry_count: number;
}

interface PathEntry {
  path: string;
  size_bytes: number;
}

interface DeltaEntry {
  path: string;
  from_size: number;
  to_size: number;
  delta_bytes: number;
}

interface DeltaResult {
  from_id: number;
  to_id: number;
  from_captured_at: number;
  to_captured_at: number;
  net_change_bytes: number;
  total_changed_paths: number;
  added: PathEntry[];
  removed: PathEntry[];
  grew: DeltaEntry[];
  shrunk: DeltaEntry[];
}

interface DspaceError {
  kind: string;
  message: string;
}

const snapshots = ref<SnapshotMeta[]>([]);
const listError = ref<string | null>(null);
const captureBusy = ref(false);
const captureError = ref<string | null>(null);

// Selection: ordered click sequence — first click = from, second click = to
const selection = ref<number[]>([]);

const delta = ref<DeltaResult | null>(null);
const deltaError = ref<string | null>(null);
const deltaLoading = ref(false);

const fromId = computed<number | null>(() =>
  selection.value.length >= 1 ? selection.value[0] : null,
);
const toId = computed<number | null>(() =>
  selection.value.length >= 2 ? selection.value[1] : null,
);

function formatBytes(b: number): string {
  if (b === 0) return "0 B";
  const sign = b < 0 ? "−" : "";
  let v = Math.abs(b);
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${sign}${v.toFixed(i >= 3 ? 2 : 0)} ${units[i]}`;
}

function formatTime(unix: number): string {
  if (!unix) return "—";
  const d = new Date(unix * 1000);
  return d.toLocaleString(localeTag());
}

function formatIpcError(err: unknown): string {
  if (typeof err === "string") return err;
  const e = err as DspaceError;
  if (e && e.kind && e.message) return `[${e.kind}] ${e.message}`;
  return JSON.stringify(err);
}

function truncatePath(p: string, max = 56): string {
  if (p.length <= max) return p;
  const keep = Math.floor((max - 1) / 2);
  return p.slice(0, keep) + "…" + p.slice(p.length - keep);
}

async function refreshList() {
  listError.value = null;
  try {
    snapshots.value = await invoke<SnapshotMeta[]>("list_snapshots");
  } catch (err) {
    listError.value = formatIpcError(err);
  }
}

async function captureSnapshot() {
  captureBusy.value = true;
  captureError.value = null;
  try {
    await invoke<SnapshotMeta>("capture_snapshot");
    await refreshList();
  } catch (err) {
    captureError.value = formatIpcError(err);
  } finally {
    captureBusy.value = false;
  }
}

async function loadDelta() {
  if (fromId.value === null || toId.value === null) {
    delta.value = null;
    return;
  }
  deltaLoading.value = true;
  deltaError.value = null;
  delta.value = null;
  try {
    delta.value = await invoke<DeltaResult>("snapshot_delta", {
      from: fromId.value,
      to: toId.value,
    });
  } catch (err) {
    deltaError.value = formatIpcError(err);
  } finally {
    deltaLoading.value = false;
  }
}

function toggleSelection(id: number) {
  const idx = selection.value.indexOf(id);
  if (idx !== -1) {
    // Already selected — remove
    selection.value.splice(idx, 1);
    delta.value = null;
    deltaError.value = null;
    return;
  }
  if (selection.value.length >= 2) {
    // Replace oldest (shift) to keep last-two
    selection.value.shift();
  }
  selection.value.push(id);
  if (selection.value.length === 2) {
    loadDelta();
  } else {
    delta.value = null;
    deltaError.value = null;
  }
}

function selectionRole(id: number): "from" | "to" | null {
  if (fromId.value === id) return "from";
  if (toId.value === id) return "to";
  return null;
}

const netLabel = computed(() => {
  if (!delta.value) return "";
  const n = delta.value.net_change_bytes;
  if (n > 0) return `⬆ +${formatBytes(n)} büyüdü`;
  if (n < 0) return `⬇ ${formatBytes(n)} küçüldü`;
  return "± 0 B (değişim yok)";
});

const netClass = computed(() => {
  if (!delta.value) return "";
  const n = delta.value.net_change_bytes;
  if (n > 0) return "net-up";
  if (n < 0) return "net-down";
  return "net-flat";
});

onMounted(() => {
  refreshList();
});
</script>

<template>
  <section class="card">
    <div class="snap-head">
      <h2>
        {{ t("snapshot.title") }}
        <InfoButton :text="t('snapshot.intro')" />
      </h2>
      <button
        type="button"
        class="probe-btn"
        :disabled="captureBusy"
        @click="captureSnapshot"
      >
        {{ captureBusy ? t("snapshot.snapBusy") : t("snapshot.snap") }}
      </button>
    </div>

    <p v-if="captureError" class="err">{{ captureError }}</p>
    <p v-if="listError" class="err">{{ listError }}</p>

    <p v-if="snapshots.length === 0 && !listError" class="muted">
      {{ t("snapshot.empty") }}
    </p>

    <ul v-else class="snap-list">
      <li
        v-for="s in snapshots"
        :key="s.id"
        class="snap-row"
        :class="{
          'snap-from': selectionRole(s.id) === 'from',
          'snap-to': selectionRole(s.id) === 'to',
        }"
        @click="toggleSelection(s.id)"
      >
        <span class="snap-icon">📸</span>
        <span class="snap-time mono">{{ formatTime(s.captured_at_unix) }}</span>
        <span class="snap-count mono">
          {{ s.entry_count.toLocaleString(localeTag()) }} {{ t("snapshot.entriesUnit") }}
        </span>
        <span class="snap-size mono">{{ formatBytes(s.total_size_bytes) }}</span>
        <span
          v-if="selectionRole(s.id)"
          class="snap-role"
          :class="selectionRole(s.id) === 'from' ? 'role-from' : 'role-to'"
        >
          {{ selectionRole(s.id) === "from" ? "FROM" : "TO" }}
        </span>
      </li>
    </ul>

    <p v-if="snapshots.length > 0 && !delta && !deltaLoading && !deltaError" class="muted hint">
      Karşılaştırmak için iki snapshot seç (eski → yeni).
      {{ selection.length === 1 ? "Bir tane daha seç." : "" }}
    </p>

    <div v-if="deltaLoading" class="muted hint">{{ t("snapshot.deltaLoading") }}</div>
    <p v-if="deltaError" class="err">{{ deltaError }}</p>

    <div v-if="delta" class="delta-block">
      <div class="delta-head">
        <div class="delta-range mono">
          {{ formatTime(delta.from_captured_at) }} → {{ formatTime(delta.to_captured_at) }}
        </div>
        <div class="delta-net" :class="netClass">{{ netLabel }}</div>
        <div class="delta-meta mono">
          {{ delta.total_changed_paths.toLocaleString(localeTag()) }} {{ t("snapshot.changedPaths") }}
        </div>
      </div>

      <div class="delta-grid">
        <div class="delta-panel">
          <div class="delta-panel-head">
            <span class="delta-pill pill-added">{{ t("snapshot.added") }}</span>
            <span class="delta-count mono">{{ delta.added.length }}</span>
          </div>
          <ul v-if="delta.added.length" class="delta-list">
            <li v-for="(e, i) in delta.added" :key="'a' + i" class="delta-item">
              <span class="delta-path mono" :title="e.path">{{ truncatePath(e.path) }}</span>
              <span class="delta-val mono val-added">+{{ formatBytes(e.size_bytes) }}</span>
            </li>
          </ul>
          <div v-else class="delta-empty">—</div>
        </div>

        <div class="delta-panel">
          <div class="delta-panel-head">
            <span class="delta-pill pill-removed">{{ t("snapshot.removed") }}</span>
            <span class="delta-count mono">{{ delta.removed.length }}</span>
          </div>
          <ul v-if="delta.removed.length" class="delta-list">
            <li v-for="(e, i) in delta.removed" :key="'r' + i" class="delta-item">
              <span class="delta-path mono" :title="e.path">{{ truncatePath(e.path) }}</span>
              <span class="delta-val mono val-removed">−{{ formatBytes(e.size_bytes) }}</span>
            </li>
          </ul>
          <div v-else class="delta-empty">—</div>
        </div>

        <div class="delta-panel">
          <div class="delta-panel-head">
            <span class="delta-pill pill-grew">{{ t("snapshot.grew") }}</span>
            <span class="delta-count mono">{{ delta.grew.length }}</span>
          </div>
          <ul v-if="delta.grew.length" class="delta-list">
            <li v-for="(e, i) in delta.grew" :key="'g' + i" class="delta-item">
              <span class="delta-path mono" :title="e.path">{{ truncatePath(e.path) }}</span>
              <span class="delta-val mono val-grew">+{{ formatBytes(e.delta_bytes) }}</span>
            </li>
          </ul>
          <div v-else class="delta-empty">—</div>
        </div>

        <div class="delta-panel">
          <div class="delta-panel-head">
            <span class="delta-pill pill-shrunk">{{ t("snapshot.shrunk") }}</span>
            <span class="delta-count mono">{{ delta.shrunk.length }}</span>
          </div>
          <ul v-if="delta.shrunk.length" class="delta-list">
            <li v-for="(e, i) in delta.shrunk" :key="'s' + i" class="delta-item">
              <span class="delta-path mono" :title="e.path">{{ truncatePath(e.path) }}</span>
              <span class="delta-val mono val-shrunk">
                {{ formatBytes(e.delta_bytes) }}
              </span>
            </li>
          </ul>
          <div v-else class="delta-empty">—</div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 14px;
  padding: 20px 22px;
}

.card h2 {
  margin: 0;
  font-size: 14px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--muted);
  font-weight: 600;
}

.snap-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 14px;
}

.probe-btn {
  padding: 8px 16px;
  background: #1f6f7c;
  border: 1px solid #2a8a99;
  border-radius: 8px;
  color: #e7fafe;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s;
}

.probe-btn:hover:not(:disabled) {
  background: #2a8a99;
}

.probe-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 13px;
}

.err {
  color: #fca5a5;
  font-family: ui-monospace, monospace;
  font-size: 13px;
  margin: 6px 0;
}

.muted {
  color: var(--muted);
  font-size: 14px;
}

.hint {
  margin: 10px 0 0;
  font-size: 12px;
}

.snap-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 320px;
  overflow-y: auto;
}

.snap-row {
  display: grid;
  grid-template-columns: 22px 1fr 110px 100px 50px;
  gap: 10px;
  align-items: center;
  padding: 6px 8px 6px 12px;
  font-size: 12px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg);
  cursor: pointer;
  position: relative;
  transition: background 0.15s, border-color 0.15s;
}

.snap-row:hover {
  border-color: #2a8a99;
}

.snap-row::before {
  content: "";
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 4px;
  border-radius: 6px 0 0 6px;
  background: transparent;
  transition: background 0.15s;
}

.snap-from {
  background: #1e3a8a22;
  border-color: #1e3a8a99;
}

.snap-from::before {
  background: #60a5fa;
}

.snap-to {
  background: #14532d22;
  border-color: #14532d99;
}

.snap-to::before {
  background: #6ee7b7;
}

.snap-icon {
  text-align: center;
}

.snap-time {
  color: var(--fg);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.snap-count {
  color: var(--muted);
  font-size: 11px;
  text-align: right;
}

.snap-size {
  color: #6ee7b7;
  text-align: right;
  font-weight: 500;
}

.snap-role {
  font-size: 10px;
  font-weight: 700;
  padding: 2px 6px;
  border-radius: 4px;
  letter-spacing: 0.04em;
  text-align: center;
  font-family: ui-monospace, monospace;
}

.role-from {
  background: #1e3a8a55;
  color: #93c5fd;
  border: 1px solid #1e3a8a;
}

.role-to {
  background: #14532d55;
  color: #6ee7b7;
  border: 1px solid #14532d;
}

.delta-block {
  margin-top: 16px;
  padding-top: 14px;
  border-top: 1px solid var(--border);
}

.delta-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 10px;
  margin-bottom: 14px;
}

.delta-range {
  color: var(--muted);
  font-size: 11px;
}

.delta-net {
  font-size: 14px;
  font-weight: 600;
  padding: 4px 12px;
  border-radius: 8px;
  font-family: ui-monospace, monospace;
}

.net-up {
  background: #7f1d1d33;
  color: #fca5a5;
  border: 1px solid #7f1d1d80;
}

.net-down {
  background: #14532d33;
  color: #6ee7b7;
  border: 1px solid #14532d80;
}

.net-flat {
  background: var(--bg);
  color: var(--muted);
  border: 1px solid var(--border);
}

.delta-meta {
  color: var(--muted);
  font-size: 11px;
}

.delta-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.delta-panel {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 12px;
  min-width: 0;
}

.delta-panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.delta-pill {
  font-size: 11px;
  padding: 3px 10px;
  border-radius: 999px;
  letter-spacing: 0.04em;
  font-weight: 600;
}

.pill-added {
  background: #14532d33;
  color: #6ee7b7;
  border: 1px solid #14532d80;
}

.pill-removed {
  background: #1f242c;
  color: var(--muted);
  border: 1px solid var(--border);
}

.pill-grew {
  background: #7f1d1d33;
  color: #fca5a5;
  border: 1px solid #7f1d1d80;
}

.pill-shrunk {
  background: #1e3a8a33;
  color: #93c5fd;
  border: 1px solid #1e3a8a80;
}

.delta-count {
  color: var(--muted);
  font-size: 11px;
}

.delta-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 240px;
  overflow-y: auto;
}

.delta-item {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 8px;
  align-items: center;
  padding: 3px 4px;
  font-size: 12px;
  border-radius: 4px;
}

.delta-item:hover {
  background: var(--surface);
}

.delta-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--fg);
  font-size: 11px;
}

.delta-val {
  font-size: 11px;
  font-weight: 500;
  white-space: nowrap;
}

.val-added {
  color: #6ee7b7;
}

.val-removed {
  color: var(--muted);
}

.val-grew {
  color: #fca5a5;
}

.val-shrunk {
  color: #93c5fd;
}

.delta-empty {
  color: var(--muted);
  font-size: 12px;
  font-family: ui-monospace, monospace;
  text-align: center;
  padding: 12px 4px;
}

@media (max-width: 720px) {
  .delta-grid {
    grid-template-columns: 1fr;
  }
}
</style>
