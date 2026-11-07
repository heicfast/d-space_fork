<div align="center">

# D-Space

**Akıllı Disk Analiz ve Geri Kazanım Platformu — Windows için**
*Smart disk analysis & recovery platform for Windows*

*Görmek, anlamak, geri kazanmak.*
*See, understand, reclaim.*

🌐 **TR · EN** — Bu README iki dillidir / This README is bilingual (English collapsibles below each section)

> ℹ️ **Bu depo / This repository:** `heicfast/d-space_fork` — upstream
> [AmrasElessar/d-space](https://github.com/AmrasElessar/d-space)'in sürüm kanalı
> ayrımı. Upstream katkıları için orijinal depoyu kullanın.
> *Release channel fork of the upstream project; use the original repo for upstream contributions.*

</div>

<div align="center">

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPLv3%2B-blue.svg)](https://www.gnu.org/licenses/gpl-3.0.en.html)
![Status](https://img.shields.io/badge/status-beta-orange)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11%20%C2%B7%20x64-0078D6)
![Rust](https://img.shields.io/badge/Rust-1.80%2B-CE412B?logo=rust)
![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB?logo=tauri)
![Vue](https://img.shields.io/badge/Vue-3.5-4FC08D?logo=vuedotjs)
![Spec](https://img.shields.io/badge/spec-v1.4_frozen-1e3a8a)
![D Brand](https://img.shields.io/badge/D_Brand-family-9333ea)

</div>

---

## 🎬 Demo

<div align="center">

> 🎥 Demo video yakında — Faz 1 sunburst donut + drilldown + staging/undo akışlarını kapsayan kayıt hazırlanıyor.
> *Demo video coming soon — a recording covering Phase 1 sunburst donut + drilldown + staging/undo flows is in progress.*

`[ Sunburst donut · drilldown · staging recovery · cross-volume two-phase commit ]`

</div>

---

## 📌 Kısaca

D-Space, Windows üzerinde disk alanını **saniyeler içinde haritalayan**, **ne silinebileceğini bilen** ve **geri alma garantisi sunan** akıllı bir disk analiz platformudur. MFT tabanlı tarama motoru sayesinde 1TB SSD'yi 5 saniyenin altında tarayabilir, her dosyaya 0–100 arası güvenli-silme skoru atar ve hiçbir silmeyi doğrudan yapmaz — her şey atomik olarak staging klasörüne taşınır, 24 saatlik pencerede tek tıkla geri alınabilir.

**Tauri v2** + **Rust 1.80+** + **Vue 3** + **SQLite (WAL)** üzerine kurulu, **GPL-3.0-or-later** lisanslı bir D Brand projesidir. Hedef platform: Windows 10/11 x64 (ARM64 desteği Faz 2'de).

<details>
<summary>🇬🇧 At a glance (English)</summary>

D-Space is a smart disk analyzer for Windows that **maps disk usage in seconds**, **knows what is safe to delete**, and **guarantees recovery**. Its MFT-based scanner targets sub-5-second scans of a 1TB SSD, assigns every file a 0–100 safe-to-delete score, and never deletes directly — every deletion is atomically moved to a staging folder and can be undone in a single click within a 24-hour window.

Built on **Tauri v2** + **Rust 1.80+** + **Vue 3** + **SQLite (WAL)** under the **GPL-3.0-or-later** license, D-Space is part of the D Brand family. Target platform: Windows 10/11 x64 (ARM64 support in Phase 2).

</details>

---

## 🆕 Yenilikler / What's Done

> Faz 1 implementasyonu devam ediyor. Spec v1.4 dondurulmuş (37 bölüm); kod sırasında çıkan keşifler Bölüm 28 Discovery Log'a düşer. Aşağıda bugüne kadarki çıkış noktaları.
> *Phase 1 in progress. Spec v1.4 is frozen (37 sections); discoveries during coding land in Section 28 Discovery Log. Highlights so far below.*

- 📦 **v0.3.0-beta · 159 Rust (+9 VSS-gated) + 34 frontend + 6 e2e test geçiyor · 5/5 pillar canlı**
- ⚡ **MFT tarama motoru** — `ntfs` crate üzerine MFT probe + full walk + FindFirstFile fallback (3 katmanlı yetki stratejisi, UAC ilk açılışta sorulmuyor)
- 🍩 **Sunburst donut** — hand-rolled SVG (d3 yok), drilldown + breadcrumb, Vue 3 Composition API
- 🧠 **72 kuralı kapsayan zeka katmanı** — 4 renk tier (kırmızı/sarı/yeşil/mavi), 0–100 güvenli-silme skoru
- 🕒 **Zaman boyutu** — günlük snapshot capture + delta (added/removed/grew/shrunk top 10)
- 🛟 **Cross-volume two-phase commit** — `.dspace_tmp` + WAL + Blake3 hash verify + atomik rename, başlangıçta otomatik crash recovery
- 🪟 **Lazy viewport query** — Vue ağacı tam tutmaz; ~200 düğümlük pencere ister, Rust `Arc<ScanTree>` tek sahibi
- 🗄️ **SQLite WAL mode** — `busy_timeout=5000`, `synchronous=NORMAL`, `rusqlite_migration` ile şema yönetimi
- 🔬 **VSS hot-path izolasyonu** — VSS scan sırasında ASLA çalışmaz; sadece duplicate hash / drill-down'da

<details>
<summary>🇬🇧 What's done (English)</summary>

- 📦 **v0.3.0-beta · 159 Rust (+9 VSS-gated) + 34 frontend + 6 e2e tests passing · 5/5 pillars live**
- ⚡ **MFT scan engine** — MFT probe + full walk + FindFirstFile fallback on the `ntfs` crate (three-tier privilege strategy; no UAC on first launch)
- 🍩 **Sunburst donut** — hand-rolled SVG (no d3), drilldown + breadcrumb, Vue 3 Composition API
- 🧠 **72-rule intelligence layer** — 4-color tier (red/yellow/green/blue), 0–100 safe-to-delete score
- 🕒 **Time dimension** — daily snapshot capture + delta (added/removed/grew/shrunk top 10)
- 🛟 **Cross-volume two-phase commit** — `.dspace_tmp` + WAL + Blake3 hash verify + atomic rename, automatic crash recovery on startup
- 🪟 **Lazy viewport query** — Vue never owns the full tree; requests ~200-node windows; Rust holds the single `Arc<ScanTree>`
- 🗄️ **SQLite WAL mode** — `busy_timeout=5000`, `synchronous=NORMAL`, schema management via `rusqlite_migration`
- 🔬 **VSS hot-path isolation** — VSS NEVER runs during a scan; only during duplicate hashing / drill-down

</details>

---

## 🎯 Vizyon / Vision

D-Space, Windows üzerinde disk alanını saniyeler içinde haritalayan, ne silinebileceğini bilen ve geri alma garantisi sunan akıllı disk analiz platformudur. WinDirStat, TreeSize ve SpaceSniffer gibi araçların sağlam temellerini referans alır; üstüne **MFT-yerli hız**, **kural + skor tabanlı zeka katmanı**, **zaman boyutu** ve **atomik geri-kazanım** ekler.

Hedef, kullanıcının diske dair tüm soruları — "ne nerede?", "ne silinebilir?", "dün ne büyüdü?", "geri alabilir miyim?" — tek bir uygulamada cevaplamasıdır.

<details>
<summary>🇬🇧 Vision (English)</summary>

D-Space is a smart disk analyzer for Windows that maps disk usage in seconds, knows what's safe to delete, and guarantees recovery. It builds on the solid foundations of tools like WinDirStat, TreeSize, and SpaceSniffer, and adds **MFT-native speed**, a **rule + score intelligence layer**, a **time dimension**, and **atomic recovery**.

The goal: answer every question a user has about their disk — "what is where?", "what is safe to delete?", "what grew yesterday?", "can I undo this?" — inside a single application.

</details>

---

## 🚀 İlk Adımlar / Quick Start

### 💻 Geliştirici ortamı / Developer setup

```powershell
# Bağımlılıklar / Dependencies
pnpm install

# Geliştirme — pencereyi açar (Vite dev server + Tauri)
# Development — opens the window (Vite dev server + Tauri)
pnpm tauri dev

# Yapım — debug binary (src-tauri/target/debug/dspace.exe)
# Build — debug binary
cd src-tauri
cargo build

# Test (Windows)
cargo test --lib            # 159 birim test geçer / unit tests pass
cargo test --lib --features vss  # +9 VSS-gated test
pnpm build                  # vue-tsc + vite build
```

### 🪜 İlk denemeler / First things to try

1. **`pnpm tauri dev`** çalıştır → uygulama açılır, ilk taramayı başlat.
2. Bir sürücü seç (`C:`, `D:` …) → MFT probe denenir; başarısızsa otomatik FindFirstFile fallback'e düşer.
3. **Sunburst donut** üzerinden drilldown yap → breadcrumb ile geri dön.
4. **Silmek istediğin** bir dosyayı seç → staging'e taşınır, 24 saat boyunca **Undo** ile geri alınabilir.
5. **`cargo test --lib`** → 159 birim test, pillar başına regression koruması.

<details>
<summary>🇬🇧 Quick Start (English)</summary>

### Developer setup

```powershell
pnpm install
pnpm tauri dev
cd src-tauri && cargo build
cargo test --lib
pnpm build
```

### First things to try

1. Run `pnpm tauri dev` → the app opens; start the first scan.
2. Pick a volume (`C:`, `D:`, …) → MFT probe is attempted; on failure it falls back to FindFirstFile automatically.
3. Drilldown through the **sunburst donut** → use the breadcrumb to navigate back.
4. Select a file to **delete** → it is moved to staging and can be **undone** for 24 hours.
5. Run `cargo test --lib` → 159 unit tests, per-pillar regression coverage.

</details>

---

## ✨ Öne Çıkan Özellikler / Key Features

### 🤝 Üç Çekirdek Söz / Three Core Promises

- **⚡ Hız / Speed** — MFT tabanlı tarama motoru ile **1TB SSD < 5 saniye** hedefi. NTFS Master File Table doğrudan okunur; dosya sistemi gezme overhead'i ortadan kalkar.
- **🧠 Zeka / Intelligence** — Her dosyaya 0–100 arası **güvenli-silme skoru**. 33 kural + 4 renk tier (kırmızı/sarı/yeşil/mavi). ML tier v2'de.
- **🛟 Geri kazanım / Recovery** — **Direkt silme yok.** Tüm silmeler staging klasörüne atomik MOVE'lanır; 24 saatlik pencerede tek tıkla geri alınabilir. Cross-volume durumlarda two-phase commit + Blake3 hash doğrulaması.

<details>
<summary>🇬🇧 Three Core Promises (English)</summary>

- **⚡ Speed** — MFT-based scanner targeting **1TB SSD in under 5 seconds**. Reading NTFS MFT directly removes filesystem-walk overhead.
- **🧠 Intelligence** — A 0–100 **safe-to-delete score** for every file. 33 rules + 4-color tier (red/yellow/green/blue). ML tier in v2.
- **🛟 Recovery** — **No direct deletion.** All deletions are atomically moved to a staging folder and can be undone in one click within a 24-hour window. Cross-volume operations use two-phase commit + Blake3 hash verification.

</details>

### 🏛️ Beş Pillar / Five Pillars (Bölüm / Section 3)

| # | Pillar | Durum / Status |
|---|--------|----------------|
| 1 | **Hız / Speed** — MFT motoru / engine | ✅ MFT probe + full walk + FindFirstFile fallback |
| 2 | **Görsel zarafet / Visual elegance** — sunburst/treemap/bubble/timeline | ✅ Sunburst donut + drilldown + breadcrumb |
| 3 | **Zeka katmanı / Intelligence layer** — 50+ kural / rules + ML tier | ✅ 72 kural / rules, 4 renk tier / 4-color tier (kırmızı/sarı/yeşil/mavi); ML v2 |
| 4 | **Zaman boyutu / Time dimension** — günlük snapshot + delta / daily snapshot + delta | ✅ Capture + delta (added/removed/grew/shrunk top 10) |
| 5 | **Geri kazanım garantisi / Recovery guarantee** — staging + undo | ✅ Same-volume + cross-volume two-phase commit + WAL |

### 🏗️ Mimari İlkeler / Architectural Principles

- **Single Source of Truth (Bölüm / Section 4.4)** — `Arc<ScanTree>` Rust tarafında **tek sahip**. Vue sadece `NodeId` referansı tutar; ağaç hiçbir zaman frontend'e kopyalanmaz.
- **Üç Katmanlı Yetki Stratejisi / Three-Tier Privilege Strategy (Bölüm / Section 5.2A)** — MFT Service → admin raw volume → FindFirstFile fallback. **UAC ilk açılışta sorulmaz / not prompted on first launch**.
- **VSS Hot-Path İzolasyonu / VSS Hot-Path Isolation (Bölüm / Section 34.5.1)** — VSS scan sırasında **ASLA / NEVER** çalışmaz; sadece duplicate hash / drill-down'da.
- **Cross-Volume Two-Phase Commit (Bölüm / Section 12.3 v1.4 fix)** — `.dspace_tmp` + WAL + Blake3 hash verify + atomik rename. Başlangıçta otomatik crash recovery.
- **Lazy Viewport Query (Bölüm / Section 9.6)** — Vue hiçbir zaman tam ağaca sahip olmaz; sadece görünen ~200 düğümlük pencereyi ister.

<details>
<summary>🇬🇧 Architectural principles (English)</summary>

- **Single Source of Truth (Section 4.4)** — `Arc<ScanTree>` lives in Rust as the sole owner. Vue only holds `NodeId` references; the tree is never copied to the frontend.
- **Three-Tier Privilege Strategy (Section 5.2A)** — MFT Service → admin raw volume → FindFirstFile fallback. **UAC is never prompted on first launch**.
- **VSS Hot-Path Isolation (Section 34.5.1)** — VSS **NEVER** runs during a scan; only for duplicate hashing and drill-down.
- **Cross-Volume Two-Phase Commit (Section 12.3 v1.4 fix)** — `.dspace_tmp` + WAL + Blake3 hash verify + atomic rename. Automatic crash recovery on startup.
- **Lazy Viewport Query (Section 9.6)** — Vue never owns the full tree; only requests visible ~200-node windows.

</details>

### 🍩 Görsel Katmanı / Visual Layer

- **Sunburst donut** — hand-rolled SVG (d3 yok / no d3), drilldown + breadcrumb
- **Treemap / bubble / timeline / 3D sunburst** — hepsi canlı / all live
- **Tema-uyumlu / theme-aware** rendering
- Vue 3 Composition API + TypeScript + Vite

### 🧠 Zeka Katmanı / Intelligence Layer

- **72 kural / rules** canlı (hedef: 50+ aşıldı)
- **4 renk tier / 4-color tier**: 🔴 kırmızı (silinebilir / deletable) · 🟡 sarı (dikkat / caution) · 🟢 yeşil (güvenli / safe-keep) · 🔵 mavi (sistem / system)
- **0–100 güvenli-silme skoru / safe-to-delete score** her dosyaya
- **ML v2** — pattern öğrenme + kullanıcı davranışından kalibrasyon (v2'de)

### 🕒 Zaman Boyutu / Time Dimension

- **Günlük snapshot capture / daily snapshot capture**
- **Delta**: added / removed / grew / shrunk **top 10**
- "Dün gece ne büyüdü?" / "What grew last night?" sorusu doğrudan cevaplanır

### 🛟 Geri Kazanım / Recovery

- **Direkt silme yok / No direct deletion** — her şey staging klasörüne **atomik MOVE**
- **24 saat / 24-hour** geri-al penceresi / undo window
- **Cross-volume**: `.dspace_tmp` + WAL + **Blake3 hash verify** + atomik rename
- **Crash recovery**: başlangıçta WAL replay / WAL replay on startup

---

## 🛠️ Teknoloji / Tech Stack

| Katman / Layer | Teknoloji / Technology |
|----------------|------------------------|
| **Backend** | Rust 1.80+ — `ntfs`, `windows-rs`, `rusqlite` + `rusqlite_migration`, `blake3`, `tokio`, `tracing`, `thiserror`, `dirs` |
| **Frontend** | Vue 3 + TypeScript + Vite — hand-rolled SVG sunburst (d3 yok / no d3) |
| **Shell** | Tauri 2.x |
| **DB** | SQLite — WAL mode, `busy_timeout=5000`, `synchronous=NORMAL` |
| **Platform** | Windows 10/11 x64 (ARM64 Faz 2 / Phase 2) |

### 📐 Mimari Belgeleri / Architecture Documents

Mimari kararlar ve detaylı tasarım için: **`D-Space-Mimari-v1.4.docx`** (repo kökü / repo root) — 37 bölüm, dondurulmuş / frozen.
*For architectural decisions and detailed design: see `D-Space-Mimari-v1.4.docx` in the repo root — 37 sections, frozen.*

> 🌐 Domain: `dspace.app` (planlanan / planned)

---

## 🗺️ Yol Haritası / Roadmap

| Faz / Phase | Hedef / Target | İçerik / Content |
|-------------|----------------|------------------|
| **Faz 1 / Phase 1** | 🟡 devam ediyor / in progress | Spec v1.4 dondurulmuş / frozen; 5/5 pillar canlı / live; sunburst donut + staging/undo + cross-volume two-phase commit; 33 kural / rules; 36/36 test |
| **Faz 1.5 / Phase 1.5** | sonraki / next | Treemap + bubble + timeline görselleri / visuals; kural sayısı 50+ / rule count 50+; performans tuning |
| **Faz 2 / Phase 2** | ileri / later | **ARM64 desteği / support**, **ML v2** zeka katmanı / intelligence layer (kullanıcı davranışı kalibrasyonu / user-behavior calibration), Microsoft Store hazırlığı / prep |
| **Faz 3 / Phase 3** | — | Çoklu dil / Multi-language UI, paylaşılan rapor / shared reports, plugin sistemi / plugin system |

> 📌 Spec v1.4 dondurulmuş — yeni revizyon eklenmez; kod yazılırken çıkan keşifler **Bölüm 28 Discovery Log'a** düşer.
> *Spec v1.4 is frozen — no new revisions; coding-time discoveries go into **Section 28 Discovery Log**.*

---

## 🛡️ Güvenlik & Gizlilik / Security & Privacy

- **🏠 Tamamen lokal / Fully local** — Hiçbir tarama verisi internete çıkmaz. D-Space hiçbir telemetri, analytics veya uzak sunucu kullanmaz.
- **🔒 KVKK / GDPR uyumlu / compliant** — Hostname ve IP varsayılan **maskelenir / masked** (raporlama ve paylaşımda). Kullanıcı tek tıkla açar/kapar.
- **🗄️ Veriler yerel / Local data** — Tüm tarama sonuçları, kurallar ve staging içeriği **kullanıcı profili** altında SQLite veritabanında tutulur (`%LOCALAPPDATA%\DSpace\`).
- **🔬 Salt-okunur tarama / Read-only scanning** — MFT erişimi salt-okunur; tarama hiçbir dosyayı değiştirmez.
- **🛟 Yıkıcı işlem yok / No destructive ops** — Silme yok, sadece staging. **Geri al / Undo** her zaman 24 saat boyunca açık.
- **🔐 Atomik garantiler / Atomic guarantees** — Cross-volume MOVE'larda Blake3 hash verify + WAL; crash sonrası otomatik recovery.
- **📜 Açık kaynak / Open source** — GPL-3.0-or-later. Her kaynak dosya başlığında `SPDX-License-Identifier: GPL-3.0-or-later`.

<details>
<summary>🇬🇧 Security & Privacy (English)</summary>

- **🏠 Fully local** — No scan data leaves the machine. D-Space uses no telemetry, no analytics, no remote servers.
- **🔒 GDPR / KVKK compliant** — Hostname and IP are **masked by default** in reports and shared output; the user toggles them on/off with one click.
- **🗄️ Local data** — All scan results, rules, and staging contents are kept in a SQLite database under the **user profile** (`%LOCALAPPDATA%\DSpace\`).
- **🔬 Read-only scanning** — MFT access is read-only; scans never modify a single file.
- **🛟 No destructive ops** — No deletion, only staging. **Undo** is always available for 24 hours.
- **🔐 Atomic guarantees** — Cross-volume MOVEs use Blake3 hash verify + WAL; automatic recovery after crash.
- **📜 Open source** — GPL-3.0-or-later. Every source file carries `SPDX-License-Identifier: GPL-3.0-or-later`.

</details>

---

## 🤝 Katkı / Contributing

D-Space **kişisel bir D Brand projesidir** ve topluluk katkı kapsamı bilinçli olarak dar tutulmuştur. Mimari spec v1.4 **dondurulmuş**; çekirdek pillar geliştirme tek elden ilerliyor. Topluluğun değer katabileceği şeritler aşağıdadır.

| ✅ Kabul edilen / Accepted | ❌ Şu an kabul edilmeyen / Not currently accepted |
|----------------------------|--------------------------------------------------|
| 🐛 Bug raporu / Bug reports | 🏗️ Mimari / refactor PR'ları / PRs |
| 💡 Feature **fikri / ideas** (Issue) | ✨ Yeni pillar / yeni kural / yeni view PR'ı / PRs |
| 📊 Performans regression / Performance regressions | 🔧 Spec değişikliği teklifleri (spec dondurulmuş) / Spec change proposals (spec frozen) |
| 🧪 Test case / Test cases | |
| 📚 Belgeleme / Documentation | |

> Spec v1.4 dondurulduğu için **mimari önerilerin Bölüm 28 Discovery Log'a** girmesi gerekir.
> *Because spec v1.4 is frozen, **architectural proposals must go through Section 28 Discovery Log**.*

<details>
<summary>🇬🇧 Contributing (English)</summary>

D-Space is a **personal D Brand project**, and the community contribution scope is intentionally narrow. The architectural spec v1.4 is **frozen**; core pillar work is owned by the maintainer. Lanes where the community can add value are listed in the table above.

</details>

---

## 🎨 D Brand Ailesi / D Brand Family

D-Space, D Brand ailesinin Windows disk analiz ayağıdır. Aile üyeleri "Denizhan" adından ilham alır.

| Ürün / Product | Platform | Açıklama / Description |
|-----------------|----------|------------------------|
| **D-Terminal** | Windows | Agent-aware Windows terminal *(pre-alpha — [github.com/AmrasElessar/d-terminal](https://github.com/AmrasElessar/d-terminal))* |
| **D-Space** | Windows | Smart disk analyzer with recovery guarantee *(bu proje, alpha / this project, alpha)* |
| **D-Player** | Android | Kişisel müzik çalar, DSP motoru / personal music player with DSP engine *(in development)* |
| **DCar Launcher** | Android (Auto) | Head Unit araç içi OS katmanı / Head Unit in-car OS layer *(in development)* |
| **D-Watchtower** | — | Gözetim ve izleme platformu / surveillance & monitoring platform *(in development)* |
| **D-FTP Client** | Windows | Modern FTP/SFTP istemcisi / modern FTP/SFTP client *(in development)* |

---

## 💖 Sponsorlar / Sponsors

D-Space açık kaynak (GPL-3.0+) ve sürekli geliştiriliyor. Sponsorluk doğrudan **yeni pillar geliştirmeye** ve **D Brand ailesine yeni uygulama eklemeye** dönüşür.

[![Sponsor on GitHub](https://img.shields.io/badge/Sponsor-AmrasElessar-db61a2?logo=githubsponsors)](https://github.com/sponsors/AmrasElessar)

<details>
<summary>🇬🇧 Sponsors (English)</summary>

D-Space is open source (GPL-3.0+) and actively developed. Sponsorships translate directly into **new pillar development** and **adding new apps to the D Brand family**.

</details>

<!-- SPONSORS:HERO -->
<!-- Hero tier sponsorları buraya pinlenir / are pinned here -->
<!-- /SPONSORS:HERO -->

<!-- SPONSORS:LIST -->
<sub>Henüz sponsor yok / No sponsors yet. **İlk sponsor sen ol / Be the first →** [github.com/sponsors/AmrasElessar](https://github.com/sponsors/AmrasElessar)</sub>
<!-- /SPONSORS:LIST -->

---

## ❤️ D-Space'i destekle / Support D-Space

<table>
<tr>
<td align="center" width="33%">

### ⭐ Star at / Star it

GitHub'da **Star** projeyi başkalarına da görünür kılar.
Make the project visible to others.

[⭐ Star D-Space](https://github.com/heicfast/d-space_fork)

</td>
<td align="center" width="33%">

### 💖 Sponsor ol / Sponsor

Geliştirme aktif, **5 pillar canlı**, yapılacaklar listesinde daha çok pillar ve D Brand uygulaması var.
Active development, **5 pillars live**, more pillars and D Brand apps in queue.

[💖 github.com/sponsors/AmrasElessar](https://github.com/sponsors/AmrasElessar)

</td>
<td align="center" width="33%">

### 🧪 Test & Geri Bildirim / Test & Feedback

Faz 1 alpha — bug raporu, performans regression veya feature fikri gönder.
Phase 1 alpha — file a bug, a perf regression, or a feature idea.

[🧪 Issues](https://github.com/heicfast/d-space_fork/issues)

</td>
</tr>
</table>

---

## 📜 Lisans / License

**GPL-3.0-or-later** © Orhan Engin OKAY — bkz / see [LICENSE](./LICENSE)

Her kaynak dosya başlığında / Every source file carries:

```
SPDX-License-Identifier: GPL-3.0-or-later
```

---

<div align="center">

**D Brand** — *Görmek, anlamak, geri kazanmak.*
*See, understand, reclaim.*

📐 Master architecture document: `D-Space-Mimari-v1.4.docx` (in repo root)
🌐 Domain: `dspace.app` (planlanan / planned)

</div>
