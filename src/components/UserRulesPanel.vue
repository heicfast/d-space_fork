<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!--
  Kullanıcı Tanımlı Kurallar paneli — Bölüm 6.4.

  Built-in 63 kural sabit. Kullanıcı kendi pattern'lerini ekler;
  build_tree döngüsünde önce user rules denenir, eşleşme yoksa
  built-in. UI'da advanced mode'da görünür.

  Pattern tipi: 'name' (tam isim, case-insensitive) veya 'extension'
  (.ext suffix). Glob/regex v0.2.
-->
<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";

// Gemini review — panel tamamen i18n'e taşındı (EN locale'de Türkçe
// görünmesin).
const { t } = useI18n();

type PatternType = "name" | "extension";

interface UserRule {
  id: number;
  pattern: string;
  pattern_type: PatternType;
  score: number;
  explanation: string;
  enabled: boolean;
  created_at_unix: number;
  updated_at_unix: number;
}

interface DspaceError {
  kind: string;
  message: string;
}

const rules = ref<UserRule[]>([]);
const error = ref<string | null>(null);
const busy = ref<boolean>(false);

const newPattern = ref<string>("");
const newPatternType = ref<PatternType>("name");
const newScore = ref<number>(50);
const newExplanation = ref<string>("");

function formatIpcError(err: unknown): string {
  if (typeof err === "string") return err;
  const e = err as DspaceError;
  if (e && e.kind && e.message) return `[${e.kind}] ${e.message}`;
  return JSON.stringify(err);
}

function scoreTierClass(score: number): string {
  if (score <= 30) return "score-danger";
  if (score <= 60) return "score-caution";
  if (score <= 85) return "score-likely";
  return "score-cache";
}

function scoreTierLabel(score: number): string {
  if (score <= 30) return "DOKUNMA";
  if (score <= 60) return "İNCELE";
  if (score <= 85) return "BÜYÜK İHTİMAL";
  return "CACHE";
}

async function loadRules() {
  try {
    rules.value = await invoke<UserRule[]>("list_user_rules_cmd");
  } catch (err) {
    error.value = formatIpcError(err);
  }
}

async function addRule() {
  if (!newPattern.value.trim()) {
    error.value = "Pattern boş olamaz";
    return;
  }
  busy.value = true;
  error.value = null;
  try {
    await invoke<UserRule>("add_user_rule_cmd", {
      pattern: newPattern.value.trim(),
      patternType: newPatternType.value,
      score: newScore.value,
      explanation: newExplanation.value.trim() || "Kullanıcı kuralı",
    });
    newPattern.value = "";
    newExplanation.value = "";
    newScore.value = 50;
    await loadRules();
  } catch (err) {
    error.value = formatIpcError(err);
  } finally {
    busy.value = false;
  }
}

async function removeRule(id: number) {
  busy.value = true;
  error.value = null;
  try {
    await invoke("delete_user_rule_cmd", { id });
    await loadRules();
  } catch (err) {
    error.value = formatIpcError(err);
  } finally {
    busy.value = false;
  }
}

async function toggleRule(rule: UserRule) {
  busy.value = true;
  error.value = null;
  try {
    await invoke("toggle_user_rule_cmd", {
      id: rule.id,
      enabled: !rule.enabled,
    });
    await loadRules();
  } catch (err) {
    error.value = formatIpcError(err);
  } finally {
    busy.value = false;
  }
}

onMounted(loadRules);
</script>

