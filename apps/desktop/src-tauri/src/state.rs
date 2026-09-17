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
    /// Set when the last attempt to read `secrets.json` found it present but
    /// unparseable (as opposed to simply missing, which is fine — see
    /// `Secrets::load`). While this is `Some`, `secrets`/the executor's own
    /// snapshot keep serving the last known-good store rather than being
    /// silently emptied, and `set_secret_inner`/`delete_secret_inner` must
    /// refuse to write: `Secrets::save` writes the WHOLE map, so saving over
    /// a file that failed to parse would destroy whatever it still held.
    pub secrets_error: Mutex<Option<String>>,
    pub workspace: Mutex<Workspace>,
    /// Holds the in-memory `TokenCache` across runs so the UI does not
    /// re-fetch a chained-auth token on every click. Unlike the CLI, the GUI
    /// never persists its token cache to disk — see `TokenCache::in_memory`.
    pub executor: tokio::sync::Mutex<Executor>,
}

impl AppState {
    pub fn new(paths: Paths) -> AppState {
        let (secrets, secrets_error) = match Secrets::load(&paths) {
            Ok(s) => (s, None),
            Err(e) => (Secrets::empty(), Some(e)),
        };
        let workspace = Workspace::load(&paths);
        let executor = Executor::new(Runner::new(), TokenCache::in_memory(), secrets.clone());
        AppState {
            paths,
            secrets: Mutex::new(secrets),
            secrets_error: Mutex::new(secrets_error),
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
    /// yet absent from a mask built from some other, now-stale copy of
    /// `Secrets`. (`request_mask` in `commands.rs` reads the mask from the
    /// EXECUTOR's own snapshot via `Executor::secret_values` precisely so
    /// this can only ever be a staleness bug, never a disclosure one — but
    /// `state.secrets` must still track reality for `list_secrets`/
    /// `reveal_secret`, so it is kept in sync here too.)
    ///
    /// A `secrets.json` that fails to parse leaves both copies untouched —
    /// see `secrets_error`'s doc comment — rather than wiping a known-good
    /// snapshot just because a concurrent external edit briefly left the
    /// file malformed.
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
        match Secrets::load(&self.paths) {
            Ok(secrets) => {
                self.executor.lock().await.set_secrets(secrets.clone());
                *self.secrets.lock().unwrap() = secrets;
                *self.secrets_error.lock().unwrap() = None;
            }
            Err(e) => {
                *self.secrets_error.lock().unwrap() = Some(e);
            }
        }
    }
}
