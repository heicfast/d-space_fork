// SPDX-License-Identifier: GPL-3.0-or-later
//
// USN delta watcher — Sprint 3.8.
//
// Discovery Log #005 — Bölüm 5.6: background thread, 5 sn polling. Her
// tick'te `FSCTL_READ_USN_JOURNAL` çağrısı yapılır, gelen kayıtlar
// `apply_delta` ile DB'ye yazılır, watermark güncellenir. Shutdown
// kanalı ile durdurulur.
//
// **Yetki notu**: USN journal okuma `\\.\C:` raw volume handle açmayı
// gerektirir — admin gerekli. `spawn_watcher` admin yoksa watcher
// başlatmaz, `IndexStatus { mode: "needs_admin" }` döner. Hızlı Mod
// (Bölüm 5.2A Katman 1) yetkisinin aynısı.
//
// v0.3.1 — production'a bağlanmış durumda: `lib.rs::spawn_usn_watchers`
// açılışta ve `index_build` sonrası watcher başlatır; tick closure'ı
// `index::sync_volume` çağırır (lazy dedicated Connection ile).
// Test platform-agnostic kısımları kapsar (channel-shutdown semantiği).

use crate::error::Result;
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{info, warn};

/// Watcher poll aralığı — 60 sn (Gemini review 2.4 — batarya dostu).
/// İdeal: tamamen event-driven `FSCTL_READ_USN_JOURNAL` Timeout > 0
/// (`read_journal_blocking` index/usn.rs); v0.3.1 prod wire bunu
/// kullanacak. Bu polling interval scaffold için fallback.
pub const WATCHER_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Watcher handle — drop ile thread kapatma sinyali yollar.
pub struct WatcherHandle {
    shutdown: Option<Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl WatcherHandle {
    /// Thread'i durdur, join et. Hata varsa log'a düşer.
    pub fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            if let Err(e) = j.join() {
                warn!(?e, "watcher join hatası");
            }
        }
    }
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        // Join'i drop'a bırakmıyoruz — caller `shutdown()` çağrısıyla
        // explicit yapsın; drop'ta detach.
    }
}

/// Watcher thread'i başlat. **Test sahnesinde** gerçek DeviceIoControl
/// yerine kullanıcı tarafından enjekte edilen tick fonksiyonu çağrılır;
/// production'da `cfg(windows)` altındaki real-impl çalışır.
///
/// `is_admin` false ise watcher BAŞLATILMAZ, `Ok(None)` döner. Caller
/// bunu "needs_admin" status'una çevirir.
pub fn spawn_watcher<F>(
    volume_id: String,
    is_admin: bool,
    mut tick: F,
) -> Result<Option<WatcherHandle>>
where
    F: FnMut(&str) -> Result<()> + Send + 'static,
{
    if !is_admin {
        info!(volume = volume_id, "USN watcher başlatılmadı: admin yok");
        return Ok(None);
    }
    let (tx, rx) = channel::<()>();
    let join = std::thread::Builder::new()
        .name(format!(
            "dspace-usn-watcher-{}",
            volume_id.replace(['\\', ':'], "_")
        ))
        .spawn(move || {
            info!(volume = volume_id, "USN watcher thread başladı");
            loop {
                match tick(&volume_id) {
                    Ok(()) => {}
                    Err(e) => {
                        warn!(volume = volume_id, ?e, "USN watcher tick hatası");
                    }
                }
                // Shutdown sinyali veya 5 sn time-out
                match rx.recv_timeout(WATCHER_POLL_INTERVAL) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        info!(volume = volume_id, "USN watcher kapatılıyor");
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        continue;
                    }
                }
            }
        })
        .map_err(|e| crate::error::Error::Index(format!("watcher thread spawn: {}", e)))?;
    Ok(Some(WatcherHandle {
        shutdown: Some(tx),
        join: Some(join),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn watcher_returns_none_when_not_admin() {
        let res = spawn_watcher(r"\\.\C:".to_string(), false, |_| Ok(())).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn watcher_shutdown_terminates_thread() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let handle = spawn_watcher(r"\\.\C:".to_string(), true, move |_| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap()
        .expect("watcher handle bekleniyor");

        // İlk tick anında çalışmalı
        std::thread::sleep(Duration::from_millis(50));
        let before_shutdown = counter.load(Ordering::SeqCst);
        assert!(before_shutdown >= 1, "tick en az 1 kez çalışmalı");

        // shutdown çağrısı thread'i join etmeli
        handle.shutdown();
        // Shutdown sonrası counter artık değişmemeli
        let after = counter.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(counter.load(Ordering::SeqCst), after);
    }
}
