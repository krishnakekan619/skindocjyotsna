use std::path::PathBuf;

use clinic_sqlite::{BACKUP_EXTENSION, BackupFile, BackupKind};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::CommandError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDto {
    file_name: String,
    size_bytes: u64,
    kind: Option<&'static str>,
    created_utc: Option<String>,
    app_version: Option<String>,
    /// Set when the file cannot be used; written for staff.
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResultDto {
    restored_from: BackupDto,
    safety_backup: BackupDto,
}

#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupDto>, CommandError> {
    Ok(clinic_sqlite::list_backups(&state.paths.backup_dir)?.into_iter().map(BackupDto::from).collect())
}

#[tauri::command]
pub fn create_backup(app: AppHandle, state: State<'_, AppState>) -> Result<BackupDto, CommandError> {
    let version = app.package_info().version.to_string();
    let db = state.db.lock().map_err(|_| CommandError::internal())?;
    Ok(db.create_backup(&state.paths.backup_dir, BackupKind::Manual, &version)?.into())
}

/// Restores a backup from the app's backup folder, identified by file name only
/// (the UI can never point this at an arbitrary path).
#[tauri::command]
pub fn restore_backup(app: AppHandle, state: State<'_, AppState>, file_name: String) -> Result<RestoreResultDto, CommandError> {
    let path = backup_path(&state.paths.backup_dir, &file_name)?;
    let version = app.package_info().version.to_string();
    let mut db = state.db.lock().map_err(|_| CommandError::internal())?;
    let outcome = db.restore_backup(&path, &state.paths.backup_dir, &version)?;
    Ok(RestoreResultDto { restored_from: outcome.restored_from.into(), safety_backup: outcome.safety_backup.into() })
}

fn backup_path(dir: &std::path::Path, file_name: &str) -> Result<PathBuf, CommandError> {
    let plain_name = !file_name.is_empty()
        && !file_name.contains(['/', '\\', ':'])
        && !file_name.starts_with('.')
        && file_name.ends_with(&format!(".{BACKUP_EXTENSION}"));
    let path = dir.join(file_name);
    if !plain_name || !path.is_file() {
        return Err(CommandError::user("BACKUP_NOT_FOUND", "That backup file could not be found. Refresh the list and try again."));
    }
    Ok(path)
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
