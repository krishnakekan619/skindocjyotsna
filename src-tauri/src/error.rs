use clinic_sqlite::DbError;
use serde::Serialize;

/// Error returned to the UI. `message` is written for clinic staff; technical detail
/// stays in the application log and is never sent to the UI.
#[derive(Debug, Serialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}

impl CommandError {
    pub fn user(code: &'static str, message: &str) -> Self {
        Self { code, message: message.to_string() }
    }

    pub fn internal() -> Self {
        Self::user("INTERNAL", "Something went wrong. Please try again, or tell the administrator.")
    }
}

impl From<DbError> for CommandError {
    fn from(error: DbError) -> Self {
        eprintln!("database error: {error}"); // replaced by the file logger in Phase 1
        match error {
            DbError::InvalidBackup(_) => Self::user(
                "BACKUP_INVALID",
                "This backup file is damaged or is not a SkinDocJyotsna backup. Nothing was changed.",
            ),
            DbError::BackupFromNewerVersion { .. } => Self::user(
                "BACKUP_TOO_NEW",
                "This backup was made by a newer version of SkinDocJyotsna. Update the app, then restore again.",
            ),
            DbError::IntegrityFailed(_) => Self::user(
                "INTEGRITY_FAILED",
                "The database check found a problem. Your data was not changed. Please tell the administrator.",
            ),
            _ => Self::internal(),
        }
    }
}
