<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!--
  Tauri auto-updater UI — Sprint 3.6 (Bölüm 21.4).

  Davranış:
    * Header'a yerleştirilen küçük buton. Tıklayınca `check()` çağrılır.
    * Güncelleme varsa modal: sürüm, notlar, "İndir ve kur" / "Kapat".
    * İndirme sırasında progress (done/total bayt) gösterilir; tamam olunca
      `relaunch()` çağrılır.
    * Hatalar tip ayrımıyla i18n metni: imza doğrulama, ağ, beklenmedik.

  Tauri 2 `@tauri-apps/plugin-updater` API'sini kullanır. pubkey eşleşmezse
  `check()` veya `downloadAndInstall()` reddeder; hata kullanıcıya gösterilir.
-->
<script setup lang="ts">
import { ref, computed } from "vue";
import { useI18n } from "vue-i18n";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

const { t } = useI18n();

type Phase =
  | "idle"
  | "checking"
  | "up_to_date"
  | "available"
  | "installing"
  | "error";

const phase = ref<Phase>("idle");
const currentVersion = ref<string>("");
const newVersion = ref<string>("");
const notes = ref<string>("");
const progressDone = ref<number>(0);
const progressTotal = ref<number>(0);
const errorMessage = ref<string>("");
const errorKind = ref<"verify" | "network" | "unknown" | null>(null);
const pendingUpdate = ref<Update | null>(null);

const showModal = computed(
  () => phase.value === "available" || phase.value === "installing",
);

function classifyError(err: unknown): "verify" | "network" | "unknown" {
  const msg = String(err ?? "").toLowerCase();
  if (
    msg.includes("signature") ||
    msg.includes("verify") ||
    msg.includes("pubkey") ||
    msg.includes("pub_key") ||
    msg.includes("ed25519")
  ) {
    return "verify";
  }
  if (
    msg.includes("network") ||
    msg.includes("dns") ||
    msg.includes("connect") ||
    msg.includes("timeout") ||
    msg.includes("fetch") ||
    msg.includes("reqwest")
  ) {
    return "network";
  }
  return "unknown";
}

async function onCheck(): Promise<void> {
  errorMessage.value = "";
  errorKind.value = null;
  phase.value = "checking";
  try {
    const update = await check();
    if (!update) {
      phase.value = "up_to_date";
      return;
    }
    pendingUpdate.value = update;
    currentVersion.value = update.currentVersion;
    newVersion.value = update.version;
    notes.value = update.body ?? "";
    phase.value = "available";
  } catch (err) {
    errorKind.value = classifyError(err);
    errorMessage.value = String(err ?? "");
    phase.value = "error";
  }
}

async function onInstall(): Promise<void> {
  if (!pendingUpdate.value) return;
  phase.value = "installing";
  progressDone.value = 0;
  progressTotal.value = 0;
  try {
    await pendingUpdate.value.downloadAndInstall((event) => {
      if (event.event === "Started") {
        progressTotal.value = event.data.contentLength ?? 0;
      } else if (event.event === "Progress") {
        progressDone.value += event.data.chunkLength;
      }
    });
    await relaunch();
  } catch (err) {
    errorKind.value = classifyError(err);
    errorMessage.value = String(err ?? "");
    phase.value = "error";
  }
}

function onDismiss(): void {
  // Gemini review — Update nesnesi kapatılmalıdır (Tauri updater kaynağı
  // serbest kalır; bırakmak kaynak sızdırıyordu). Mock/test nesnelerinde
  // metod yokluğuna karşı savunmacı çağrı.
  const upd = pendingUpdate.value as { close?: () => void } | null;
  if (upd && typeof upd.close === "function") {
    upd.close();
  }
  phase.value = "idle";
  pendingUpdate.value = null;
  newVersion.value = "";
  notes.value = "";
  progressDone.value = 0;
  progressTotal.value = 0;
}

const errorText = computed<string>(() => {
  if (errorKind.value === "verify") return t("update.errorVerify");
  if (errorKind.value === "network")
    return t("update.errorNetwork", { msg: errorMessage.value });
  return t("update.errorUnknown", { msg: errorMessage.value });
});
</script>

