use reqchain_core::cache::TokenCache;
use reqchain_core::chain::Executor;
use reqchain_core::exec::Runner;
use reqchain_core::paths::Paths;
use reqchain_core::secrets::Secrets;
use reqchain_core::store::Workspace;
use std::sync::Mutex;

pub struct AppState {
    pub paths: Paths,
    pub secrets: Mutex<Secrets>,
    pub workspace: Mutex<Workspace>,
    /// Holds the in-memory `TokenCache` across runs so the UI does not
    /// re-fetch a chained-auth token on every click. Unlike the CLI, the GUI
    /// never persists its token cache to disk — see `TokenCache::in_memory`.
    pub executor: tokio::sync::Mutex<Executor>,
}

impl AppState {
    pub fn new(paths: Paths) -> AppState {
        let secrets = Secrets::load(&paths);
        let workspace = Workspace::load(&paths);
        let executor = Executor::new(Runner::new(), TokenCache::in_memory(), secrets.clone());
        AppState {
            paths,
            secrets: Mutex::new(secrets),
            workspace: Mutex::new(workspace),
            executor: tokio::sync::Mutex::new(executor),
        }
    }

    /// Re-reads secrets and workspace from disk. Called on startup and on every
    /// watcher event.
    ///
    /// The `Executor` keeps its own copy of `Secrets` (it needs `&Secrets` to
    /// build a `Scope` for every run) — without pushing the fresh copy into
    /// it too, a rotated or deleted secret would keep being resolved from the
    /// stale snapshot the executor was constructed with: sent on the wire,
    /// yet absent from `request_mask`/`response_mask` (which read the LIVE
    /// `state.secrets`), so it would appear unmasked in the effective
    /// request, the curl export, and history.
    ///
    /// This is `async` — not `blocking_lock` on a sync `reload` — because
    /// Tauri does not guarantee a non-async command runs off the async
    /// reactor's own worker threads; `blocking_lock` there panics ("cannot
    /// block the current thread from within a runtime"), and spin-waiting
    /// would risk starving the very thread a concurrent `run_endpoint` needs
    /// to make progress on. Callers (`load_workspace_inner`,
    /// `save_api_inner`) are `async fn` for this reason.
    pub async fn reload(&self) {
        *self.workspace.lock().unwrap() = Workspace::load(&self.paths);
        let secrets = Secrets::load(&self.paths);
        self.executor.lock().await.set_secrets(secrets.clone());
        *self.secrets.lock().unwrap() = secrets;
    }
}
