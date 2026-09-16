//! Debounced watcher over the workspace `apis/` directory. The UI reloads the
//! whole workspace on any change: loading is cheap and per-file error isolation
//! already lives in `store::Workspace::load`.
use crate::paths::Paths;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// Holds the notify watcher and the debounce thread. Dropping it stops both.
pub struct Watcher {
    _inner: RecommendedWatcher,
    _stop: mpsc::Sender<()>,
}

pub fn watch(paths: &Paths, on_change: impl Fn() + Send + 'static) -> notify::Result<Watcher> {
    let dir = paths.apis_dir();
    std::fs::create_dir_all(&dir).ok();

    let (events_tx, events_rx) = mpsc::channel::<()>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    let mut inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                notify::EventKind::Create(_)
                    | notify::EventKind::Modify(_)
                    | notify::EventKind::Remove(_)
            ) {
                let _ = events_tx.send(());
            }
        }
    })?;
    inner.watch(&dir, RecursiveMode::Recursive)?;

    std::thread::spawn(move || loop {
        if stop_rx.try_recv().is_ok() {
            return;
        }
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
                if stop_rx.try_recv().is_ok() {
                    return;
                }
                on_change();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    });

    Ok(Watcher {
        _inner: inner,
        _stop: stop_tx,
    })
}
