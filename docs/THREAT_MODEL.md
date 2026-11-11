<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# D-Space Threat Model

Master mimari Bölüm 27. AdminToolkit ve DTransfer'dan öğrenilen ders:
saldırgan modelini formalize et — ISO 27001 denetimi için de gereklidir.

Bu doküman v0.1 (alpha) için canlı durum yansıtır. v2.0'da üçüncü taraf
pen-test sonrası genişletilecek.

---

## 1. Saldırgan Modeli (Bölüm 27.1)

| Aktör                                      | Yetenek                   | Motivasyon                     | Faz 1 mitigasyon durumu                               |
| ------------------------------------------ | ------------------------- | ------------------------------ | ----------------------------------------------------- |
| **A1. Kötü amaçlı yazılım (sandbox dışı)** | User space code execution | Kullanıcı verisi çalmak        | **Kısmen** — staging ACL v0.2 (T2)                    |
| **A2. Standart Windows kullanıcısı**       | Tarayıcı, dosya sistemi   | Lisans atlatma, kullanım hakkı | **Beklemede** — lisans server v2.0 (T4)               |
| **A3. Yerel admin kullanıcısı**            | Sistem geneli erişim      | Forensic kanıt silme           | **Tasarımda** — Event Log v0.2 (T3)                   |
| **A4. Ağ saldırganı (MITM)**               | Network sniffing          | Lisans bilgisi, telemetri      | **Beklemede** — telemetry v0.2, opt-in default kapalı |
| **A5. Anti-rakip aktör**                   | Public review platforms   | Reputation damage              | **Tasarımda** — AV whitelist Bölüm 18.4               |

**Kapsam dışı (v1.0 alpha):** kernel-mode malware, physical attack
(soğutma saldırısı/cold-boot), supply-chain compromise of Rust crates
(yan modül: cargo audit v0.2).

---

## 2. Saldırı Yüzeyi (Bölüm 27.2)

| Yüzey                                                    | Risk                              | Faz 1 mitigasyonu                                                                                          | Eksik                                                      |
| -------------------------------------------------------- | --------------------------------- | ---------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| **Tauri IPC** (`invoke_handler`)                         | Yetki yükseltme via fake command  | Tauri capability sistemi varsayılan (`default.json`); tüm Rust komutları tipli + `Result<T, Error>` döner  | Capability sınırlama v0.2                                  |
| **MFT raw read** (`\\.\C:`)                              | Admin-only API, fallback gerekli  | Bölüm 5.2A üç katmanlı strateji: elevated yoksa FindFirstFile fallback                                     | MFT Service named pipe ACL v2.0                            |
| **Staging klasörü** (`%LOCALAPPDATA%\DSpace\staging`)    | Race condition, exfiltration      | Atomic `fs::rename` + cross-volume 2PC + WAL (Bölüm 12.3)                                                  | ACL kısıtı + SQLite şifreleme (v0.2)                       |
| **SQLite DB** (`%LOCALAPPDATA%\DSpace\db\dspace.sqlite`) | Local tampering, schema downgrade | Forward-only migration (rusqlite_migration), `permanent_deletes_forensic` ledger                           | WORM log + Event Log v0.2                                  |
| **RestartManager probe** (Bölüm 34.4)                    | Sistem handle table reveal        | On-demand only, hot-path izolasyonu (Bölüm 34.5.1); RM PID + app name, sistem handle'ları leak etmez       | NtQuerySystemInformation derin probe gerekmez (RM yeterli) |
| **Locked file probe** (`detect.rs`)                      | DoS via CreateFileW spamming      | UI'da explicit user click; spawn_blocking ile asenkron                                                     | Rate limit yok (v0.2 — `0.5 saniye debounce`)              |
| **Telemetry endpoint** (opsiyonel)                       | Data leak                         | **Yok**: gerçek endpoint v0.2; bu sprint sadece settings flag                                              | HTTPS + cert pinning v0.2                                  |
| **Auto-update** (Tauri updater)                          | Supply chain                      | **Aktif** (v0.3.0): Ed25519 imza + latest.json endpoint; `createUpdaterArtifacts` ile .sig üretimi zorunlu | Ed25519 signature check + tag-based release                |

---

## 3. Tehdit Senaryoları (Bölüm 27.3)

### T1: Malware MFT Service'i kötüye kullanır

**Saldırgan:** A1 user space malware. **Hedef:** SYSTEM yetkisiyle dosya silme.

**Mitigasyon (Faz 1):** D-Space tek-process Tauri uygulamasıdır; ayrı bir
MFT Service'i yok (Bölüm 5.2A Katman 3 stub). Silme/staging operasyonları
yalnızca kullanıcı bağlamında çalışır. v2.0'da MFT Service eklenirse
named pipe ACL `local user only` + signed binary check zorunlu olacak.

**Durum:** Kapsam dışı (Katman 3 yok).

### T2: Staging klasörü exfiltration

