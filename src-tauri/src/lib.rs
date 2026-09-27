//! Desktop shell: wires the UI (IPC commands) to the services and the database.

mod commands;
mod disk;
mod error;
mod mirror;
mod paths;
mod state;

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use clinic_services::maintenance::{self, AUTO_BACKUP_INTERVAL_SECS, EXIT_BACKUP_INTERVAL_SECS};
use tauri::{AppHandle, Manager, RunEvent};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};

use crate::paths::AppPaths;
use crate::state::{AppState, AuthState, now};

/// Where a crash is written, synchronously (the normal log writes on a background thread, which
/// may not get to run when the app stops on an internal error).
static CRASH_LOG: OnceLock<PathBuf> = OnceLock::new();

/// Release builds stop on an internal error (`panic = "abort"`): record why before that happens.
fn install_crash_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(path) = CRASH_LOG.get() {
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{} SkinDocJyotsna stopped after an internal error: {info}", now());
            }
        }
        tracing::error!(%info, "internal error");
        previous(info);
    }));
}

/// Shows why the app could not start (otherwise a Windows release build just closes silently).
fn show_startup_error(message: &str) {
    let title = "SkinDocJyotsna could not start";
    let text = format!("{message}\n\nNothing was changed. Details are in the log folder. If this keeps happening, restore the latest backup or contact support.");
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", "on run argv", "-e", "display alert (item 1 of argv) message (item 2 of argv) as critical", "-e", "end run", title, &text])
        .status();
    #[cfg(windows)]
    let _ = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show($env:SKINDOC_TEXT, $env:SKINDOC_TITLE, 'OK', 'Error') | Out-Null",
            ])
            .env("SKINDOC_TEXT", &text)
            .env("SKINDOC_TITLE", title)
            .creation_flags(CREATE_NO_WINDOW)
            .status()
    };
    eprintln!("{title}: {text}");
}

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
        Ok(Some(file)) => {
            tracing::info!(file = %file.file_name, "automatic backup created");
            mirror::copy_backup(&file, &state.paths.mirror_dir);
        }
        Ok(None) => {}
        Err(error) => tracing::error!(%error, "automatic backup failed"),
    }
}

/// Everything the app needs before the window opens. An error here is shown to the user.
fn start(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let paths = AppPaths::resolve(app.handle())?;
    let _ = CRASH_LOG.set(paths.log_dir.join("crash.log"));
    // Logging problems must not stop the clinic from billing.
    match init_logging(&paths) {
        Ok(guard) => {
            app.manage(LogGuard(guard));
        }
        Err(error) => eprintln!("logging is not available: {error}"),
    }
    let version = app.package_info().version.to_string();
    tracing::info!(%version, os = std::env::consts::OS, "SkinDocJyotsna starting");
    // A new version that changes the database first saves a "pre-upgrade" backup.
    let (db, upgrade_backup) = clinic_sqlite::Database::open_with_upgrade_backup(&paths.database_file(), &paths.backup_dir, &version)?;
    if let Some(file) = upgrade_backup {
        tracing::info!(file = %file.file_name, "backup made before upgrading the database");
        mirror::copy_backup(&file, &paths.mirror_dir);
    }
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
}

pub fn run() {
    install_crash_log();
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
            start(app).map_err(|error| {
                tracing::error!(%error, "could not start");
                show_startup_error(&error.to_string());
                error
            })
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
            commands::inventory::add_inventory,
            commands::inventory::delete_product,
            commands::inventory::adjust_stock,
            commands::inventory::list_stock_ledger,
            commands::inventory::list_expiring,
            commands::inventory::search_products_for_sale,
            commands::inventory::recent_products_for_sale,
            commands::clients::search_clients,
            commands::clients::save_client,
            commands::clients::get_client_profile,
            commands::clients::check_client_duplicates,
            commands::clients::find_duplicate_clients,
            commands::clients::merge_clients,
            commands::catalog::list_services,
            commands::catalog::save_service,
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
            commands::billing::open_whatsapp,
            commands::reports::get_dashboard,
            commands::reports::sales_report,
            commands::reports::product_sales_report,
            commands::reports::stock_report,
            commands::reports::top_sellers,
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
