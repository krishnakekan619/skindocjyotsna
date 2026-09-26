use clinic_core::auth::PolicyViolation;
use clinic_core::auth::hashing::HashError;
use clinic_sqlite::DbError;

/// Everything a service can refuse or fail with. The shell turns these into staff-friendly
/// messages; `Validation` messages are already written for staff.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("{message}")]
    Validation { field: &'static str, message: String },
    #[error("setup has already been completed")]
    SetupAlreadyDone,
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("account locked until {until}")]
    AccountLocked { until: i64 },
    #[error("account is disabled")]
    AccountDisabled,
    #[error("no PIN is set")]
    PinNotSet,
    #[error("PIN unlock is disabled after too many wrong PINs")]
    PinLocked,
    #[error("wrong PIN ({remaining} tries left)")]
    WrongPin { remaining: i64 },
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(String),
    #[error("the last active administrator cannot be deactivated or demoted")]
    LastAdmin,
    #[error("permission denied")]
    PermissionDenied,
    /// A business rule refused the action; the message is written for staff.
    #[error("{0}")]
    NotAllowed(String),
    #[error("insufficient stock for {product}: {available} available")]
    InsufficientStock { product: String, available: i64 },
    #[error("discount above {cap_percent}% needs administrator approval")]
    DiscountApprovalRequired { cap_percent: u32 },
    /// A consultation or procedure price below the catalog price needs an administrator.
    #[error("a price below the standard fee needs administrator approval")]
    PriceApprovalRequired,
    /// Creating this client would likely duplicate an existing one; the message is for staff.
    #[error("{0}")]
    PossibleDuplicate(String),
    /// The bill's client has no mobile number (or the bill has no client).
    #[error("{0}")]
    NoPhone(String),
    #[error("stored data is invalid: {0}")]
    Corrupt(String),
    #[error(transparent)]
    Hashing(#[from] HashError),
    #[error(transparent)]
    Db(#[from] DbError),
}

impl From<clinic_sqlite::rusqlite::Error> for ServiceError {
    fn from(error: clinic_sqlite::rusqlite::Error) -> Self {
        ServiceError::Db(DbError::Sqlite(error))
    }
}

impl From<PolicyViolation> for ServiceError {
    fn from(violation: PolicyViolation) -> Self {
        ServiceError::Validation { field: violation.field, message: violation.message }
    }
}

pub(crate) fn invalid(field: &'static str, message: &str) -> ServiceError {
    ServiceError::Validation { field, message: message.to_string() }
}
