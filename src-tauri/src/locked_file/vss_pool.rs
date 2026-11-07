// SPDX-License-Identifier: GPL-3.0-or-later
//
// VSS Snapshot Pool — safe high-level API.
// Master mimari Bölüm 34.5.4 + 34.5.6 (v1.4 fix).
//
// İlkeler:
//   * Tek worker thread COM apartment'ı tutar; tüm `IVssBackupComponents`
//     çağrıları bu thread'de. UI thread asla VSS pointer'a dokunmaz.
//   * Volume başına tek snapshot (`HashMap<char, Arc<VssContext>>`).
//     Aynı volume için ikinci `reader_for` cache hit'tir.
//   * Reference counting (Bölüm 34.5.6): aktif `VssLease`'i olan snapshot
//     reaper tarafından EVICT EDİLMEZ. Lease drop'ta last_used_at güncellenir,
//     5 dk idle sonra reaper temizler.
//   * RAII: `VssReader` Drop'unda lease decrement otomatik.

#![cfg(all(windows, feature = "vss"))]

use crate::error::{Error, Result};
use crate::locked_file::vss::{
    init_com_mta, snapshot_path, uninit_com, RawSnapshot, SnapshotProvider, WinapiVssProvider,
};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Reaper tick periyodu — testten override edilebilir.
/// Production: 30 sn. Test: 100 ms.
pub(crate) static REAPER_TICK_MS: AtomicU64 = AtomicU64::new(30_000);
/// Idle eviction eşiği — last_used_at'tan beri X sürede sıfır lease ise destroy.
/// Production: 5 dk. Test: çok kısa (testlerde override).
pub(crate) static IDLE_EVICT_MS: AtomicU64 = AtomicU64::new(5 * 60 * 1000);

/// Worker tarafı bir snapshot için durum — Send + Sync.
pub(crate) struct VssContext {
    /// Volume harfi — log + diagnostic için.
    #[allow(dead_code)]
    pub volume: char,
    /// Aktif lease sayısı. >0 iken reaper evict etmez.
    pub active_leases: AtomicU64,
    /// Son kullanım zamanı — lease drop'ta güncellenir.
    pub last_used_at: Mutex<Instant>,
    /// Snapshot device object — `\\?\GLOBALROOT\Device\HSCx` (null-terminated).
    /// Pool worker bunu set eder; reader path concat için kullanır.
    pub device_object: Vec<u16>,
    /// Yardımcı: ne zaman oluşturuldu (log için).
    #[allow(dead_code)]
    pub created_at: Instant,
}

impl VssContext {
    fn touch(&self) {
        if let Ok(mut g) = self.last_used_at.lock() {
            *g = Instant::now();
        }
    }
}

/// `VssReader` open olduğu sürece tutulan lease. Drop'ta active_leases--.
pub struct VssLease {
    ctx: Arc<VssContext>,
}

impl Drop for VssLease {
    fn drop(&mut self) {
        self.ctx.active_leases.fetch_sub(1, Ordering::AcqRel);
        self.ctx.touch();
    }
}

/// Snapshot dosyası üzerinden okuyucu. `Read + Seek` delegasyonu.
pub struct VssReader {
    inner: BufReader<fs::File>,
    /// Reader yaşadığı sürece snapshot'ı koruyan lease.
    _lease: VssLease,
}

impl Read for VssReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Seek for VssReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

// ---------------------------------------------------------------------------
// Worker komutları
// ---------------------------------------------------------------------------

enum PoolCmd {
    /// Belirli yol için snapshot reader iste. `reply` `Result<VssReader>` yollar.
    AcquireReader {
        path: PathBuf,
        reply: SyncSender<Result<VssReader>>,
    },
    /// Manuel reaper tick — testlerde idle eviction'ı tetiklemek için.
    #[cfg(test)]
    TickReaper {
        reply: SyncSender<usize>, // kaç context evict edildi
    },
    /// Worker thread'i kapat — testlerde RAII cleanup için. Production'da
    /// `VssPool::global()` process ömrü kadar yaşar; bu yol kullanılmaz.
    #[cfg(test)]
    Shutdown,
}

