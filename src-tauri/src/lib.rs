//! Desktop shell: wires the UI (IPC commands) to the services and the database.

mod commands;
mod error;
mod paths;
mod state;

use std::sync::Mutex;
use std::time::Duration;

use clinic_services::maintenance::{self, AUTO_BACKUP_INTERVAL_SECS, EXIT_BACKUP_INTERVAL_SECS};
use tauri::{AppHandle, Manager, RunEvent};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};

use crate::paths::AppPaths;
use crate::state::{AppState, AuthState, now};

/// Keeps the log writer alive for the whole run (dropping it flushes the log file).
struct LogGuard(#[allow(dead_code)] WorkerGuard);

fn init_logging(paths: &AppPaths) -> Result<WorkerGuard, Box<dyn std::error::Error>> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("skindocjyotsna")
        .filename_suffix("log")
        .max_log_files(30)
        .build(&paths.log_dir)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt().with_writer(writer).with_ansi(false).with_target(false).init();
    Ok(guard)
}

/// Daily automatic backup (D22). Never blocks or breaks the app: failures are only logged.
fn auto_backup(app: &AppHandle, min_age_secs: i64) {
    let state = app.state::<AppState>();
    let mut db = match state.db.lock() {
        Ok(db) => db,
        Err(poisoned) => {
            tracing::warn!("automatic backup: recovered a lock poisoned by an earlier panic");
            poisoned.into_inner()
        }
    };
    match maintenance::auto_backup_if_due(&mut db, &state.paths.backup_dir, &state.app_version, now(), min_age_secs) {
        Ok(Some(file)) => tracing::info!(file = %file.file_name, "automatic backup created"),
        Ok(None) => {}
        Err(error) => tracing::error!(%error, "automatic backup failed"),
    }
}

pub fn run() {
    let app = tauri::Builder::default()
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
            app.manage(LogGuard(init_logging(&paths)?));
            let version = app.package_info().version.to_string();
            tracing::info!(%version, os = std::env::consts::OS, "SkinDocJyotsna starting");
            let db = clinic_sqlite::Database::open(&paths.database_file())?;
            app.manage(AppState {
                paths,
                db: Mutex::new(db),
                auth: Mutex::new(AuthState { session: None, locked: false, last_activity: 0, idle_lock_secs: 15 * 60 }),
                app_version: version,
            });
            // Automatic backups: one minute after start (covers PCs switched off at backup
            // time), then every 30 minutes the daily one is made when due.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(60));
                loop {
                    auto_backup(&handle, AUTO_BACKUP_INTERVAL_SECS);
                    std::thread::sleep(Duration::from_secs(30 * 60));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::session::get_app_status,
            commands::session::complete_setup,
            commands::session::login,
            commands::session::logout,
            commands::session::lock_screen,
            commands::session::unlock_with_pin,
            commands::session::unlock_with_password,
            commands::session::heartbeat,
            commands::session::change_own_password,
            commands::session::set_own_pin,
            commands::admin::list_users,
            commands::admin::create_user,
            commands::admin::update_user,
            commands::admin::reset_user_password,
            commands::admin::get_clinic_settings,
            commands::admin::update_clinic_settings,
            commands::admin::list_audit,
            commands::inventory::list_categories,
            commands::inventory::save_category,
            commands::inventory::list_suppliers,
            commands::inventory::save_supplier,
            commands::inventory::list_products,
            commands::inventory::get_product,
            commands::inventory::save_product,
            commands::inventory::stock_in,
            commands::inventory::adjust_stock,
            commands::inventory::list_stock_ledger,
            commands::inventory::list_expiring,
            commands::inventory::search_products_for_sale,
            commands::clients::search_clients,
            commands::clients::save_client,
            commands::clients::get_client_profile,
            commands::billing::quote_bill,
            commands::billing::finalize_bill,
            commands::billing::list_bills,
            commands::billing::get_bill,
            commands::billing::cancel_bill,
            commands::billing::return_bill_items,
            commands::billing::correct_bill,
            commands::billing::get_receipt,
            commands::billing::export_receipt_pdf,
            commands::billing::open_export,
            commands::reports::get_dashboard,
            commands::reports::sales_report,
            commands::reports::product_sales_report,
            commands::reports::stock_report,
            commands::system::get_system_info,
            commands::system::get_health,
            commands::system::list_backups,
            commands::system::create_backup,
            commands::system::restore_backup,
            commands::system::open_folder,
        ])
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(error) => {
            // The log may not exist yet at this point; stderr is the only place left.
            eprintln!("SkinDocJyotsna failed to start: {error}");
            std::process::exit(1);
        }
    };
    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // Back up on close if the last automatic backup is older than 12 hours (D22).
            auto_backup(handle, EXIT_BACKUP_INTERVAL_SECS);
            tracing::info!("SkinDocJyotsna stopped");
        }
    });
}
