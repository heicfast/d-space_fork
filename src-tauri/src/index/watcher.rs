// SPDX-License-Identifier: GPL-3.0-or-later
//
// USN delta watcher — Sprint 3.8.
//
// Discovery Log #005 — Chapter 5.6: background thread, 5 sec polling. On
// each tick `FSCTL_READ_USN_JOURNAL` is called, the incoming records
// are written to the DB via `apply_delta`, the watermark is updated.
// Stopped via the shutdown channel.
//
// **Privilege note**: reading the USN journal requires opening a `\\.\C:`
// raw volume handle — admin required. `spawn_watcher` does not start the
// watcher without admin, it returns `IndexStatus { mode: "needs_admin" }`.
// The same privilege as Fast Mode (Chapter 5.2A Layer 1).
//
// v0.3.1 — now wired into production: `lib.rs::spawn_usn_watchers`
// starts the watchers at launch and after `index_build`; the tick closure
// calls `index::sync_volume` (with a lazy dedicated Connection).
// The tests cover the platform-agnostic parts (channel-shutdown semantics).

use crate::error::Result;
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{info, warn};

/// Watcher poll interval — 60 sec (Gemini review 2.4 — battery friendly).
/// Ideal: fully event-driven `FSCTL_READ_USN_JOURNAL` with Timeout > 0
/// (`read_journal_blocking` in index/usn.rs); the v0.3.1 prod wire will
/// use that. This polling interval is the fallback for the scaffold.
pub const WATCHER_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Watcher handle — drop sends the thread-shutdown signal.
pub struct WatcherHandle {
    shutdown: Option<Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl WatcherHandle {
    /// Stop the thread and join it. Errors are logged.
    pub fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            if let Err(e) = j.join() {
                warn!(?e, "watcher join failed");
            }
        }
    }
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        // We do not leave the join to drop — the caller should do it
        // explicitly via `shutdown()`; detach on drop.
    }
}

/// Start the watcher thread. **In the test harness**, instead of the real
/// DeviceIoControl the tick function injected by the caller is invoked;
/// in production the real impl under `cfg(windows)` runs.
///
/// If `is_admin` is false the watcher is NOT started, `Ok(None)` is returned.
/// The caller turns this into the "needs_admin" status.
pub fn spawn_watcher<F>(
    volume_id: String,
    is_admin: bool,
    mut tick: F,
) -> Result<Option<WatcherHandle>>
where
    F: FnMut(&str) -> Result<()> + Send + 'static,
{
    if !is_admin {
        info!(
            volume = volume_id,
            "USN watcher not started: no admin privileges"
        );
        return Ok(None);
    }
    let (tx, rx) = channel::<()>();
    let join = std::thread::Builder::new()
        .name(format!(
            "dspace-usn-watcher-{}",
            volume_id.replace(['\\', ':'], "_")
        ))
        .spawn(move || {
            info!(volume = volume_id, "USN watcher thread started");
            loop {
                match tick(&volume_id) {
                    Ok(()) => {}
                    Err(e) => {
                        warn!(volume = volume_id, ?e, "USN watcher tick failed");
                    }
                }
                // Shutdown signal or 5 sec time-out
                match rx.recv_timeout(WATCHER_POLL_INTERVAL) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        info!(volume = volume_id, "USN watcher shutting down");
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
        .expect("expected watcher handle");

        // The first tick must run immediately
        std::thread::sleep(Duration::from_millis(50));
        let before_shutdown = counter.load(Ordering::SeqCst);
        assert!(before_shutdown >= 1, "tick must run at least once");

        // The shutdown call must join the thread
        handle.shutdown();
        // After shutdown the counter must no longer change
        let after = counter.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(counter.load(Ordering::SeqCst), after);
    }
}
