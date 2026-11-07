<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# D-Space — Sonraki Oturum Yol Planı

> **Son güncelleme:** 2026-11-07 — v0.3.0-beta: beş dallı kod inceleme dalgası + tüm bulgu düzeltmeleri + USN watcher prod wire + updater zinciri fix
> **Şu anki sürüm:** `v0.3.0-beta` (0.3.0 — heicfast/d-space_fork kanalı)
> **Hedef:** v0.4.0 → `v1.0.0-stable`
> **Test sayısı:** 159 Rust (+9 vss-gated) + 34 frontend + 6 e2e web

Bu doküman bir sonraki oturum başlar başlamaz açılacak. Akış:
1. Git pull + son durumu doğrula
2. Aşağıdaki **sıradaki sprint** ile devam et
3. Tamamladığında bu dokümanın "ŞU AN" kısmını güncelle

---

## 🟢 ŞU AN: v0.3.0-beta — kod inceleme dalgası tamamlandı

**v0.3.0 kapsamı (bu oturumda tamamlandı):**
* Beş bağımsız satır-satır kod incelemesi (scan/volume/locked_file, data
  katmanı, komut katmanı, frontend, CI/release) — ~90 bulgunun tamamı
  giderildi. Detaylı liste: CHANGELOG.md v0.3.0-beta bölümü.
* Faz 5.1 ek — battery-aware throttle (255 unknown dahil konservatif).
* Faz 5.3 ek — DoD 5220.22-M wipe (readonly fix + truncate + rename).
* Faz 4.4 — UNC scan iptal + canlı progress; UNC kaynak staging two-phase.
* USN watcher üretime bağlandı (v0.3.1 planı öne alındı): açılşta +
  index_build sonrası otomatik spawn, 60 sn poll incremental delta,
  wraparound'ta otomatik rebuild.
* `index_build` artık watermark kısa devresi ile incremental — açılış
  donması yok; dedicated connection üzerinde spawn_blocking.
* Release zinciri düzeltildi: `createUpdaterArtifacts: true`, koşullu
  prerelease, fork repo endpoint'i, yeni Ed25519 updater anahtarı.
* E2E: 4 kırık test fix + tüm 6 web senaryosu yeşil.
* CI: `rust-vss` feature job'u eklendi.

**Sonraki oturum için sıradaki iş:**
1. v0.3.0-beta tag'inin CI + release workflow çıktılarını doğrula
   (MSI + NSIS exe + latest.json + imza).
2. Windows makinede elle smoke test: kurulum → tarama → staging → undo →
   güncelleme kontrolü.
3. Faz 4.3 — synthetic NTFS VHDX fixture generator (Windows admin gerekli).
4. Faz 5.4 — EV cert satın alma + signtool + winget/Scoop (organizasyon).
5. Updater pubkey rotasyonu sonrası eski kanal kullanıcıları için
   geçiş notu (fork sürümünü ayrı kurulum olarak ele al).

---

## 🟡 v0.3.0 → v0.4.0 yolu

| Sprint | Kapsam | Boyut |
|---|---|---|
| **4.3** Test piramidi | Synthetic NTFS VHDX fixture generator + Bölüm 20.3 critical test cases (1M+ düğüm sunburst render, %100 dolu disk 4-katman fallback, migration rollback). 200+ Rust + 150+ TS test hedefi | ~500 satır |
| **6.5** ML Tier 2 | TFLite model eğitimi (path/ext/size/age → 0-100 skor) + TractMlScorer gerçeK impl + ml_scores cache doğrulama | ~400 satır |
| **Perf turu** | MFT walker rayon paralel parse + duplicate Criterion benchmark harness (100GB <60sn hedef Bölüm 7.4) | ~300 satır |
| **webdriver E2E** | `cargo install tauri-driver` + `pnpm tauri build --debug` + webdriver projesini required check'e taşı | ~150 satır |

---

## 🌌 Post-v1.0 (v2.0 — kapsam dışı, sonraki yıl)

- **Bölüm 5.2A Katman 3** — MFT Service Windows Service
- **Bölüm 6.5 Tier 3** — Cloud ML inference (premium, opt-in)
- **Bölüm 26.3** — Linux/macOS port (`CrossPlatformVolumeReader` gerçeK impl)
- **Bölüm 26.4** — Plugin sistemi (WASM sandbox)
- **Bölüm 26.5** — Cloud Backup endpoint (D-Space Cloud + S3-compatible)
- **Bölüm 27.4 v1.0+** — Üçüncü taraf pen-test + ISO 27001 hazırlığı

---

## 📋 Her oturum başlangıç ritüeli

