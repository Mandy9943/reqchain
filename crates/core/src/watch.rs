//! Debounced watcher over the workspace `apis/` directory. The UI reloads the
//! whole workspace on any change: loading is cheap and per-file error isolation
//! already lives in `store::Workspace::load`.
use crate::paths::Paths;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// Holds the notify watcher; dropping it stops watching.
///
/// There is no explicit stop signal: dropping `_inner` drops the notify
/// event closure, which owns the debounce thread's `events_tx` sender. The
/// thread's blocking `recv_timeout` then observes `Disconnected` and exits
/// immediately. This relies on `notify`'s `Drop` impl tearing down its
/// platform watcher (and thus the closure) synchronously before returning —
/// re-check this if `notify` is upgraded.
pub struct Watcher {
    _inner: RecommendedWatcher,
}

pub fn watch(paths: &Paths, on_change: impl Fn() + Send + 'static) -> notify::Result<Watcher> {
    let dir = paths.apis_dir();
    match std::fs::create_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }

    let (events_tx, events_rx) = mpsc::channel::<()>();

    let mut inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        match res {
            Ok(event) => {
                if matches!(
                    event.kind,
                    notify::EventKind::Create(_)
                        | notify::EventKind::Modify(_)
                        | notify::EventKind::Remove(_)
                ) {
                    let _ = events_tx.send(());
                }
            }
            // A dead watch (descriptor exhaustion, permission revoked, ...)
            // must not look like a quiet workspace: there is no channel back
            // to the caller here, so surface it the same way the rest of the
            // app reports unexpected I/O failures. Never treat an error as a
            // change.
            Err(e) => {
                eprintln!("reqchain: file watching error: {e}");
            }
        }
    })?;
    inner.watch(&dir, RecursiveMode::Recursive)?;

    std::thread::spawn(move || loop {
        match events_rx.recv_timeout(DEBOUNCE) {
            Ok(()) => {
                // A burst started. Drain it, but only up to a fixed deadline
                // measured from the *first* event of the burst, not a
                // quiet-period that resets on every event received — a
                // steady stream of events (faster than DEBOUNCE apart) would
                // otherwise keep resetting the wait and starve the callback
                // forever. This guarantees on_change fires within DEBOUNCE
                // of the burst starting, and at most once per window.
                let deadline = Instant::now() + DEBOUNCE;
                loop {
                    let now = Instant::now();
                    if now >= deadline {
                        break;
                    }
                    match events_rx.recv_timeout(deadline - now) {
                        Ok(()) => continue,
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
                on_change();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    });

    Ok(Watcher { _inner: inner })
}