<template>
  <div class="update-notification">
    <button
      type="button"
      class="check-btn"
      :disabled="phase === 'checking' || phase === 'installing'"
      :aria-label="t('update.check')"
      @click="onCheck"
    >
      <span v-if="phase === 'checking'">⏳ {{ t("update.checking") }}</span>
      <span v-else-if="phase === 'up_to_date'">✔ {{ t("update.upToDate", { version: newVersion || currentVersion }) }}</span>
      <span v-else-if="phase === 'error'">⚠ {{ errorText }}</span>
      <span v-else>⤓ {{ t("update.check") }}</span>
    </button>

    <div
      v-if="showModal"
      class="update-modal-backdrop"
      role="dialog"
      :aria-label="t('update.available', { version: newVersion })"
    >
      <div class="update-modal">
        <h2>{{ t("update.available", { version: newVersion }) }}</h2>
        <p class="current-line">
          {{ t("update.currentLine", { current: currentVersion }) }}
        </p>
        <div v-if="notes" class="notes">
          <h3>{{ t("update.notes") }}</h3>
          <pre>{{ notes }}</pre>
        </div>
        <div v-if="phase === 'installing'" class="progress">
          <div class="progress-text">
            {{
              t("update.progress", {
                done: progressDone,
                total: progressTotal,
              })
            }}
          </div>
          <div
            class="progress-bar"
            :style="{
              width:
                progressTotal > 0
                  ? Math.min(100, (progressDone / progressTotal) * 100) + '%'
                  : '0%',
            }"
          ></div>
          <p class="hint">{{ t("update.restartHint") }}</p>
        </div>
        <div v-else class="actions">
          <button type="button" class="primary" @click="onInstall">
            {{ t("update.install") }}
          </button>
          <button type="button" class="secondary" @click="onDismiss">
            {{ t("update.dismiss") }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.update-notification {
  display: inline-flex;
  align-items: center;
}

.check-btn {
  font-size: 12px;
  padding: 4px 10px;
  border-radius: 6px;
  border: 1px solid var(--border, #2a2c34);
  background: var(--surface, transparent);
  color: var(--fg, inherit);
  cursor: pointer;
  white-space: nowrap;
}

.check-btn:hover:not(:disabled) {
  border-color: var(--fg, currentColor);
}

.check-btn:disabled {
  opacity: 0.6;
  cursor: progress;
}

.update-modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.update-modal {
  background: var(--surface, #14171c);
  color: var(--fg, #e5e7eb);
  width: min(560px, 92vw);
  max-height: 80vh;
  border: 1px solid var(--border, #1f242c);
  border-radius: 10px;
  padding: 24px;
  box-shadow: 0 20px 50px var(--shadow, rgba(0, 0, 0, 0.4));
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.update-modal h2 {
  margin: 0;
  font-size: 18px;
}

.current-line {
  color: var(--muted, #9ca3af);
  font-size: 13px;
  margin: 0;
}

.notes pre {
  white-space: pre-wrap;
  font-family: inherit;
  background: var(--bg, rgba(0, 0, 0, 0.2));
  color: var(--fg, inherit);
  padding: 8px 10px;
  border-radius: 6px;
  border: 1px solid var(--border, transparent);
  font-size: 13px;
  max-height: 260px;
  overflow: auto;
}

.actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  margin-top: 8px;
}

.actions button {
  padding: 8px 16px;
  border-radius: 6px;
  border: 1px solid transparent;
  font-size: 13px;
  cursor: pointer;
}

.actions .primary {
  background: #3b8eea;
  color: white;
}

.actions .primary:hover:not(:disabled) {
  background: #4d9aee;
}

.actions .secondary {
  background: transparent;
  border-color: var(--border, #2a2c34);
  color: var(--fg, inherit);
}

.actions .secondary:hover {
  border-color: var(--fg, currentColor);
}

.progress {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.progress-text {
  font-size: 12px;
  color: var(--muted, #9ca3af);
}

.progress-bar {
  height: 6px;
  background: #3b8eea;
  border-radius: 3px;
  transition: width 0.2s ease-out;
  min-width: 4px;
}

.hint {
  font-size: 12px;
  margin: 4px 0 0;
  color: var(--muted, #9ca3af);
}
</style>
