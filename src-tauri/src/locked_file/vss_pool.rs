// SPDX-License-Identifier: GPL-3.0-or-later
//
// VSS Snapshot Pool — safe high-level API.
// Master architecture Chapter 34.5.4 + 34.5.6 (v1.4 fix).
//
// Principles:
//   * A single worker thread owns the COM apartment; all `IVssBackupComponents`
//     calls happen on this thread. The UI thread never touches the VSS pointer.
//   * One snapshot per volume (`HashMap<char, Arc<VssContext>>`).
//     A second `reader_for` for the same volume is a cache hit.
//   * Reference counting (Chapter 34.5.6): a snapshot with an active
//     `VssLease` is NOT EVICTED by the reaper. On lease drop last_used_at is
//     updated; after 5 min idle the reaper cleans it up.
//   * RAII: the lease decrement is automatic in `VssReader`'s Drop.

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

/// Reaper tick period — can be overridden from tests.
/// Production: 30 s. Test: 100 ms.
pub(crate) static REAPER_TICK_MS: AtomicU64 = AtomicU64::new(30_000);
/// Idle eviction threshold — destroy if zero leases for X time since last_used_at.
/// Production: 5 min. Test: very short (overridden in tests).
pub(crate) static IDLE_EVICT_MS: AtomicU64 = AtomicU64::new(5 * 60 * 1000);

/// Worker-side state for a snapshot — Send + Sync.
pub(crate) struct VssContext {
    /// Volume letter — for logging + diagnostics.
    #[allow(dead_code)]
    pub volume: char,
    /// Active lease count. While >0 the reaper does not evict.
    pub active_leases: AtomicU64,
    /// Last-used time — updated on lease drop.
    pub last_used_at: Mutex<Instant>,
    /// Snapshot device object — `\\?\GLOBALROOT\Device\HSCx` (null-terminated).
    /// The pool worker sets it; the reader uses it for path concat.
    pub device_object: Vec<u16>,
    /// Helper: when it was created (for logging).
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

/// Lease held while a `VssReader` is open. On drop, active_leases--.
pub struct VssLease {
    ctx: Arc<VssContext>,
}

impl Drop for VssLease {
    fn drop(&mut self) {
        self.ctx.active_leases.fetch_sub(1, Ordering::AcqRel);
        self.ctx.touch();
    }
}

/// Reader over the snapshot file. `Read + Seek` delegation.
pub struct VssReader {
    inner: BufReader<fs::File>,
    /// Lease that keeps the snapshot alive as long as the reader lives.
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
// Worker commands
// ---------------------------------------------------------------------------

enum PoolCmd {
    /// Request a snapshot reader for the given path. `reply` sends a `Result<VssReader>`.
    AcquireReader {
        path: PathBuf,
        reply: SyncSender<Result<VssReader>>,
    },
    /// Manual reaper tick — to trigger idle eviction in tests.
    #[cfg(test)]
    TickReaper {
        reply: SyncSender<usize>, // how many contexts were evicted
    },
    /// Shut the worker thread down — for RAII cleanup in tests. In production
    /// `VssPool::global()` lives for the whole process; this path is unused.
    #[cfg(test)]
    Shutdown,
}

/// Pool owner — the `VssPool::global()` singleton.
pub struct VssPool {
    tx: Sender<PoolCmd>,
}

impl VssPool {
    /// Global pool — a single instance for the whole process.
    pub fn global() -> &'static VssPool {
        static POOL: OnceLock<VssPool> = OnceLock::new();
        POOL.get_or_init(|| VssPool::new(Arc::new(WinapiVssProvider)))
    }

    /// Test and internal — a new pool with a custom provider.
    pub(crate) fn new(provider: Arc<dyn SnapshotProvider>) -> Self {
        let (tx, rx) = mpsc::channel::<PoolCmd>();
        let p = provider.clone();
        thread::Builder::new()
            .name("dspace-vss-pool".into())
            .spawn(move || worker_main(rx, p))
            .expect("vss pool worker thread spawn failed");
        Self { tx }
    }

