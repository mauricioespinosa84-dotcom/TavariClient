#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod backend;
mod launcher;
mod models;
mod startup;
mod storage;

fn main() {
    tauri::Builder::default()
        .manage(startup::StartupState::default())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            backend::get_bootstrap,
            storage::save_settings,
            storage::logout,
            launcher::login_offline,
            launcher::login_microsoft,
            launcher::launch_instance,
            startup::startup_ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tavari Client");
}
