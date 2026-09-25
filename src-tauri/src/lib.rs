//! Desktop shell: wires the UI (IPC commands) to the business logic and database.

mod commands;
mod error;
mod paths;
mod state;

use std::sync::Mutex;

use tauri::Manager;

use crate::paths::AppPaths;
use crate::state::AppState;

pub fn run() {
    let result = tauri::Builder::default()
        // Must be registered first: a second launch focuses the running window instead of
        // opening a second copy that would also write to the database.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let paths = AppPaths::resolve(app.handle())?;
            let db = clinic_sqlite::Database::open(&paths.database_file())?;
            app.manage(AppState { paths, db: Mutex::new(db) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::get_system_info,
            commands::backup::list_backups,
            commands::backup::create_backup,
            commands::backup::restore_backup,
            commands::receipt::get_sample_receipt,
            commands::receipt::export_sample_receipt_pdf,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        // Logging to file arrives in Phase 1; until then stderr is the only sink.
        eprintln!("SkinDocJyotsna failed to start: {error}");
        std::process::exit(1);
    }
}