<template>
  <section class="card">
    <h2>{{ t("userRules.title") }}</h2>
    <p class="muted">{{ t("userRules.intro") }}</p>

    <div class="rule-add">
      <input
        v-model="newPattern"
        class="rule-input mono"
        :placeholder="t('userRules.patternPlaceholder')"
        :disabled="busy"
      />
      <select
        v-model="newPatternType"
        class="rule-select"
        :disabled="busy"
      >
        <option value="name">{{ t("userRules.typeName") }}</option>
        <option value="extension">{{ t("userRules.typeExtension") }}</option>
      </select>
      <input
        v-model.number="newScore"
        type="number"
        min="0"
        max="100"
        class="rule-score mono"
        :disabled="busy"
      />
      <span class="score-pill" :class="scoreTierClass(newScore)">
        {{ scoreTierLabel(newScore) }}
      </span>
      <input
        v-model="newExplanation"
        class="rule-input mono"
        :placeholder="t('userRules.explanationPlaceholder')"
        :disabled="busy"
      />
      <button
        type="button"
        class="probe-btn"
        :disabled="busy || !newPattern.trim()"
        @click="addRule"
      >
        {{ t("userRules.add") }}
      </button>
    </div>

    <ul v-if="rules.length" class="rule-list">
      <li
        v-for="r in rules"
        :key="r.id"
        class="rule-row"
        :class="{ 'rule-disabled': !r.enabled }"
      >
        <span class="rule-type mono">
          {{ r.pattern_type === "name" ? "📁" : ".🏷" }}
        </span>
        <span class="rule-pattern mono">{{ r.pattern }}</span>
        <span class="score-pill" :class="scoreTierClass(r.score)">
          {{ r.score }} · {{ scoreTierLabel(r.score) }}
        </span>
        <span class="rule-explanation">{{ r.explanation }}</span>
        <button
          type="button"
          class="stage-btn"
          :disabled="busy"
          :title="r.enabled ? t('userRules.disable') : t('userRules.enable')"
          @click="toggleRule(r)"
        >
          {{ r.enabled ? "⏸" : "▶" }}
        </button>
        <button
          type="button"
          class="stage-btn perm-trigger"
          :disabled="busy"
          :title="t('userRules.delete')"
          @click="removeRule(r.id)"
        >
          🗑
        </button>
      </li>
    </ul>
    <p v-else class="muted">{{ t("userRules.empty") }}</p>
    <p v-if="error" class="err">{{ error }}</p>
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
  margin: 0 0 8px;
  font-size: 14px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--muted);
  font-weight: 600;
}

.muted {
  color: var(--muted);
  font-size: 12px;
  margin: 0 0 14px;
}

.rule-add {
  display: grid;
  grid-template-columns: 1.5fr 1.2fr 70px 90px 1.5fr 80px;
  gap: 8px;
  align-items: center;
  margin-bottom: 14px;
  flex-wrap: wrap;
}

.rule-input,
.rule-select,
.rule-score {
  padding: 6px 10px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--fg);
  font-size: 12px;
}

.rule-input:focus,
.rule-select:focus,
.rule-score:focus {
  outline: none;
  border-color: #24c8db;
}

.rule-score {
  text-align: right;
}

.probe-btn {
  padding: 6px 14px;
  background: #2563eb;
  border: 1px solid #1d4ed8;
  border-radius: 6px;
  color: #ffffff;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease,
    transform 0.08s ease;
}

.probe-btn:hover:not(:disabled) {
  background: #1d4ed8;
}

.probe-btn:active:not(:disabled) {
  transform: scale(0.97);
}

.probe-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.rule-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.rule-row {
  display: grid;
  grid-template-columns: 28px 1.5fr 130px 1fr 36px 36px;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: 12px;
}

.rule-disabled {
  opacity: 0.5;
}

.rule-type {
  text-align: center;
}

.rule-pattern {
  color: var(--fg);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.score-pill {
  font-size: 10px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 999px;
  text-align: center;
  letter-spacing: 0.04em;
  white-space: nowrap;
  font-family: ui-monospace, monospace;
}

.score-danger {
  background: #7f1d1d33;
  color: #fca5a5;
  border: 1px solid #7f1d1d80;
}

.score-caution {
  background: #78350f33;
  color: #fcd34d;
  border: 1px solid #78350f80;
}

.score-likely {
  background: #14532d33;
  color: #6ee7b7;
  border: 1px solid #14532d80;
}

.score-cache {
  background: #1e3a8a33;
  color: #93c5fd;
  border: 1px solid #1e3a8a80;
}

.rule-explanation {
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.stage-btn {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--fg);
  padding: 3px 8px;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
}

.stage-btn:hover:not(:disabled) {
  background: #1f6f7c33;
  border-color: #24c8db66;
}

.perm-trigger:hover:not(:disabled) {
  background: #7f1d1d33;
  border-color: #7f1d1d80;
}

.err {
  color: #fca5a5;
  font-family: ui-monospace, monospace;
  font-size: 12px;
  margin-top: 8px;
}
</style>
