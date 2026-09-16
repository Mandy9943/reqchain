//! Bridges `core::watch`'s debounced filesystem watcher to a Tauri event so
//! the frontend can hot-reload when a workspace file changes on disk.
use reqchain_core::watch::{self, Watcher};
use std::future::Future;
use tauri::{AppHandle, Emitter, Manager};

/// Event name the frontend listens for. Carries no payload — on receipt it
/// simply re-invokes `load_workspace`.
pub const WORKSPACE_CHANGED_EVENT: &str = "workspace-changed";

/// Awaits `reload`, then calls `emit` — never the other way around, so a
/// frontend that re-fetches state on the event observes the reload's result,
/// not what was there before it.
///
/// Factored out of [`start`] so this ordering can be unit-tested without a
/// Tauri `AppHandle` (which cannot be constructed outside a running app):
/// the caller supplies the reload future and the emit closure directly.
pub async fn reload_and_notify(reload: impl Future<Output = ()>, emit: impl FnOnce()) {
    reload.await;
    emit();
}

/// Starts watching the workspace and wires each debounced change to a
/// reload-then-emit sequence. The `on_change` callback handed to
/// `core::watch::watch` runs on a plain `std::thread` (see `watch`'s
/// doc comment) and cannot `.await`, so it hands the async work off to
/// `tauri::async_runtime::spawn` instead of, say, `blocking_lock`ing onto
/// the executor mutex from a non-async context (that panics in practice —
/// see the task brief).
///
/// A watcher that fails to start (e.g. the workspace directory cannot be
/// created or watched) degrades to a manual-reload app: the error is
/// reported on stderr and `None` is returned. It must never prevent
/// startup, so callers should `app.manage(start(app))` unconditionally.
pub fn start(app: &AppHandle) -> Option<Watcher> {
    let paths = app.state::<crate::state::AppState>().paths.clone();
    let handle = app.clone();
    match watch::watch(&paths, move || {
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            let state = handle.state::<crate::state::AppState>();
            reload_and_notify(state.reload(), || {
                let _ = handle.emit(WORKSPACE_CHANGED_EVENT, ());
            })
            .await;
        });
    }) {
        Ok(w) => Some(w),
        Err(e) => {
            eprintln!("reqchain: file watching disabled: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Drives `reload_and_notify` with a fake reload future that yields
    /// (crosses a real suspend point, not just an immediately-ready future)
    /// and a fake emit closure, and asserts the emit is observed only after
    /// the reload has recorded its completion. This is the part of
    /// `watcher::start` that is logic rather than Tauri wiring, and the part
    /// this task can actually unit-test: a `tauri::AppHandle` cannot be
    /// constructed outside a running app, so `start` itself is verified only
    /// by inspection (and by the "verify by hand" step in the task brief).
    #[tokio::test]
    async fn emits_only_after_reload_completes() {
        let order = Rc::new(RefCell::new(Vec::new()));

        let reload_order = order.clone();
        let reload = async move {
            // Cross a real await point so a buggy implementation that emits
            // before the reload future is polled to completion would be
            // caught, not accidentally passed because everything ran
            // synchronously.
            tokio::task::yield_now().await;
            reload_order.borrow_mut().push("reload");
        };

        let emit_order = order.clone();
        let emit = || emit_order.borrow_mut().push("emit");

        reload_and_notify(reload, emit).await;

        assert_eq!(*order.borrow(), vec!["reload", "emit"]);
    }
}
