<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# DiskWizard Changelog

Spec v1.4 dondurulmuş (D-Space lineage). Implementation farkı
`docs/DISCOVERY_LOG.md`'de.

## [v0.4.0-beta] — 2026-11-12

**DiskWizard.** The Recovery Wizard React UI becomes the product
frontend (replacing the Vue app), wired end-to-end to the real engine —
the mock simulated backend is eradicated from every production surface,
and the D-Space feature set returns inside the new design language.

### Product

- Rebrand to **DiskWizard** (com.heicfast.diskwizard): English-first UI
  (manual language switch to 简体中文), custom window chrome, original
  emblem, all corpus-derived branding (incl. the EaseUS-named app
  string) removed; activation/licensing surfaces deleted
- `requireAdministrator` manifest — the UAC prompt at launch replaces
  every scan dying with ACCESS_DENIED (raw volume access, MFT engine,
  deleted-file recovery, VSS and repair all need elevation)
- New sidebar: Data Recovery · Advanced Features · **Disk Cleanup** ·
  File Backup

### The real recovery engine (`src-tauri/src/recovery/`)

- **Unified MFT walk** — one pass collects live AND deleted records;
  deleted entries keep data-run maps; $Bitmap cluster-allocation
  reader; pause/resume/cancel with partial results
- **Confidence grading** — cluster-reuse model (excellent/good/fair/
  poor); fresh record re-parse + allocation re-check at read and
  recovery time (stale scan state never recovers wrong bytes)
- **Server-side results** — filters (category/date/size/extension/
  origin/search), 5 sort keys, pagination with totals, path tree with
  the Lost Path branch, 8-category type tree; cleanup scores ride on
  node attributes
- **Verified recovery jobs** — collision policy, folder rebuild,
  Blake3 verify-on-write, per-file progress events, honest failure
  codes (record-reused, clusters-overwritten, unsupported-encoding)
- **Bounded previews** — image (≤8 MiB, header-parsed dimensions),
  text (UTF-16LE/UTF-8 detect), hex (≤64 KiB); deleted files read
  through their data runs
- **Sessions** — scans persist to compact `.dws` binaries (SQLite
  registry, migration 0004); completed and cancelled scans auto-save
- **Health** — SMART via ATA pass-through (fixture-tested sector
  parser), physical-drive mapping, honest health-unavailable
- **Volume repair** — NTFS boot/backup + $MFT/$MFTMirr dry-run
  findings; consent-gated apply with undo log (non-system volumes)
- **Lost partitions** — GPT/MBR layout parsing, unallocated-gap boot
  probing, and the whole MFT engine running over lost volumes via an
  offset reader

### UI (React 19, connector-driven)

- Real IPC bridge over 46 `dw_*` commands; 27 wiring tests pin every
  command name, argument casing and result mapping; connector default
  is the real engine (mock is an explicit test/dev opt-in)
- Size Map squarified treemap view; cleanup-score pills; staging from
  the results context menu
- Cleanup Bin (restore, conflict resolution, typed-phrase permanent
  delete with DoD wipe, expired batch with threshold abort), Duplicate
  Finder, Size Snapshots with two-select deltas, user rules in Settings
- Volume Repair page (mock per-file dropzone deleted), Lost Partition
  Rescue from real disk layouts, Home quick targets via the native
  folder picker and real special folders
- Auto-updater dialog (Ed25519-signed passive install + relaunch),
  close-while-busy guard, tray scan hand-off and full-disk alert toasts

### Engine housekeeping

- English error/tray strings; DiskWizard data folders
  (`%LOCALAPPDATA%\DiskWizard\{db,staging,sessions}`); single-instance
  plugin; superseded commands removed (internals + tests retained)
- CI: merged pipeline — Rust fmt/clippy/tests (default + vss) and the
  full frontend gate stack on every push; blocking e2e smoke +
  tauri-debug link; release workflow gains a version-match gate and
  builds with the REAL connector only

### Numbers