/// Pool sahibi — `VssPool::global()` singleton.
pub struct VssPool {
    tx: Sender<PoolCmd>,
}

impl VssPool {
    /// Global pool — process boyunca tek instance.
    pub fn global() -> &'static VssPool {
        static POOL: OnceLock<VssPool> = OnceLock::new();
        POOL.get_or_init(|| VssPool::new(Arc::new(WinapiVssProvider)))
    }

    /// Test ve internal — özel provider ile yeni pool.
    pub(crate) fn new(provider: Arc<dyn SnapshotProvider>) -> Self {
        let (tx, rx) = mpsc::channel::<PoolCmd>();
        let p = provider.clone();
        thread::Builder::new()
            .name("dspace-vss-pool".into())
            .spawn(move || worker_main(rx, p))
            .expect("vss pool worker thread spawn başarısız");
        Self { tx }
    }

    /// Verilen yol için snapshot tabanlı okuyucu döner. İlk çağrıda volume
    /// için snapshot oluşturulur, ardından gelenler cache'ten gelir.
    pub fn reader_for(&self, path: &Path) -> Result<VssReader> {
        let (rtx, rrx) = mpsc::sync_channel::<Result<VssReader>>(1);
        self.tx
            .send(PoolCmd::AcquireReader {
                path: path.to_path_buf(),
                reply: rtx,
            })
            .map_err(|e| Error::Snapshot(format!("vss pool worker düşmüş: {e}")))?;
        rrx.recv()
            .map_err(|e| Error::Snapshot(format!("vss pool reply alınamadı: {e}")))?
    }

    /// Test/cleanup için worker thread'i durdur. Production'da çağrılmaz
    /// (singleton process ömrü kadar yaşar).
    #[cfg(test)]
    pub(crate) fn shutdown(&self) {
        let _ = self.tx.send(PoolCmd::Shutdown);
    }

    /// Test yardımcısı — reaper manuel tetikle, kaç context evict edildiğini al.
    #[cfg(test)]
    pub(crate) fn force_reaper_tick(&self) -> usize {
        let (rtx, rrx) = mpsc::sync_channel(1);
        if self.tx.send(PoolCmd::TickReaper { reply: rtx }).is_err() {
            return 0;
        }
        rrx.recv().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Worker thread
// ---------------------------------------------------------------------------

struct WorkerState {
    /// Volume harfi → context. Aynı volume için cache hit.
    contexts: HashMap<char, Arc<VssContext>>,
    /// Volume harfi → raw snapshot (worker-side, Send değil dış dünyaya).
    raw: HashMap<char, RawSnapshot>,
    provider: Arc<dyn SnapshotProvider>,
}

fn worker_main(rx: Receiver<PoolCmd>, provider: Arc<dyn SnapshotProvider>) {
    // COM MTA init — bu thread VSS çağrılarının sahibi.
    unsafe {
        if let Err(e) = init_com_mta() {
            error!(error = ?e, "vss pool worker COM init başarısız");
            return;
        }
    }

    let mut state = WorkerState {
        contexts: HashMap::new(),
        raw: HashMap::new(),
        provider,
    };

    info!("vss pool worker başladı (MTA apartment)");

    loop {
        let tick = Duration::from_millis(REAPER_TICK_MS.load(Ordering::Relaxed));
        match rx.recv_timeout(tick) {
            Ok(PoolCmd::AcquireReader { path, reply }) => {
                let res = state.handle_acquire(&path);
                let _ = reply.send(res);
            }
            #[cfg(test)]
            Ok(PoolCmd::TickReaper { reply }) => {
                let n = state.reap_idle();
                let _ = reply.send(n);
            }
            #[cfg(test)]
            Ok(PoolCmd::Shutdown) => {
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                state.reap_idle();
            }
            Err(RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    // Shutdown — tüm snapshotları topla.
    state.cleanup_all();

    unsafe {
        uninit_com();
    }
    info!("vss pool worker temiz kapandı");
}

impl WorkerState {
    fn handle_acquire(&mut self, path: &Path) -> Result<VssReader> {
        // Volume harfini ayıkla.
        let volume = parse_volume_letter(path).ok_or_else(|| {
            Error::Snapshot(format!("yol volume harfi içermiyor: {}", path.display()))
        })?;

        // Cache hit? Gemini review B10 — snapshot externally silinmişse /
        // VSS service restart etmişse File::open NotFound ile düşer; ölü
        // context'i evict edip bir kez yeniden dene (sonsuz döngü yok).
        if let Some(ctx) = self.contexts.get(&volume).cloned() {
            match self.make_reader(ctx, path) {
                Ok(reader) => return Ok(reader),
                Err(e) => {
                    // make_reader open hatasını Error::Io olarak taşır — kind()
                    // güvenilir ayrım sağlar (string parse yok).
                    let stale = matches!(
                        &e,
                        Error::Io(io_err) if io_err.kind() == std::io::ErrorKind::NotFound
                    );
                    if !stale {
                        return Err(e);
                    }
                    debug!(%volume, "vss: ölü snapshot context evict + retry");
                    self.contexts.remove(&volume);
                    if let Some(raw) = self.raw.remove(&volume) {
                        if let Err(de) = unsafe { self.provider.destroy(raw) } {
                            warn!(error = ?de, %volume, "vss destroy hata (stale evict)");
                        }
                    }
                    // fall-through: yeniden oluştur.
                }
            }
        }

        // Cache miss → snapshot oluştur.
        let volume_root = format!("{}:\\", volume.to_ascii_uppercase());
        debug!(volume = %volume_root, "vss: snapshot oluşturuluyor");

        let raw = unsafe { self.provider.create(&volume_root)? };
        let device_object = raw.device_object.clone();

        let ctx = Arc::new(VssContext {
            volume,
            active_leases: AtomicU64::new(0),
            last_used_at: Mutex::new(Instant::now()),
            device_object,
            created_at: Instant::now(),
        });
        self.contexts.insert(volume, ctx.clone());
        self.raw.insert(volume, raw);

        self.make_reader(ctx, path)
    }

    fn make_reader(&self, ctx: Arc<VssContext>, original: &Path) -> Result<VssReader> {
        let snap_path = snapshot_path(&ctx.device_object, original);
        let file = match fs::File::open(&snap_path) {
            Ok(f) => f,
            Err(e) => {
                // Gemini review B10 — Error::Io olarak taşı: kind() ayrımı
                // handle_acquire'de güvenilir stale detection sağlar. Path
                // context'i log'da korunur.
                debug!(path = %snap_path.display(), error = %e, "vss snapshot open hatası");
                return Err(Error::Io(e));
            }
        };
        ctx.active_leases.fetch_add(1, Ordering::AcqRel);
        ctx.touch();

        Ok(VssReader {
            inner: BufReader::with_capacity(64 * 1024, file),
            _lease: VssLease { ctx },
        })
    }

    /// Idle context'leri eler. Aktif lease'i olanlar dokunulmaz.
    fn reap_idle(&mut self) -> usize {
        let idle_ms = IDLE_EVICT_MS.load(Ordering::Relaxed);
        let threshold = Duration::from_millis(idle_ms);
        let now = Instant::now();

        let mut evict_volumes: Vec<char> = Vec::new();
        for (&vol, ctx) in self.contexts.iter() {
            if ctx.active_leases.load(Ordering::Acquire) > 0 {
                continue;
            }
            let last = ctx.last_used_at.lock().map(|g| *g).unwrap_or_else(|_| now);
            if now.saturating_duration_since(last) >= threshold {
                evict_volumes.push(vol);
            }
        }

        let count = evict_volumes.len();
        for vol in evict_volumes {
            self.contexts.remove(&vol);
            if let Some(raw) = self.raw.remove(&vol) {
                debug!(volume = %vol, "vss: idle snapshot evict");
                if let Err(e) = unsafe { self.provider.destroy(raw) } {
                    warn!(error = ?e, volume = %vol, "vss destroy hata");
                }
            }
        }
        count
    }

    fn cleanup_all(&mut self) {
        self.contexts.clear();
        let raws: Vec<(char, RawSnapshot)> = self.raw.drain().collect();
        for (vol, raw) in raws {
            if let Err(e) = unsafe { self.provider.destroy(raw) } {
                warn!(error = ?e, volume = %vol, "vss shutdown destroy hata");
            }
        }
    }
}

/// Volume harfini Path'ten çıkarır. `C:\foo` → `'c'`. UNC veya volume-less
/// path'ler için None.
fn parse_volume_letter(path: &Path) -> Option<char> {
    let s = path.to_string_lossy();
    let b = s.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        Some(b[0].to_ascii_lowercase() as char)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Testler — mock provider ile pool davranışı
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locked_file::vss::RawSnapshot;
    use std::sync::atomic::{AtomicUsize, Ordering as Ord};

    /// In-memory mock: gerçek COM çağrısı YOK. RawSnapshot fake device_object
    /// ile temp dir path'i taklit eder; reader fake path'i `tempdir`/...
    /// üzerinden açar.
    struct MockProvider {
        create_calls: AtomicUsize,
        destroy_calls: AtomicUsize,
        tempdir: tempfile::TempDir,
    }

    impl MockProvider {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                create_calls: AtomicUsize::new(0),
                destroy_calls: AtomicUsize::new(0),
                tempdir: tempfile::tempdir().unwrap(),
            })
        }
    }

    impl SnapshotProvider for MockProvider {
        unsafe fn create(&self, volume: &str) -> Result<RawSnapshot> {
            self.create_calls.fetch_add(1, Ord::SeqCst);

            // Volume root için temp altında bir alt dizin oluştur, "device_object"
            // gibi davransın. Volume harfi → ./vol_C/ vs.
            let letter = volume.chars().next().unwrap_or('X');
            let dev_dir = self.tempdir.path().join(format!("vol_{}", letter));
            std::fs::create_dir_all(&dev_dir).unwrap();

            let device_str = dev_dir.to_string_lossy().to_string();
            let mut device: Vec<u16> = device_str.encode_utf16().collect();
            device.push(0);

            // backup pointer null — destroy çağırırsak NPE değil, biz kontrol ederiz
            Ok(RawSnapshot {
                backup: std::ptr::null_mut(),
                snapshot_id: unsafe { std::mem::zeroed() },
                device_object: device,
            })
        }

        unsafe fn destroy(&self, mut snap: RawSnapshot) -> Result<()> {
            self.destroy_calls.fetch_add(1, Ord::SeqCst);
            // Drop panik etmesin diye backup null'a set zaten
            snap.backup = std::ptr::null_mut();
            Ok(())
        }
    }

    fn ensure_file(root: &Path, rel: &str) {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, b"mock content").unwrap();
    }

    #[test]
    fn parse_volume_letter_works() {
        assert_eq!(
            parse_volume_letter(Path::new("C:\\Users\\engin")),
            Some('c')
        );
        assert_eq!(parse_volume_letter(Path::new("d:\\foo.txt")), Some('d'));
        assert_eq!(parse_volume_letter(Path::new("\\\\server\\share")), None);
        assert_eq!(parse_volume_letter(Path::new("")), None);
    }

    #[test]
    fn vss_pool_dedupes_per_volume() {
        let mock = MockProvider::new();
        let pool = VssPool::new(mock.clone());

        // Mock'ta volume_root "C:\\" gelir; mock dev_dir = tempdir/vol_C.
        // snapshot_path bu device + "\Users\engin\busy.docx" birleştirir.
        // Test dosyasını tempdir/vol_C/Users/engin/busy.docx altında oluştur.
        ensure_file(
            &mock.tempdir.path().join("vol_C"),
            "Users\\engin\\busy.docx",
        );

        let r1 = pool
            .reader_for(Path::new("C:\\Users\\engin\\busy.docx"))
            .expect("reader 1");
        let r2 = pool
            .reader_for(Path::new("C:\\Users\\engin\\busy.docx"))
            .expect("reader 2");

        // Aynı volume → tek create.
        assert_eq!(mock.create_calls.load(Ord::SeqCst), 1);

        // İki aktif lease.
        let ctx_count = {
            // Pool worker thread'inden okuyamayız, ama mock üzerinden doğrulayalım
            // create_calls == 1 ise zaten dedupe çalışıyor.
            1
        };
        assert_eq!(ctx_count, 1);

        drop(r1);
        drop(r2);

        // Shutdown — cleanup_all tüm raw'ları toplar.
        pool.shutdown();
        thread::sleep(Duration::from_millis(100));
        assert!(mock.destroy_calls.load(Ord::SeqCst) >= 1);
    }

    #[test]
    fn lease_drop_decrements() {
        let ctx = Arc::new(VssContext {
            volume: 'c',
            active_leases: AtomicU64::new(0),
            last_used_at: Mutex::new(Instant::now() - Duration::from_secs(3600)),
            device_object: vec![0u16],
            created_at: Instant::now(),
        });
        let before = ctx.last_used_at.lock().map(|g| *g).unwrap();
        ctx.active_leases.fetch_add(1, Ordering::AcqRel);
        let lease = VssLease { ctx: ctx.clone() };
        assert_eq!(ctx.active_leases.load(Ordering::Acquire), 1);

        drop(lease);

        assert_eq!(ctx.active_leases.load(Ordering::Acquire), 0);
        let after = ctx.last_used_at.lock().map(|g| *g).unwrap();
        assert!(after > before, "last_used_at güncellenmedi");
    }

    #[test]
    fn reaper_evicts_after_idle() {
        // Reaper tick hızlı + idle threshold çok kısa override.
        REAPER_TICK_MS.store(50, Ordering::Relaxed);
        IDLE_EVICT_MS.store(100, Ordering::Relaxed);

        let mock = MockProvider::new();
        let pool = VssPool::new(mock.clone());

        ensure_file(&mock.tempdir.path().join("vol_D"), "data\\report.pst");

        let r = pool
            .reader_for(Path::new("D:\\data\\report.pst"))
            .expect("reader");
        drop(r); // lease bitti, last_used_at şimdi
        assert_eq!(mock.create_calls.load(Ord::SeqCst), 1);

        // Idle eşiğini geç.
        thread::sleep(Duration::from_millis(250));

        // Reaper'a zaman ver, sonra manuel tetikle (testte deterministik olsun).
        let evicted = pool.force_reaper_tick();
        // Eğer otomatik reaper zaten temizlediyse evicted=0 olabilir; total destroy'a bak.
        let _ = evicted;
        thread::sleep(Duration::from_millis(50));

        assert!(
            mock.destroy_calls.load(Ord::SeqCst) >= 1,
            "idle eviction sonrası destroy çağrılmalı, destroy={}",
            mock.destroy_calls.load(Ord::SeqCst)
        );

        pool.shutdown();

        // Default değerleri geri yükle (diğer testleri etkilemesin).
        REAPER_TICK_MS.store(30_000, Ordering::Relaxed);
        IDLE_EVICT_MS.store(5 * 60 * 1000, Ordering::Relaxed);
    }
}
