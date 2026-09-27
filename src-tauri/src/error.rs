use clinic_services::ServiceError;
use clinic_sqlite::DbError;
use serde::Serialize;

/// Error returned to the UI. `message` is written for clinic staff (brief §31); technical
/// detail goes only to the log file, with the same reference.
#[derive(Debug, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{code}: {message}")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
    /// Form field the problem belongs to, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<&'static str>,
}

impl CommandError {
    pub fn user(code: &'static str, message: &str) -> Self {
        Self { code, message: message.to_string(), field: None }
    }

    pub fn internal() -> Self {
        Self::user("INTERNAL", "Something went wrong. Please try again, or tell the administrator.")
    }
}

fn database_message(error: &DbError) -> CommandError {
    match error {
        DbError::InvalidBackup(_) => CommandError::user("BACKUP_INVALID", "This backup file is damaged or is not a SkinDocJyotsna backup. Nothing was changed."),
        DbError::BackupFromNewerVersion { .. } => {
            CommandError::user("BACKUP_TOO_NEW", "This backup was made by a newer version of SkinDocJyotsna. Update the app, then restore again.")
        }
        DbError::IntegrityFailed(_) => CommandError::user("INTEGRITY_FAILED", "The database check found a problem. Your data was not changed. Please tell the administrator."),
        _ => CommandError::internal(),
    }
}

impl From<DbError> for CommandError {
    fn from(error: DbError) -> Self {
        tracing::error!(%error, "database error");
        database_message(&error)
    }
}

impl From<ServiceError> for CommandError {
    fn from(error: ServiceError) -> Self {
        use ServiceError as E;
        let staff = |code: &'static str, message: String| CommandError { code, message, field: None };
        match error {
            E::Validation { field, message } => CommandError { code: "VALIDATION", message, field: Some(field) },
            E::SetupAlreadyDone => staff("SETUP_DONE", "The app is already set up. Please sign in.".into()),
            E::InvalidCredentials => staff("INVALID_CREDENTIALS", "Wrong username or password.".into()),
            E::AccountLocked { .. } => staff("ACCOUNT_LOCKED", "Too many wrong passwords. Please wait 5 minutes and try again, or ask an administrator.".into()),
            E::AccountDisabled => staff("ACCOUNT_DISABLED", "This account is disabled. Please ask an administrator.".into()),
            E::PinNotSet => staff("PIN_NOT_SET", "No PIN is set for this account. Please use your password.".into()),
            E::PinLocked => staff("PIN_LOCKED", "Too many wrong PINs. Please unlock with your password.".into()),
            E::WrongPin { remaining } => staff("WRONG_PIN", format!("Wrong PIN. {remaining} tries left before the password is required.")),
            E::NotFound(what) => staff("NOT_FOUND", format!("Unable to complete the operation. The {what} no longer exists.")),
            E::Conflict(message) => staff("CONFLICT", message),
            E::LastAdmin => staff("LAST_ADMIN", "The clinic must keep at least one active administrator.".into()),
            E::PermissionDenied => staff("PERMISSION_DENIED", "You don't have permission to do this.".into()),
            E::NotAllowed(message) => staff("NOT_ALLOWED", message),
            E::InsufficientStock { product, available } => staff("INSUFFICIENT_STOCK", format!("Insufficient stock for {product}.\nAvailable quantity: {available}")),
            E::DiscountApprovalRequired { cap_percent } => {
                staff("DISCOUNT_APPROVAL_REQUIRED", format!("Discounts above {cap_percent}% need an administrator's approval."))
            }
            E::PossibleDuplicate(message) => staff("POSSIBLE_DUPLICATE", message),
            E::NoPhone(message) => staff("NO_PHONE", message),
            E::Corrupt(detail) => {
                tracing::error!(%detail, "stored data is invalid");
                CommandError::internal()
            }
            E::Hashing(error) => {
                tracing::error!(%error, "hashing failed");
                CommandError::internal()
            }
            E::Db(error) => {
                tracing::error!(%error, "database error");
                database_message(&error)
            }
        }
    }
}