197 frontend unit tests (+ wiring suite) · 12 e2e smoke · ~170 Rust
engine tests (incl. ~40 new recovery tests, executing on Windows CI) ·
coverage 81.8/62.4/70.7/82.8 over 70/60/70/70 thresholds · bundle
143/327 KB gzip (budget 200/500)

## [v0.3.0-beta] — 2026-11-07

Üçüncü pre-release — **kod inceleme dalgası + kalan v0.3 kapsam
tamamlaması**. Beş bağımsız satır-satır kod incelemesinin (Rust scan/volume/
locked_file + data katmanı + komut katmanı + Vue frontend + CI/release)
bulguların tümü giderildi; USN watcher üretime bağlandı.

### Kritik düzeltmeler (release engelleyiciler)

- **`bundle.createUpdaterArtifacts: true`** eklendi (`tauri.conf.json`) —
  Tauri v2 bundler bu bayrak olmadan updater `.sig` artifact'ı üretmiyor;
  auto-updater zinciri (latest.json + imza doğrulama) baştan sona kırıktı.
- **`prerelease: true` sabiti kaldırıldı** (release.yml) — GitHub
  `releases/latest` yalnız stabil release gösterir; sabit prerelease
  updater endpoint'ini 404'lüyordu. Artık tag'de alpha/beta/rc geçiyorsa
  prerelease, aksi halde stabil yayınlanır.
- **UNC girdi doğrulaması** — `\\fileserver\\public` gibi girdiler eski
  `chars().find(alphabetic)` deseniyle sessizce `F:` sürücüne çevriliyordu
  (yanlıs volume'ün MFT'sini tarayıp doğru gösterme riski). Yeni katı
  `volume::parse_drive_letter` tüm komut sınırlarında UNC/path reddeder.

### Komut katmanı (lib.rs)

- **`index_build` donması fix** — açılışta her seferinde full MFT
  enumerate + global DB mutex'i tutuluyordu (UI dakikalarca kilitleniyordu).
  Artık: watermark varsa **incremental delta** (`sync_volume`), yoksa/wraparound
  ise full baseline; yürüyüş **dedicated connection + spawn_blocking**
  üzerinde koşar.
- **USN watcher üretime bağlandı** (v0.3.1 planı öne alındı) — açılışta ve
  `index_build` sonrası volume başına arka plan thread'i; 60 sn poll ile
  `FSCTL_READ_USN_JOURNAL` delta uygular, wraparound'ta otomatik rebuild.
  (`UsnWatchers` registry + `spawn_watcher` prod wire.)
- **Blocking komutlar async'e taşındı** — `stage_path`, `undo_staging`,
  `undo_resolve_staging`, `cleanup_expired_staging_cmd`, `permanent_delete_cmd`
  (DoD wipe dakikalar sürebilir), `capture_snapshot`, `index_search`,
  `index_status` artık `spawn_blocking` üzerinde; ana thread donmuyor.
  (`DbState.conn` → `Arc<Mutex<Connection>>`.)
- **İptal yarışı fix** — tek-global-flag tasarımında iptal→hızlı yeniden
  tarama eski taramanın bayrağını sıfırlayıp iki walker'ı paralel
  çalıştırıyordu. Her koşu artık kendi `Arc<AtomicBool>` bayrağını alır.
- **`scan_unc_path` iptal edilebilir + canlı progress** — SMB walk saatler
  sürebilir; artık `scan-progress` event'i emit eder ve UI'dan durdurulabilir.
- **In-memory DB fallback'e migration** — şemasız fallback tüm DB
  komutlarını "no such table" ile öldürüyordu.
- Tray interval `saturating_mul(60)`; `is_on_battery` 255 (unknown) artık
  konservatif; tokio feature set'i eksiksiz (`rt, sync, time, macros`).

### Staging / geri kazanım (Pillar 5)

