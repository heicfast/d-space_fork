<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# D-Space Release Checklist

Master mimari Bölüm 18.2 (Kod İmzalama) + 21.4 (Otomatik Güncelleme) + 18.4
(EDR/AV Sertifikasyon). Stable release öncesi tamamlanması gereken adımlar.

> **Alpha (v0.1.x):** unsigned MSI/NSIS, updater kapalı, SmartScreen
> "publisher unknown" uyarısı kabul. Kullanıcı README'den uyarılır.
>
> **Beta (v0.2.x):** code signing cert + signtool + updater key.
>
> **Stable (v1.0):** EV cert (SmartScreen reputation), Tauri updater feed,
> auto-update kanalları (Stable/Beta/Nightly), VirusTotal monitoring.

---

## 1. Code Signing (Bölüm 18.2)

### 1.1 Sertifika tipi seçimi

| Tip | Maliyet | SmartScreen davranışı | Tavsiye |
|---|---|---|---|
| **Self-signed** | ücretsiz | "publisher unknown" + kırmızı uyarı | Sadece geliştirme |
| **OV (Organization Validated)** | ~$200/yıl | "publisher unknown" 30-90 günlük reputation süresi | v0.2 beta |
| **EV (Extended Validation)** | ~$400/yıl | "publisher unknown" YOK (instant trust) | **Stable v1.0** |

EV cert sağlayıcıları: DigiCert, Sectigo, GlobalSign, SSL.com. EV ile
HSM-bound token gelir (USB dongle veya cloud HSM); özel anahtar export
edilemez.

### 1.2 signtool entegrasyonu

`tauri-action` MSI/NSIS üretir. Sign için ek adım:

```yaml
# .github/workflows/release.yml içine eklenecek (v0.2)
- name: Sign MSI/NSIS
  if: ${{ env.SIGNING_ENABLED == 'true' }}
  run: |
    & "C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\signtool.exe" `
      sign /a /tr http://timestamp.digicert.com /td sha256 /fd sha256 `
      "src-tauri/target/release/bundle/msi/*.msi" `
      "src-tauri/target/release/bundle/nsis/*.exe"
  env:
    SIGNING_ENABLED: ${{ secrets.SIGNING_ENABLED }}
```

EV cert HSM gerektirir — runner üzerinde HSM bridge (Azure Key Vault,
SignPath.io, Garantir vb.) kullanılır. **Self-hosted Windows runner** EV
imza için tek pratik yol.

### 1.3 Imza doğrulama

```powershell
Get-AuthenticodeSignature .\D-Space-0.1.0-x64.msi
```

`SignatureType` = `Authenticode` + `IsOSBinary` = `False` + `StatusMessage`
= `Signature verified` olmalı.

---

## 2. Auto-Updater (Bölüm 21.4) — Sprint 3.6 ile etkin

### 2.1 Plugin kurulumu ✅ (Sprint 3.6, commit feat(updater))

```bash
# Bir kez yapıldı, manifest'lere yapışmış durumda:
pnpm add @tauri-apps/plugin-updater @tauri-apps/plugin-process
cargo add tauri-plugin-updater tauri-plugin-process --manifest-path src-tauri/Cargo.toml
```

`src-tauri/src/lib.rs` build hook'a `tauri_plugin_updater::Builder::new().build()` +
`tauri_plugin_process::init()` zaten ekli. Capability dosyasında
`updater:default` + `process:default` izinleri açık.

### 2.2 Ed25519 key gen — **maintainer'ın yapması gereken son adım**

```bash
# Tek seferlik, lokal makinede:
pnpm tauri signer generate -w ~/.dspace/updater-private.key
# 1) Public key çıktısını kopyala → src-tauri/tauri.conf.json
#    plugins.updater.pubkey alanına yapıştır
# 2) Private key içeriğini GitHub Secret olarak ekle:
#    Settings → Secrets and variables → Actions:
#      TAURI_SIGNING_PRIVATE_KEY          ← private key dosya içeriği
#      TAURI_SIGNING_PRIVATE_KEY_PASSWORD ← keygen sırasında girilen şifre
# 3) Bu adım yapılmadan ilk v0.2 tag'i atılırsa: imzasız bundle çıkar,
#    istemci `signature verification failed` raporlar (UI'da
#    "İmza doğrulanamadı" mesajı).
```

**Mevcut placeholder pubkey:** `tauri.conf.json`'da `REPLACE_BEFORE_RELEASE`
işaretli geçici Ed25519 değeri var. v0.2.0-beta tag'inden önce maintainer
bunu gerçek pubkey ile değiştirmeli.