**Saldırgan:** A1 user space malware. **Hedef:** Staging'deki hassas dosyaları kopyalama.

**Mitigasyon (Faz 1):** Staging `%LOCALAPPDATA%\DSpace\staging` altında.
Windows ACL default olarak yalnızca kullanıcıya açık. Diğer kullanıcı
malware'i normal yolla erişemez, ama **aynı kullanıcı** malware'i bu
klasörü okur — bu kabul edilen bir risk. Snapshot SQLite şifreleme v0.2'de.

**Durum:** Default ACL korunuyor, ek koruma v0.2.

### T3: Forensic trace silme

**Saldırgan:** A3 yerel admin (suç sonrası izleri silmek isteyen).

**Mitigasyon (Faz 1):** `permanent_deletes_forensic` tablosu DB içinde,
admin yetkisi olan kullanıcı doğrudan silebilir. v0.2'de Windows Event
Log entegrasyonu (her permanent delete OS event log'a yazılır, admin bile
log'u manipule etse forensic iz Windows tarafında kalır).

**Durum:** Tablo var, OS-level ledger v0.2.

### T4: Lisans crack

**Saldırgan:** A2 standart kullanıcı, ödemeden Pro özellikleri kullanmak istiyor.

**Mitigasyon (Faz 1):** Lisans/monetizasyon henüz yok (GPL-3.0-or-later,
açık kaynak). Pro tier v2.0'da: Ed25519 imzalı lisans dosyası,
server-side activation, offline grace period 30 gün. **GPL ile uyumlu**:
crack tamamen engellenemez, ekonomik olarak caydırıcı yapılır (Bölüm
22.6 — etik kısıtlar).

**Durum:** Faz 1 için geçerli değil (Pro tier yok).

### T5: Anti-virüs spoof saldırısı

**Saldırgan:** A5 rakip aktör. **Yöntem:** D-Space binary'sini malware
kategorisinde rapor etme.

**Mitigasyon (Faz 1):** Bölüm 18.4 EDR/AV sertifikasyon yol haritası
v0.2'de — VirusTotal monitoring, proaktif AV whitelist başvurusu,
imzalanmamış MSI'lar için README uyarısı.

**Durum:** Beklemede (release workflow şu an unsigned MSI/NSIS).

---

## 4. Güvenlik Audit Yol Haritası (Bölüm 27.4)

| Faz              | Aksiyon                                                                             | Hedef tarih     |
| ---------------- | ----------------------------------------------------------------------------------- | --------------- |
| **v0.1 (şimdi)** | Internal security review (Engin + Claude) — bu doküman                              | Faz 1 alpha     |
| **v0.2**         | `cargo audit` CI gate, dependency review, SQLite şifreleme, WAL recovery test       | Faz 2 sonu      |
| **v0.3**         | Bölüm 18.2 kod imzalama (EV cert), Tauri updater Ed25519, lisans server scaffolding | Pre-stable      |
| **v1.0**         | Üçüncü taraf pen-test (Pentest As A Service)                                        | Stable release  |
| **v2.0**         | ISO 27001 hazırlığı, StokMatik ile birlikte denetim                                 | Enterprise tier |

---

## 5. Açık Riskler ve Kabul Edilen Tasarım Tradeoff'ları

| Risk                                                                                                           | Etki                                                                | Kabul nedeni                                                                                                     |
| -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Aynı kullanıcı bağlamında malware staging klasörünü okur                                                       | Veri kaçışı                                                         | Tüm desktop uygulamalar aynı risk altında; per-app sandbox Windows'ta yok. AppContainer v2.0 incelemesi          |
| RestartManager non-admin kullanıcıya PID + app name verir                                                      | Bilgi sızıntısı (zaten Windows'un kendi diyalogları aynısını yapar) | Spec Bölüm 34.4'te kabul edildi — kullanıcının `Bu dosyayı kim tutuyor?` ihtiyacı bilgi sızıntısından ağır basar |
| Telemetry opt-in flag client-side; "katılırım" diyen kullanıcı bile gerçek endpoint yokken bir şey göndermiyor | Yanıltma                                                            | Bölüm 18.3 v0.2'de gerçek endpoint geldiğinde net `flag=1 → POST` zinciri kurulur, kullanıcı dürüst görür        |
| Unsigned MSI/NSIS (alpha)                                                                                      | SmartScreen uyarısı                                                 | Bölüm 18.2 EV cert v0.2; alpha sürümünde kullanıcı README'den uyarılır                                           |
| `IVssBackupComponents` `windows` 0.61.3 eksik → VSS pool ertelendi                                             | Bölüm 34.5.4 hash-time path eksik                                   | Discovery Log #002 — manuel COM vtable veya alternatif crate v1.5+                                               |

---

## 6. Güncelleme Kuralı

Bu doküman her **major güvenlik kararı** için PR ile güncellenir. Discovery
Log (`DISCOVERY_LOG.md`) ile çapraz referans tutulur. Spec dondu, threat
model canlı.
