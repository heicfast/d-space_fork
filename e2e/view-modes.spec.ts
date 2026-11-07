// SPDX-License-Identifier: GPL-3.0-or-later
//
// E2E #2: Görsel mod değişimi — Sprint 3.5 smoke.
// Gemini review E4 — eski spec var-olmayan `button.view-mode-N` class'larını
// arıyordu. Gerçek markup: `.view-chip` + aktif mod `.view-chip-active`.
// Görünürlük için mock artık scan_full + tree_window + tree_path sağlar
// (drilldown kartı v-if="viewWindow" ile render olur); test tarama
// butonunu tetikler. hasText substring (string) kullanır — regex ^$ anchor
// whitespace'li textContent'e takılıyordu.

import { test, expect } from "@playwright/test";
import { buildTauriMockScript } from "./fixtures/tauri-mock";

const MODE_NAMES = ["Sunburst", "Treemap", "Bubble", "Timeline", "3D"];

async function startScanned(page: import("@playwright/test").Page) {
  await page.goto("/");
  await expect(page.locator(".hero h1")).toBeVisible();
  // Drilldown kartı v-if="viewWindow" — önce taramayı başlat (mock'lu).
  await page.getByRole("button", { name: /Tara:/ }).click();
  await expect(page.locator(".view-chip").first()).toBeVisible({
    timeout: 10_000,
  });
}

test.describe("View modes", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(
      buildTauriMockScript({ showOnboarding: false, withScan: true }),
    );
  });

  test("view-chip butonları mod değiştirir ve aktif sınıf taşınır", async ({
    page,
  }) => {
    await startScanned(page);

    for (const name of MODE_NAMES) {
      await page
        .locator(".view-chip", { hasText: name })
        .first()
        .click();
      await expect(
        page.locator(".view-chip-active", { hasText: name }),
      ).toHaveCount(1, { timeout: 3000 });
    }
  });

  test("Ctrl+1..5 klavye kısayolu mod değiştirir", async ({ page }) => {
    await startScanned(page);

    for (let i = 0; i < MODE_NAMES.length; i++) {
      await page.keyboard.press(`Control+${i + 1}`);
      await expect(
        page.locator(".view-chip-active", { hasText: MODE_NAMES[i] }),
      ).toHaveCount(1, { timeout: 3000 });
    }
  });
});
