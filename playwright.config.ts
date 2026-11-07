// SPDX-License-Identifier: GPL-3.0-or-later
//
// Playwright e2e config — Sprint 3.5 (Bölüm 20.4).
//
// İki kanal:
//   * `webdriver` projesi — gerçek Tauri build üzerinden `tauri-driver`
//     köprüsü. CI'da `windows-latest` runner'da `cargo install
//     tauri-driver` sonrası `pnpm tauri build --debug` + e2e koşar.
//   * `web` projesi — Vite dev server üzerinden saf web context.
//     Tauri-only API'leri stub'lar (window.__TAURI__ enjekte edilir).
//     Hızlı geri besleme — yerel geliştirici WiX/NSIS build beklemeden
//     UI akışını doğrulayabilir.
//
// Yerel komutlar:
//   pnpm test:e2e:install      # Chromium binary indir (ilk seferlik)
//   pnpm test:e2e              # Tüm e2e koşusu
//   pnpm test:e2e --project=web
//
// CI: `.github/workflows/e2e.yml` — şimdilik `continue-on-error: true`
// ile zorunlu değil (Sprint 3.5 stub; tauri-driver bridge v0.2.0-beta
// sonrası gerçek E2E gate).

import { defineConfig, devices } from "@playwright/test";

const PORT = Number(process.env.D_SPACE_E2E_PORT ?? 1420);
const HOST = process.env.D_SPACE_E2E_HOST ?? "127.0.0.1";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  expect: { timeout: 5_000 },
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? [["github"], ["list"]] : "list",

  use: {
    baseURL: `http://${HOST}:${PORT}`,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },

  projects: [
    {
      name: "web",
      use: { ...devices["Desktop Chrome"] },
    },
    // Gemini review W6 — webdriver projesi varsılan koşuda hiçbir spec'i
    // eleyemiyordu (testIgnore hiçbir şeye eşleşmiyordu) ve var-olmayan
    // tauri-driver 4444'e bağlanıp tüm spec'leri düşürüyordu. Artık
    // AÇIKÇA opt-in: yalnız `D_SPACE_E2E_WEBDRIVER=1` ile liste'ye girer.
    ...(process.env.D_SPACE_E2E_WEBDRIVER === "1"
      ? [
          {
            name: "webdriver",
            testIgnore: ["**/*.spec.ts"],
            use: {
              baseURL: `http://${HOST}:4444`,
            },
          },
        ]
      : []),
  ],

  webServer: process.env.D_SPACE_E2E_SKIP_SERVER
    ? undefined
    : {
        command: "pnpm dev",
        url: `http://${HOST}:${PORT}`,
        reuseExistingServer: !process.env.CI,
        timeout: 30_000,
      },
});