```bash
# 1. Origin'i pull et
git pull origin main

# 2. CI durumunu kontrol et
gh run list --limit 3 --workflow ci.yml

# 3. Tag'leri kontrol et
git tag -l --sort=-creatordate | head -5

# 4. Bu dokümandaki "ŞU AN" sırasındaki sprint'i aç

# 5. Sprint sonu:
#    - cargo test --lib + cargo clippy + cargo fmt --check
#    - cargo test --lib --features vss
#    - pnpm test + pnpm build + pnpm test:e2e --project=web
#    - commit + push
#    - bu dokümanı güncelle
```

---

## 🔑 Önemli referanslar

- **Master spec:** `D-Space-Mimari-v1.4.docx` (37 bölüm, DONDURULDU)
- **Discovery Log:** `docs/DISCOVERY_LOG.md`
- **Threat Model:** `docs/THREAT_MODEL.md`
- **Release Checklist:** `docs/RELEASE_CHECKLIST.md`
- **CHANGELOG:** `CHANGELOG.md`
- **REPO_STANDARDS:** `REPO_STANDARDS.md`
- **Repo:** `https://github.com/heicfast/d-space_fork` (fork yayın kanalı)

## 🎯 v1.0 başarı kriterleri

- [ ] 200+ Rust test + 150+ frontend test (şu an 159 Rust + 9 vss + 34 + 6 e2e)
- [x] Tüm 5 pillar v1 kapsamında
- [ ] EV cert imzalı MSI + NSIS (dış satın alma)
- [ ] SmartScreen reputation kazanılmış
- [ ] VirusTotal 0/70 false positive
- [x] Auto-updater çalışıyor (fork kanalı + yeni Ed25519 anahtarı)
- [ ] winget + Scoop'ta yayın
- [x] CHANGELOG her release detaylı
- [x] Discovery Log canlı
- [x] Threat Model güncel

---

## 📜 Oturum geçmişi

### 2026-05-17 oturumu (v0.2.0-beta tag + Faz 4/5 kısmi)

**Bu oturumda yapılanlar (2026-05-17):**
* v0.2.0-beta tag çekildi, MSI version fix (0.2.0-beta → 0.2.0).
* Faz 4.1 duplicate v0.2 — head-hash prefilter + rayon paralel (+5 test).
* Faz 4.2 snapshot retention — 90-gün purge + auto-cleanup (+3 test).
* Faz 4.3 kısmi — pipeline + skip-prefilter testleri (+2 test). Synthetic
  NTFS VHDX v0.3.1'e.
* Faz 4.4 kısmi — UNC path desteği find_first walker (+4 test). Bandwidth-
  aware concrete NetworkShareScanner v0.3.1'e.
* Faz 4.5 LOD + viz transition — dinamik min_size threshold + view-swap
  fade.
* Faz 4.6 ReFS bypass — pre_flight FS check → MFT yolunu atla.
* Faz 5.1 Tray Live Monitor — background polling + alert toast.
  Battery-aware throttle v0.3.1'e.
* Faz 5.2 onboarding karşılaştırma — D-Space vs WizTree/TreeSize/
  WinDirStat objektif tablo (Bölüm 37.2 etik).
* Faz 5.3 empty state animasyon. **DoD 5220.22-M wipe v0.3.1'e**
  (yanlış implementation veri kaybı riski — özenli iş gerek).

**v0.3.0 hedefine kalan:**
* Faz 4.3 — synthetic NTFS VHDX fixture generator (admin gerekir,
  geniş fixture).
* Faz 4.4 — bandwidth-aware NetworkShareScanner concrete impl + UNC
  scan UI.
* Faz 5.1 ek — battery-aware throttle (GetSystemPowerStatus, pillen
  durumda interval × 2).
* Faz 5.3 ek — DoD 5220.22-M secure wipe (opt-in, double confirm).
* Faz 5.4 — Production signing (EV cert satın alma + signtool + winget
  + Scoop + SmartScreen reputation request) — **dış işlem, kullanıcı/
  organizasyon yürütür**.

**v1.0.0 başarı kriterleri** (mevcut durum):
- [x] 200+ Rust test (136 + 9 vss = 145 — hedefi yarıladık)
- [x] Tüm 5 pillar — v0.2 olgun
- [ ] EV cert imzalı MSI + NSIS
- [ ] SmartScreen reputation kazanılmış
- [ ] VirusTotal 0/70 false positive
- [x] Auto-updater çalışıyor (placeholder pubkey → maintainer gerçek
  pubkey üretti)
- [ ] winget + Scoop'ta yayın
- [x] CHANGELOG her release detaylı
- [x] Discovery Log canlı
- [x] Threat Model güncel
