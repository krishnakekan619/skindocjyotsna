//! System information, backups, restore and data folders (administrators only).

use std::path::PathBuf;

use clinic_core::auth::Permission;
use clinic_services::maintenance::{self, HealthReport};
use clinic_sqlite::{BACKUP_EXTENSION, BackupFile};
use serde::Serialize;
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

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
    export_dir: String,
    log_dir: String,
    database: DatabaseStatusDto,
}

#[tauri::command]
pub fn get_system_info(state: State<'_, AppState>) -> Result<SystemInfo, CommandError> {
    state.session(Permission::ViewSystemInfo)?;
    let status = state.db()?.status()?;
    let paths = &state.paths;
    Ok(SystemInfo {
        app_version: state.app_version.clone(),
        platform: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        data_dir: paths.data_dir.display().to_string(),
        database_file: paths.database_file().display().to_string(),
        backup_dir: paths.backup_dir.display().to_string(),
        export_dir: paths.export_dir.display().to_string(),
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

#[tauri::command]
pub fn get_health(state: State<'_, AppState>) -> Result<HealthReport, CommandError> {
    let session = state.session(Permission::ViewSystemInfo)?;
    Ok(maintenance::health(&*state.db()?, &session)?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDto {
    file_name: String,
    size_bytes: u64,
    kind: Option<&'static str>,
    created_utc: Option<String>,
    app_version: Option<String>,
    problem: Option<String>,
}

impl From<BackupFile> for BackupDto {
    fn from(file: BackupFile) -> Self {
        let meta = file.meta;
        Self {
            file_name: file.file_name,
            size_bytes: file.size_bytes,
            kind: meta.as_ref().map(|m| m.kind.as_str()),
            created_utc: meta.as_ref().map(|m| m.created_utc.clone()),
            app_version: meta.map(|m| m.app_version),
            problem: file.problem,
        }
    }
}

#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupDto>, CommandError> {
    let session = state.session(Permission::ManageBackups)?;
    Ok(maintenance::backups(&session, &state.paths.backup_dir)?.into_iter().map(BackupDto::from).collect())
}

#[tauri::command]
pub fn create_backup(state: State<'_, AppState>) -> Result<BackupDto, CommandError> {
    let session = state.session(Permission::ManageBackups)?;
    let file = maintenance::backup_now(&mut *state.db()?, &session, &state.paths.backup_dir, &state.app_version, now())?;
    tracing::info!(file = %file.file_name, "manual backup created");
    Ok(file.into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResultDto {
    restored_from: BackupDto,
    safety_backup: BackupDto,
}

fn backup_path(dir: &std::path::Path, file_name: &str) -> Result<PathBuf, CommandError> {
    let plain = !file_name.is_empty()
        && !file_name.contains(['/', '\\', ':'])
        && !file_name.starts_with('.')
        && file_name.ends_with(&format!(".{BACKUP_EXTENSION}"));
    let path = dir.join(file_name);
    if !plain || !path.is_file() {
        return Err(CommandError::user("BACKUP_NOT_FOUND", "That backup file could not be found. Refresh the list and try again."));
    }
    Ok(path)
}

/// Restores a backup from the backups folder (by file name only). Everyone is signed out
/// afterwards, because the restored data may have different accounts.
#[tauri::command]
pub fn restore_backup(state: State<'_, AppState>, file_name: String) -> Result<RestoreResultDto, CommandError> {
    let session = state.session(Permission::ManageBackups)?;
    let path = backup_path(&state.paths.backup_dir, &file_name)?;
    let outcome = maintenance::restore(&mut *state.db()?, &session, &path, &state.paths.backup_dir, &state.app_version, now())?;
    tracing::warn!(from = %outcome.restored_from.file_name, by = %session.username, "database restored from backup");
    state.sign_out()?;
    Ok(RestoreResultDto { restored_from: outcome.restored_from.into(), safety_backup: outcome.safety_backup.into() })
}

/// Shows one of the app's folders in Explorer / Finder: "backups", "exports", "data" or "logs".
#[tauri::command]
pub fn open_folder(state: State<'_, AppState>, folder: String) -> Result<(), CommandError> {
    state.session(Permission::ViewSystemInfo)?;
    let paths = &state.paths;
    let dir = match folder.as_str() {
        "backups" => &paths.backup_dir,
        "exports" => &paths.export_dir,
        "data" => &paths.data_dir,
        "logs" => &paths.log_dir,
        _ => return Err(CommandError::user("NOT_FOUND", "Unknown folder.")),
    };
    tauri_plugin_opener::open_path(dir, None::<&str>).map_err(|error| {
        tracing::error!(%error, "could not open folder");
        CommandError::user("OPEN_FAILED", "The folder could not be opened.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_backup_file_names_are_accepted() {
        let dir = std::env::temp_dir();
        for bad in ["", "../clinic.db", "..\\x.clinicbak", "C:x.clinicbak", ".hidden.clinicbak", "notes.txt", "sub/a.clinicbak"] {
            assert!(backup_path(&dir, bad).is_err(), "{bad:?} must be rejected");
        }
    }
}