- **Cross-volume undo fix** — `fs::rename` cross-device hata veriyordu;
  undo artık kopya → Blake3 verify → atomik rename → kaynak sil zinciriyle
  çalışır (stage'in iki-fazlı protokolünün aynası).
- **Idempotent undo sessiz veri silme riski fix** — ≤ 64 MB dosyalarda tam
  Blake3 karşılaştırması; üzerinde 4 KB eşleşmesi "muhtemelen aynı"
  kabul edilip Conflict'e düşer (kullanıcı bilinçli seçer).
- **stage() crash penceresi** — DB insert başarısızsa rename geri alınır.
- **UNC kaynak staging** — UNC path artık two-phase kopya yoluna gider
  (eski kod raw rename hatası veriyordu).
- **DoD wipe sertleştirme** — readonly attribute temizliği + truncate-to-0 +
  silme öncesi rastgele isim rename (NTFS directory entry kalıntısı).
- WAL Faz 2b hatası artık açıkça `wal_abort`; boş staging bucket
  dizinleri temizlenir; expired liste LIMIT 2000; rate-limit sleep yalnız
  item'lar arasında.

### Tarama motoru (Pillar 1)

- **İptal/progress kapısı fix** — fallback walker'daki `entries.len() % 500`
  kontrolü değişken artışla atlanabiliyordu (tek büyük dizin = hiç kontrol
  yok). Deterministik `visited_dirs % 32` sayacına geçildi.
- **Canlı sunburst O(N²) fix** — 10k'da bir full-tree rebuild + full klon +
  63 kural × N düğüm eşleştirmesi yerine: klon YOK (`build_tree_light`
  borrow ile), kural skorlaması YOK, geometrik aralık (10k→40k→160k→
  640k→2.5M tavan). Toplam maliyet ~O(N log N).
- **MFT walker** — dev upfront rezervasyon clamp (1M kayıt); hard-limit
  kesilmesinde uyarı log'u; `GetCompressedFileSizeW` 0xFFFFFFFF başarı/
  hata ayrımı `GetLastError` ile.
- **Ağaç kurma** — self-cycle MFT kayıtları artık root'a rewiring (eskiden
  görünmezdi); karşılıklı parent döngülerinde aggregate şişmesi
  visited-set ile önlenir; `NameAsc` sıralama `sort_by_cached_key`;
  `symlink_metadata` yerine `DirEntry::metadata()` (SMB taramada dosya
  başına bir syscall tasarrufu).

### Kilitli dosya / VSS

- `CreateFileW` hatasında `GetLastError()` yerine güvenilir `e.code()`
  (HRESULT) ayrımı — eski çağrı error-path tarafından kirletilebiliyordu.
- VSS `Wait(INFINITE)` → 60 sn sınırlı bekleme (asılı VSS service'i
  duplicate scan'i donduruyordu).
- VSS pool ölü snapshot context eviction + tek seferlik yeniden deneme.
- `RmGetList` ERROR_MORE_DATA retry (3 deneme).

### Index (USN)

- `resolve_full_path` root self-join fix (`C:\C:\C:\…` çöpü).
- `load_watermark` DB hataları artık sessizce "watermark yok" olmaz.
- Full rebuild öncesi `purge_volume` (wraparound hayalet kayıtları).
- `index_status.last_sync_unix` per-volume.
- USN_RECORD_V3 kayıtları artık tüm baseline'ı öldürmez (skip).

### Frontend (Vue 3)

- **`nodeFullPath` çifte-klasör fix** — drillInto sonrası detay paneli
  `C:\a\b\b` gösteriyordu; stage/probe/lock yanlış yola gidiyordu.
- **Index arama pseudo-node tam yol** — backend'den çözülen `full_path`
  kullanılır; uydurma `C:\<name>` üretilmez.
- **Stale selection fix** — pencere değişinde eski selectedNode temizlenir;
  hızlı drill tıklamalarında out-of-order yanıt koruması (istek sırası).
- **i18n tamamlaması** — conflict dialog, lock etiketleri, kısayol modalı,
  Duplicate/Snapshot/UserRules panelleri, volume/cloud/sürücü uyarı
  etiketleri, expired-cleanup akışı — hepsi locale dosyalarına taşındı
  (EN locale'de Türkçe görünme sorunu bitti). Locale-aware sayı/tarih
  formatlama (sabit `tr-TR` kaldırıldı); `index.html` lang attribute'u
  dil değişinde güncellenir.
- **3D cleanup** — center disk geo/mat dispose + `forceContextLoss` +
  `rising`/`pulse` animasyon state reset (GPU sızıntısı + dispose edilmiş
  mesh'e yazma).
- **Kısayollar** — açık TÜM modal'larda bastırılır (eskiden conflict+
  perm-delete dışındakilerde çalışıp arkadaki işlemi tetikliyordu);
  Ctrl+1..5 ile 3D mod da seçilebilir; ölü `searchOpen` kaldırıldı.
- **Update.close()** — dismiss'ta updater kaynağı serbest kalır.
- **AUTO_THRESHOLD abort yüzeye çıkar** — expired cleanup eşiğini
  aştığında kullanıcı açık uyarı görür.
- `@types/three` devDependencies'e; vitest coverage bloğu (kurulu olmayan
  paket) kaldırıldı.

### E2E altyapısı (4 kırık test fix)

- Mock `onboarding_seen` → `onboarding_done` (onboarding her yüklemede
  açılıp tema/update testlerini blokluyordu).
- Onboarding spec: 4 slayt + scoped `.slide-title` selector'ları (strict-mode
  çakışması).
