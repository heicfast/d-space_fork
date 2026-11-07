// SPDX-License-Identifier: GPL-3.0-or-later
//
// Vitest config — Bölüm 20.1 (Test Piramidi, TypeScript unit).
// Gemini review — coverage bloğu kaldırıldı: `@vitest/coverage-v8`
// paketi kurulu değildi ve `vitest --coverage` çağrısını kırıyordu.
// Coverage gerektiğinde: `pnpm add -D @vitest/coverage-v8` + blok geri
// eklenebilir.

import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  test: {
    environment: "happy-dom",
    include: ["src/**/*.{test,spec}.ts"],
  },
});
