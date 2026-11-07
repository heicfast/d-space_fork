<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# D-Space e2e — Sprint 3.5+ (Bölüm 20.4)

Playwright tabanlı uçtan uca testler. İki kanal var:

| Proje | Çalışma yeri | Tauri IPC | Hız | Amaç |
|---|---|---|---|---|
| `web` | Vite dev server | stub (`__TAURI_INTERNALS__.invoke`) | hızlı | UI flow regresyon, golden path |
| `webdriver` | `tauri-driver` köprüsü | gerçek backend | yavaş | Bundle düzeyinde smoke (release adayı) — **opt-in** |

## Yerel koşum

```powershell
pnpm test:e2e:install       # Chromium binary, ilk seferlik
pnpm test:e2e --project=web # Hızlı web kanalı
```

Not: bazı ortamlarda Vite yalnız `localhost`'a bağlanır (IPv6 çözümlemesi);
`pnpm dev` 127.0.0.1'den erişilemezse `D_SPACE_E2E_HOST=localhost` ortam
değişkeniyle koşun.

`webdriver` projesi varsayılan koşuda **dahil değildir** (tauri-driver
gerektirir); açıkça opt-in edilir:

```powershell
cargo install tauri-driver --locked
pnpm tauri build --debug
$env:D_SPACE_E2E_WEBDRIVER = "1"
pnpm test:e2e --project=webdriver
```

## Smoke senaryoları (`web` kanal stub'ları)

`web` kanalı IPC'yi mocklar — `e2e/fixtures/tauri-mock.ts` her `invoke`
çağrısını deterministik response'larla cevaplar. Mock, `onboarding_done`
settings anahtarını, `withScan: true` ile `scan_full` + `tree_window` +
`tree_path` verilerini de sağlar (drilldown kartı render olur). Smoke
testler 6 akışı kapsar:

1. **`onboarding.spec.ts`** — 4 slide ileri, mod seçimi, "Başla" → kapanış
2. **`quick-search.spec.ts`** — Ctrl+F arama kutusu fokus + debounce
3. **`theme-toggle.spec.ts`** — tema butonu üç state döngüsü
4. **`update-check.spec.ts`** — güncelleme kontrolü + hata mesajı
5. **`view-modes.spec.ts`** — view-chip tıklama + Ctrl+1..5 görsel mod değişimi
6. (Aynı dosyada) aktif mod `.view-chip-active` sınıf taşıması

Her smoke yalnız UI akışını doğrular; gerçek backend davranışı
`tauri-driver` kanalında ayrı koşulur (v0.4 planı).
