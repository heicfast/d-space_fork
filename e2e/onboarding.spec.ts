// SPDX-License-Identifier: GPL-3.0-or-later
//
// E2E #1: Onboarding flow — Sprint 3.5 smoke (Bölüm 20.4).
// Gemini review E2/E3 — selector'lar .slide-title'a scope'landı (strict-mode
// çakışması: "Görmek" hem slide başlığında hem tagline'da geçiyordu) ve
// slayt sayısı 4'e güncellendi (karşılaştırma tablosu slaytı eklendi).

import { test, expect } from "@playwright/test";
import { buildTauriMockScript } from "./fixtures/tauri-mock";

test.describe("Onboarding", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(
      buildTauriMockScript({ showOnboarding: true, indexMode: "idle" }),
    );
  });

  test("dört slide ilerler, mod seçimi sonrası kapanır", async ({ page }) => {
    await page.goto("/");
    const slideTitle = page.locator(".slide-title");

    await expect(slideTitle.first()).toBeVisible();
    await page.getByRole("button", { name: /İleri/ }).click();
    await expect(slideTitle.first()).toBeVisible();
    await page.getByRole("button", { name: /İleri/ }).click();
    await expect(slideTitle.first()).toBeVisible();
    await page.getByRole("button", { name: /İleri/ }).click();
    await expect(slideTitle.first()).toBeVisible();
    // 4. slayttan sonra mod seçim ekranına geçiş.
    await page.getByRole("button", { name: /Devam/ }).click();

    await expect(page.getByText(/Tarama Modu Seçimi/)).toBeVisible();
    await page.getByText(/Hızlı Mod/).click();
    await page.getByRole("button", { name: /Başla/ }).click();
    // Onboarding kapanmış olmalı — header görünür
    await expect(page.locator(".hero h1")).toBeVisible();
  });
});
