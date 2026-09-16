#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use reqchain_core::paths::Paths;
use reqchain_desktop::commands;
use reqchain_desktop::state::AppState;

fn main() {
    tauri::Builder::default()
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
        .run(tauri::generate_context!())
        .expect("error while running reqchain");
}
