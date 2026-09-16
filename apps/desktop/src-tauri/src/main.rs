#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use reqchain_core::paths::Paths;
use reqchain_desktop::commands;
use reqchain_desktop::state::AppState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        // "Copy as curl" (spec §8) cannot use `navigator.clipboard`: Tauri's
        // `WebviewAttributes.clipboard` defaults to false, and wry only enables
        // WebKitGTK clipboard access when it is set, so `writeText` rejects on
        // the only platform this ships to. The plugin writes through the
        // system clipboard directly instead. Write only — see
        // capabilities/default.json.
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::new(Paths::from_env()))
        .invoke_handler(tauri::generate_handler![
            commands::load_workspace,
            commands::lint,
            commands::save_api,
            commands::run_endpoint,
            commands::preview_endpoint,
            commands::curl_command,
            commands::history
        ])
        .setup(|app| {
            let watcher = reqchain_desktop::watcher::start(app.handle());
            app.manage(watcher);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running reqchain");
}