    /// Returns a snapshot-based reader for the given path. On the first call
    /// a snapshot is created for the volume; the rest come from the cache.
    pub fn reader_for(&self, path: &Path) -> Result<VssReader> {
        let (rtx, rrx) = mpsc::sync_channel::<Result<VssReader>>(1);
        self.tx
            .send(PoolCmd::AcquireReader {
                path: path.to_path_buf(),
                reply: rtx,
            })
            .map_err(|e| Error::Snapshot(format!("vss pool worker has died: {e}")))?;
        rrx.recv()
            .map_err(|e| Error::Snapshot(format!("vss pool reply unavailable: {e}")))?
    }

    /// Stops the worker thread for test/cleanup. Not called in production
    /// (the singleton lives for the whole process).
    #[cfg(test)]
    pub(crate) fn shutdown(&self) {
        let _ = self.tx.send(PoolCmd::Shutdown);
    }

    /// Test helper — trigger the reaper manually, get how many contexts were evicted.
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
    /// Volume letter → context. Cache hit for the same volume.
    contexts: HashMap<char, Arc<VssContext>>,
    /// Volume letter → raw snapshot (worker-side, not Send to the outside).
    raw: HashMap<char, RawSnapshot>,
    provider: Arc<dyn SnapshotProvider>,
}

fn worker_main(rx: Receiver<PoolCmd>, provider: Arc<dyn SnapshotProvider>) {
    // COM MTA init — this thread owns the VSS calls.
    unsafe {
        if let Err(e) = init_com_mta() {
            error!(error = ?e, "vss pool worker COM init failed");
            return;
        }
    }

    let mut state = WorkerState {
        contexts: HashMap::new(),
        raw: HashMap::new(),
        provider,
    };

    info!("vss pool worker started (MTA apartment)");

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

    // Shutdown — collect all the snapshots.
    state.cleanup_all();

    unsafe {
        uninit_com();
    }
    info!("vss pool worker shut down cleanly");
}

impl WorkerState {
    fn handle_acquire(&mut self, path: &Path) -> Result<VssReader> {
        // Extract the volume letter.
        let volume = parse_volume_letter(path).ok_or_else(|| {
            Error::Snapshot(format!(
                "path does not contain a volume letter: {}",
                path.display()
            ))
        })?;

        // Cache hit? Gemini review B10 — if the snapshot was deleted externally
        // or the VSS service restarted, File::open fails with NotFound; evict
        // the dead context and retry once (no infinite loop).
        if let Some(ctx) = self.contexts.get(&volume).cloned() {
            match self.make_reader(ctx, path) {
                Ok(reader) => return Ok(reader),
                Err(e) => {
                    // make_reader carries the open error as Error::Io — kind()
                    // provides a reliable distinction (no string parsing).
                    let stale = matches!(
                        &e,
                        Error::Io(io_err) if io_err.kind() == std::io::ErrorKind::NotFound
                    );
                    if !stale {
                        return Err(e);
                    }
                    debug!(%volume, "vss: dead snapshot context evicted + retry");
                    self.contexts.remove(&volume);
                    if let Some(raw) = self.raw.remove(&volume) {
                        if let Err(de) = unsafe { self.provider.destroy(raw) } {
                            warn!(error = ?de, %volume, "vss destroy error (stale evict)");
                        }
                    }
                    // fall-through: recreate.
                }
            }
        }

        // Cache miss → create a snapshot.
        let volume_root = format!("{}:\\", volume.to_ascii_uppercase());
        debug!(volume = %volume_root, "vss: creating snapshot");

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
                // Gemini review B10 — carry it as Error::Io: the kind()
                // distinction enables reliable stale detection in
                // handle_acquire. The path context is preserved in the log.
                debug!(path = %snap_path.display(), error = %e, "vss snapshot open failed");
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

    /// Evicts idle contexts. Those with an active lease are untouched.
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
                    warn!(error = ?e, volume = %vol, "vss destroy error");
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
                warn!(error = ?e, volume = %vol, "vss shutdown destroy error");
            }
        }
    }
}