- View-mode spec: gerçek `.view-chip`/`.view-chip-active` markup + mock'lu
  scan verisi + Ctrl+1..5 klavye testi.
- Playwright webdriver projesi açıkça opt-in (`D_SPACE_E2E_WEBDRIVER=1`);
  CI e2e `--with-deps chromium` + html reporter + localhost host.

### CI

- Yeni `rust-vss` job — `--features vss` clippy + 9 VSS testi artık
  varsayılan koşuda derlenmiyor olmak yerine her push'ta doğrulanır.

### Bilinen sınırlar (v1.0 yolunda)

- **Kod imzasız** — SmartScreen "publisher unknown" uyarısı sürer (EV
  cert Faz 5.4, dış satın alma).
- **EV cert / winget / Scoop** — Faz 5.4 (organizasyon eylemi).
- **Synthetic NTFS VHDX fixture** (Faz 4.3) — Windows admin ortamı gerektirir.
- **TFLite model** (Tier 2 ML) — v0.4; altyapı hazır, model eğitilmedi.

---

## [v0.2.0-beta] — 2026-05-17

İkinci public release — **Beta kanalı**. Hâlâ unsigned (EV cert Bölüm 18.2
stable hedefi), ama auto-updater plugin'i etkin; sonraki sürümler imzalı
gelebilir.

### Yapıldı

**Görselleştirme (Pillar 2 olgunlaşma):**

- Üç-kolon workspace (Sprint 3.1c) — sol VolumeSidebar (drive listesi +
  kullanım barı + status pill) + orta workspace + sağ detay paneli;
  responsive `<960px` üst sticky.
- Canlı tarama Sunburst (Sprint 3.7) — `ScanProgress` overlay split
  layout, sol 360×360 SVG arc (saf hand-rolled, 2 halka), sağ sayısal
  stats; backend her 10k entry'de `partial_tree` event'i emit eder.
- Light/dark/auto tema toggle (Bölüm 9.5).

