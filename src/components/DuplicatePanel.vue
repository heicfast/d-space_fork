<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import InfoButton from "./InfoButton.vue";

const { t, locale } = useI18n();

// Gemini review — locale-aware sayı formatı (sabit "tr-TR" kaldırıldı).
function localeTag(): string {
  return locale.value === "tr" ? "tr-TR" : "en-US";
}

interface DuplicateGroup {
  hash_hex: string;
  size_bytes: number;
  paths: string[];
}

interface DuplicateStats {
  group_count: number;
  redundant_bytes: number;
  elapsed_ms: number;
}

interface DuplicateScanResult {
  drive_letter: string;
  scanned_files: number;
  filtered_small: number;
  candidate_pairs: number;
  hash_errors: number;
  groups: DuplicateGroup[];
  stats: DuplicateStats;
}

interface DspaceError {
  kind: string;
  message: string;
}

const props = defineProps<{ drive: string; hasScan: boolean }>();

const minSizeKb = ref<number>(4);
const sizeOnly = ref<boolean>(false);
const result = ref<DuplicateScanResult | null>(null);
const error = ref<string | null>(null);
const running = ref<boolean>(false);
const expanded = ref<Set<string>>(new Set());

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

function formatIpcError(err: unknown): string {
  if (typeof err === "string") return err;
  const e = err as DspaceError;
  if (e && e.kind && e.message) return `[${e.kind}] ${e.message}`;
  return JSON.stringify(err);
}

function shortHash(h: string): string {
  if (h.startsWith("size:")) return h;
  return h.slice(0, 12) + "…" + h.slice(-4);
}

function reclaimableBytes(g: DuplicateGroup): number {
  return g.size_bytes * Math.max(0, g.paths.length - 1);
}

async function runScan() {
  running.value = true;
  error.value = null;
  result.value = null;
  try {
    result.value = await invoke<DuplicateScanResult>("find_duplicates_cmd", {
      drive: props.drive,
      minSizeBytes: minSizeKb.value * 1024,
      sizeOnly: sizeOnly.value,
    });
  } catch (err) {
    error.value = formatIpcError(err);
  } finally {
    running.value = false;
  }
}

function toggle(hash: string) {
  const next = new Set(expanded.value);
  if (next.has(hash)) next.delete(hash);
  else next.add(hash);
  expanded.value = next;
}
</script>