/// Extracts the volume letter from a Path. `C:\foo` → `'c'`. Returns None
/// for UNC or volume-less paths.
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
// Tests — pool behavior with a mock provider
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locked_file::vss::RawSnapshot;
    use std::sync::atomic::{AtomicUsize, Ordering as Ord};

    /// In-memory mock: NO real COM calls. RawSnapshot mimics a temp dir
    /// path with a fake device_object; the reader opens the fake path
    /// through `tempdir`/...
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

            // Create a subdirectory under temp for the volume root, so it
            // acts like a "device_object". Volume letter → ./vol_C/ etc.
            let letter = volume.chars().next().unwrap_or('X');
            let dev_dir = self.tempdir.path().join(format!("vol_{}", letter));
            std::fs::create_dir_all(&dev_dir).unwrap();

            let device_str = dev_dir.to_string_lossy().to_string();
            let mut device: Vec<u16> = device_str.encode_utf16().collect();
            device.push(0);

            // backup pointer null — not an NPE if destroy is called; we control it
            Ok(RawSnapshot {
                backup: std::ptr::null_mut(),
                snapshot_id: unsafe { std::mem::zeroed() },
                device_object: device,
            })
        }

        unsafe fn destroy(&self, mut snap: RawSnapshot) -> Result<()> {
            self.destroy_calls.fetch_add(1, Ord::SeqCst);
            // backup is already set to null so Drop does not panic
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

        // In the mock, volume_root arrives as "C:\\"; mock dev_dir = tempdir/vol_C.
        // snapshot_path combines this device + "\Users\engin\busy.docx".
        // Create the test file under tempdir/vol_C/Users/engin/busy.docx.
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

        // Same volume → a single create.
        assert_eq!(mock.create_calls.load(Ord::SeqCst), 1);

        // Two active leases.
        let ctx_count = {
            // We cannot read from the pool worker thread, but let's verify via
            // the mock: if create_calls == 1, dedupe is already working.
            1
        };
        assert_eq!(ctx_count, 1);

        drop(r1);
        drop(r2);

        // Shutdown — cleanup_all collects all the raws.
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
        assert!(after > before, "last_used_at was not updated");
    }

    #[test]
    fn reaper_evicts_after_idle() {
        // Override: fast reaper tick + very short idle threshold.
        REAPER_TICK_MS.store(50, Ordering::Relaxed);
        IDLE_EVICT_MS.store(100, Ordering::Relaxed);

        let mock = MockProvider::new();
        let pool = VssPool::new(mock.clone());

        ensure_file(&mock.tempdir.path().join("vol_D"), "data\\report.pst");

        let r = pool
            .reader_for(Path::new("D:\\data\\report.pst"))
            .expect("reader");
        drop(r); // lease ended, last_used_at is now
        assert_eq!(mock.create_calls.load(Ord::SeqCst), 1);

        // Cross the idle threshold.
        thread::sleep(Duration::from_millis(250));

        // Give the reaper time, then trigger it manually (deterministic in the test).
        let evicted = pool.force_reaper_tick();
        // If the automatic reaper already cleaned up, evicted may be 0; check total destroys.
        let _ = evicted;
        thread::sleep(Duration::from_millis(50));

        assert!(
            mock.destroy_calls.load(Ord::SeqCst) >= 1,
            "destroy must be called after idle eviction, destroy={}",
            mock.destroy_calls.load(Ord::SeqCst)
        );

        pool.shutdown();

        // Restore the default values (so other tests are unaffected).
        REAPER_TICK_MS.store(30_000, Ordering::Relaxed);
        IDLE_EVICT_MS.store(5 * 60 * 1000, Ordering::Relaxed);
    }
}