**Hızlı arama (Discovery #005 / Bölüm 5.6):**

- NTFS USN Journal index katmanı (Sprint 3.8 + 3.8.1) — `usn_index` +
  `usn_watermark` SQLite tabloları, real-time delta watcher, `Everything`
  benzeri substring arama.
- Baseline walker (`FSCTL_ENUM_USN_DATA` + `USN_RECORD_V2` parser) tüm
  MFT'yi 1 MB tampon ile gezer; watermark journal_id + next_usn'i
  kaydeder, sonraki açılışlarda yalnız delta.
- `IndexSearchBar` sticky üst kutu, Ctrl+F kısayolu, 150 ms debounce,
  top-50 substring eşleşme + opsiyonel `WITH RECURSIVE` full_path.
- Admin yoksa graceful: `mode="needs_admin"` rozeti.

**VSS pool (Pillar 5 ek):**

- Sprint 2H.3 `IVssBackupComponents` snapshot pool — `winapi 0.3.9`
  COM köprüsü (Discovery #002 çözüldü), worker thread + per-volume
  dedupe + lease renewal + reaper.
- Default kapalı (`vss` Cargo feature), opt-in build.
- Bölüm 34.5.4 hash-time + 34.5.6 reference counting davranışı.

**Auto-updater (Bölüm 21.4) — Sprint 3.6:**

- `tauri-plugin-updater` + `tauri-plugin-process` etkin;
  `tauri.conf.json` plugins.updater GitHub Releases latest.json
  endpoint'ine bağlı; release workflow `includeUpdaterJson: true`.
- `UpdateNotification.vue` — header'da "Güncelleme kontrol et"
  butonu; modal (sürüm + notlar + indirme progress) → relaunch.
- Hata tip ayrımı: imza/ağ/diğer (TR + EN i18n).
- **Ed25519 pubkey placeholder** — maintainer ilk gerçek beta release
  öncesi `pnpm tauri signer generate` ile üretmeli (RELEASE_CHECKLIST §2).

**Cloud placeholder detection (Bölüm 11.1):**

- Win32 `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` bayrak detection — UI'da
  ☁ rozeti gösterir.

**E2E altyapı (Sprint 3.5):**

- Playwright dev dep + `playwright.config.ts` (web + webdriver iki proje).
- 5 smoke senaryo: onboarding, görsel mod toggle, IndexSearchBar, tema
  cycle, update check fallback.
- `e2e/fixtures/tauri-mock.ts` Tauri IPC stub'ı — Vite dev server üzerinde
  backend bağımsız UI akış doğrulama.
- `.github/workflows/e2e.yml` ubuntu runner; v0.2.0 sonrası required.

**Disk donanım profili (2026-05-17 polish):**

- `IOCTL_STORAGE_QUERY_PROPERTY` ile her sürücü için bus tipi (NVMe /
  SATA / USB / SAS / SCM), medya tipi (SSD / HDD), üretici, model,
  seri, tipik okuma hızı (MB/s, bus tipinden türetilmiş).
- VolumeSidebar her drive kartında küçük rozet (örn. "NVMe SSD ~3500
  MB/sn") + üretici/model satırı.

**Tarama durdur/iptal (2026-05-17 polish):**

- `ScanCancel(Arc<AtomicBool>)` Tauri state. `scan_cancel` komutu
  bayrağı set eder, walker periyodik kontrol eder, `scan-cancelled`
  hatası ile bail.
- ScanProgress overlay'ine "Taramayı durdur" pill butonu — tıklanınca
  sessizce kapatır.

**Hard link dedup (2026-05-17 fix):**

- Standart Mod `find_first` walker NTFS file_index tabanlı dedupe ekledi
  (Win32 GetFileInformationByHandle). 64 KB+ dosyalar için aynı fiziksel
  veri ikinci görüldüğünde data_size = 0 → toplam disk doluluğunu aşmaz.
  Önceki bug: 1 TB diskte 1.2 TB rapor (WinSxS hard link'leri).

**Tema + UX iyileştirmeleri (2026-05-17 polish):**

- Info popover pattern (`InfoButton.vue`): başlık yanında ⓘ → tıkla,
  paragraf açılır. Sürekli görünen intro paragrafları kaldırıldı.
- 3-kolon workspace (Snapshot | Duplicate | Detail) >1600px'de yan
  yana; 960-1600px arası 2-kolon; <960 tek kolon.
- Canlı sunburst: Tableau 10 palet + SVG radial gradient gloss + drop
  shadow → hafif 3D. Hover scale 1.025 + brightness. Click → seçili
  wedge halo + alt panelde detay chip'i.
- Pill renkleri (pill-ok / pill-warn / pill-frozen, score tier'lar,
  status pill, tier pill, lock pill) per-tema text override'larıyla
  light/dark her ikisinde 5:1+ kontrast.
- Probe-btn dark teal → vibrant blue (#2563eb).
- Bekleyen Silmeler kuyruğunda enter/leave/move animasyonları.
- Tarama canlı log: bold dosya adı + üst klasör iki satır, scrollbar,
  her path için tooltip, satır arası dashed border.
- Locale leak fix: tüm sabit TR metinler i18n'e taşındı (Staging
  Kuyruğu → Bekleyen Silmeler, "Forensic ledger" → "denetim defteri",
  "Drilldown" → "Klasör Görseli", score tier label'ları). `toLocaleString`
  sabit "tr-TR" çağrıları locale-aware (TR/EN otomatik).

**Test:**

- 124 Rust default (+9 vss-gated) + 34 frontend = **158 test passing**
  (v0.1.0-alpha: 79).
- Cargo clippy strict + cargo fmt + vue-tsc + vite build temiz.

### Bilinen sınırlar (sonraki sprint'lerde)

- **Kod imzasız** — SmartScreen "publisher unknown" uyarısı sürer (EV
  cert v1.0).
- **Updater pubkey placeholder** — gerçek yayında imza doğrulama hatası
  vermesin diye maintainer üretmeli.
- **Duplicate v0.2 yok** — head-hash prefilter + rayon paralel (Faz 4.1).
- **Snapshot retention yok** — 90 gün cleanup + streaming delta loader
  (Faz 4.2).
- **Network scanner yok** — UNC path scan + bandwidth-aware (Faz 4.4).
- **ML model yok** — TFLite altyapı hazır, model v0.3'te.
- **Tray live monitor yok** — sadece launcher/quit (Faz 5.1).
- **EV cert yok** — SmartScreen reputation v1.0 yolunda (Faz 5.4).

---

## [v0.1.0-alpha] — 2026-05-15

İlk public alpha. **Unsigned** MSI/NSIS — Windows SmartScreen "publisher
unknown" uyarısı verecek (Bölüm 18.2 EV cert v0.2 sprint'inde).

### Yapıldı

**Pillar 1 — Hız (Bölüm 5):**

- MFT direkt okuma (`ntfs` crate) — admin ile 1 TB hedef <5 sn
- FindFirstFile fallback — non-elevated mod, her durum çalışır
- Üç katmanlı yetki stratejisi (Bölüm 5.2A K1+K2)
- Volume pre-flight (Bölüm 33.2 Katman 0): ReFS/Remote/Removable/CdRom uyarıları

**Pillar 2 — Görsel zarafet (Bölüm 9.1):**

- Sunburst donut (hand-rolled SVG)
- Treemap (squarified Bruls/Huijsen/van Wijk 2000)
- Bubble pack (Vogel spiral + 80-iter force relax)
- Timeline (mtime ekseni + Y-relax, log scale > 1 yıl)
- Lazy viewport-aware query (Bölüm 9.6): ~200 düğüm/window

**Pillar 3 — Zeka katmanı (Bölüm 6):**

- 63 built-in kural — dev cache / sistem koruma / cloud sync / VM disk
- Skor tier renkleri: 0-30 kırmızı, 31-60 sarı, 61-85 yeşil, 86-100 mavi
- Kullanıcı tanımlı kurallar — UI editör + DB persist + runtime merge (Bölüm 6.4)
- `RulesScorer` concrete impl + SQLite `ml_scores` cache (Bölüm 6.5.2)
- TFLite altyapı `ml-tflite` feature gate (model v0.3'te)

**Pillar 4 — Zaman boyutu (Bölüm 8):**

- Snapshot capture (dir-only v0.1)
- Delta hesaplama (added/removed/grew/shrunk top-10)

**Pillar 5 — Geri kazanım garantisi (Bölüm 12):**

- Same-volume staging + 24h undo
- Cross-volume two-phase commit + WAL recovery (Bölüm 12.3 v1.4 fix)
- Conflict resolution dialog (Bölüm 12.2.4): Overwrite/Rename/KeepBoth/Cancel
- Permanent delete + forensic ledger (Bölüm 12.4) — çift onay (file name typed)
- Lazy expiry cleanup (Bölüm 12.2.1-2.3) — rate-limited, AUTO_THRESHOLD=100

**Duplicate detector (Bölüm 7) v0.1:**

- Blake3 streaming hash
- Boyut bucket → hash bucket pipeline
- Reclaimable bytes raporu

**Locked file (Bölüm 34) v0.1:**

- CreateFileW share-violation probe
- Windows Restart Manager owner detection (PID + app name)
- Scan-time hot-path izolasyonu (Bölüm 34.5.1)
- VSS pool **ertelendi** — Discovery Log #002 (windows-rs 0.61
  `IVssBackupComponents` eksik, v0.2'de albertony/vss port)

**UI/UX (Bölüm 15):**

- Progressive Disclosure 3-seviye (özet · expand · advanced)
- Klavye-first: Ctrl+R, Ctrl+1-4, Backspace, Delete, Ctrl+Z, Ctrl+F, Ctrl+?
- Onboarding 3-slide + Hızlı/Standart mod seçimi (Bölüm 15.3 + 37)
- Sistem tray ikon + menü (Bölüm 13.1)
- Çakışma/permanent delete modal'ları

**Çoklu dil (Bölüm 19):**

- vue-i18n v10 + TR/EN locale + dil toggle chip
- Onboarding tamamen çevrilmiş, ana akış çevrilmiş

**Altyapı:**

- SQLite (Bölüm 14): 9 tablo, WAL mode, 2 migration
- Settings KV API (onboarding/dil/telemetry tercihleri)
- Perf telemetrisi yerel (son 20 örnek, gönderim YOK)
- Telemetry opt-in flag (default kapalı, Bölüm 18.3)

**v2 trait rezervasyonları (Bölüm 26):**

- 5 trait stub: MlSafeDeleteScorer · NetworkShareScanner ·
  CrossPlatformVolumeReader · Plugin · CloudBackupIntegration

**Build + dağıtım:**

- GitHub Actions CI: fmt + clippy `-D warnings` + cargo test + pnpm build + vitest
- Release workflow: tag → tauri-action → MSI + NSIS + GitHub Release

**Test:**

- 69 Rust unit/integration + 10 frontend Vitest = **79 test**

**Dokümantasyon:**

- README iki dilli (TR/EN) D Brand template
- REPO_STANDARDS.md
- docs/DISCOVERY_LOG.md (#001-003)
- docs/THREAT_MODEL.md (Bölüm 27, 5 aktör + 8 yüzey + T1-T5)
- docs/RELEASE_CHECKLIST.md (Bölüm 18.2 + 21.4 + 35)

### Bilinen sınırlar (v0.2 sprint'lerine)

- **Kod imzasız** — SmartScreen "publisher unknown" uyarısı (EV cert v0.2)
- **VSS pool yok** — kilitli dosyalar hash retry yapamaz (Discovery Log #002)
- **ML inference yok** — model dosyası v0.3'te eğitilecek
- **Auto-updater pasif** — Ed25519 key + endpoint v0.2'de
- **Üç-kolon layout yok** — tek-kolon (v0.2 Faz 3.1)
- **Light tema yok** — sadece dark (v0.2 Faz 3.2)
- **Tray polling/auto-clean yok** — sadece launcher/quit (v0.2 Faz 3.4)
- **Win32 reparse tag detection yok** — cloud placeholder pattern-bazlı
- **Playwright e2e yok** — sadece Vitest unit (v0.2 Faz 3.5)

### Lisans

GPL-3.0-or-later. Doküman Bölüm 22.4 MIT öneriyordu, 2026-05-14
override edildi.