<template>
  <section class="card">
    <h2>
      {{ t("duplicate.title") }}
      <InfoButton :text="t('duplicate.intro')" />
    </h2>

    <p v-if="!hasScan" class="muted">
      {{ t("duplicate.needsScan") }}
    </p>

    <template v-else>
      <div class="dup-bar">
        <label class="dup-label">
          {{ t("duplicate.minSize") }}
          <input
            v-model.number="minSizeKb"
            type="number"
            min="1"
            step="1"
            class="dup-input mono"
          />
          KB
        </label>
        <label class="dup-label">
          <input v-model="sizeOnly" type="checkbox" />
          {{ t("duplicate.sizeOnly") }}
        </label>
        <button
          type="button"
          class="probe-btn"
          :disabled="running"
          @click="runScan"
        >
          {{ running ? t("duplicate.scanBusy") : t("duplicate.scan") }}
        </button>
      </div>

      <div v-if="result" class="grid">
        <div class="row">
          <span class="key">{{ t("duplicate.scanned") }}</span>
          <span class="val mono">
            {{ result.scanned_files.toLocaleString(localeTag()) }}
            {{ t("duplicate.filesUnit") }}
          </span>
          <span class="pill pill-ok">
            {{ result.candidate_pairs }} {{ t("duplicate.candidates") }}
          </span>
        </div>
        <div class="row">
          <span class="key">{{ t("duplicate.filtered") }}</span>
          <span class="val mono">
            {{
              result.filtered_small.toLocaleString(localeTag())
            }} {{ t("duplicate.smallFiles") }}
          </span>
        </div>
        <div class="row">
          <span class="key">{{ t("duplicate.groupCount") }}</span>
          <span class="val mono">{{ result.stats.group_count }}</span>
          <span
            v-if="result.stats.group_count > result.groups.length"
            class="pill pill-warn"
            :title="
              t('duplicate.truncatedTitle', {
                shown: result.groups.length,
                total: result.stats.group_count,
              })
            "
          >
            {{
              t("duplicate.truncated", {
                shown: result.groups.length,
                total: result.stats.group_count,
              })
            }}
          </span>
        </div>
        <div class="row">
          <span class="key">{{ t("duplicate.reclaim") }}</span>
          <span class="val mono">{{ formatBytes(result.stats.redundant_bytes) }}</span>
          <span class="pill pill-ok">{{ t("duplicate.canDelete") }}</span>
        </div>
        <div v-if="result.hash_errors > 0" class="row">
          <span class="key">{{ t("duplicate.hashErrors") }}</span>
          <span class="val mono">{{ result.hash_errors }}</span>
          <span
            class="pill pill-warn"
            :title="t('duplicate.hashErrorsTitle')"
          >
            {{ t("duplicate.lockedHint") }}
          </span>
        </div>
        <div class="row">
          <span class="key">{{ t("duplicate.duration") }}</span>
          <span class="val mono">{{ result.stats.elapsed_ms }} ms</span>
        </div>
      </div>

      <ul v-if="result && result.groups.length" class="dup-list">
        <li
          v-for="g in result.groups"
          :key="g.hash_hex"
          class="dup-group"
        >
          <button
            type="button"
            class="dup-head"
            :aria-expanded="expanded.has(g.hash_hex)"
            @click="toggle(g.hash_hex)"
          >
            <span class="dup-toggle">
              {{ expanded.has(g.hash_hex) ? "▾" : "▸" }}
            </span>
            <span class="dup-hash mono" :title="g.hash_hex">
              {{ shortHash(g.hash_hex) }}
            </span>
            <span class="dup-count">×{{ g.paths.length }}</span>
            <span class="dup-each mono">{{ formatBytes(g.size_bytes) }}</span>
            <span class="dup-reclaim mono">
              +{{ formatBytes(reclaimableBytes(g)) }}
            </span>
          </button>
          <ul v-if="expanded.has(g.hash_hex)" class="dup-paths">
            <li v-for="p in g.paths" :key="p" class="dup-path mono">
              {{ p }}
            </li>
          </ul>
        </li>
      </ul>
      <p v-if="result && result.groups.length === 0" class="muted">
        Eşleşen grup yok — bu bucket'larda hiç çift bulunmadı.
      </p>
      <p v-if="error" class="err">{{ error }}</p>
    </template>
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
  margin: 0 0 14px;
  font-size: 14px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--muted);
  font-weight: 600;
}

.muted {
  color: var(--muted);
  font-size: 13px;
}

.grid {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-bottom: 12px;
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
  padding: 2px 8px;
  border-radius: 999px;
  letter-spacing: 0.04em;
  font-weight: 600;
}

.pill-ok {
  background: #14532d33;
  color: #6ee7b7;
  border: 1px solid #14532d66;
}

.dup-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 14px;
  flex-wrap: wrap;
}

.dup-label {
  font-size: 12px;
  color: var(--muted);
  display: flex;
  align-items: center;
  gap: 6px;
}

.dup-input {
  width: 64px;
  padding: 4px 8px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--fg);
  text-align: right;
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

.dup-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 440px;
  overflow-y: auto;
}

.dup-group {
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
}

.dup-head {
  display: grid;
  grid-template-columns: 18px 180px 50px 100px 130px;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 8px 12px;
  background: transparent;
  border: none;
  color: var(--fg);
  cursor: pointer;
  text-align: left;
  font-size: 13px;
  border-radius: 8px;
  transition: background 0.15s;
}

.dup-head:hover {
  background: #1f6f7c22;
}

.dup-toggle {
  color: var(--muted);
}

.dup-hash {
  color: #93c5fd;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dup-count {
  color: #fcd34d;
  font-weight: 600;
  font-size: 12px;
}

.dup-each {
  color: var(--fg);
  text-align: right;
}

.dup-reclaim {
  color: #6ee7b7;
  font-weight: 500;
  text-align: right;
}

.dup-paths {
  list-style: none;
  margin: 0;
  padding: 0 12px 10px 38px;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.dup-path {
  font-size: 12px;
  color: var(--fg);
  word-break: break-all;
  opacity: 0.9;
}

.err {
  color: #fca5a5;
  font-family: ui-monospace, monospace;
  font-size: 13px;
  margin-top: 10px;
}
</style>
