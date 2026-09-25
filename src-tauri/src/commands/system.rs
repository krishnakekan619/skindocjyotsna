use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::CommandError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStatusDto {
    sqlite_version: String,
    schema_version: i64,
    journal_mode: String,
    integrity_ok: bool,
    fts5_available: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    app_version: String,
    platform: &'static str,
    arch: &'static str,
    data_dir: String,
    database_file: String,
    backup_dir: String,
    log_dir: String,
    database: DatabaseStatusDto,
}

/// Health and location information for the System screen (admin-only from Phase 1).
#[tauri::command]
pub fn get_system_info(app: AppHandle, state: State<'_, AppState>) -> Result<SystemInfo, CommandError> {
    let status = {
        let db = state.db.lock().map_err(|_| CommandError::internal())?;
        db.status()?
    };
    let paths = &state.paths;
    Ok(SystemInfo {
        app_version: app.package_info().version.to_string(),
        platform: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        data_dir: paths.data_dir.display().to_string(),
        database_file: paths.database_file().display().to_string(),
        backup_dir: paths.backup_dir.display().to_string(),
        log_dir: paths.log_dir.display().to_string(),
        database: DatabaseStatusDto {
            sqlite_version: status.sqlite_version,
            schema_version: status.schema_version,
            journal_mode: status.journal_mode,
            integrity_ok: status.integrity_ok,
            fts5_available: status.fts5_available,
        },
    })
}
