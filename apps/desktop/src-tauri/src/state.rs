use reqchain_core::paths::Paths;
use reqchain_core::secrets::Secrets;
use reqchain_core::store::Workspace;
use std::sync::Mutex;

pub struct AppState {
    pub paths: Paths,
    pub secrets: Mutex<Secrets>,
    pub workspace: Mutex<Workspace>,
}

impl AppState {
    pub fn new(paths: Paths) -> AppState {
        let secrets = Secrets::load(&paths);
        let workspace = Workspace::load(&paths);
        AppState {
            paths,
            secrets: Mutex::new(secrets),
            workspace: Mutex::new(workspace),
        }
    }

    /// Re-reads secrets and workspace from disk. Called on startup and on every
    /// watcher event.
    pub fn reload(&self) {
        *self.workspace.lock().unwrap() = Workspace::load(&self.paths);
        *self.secrets.lock().unwrap() = Secrets::load(&self.paths);
    }
}