### 2.3 tauri.conf.json plugins.updater (mevcut)

```jsonc
{
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/heicfast/d-space_fork/releases/latest/download/latest.json"
      ],
      "pubkey": "<v0.2 öncesi: pnpm tauri signer generate çıktısı>",
      "windows": { "installMode": "passive" }
    }
  }
}
```

### 2.4 latest.json (otomatik)

`tauri-action` release workflow'da `includeUpdaterJson: true` aktif —
her release'e şu formatta `latest.json` ekler:

```json
{
  "version": "v0.2.0",
  "notes": "Release notes",
  "pub_date": "2026-06-01T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<Ed25519 signature>",
      "url": "https://github.com/.../releases/download/v0.2.0/D-Space_0.2.0_x64-setup.exe"
    }
  }
}
```

İstemci `tauri.conf.json` endpoint'inden son JSON'u indirir, signature
pubkey ile doğrular, kullanıcıya UpdateNotification modal'ı gösterir,
"İndir ve kur" → `downloadAndInstall` + `relaunch()`.

### 2.5 UI akışı (`src/components/UpdateNotification.vue`)

Header'da küçük "⤓ Güncelleme kontrol et" butonu. Tıklayınca:
* Güncelleme yok → "✔ Güncel sürüm"
* Güncelleme var → modal: yeni sürüm + notlar + "İndir ve kur" / "Kapat"
* İndirme sırasında progress bar (event.Started.contentLength →
  event.Progress.chunkLength toplamı)
* İmza/ağ/diğer hatalar i18n'li mesaja çevrilir (`update.errorVerify` vb.)

### 2.6 Kanal stratejisi (v1.0+)

* **Stable** — varsayılan endpoint, GA sürümler
* **Beta** — `pre-release: true` GitHub releases (şu an v0.2.0-beta burada)
* **Nightly** — günlük build, ayrı feed (v1.0)

Kullanıcı kanal seçimi `settings.update_channel`.

---

## 3. SmartScreen Reputation Building

EV cert satın alındıktan sonra bile bazı durumlarda SmartScreen ilk
indirmelerde uyarı verir. Reputation building süresi (~2-4 hafta):

1. EV cert ile imzalı release yayınla
2. Microsoft submission portal: SmartScreen reputation request
3. VirusTotal taraması: 0/70 false positive
4. İlk 1000 indirme süresince hızlı sorun raporu kanalı açık tut
5. AV vendor whitelisting (Bölüm 18.4):
   * BitDefender — submit
   * Kaspersky — submit
   * Symantec — submit
   * Defender'ın kendi telemetrisi reputation building'i otomatik yürütür

---

## 4. Pre-Release Quality Gate

Her release tag (`v*.*.*`) push'undan önce manuel checklist:

- [ ] `cargo test --lib` 100% pass
- [ ] `cargo clippy -- -D warnings` warning sıfır
- [ ] `cargo fmt --check` diff sıfır
- [ ] `pnpm test` (Vitest) tüm suite pass
- [ ] `pnpm run build` vue-tsc + vite hata sıfır
- [ ] `cargo audit` (v0.2) — `cargo install cargo-audit && cargo audit`
  güvenlik açığı sıfır
- [ ] Bundle size delta < %5 (önceki release ile)
- [ ] `docs/DISCOVERY_LOG.md` güncel
- [ ] Manuel smoke test: Hızlı + Standart mod tarama, drilldown 4 görsel
  mod geçişi, staging + undo + permanent delete (test fixture'ı), tray
  menü
- [ ] `docs/THREAT_MODEL.md` riskler güncel
- [ ] CHANGELOG.md (v0.2'den itibaren) — version bump

---

## 5. Bölüm 35 Cloud Connector Dayanıklılığı (özet)

Bölüm 35'te detaylı; alpha kapsamı için kısa not:

* **Yol A** (premium): D-Space Cloud endpoint — staging buluta yedek
  (Bölüm 26.5 trait stub)
* **Yol B** (default, free): manuel klasör scan fallback — OneDrive/
  Dropbox/Google Drive yerel klasörü taranır
* **Hibrit** (Bölüm 35.3): yerel scan + opsiyonel bulut sync — kullanıcı
  network maliyetini görür
* **Cert revoke hazırlığı** (Bölüm 35.4): bulut endpoint cert revoke
  edilirse 30 gün grace period, kullanıcıya offline downgrade önerilir

Faz 2H'ta gerçek bulut entegrasyonu; şu sprint'te yalnızca trait
rezervasyonu (Bölüm 26.5).
