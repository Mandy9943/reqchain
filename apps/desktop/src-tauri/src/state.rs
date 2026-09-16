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
    pub fn reload(&self) {
        *self.workspace.lock().unwrap() = Workspace::load(&self.paths);
        *self.secrets.lock().unwrap() = Secrets::load(&self.paths);
    }
}
