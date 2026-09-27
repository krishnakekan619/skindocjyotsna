use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use clinic_core::auth::Permission;
use clinic_services::Session;
use clinic_sqlite::Database;

use crate::error::CommandError;
use crate::paths::AppPaths;

/// Who is signed in, and whether the screen is locked (DEC-023/DEC-024).
pub struct AuthState {
    pub session: Option<Session>,
    pub locked: bool,
    pub last_activity: i64,
    pub idle_lock_secs: i64,
}

/// Shared application state. The single database connection is the only writer, so all
/// writes are serialised (no "database is busy" errors during billing).
pub struct AppState {
    pub paths: AppPaths,
    pub db: Mutex<Database>,
    pub auth: Mutex<AuthState>,
    pub app_version: String,
}

/// The server locks this much later than the UI's own timer, because the UI reports activity
/// at most once a minute (the UI locks on time; this is the backstop).
const IDLE_GRACE_SECS: i64 = 60;

/// A panic while a lock was held must not break the app until restart. The database stays
/// consistent (an unfinished transaction rolls back when dropped), so carry on with a warning.
fn recover<T>(poisoned: PoisonError<MutexGuard<'_, T>>) -> MutexGuard<'_, T> {
    tracing::warn!("recovered a lock poisoned by an earlier panic");
    poisoned.into_inner()
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| i64::try_from(d.as_secs()).unwrap_or(0)).unwrap_or(0)
}

impl AppState {
    pub fn db(&self) -> Result<MutexGuard<'_, Database>, CommandError> {
        Ok(self.db.lock().unwrap_or_else(recover))
    }

    pub fn auth(&self) -> Result<MutexGuard<'_, AuthState>, CommandError> {
        Ok(self.auth.lock().unwrap_or_else(recover))
    }

    /// The signed-in user, if the screen is unlocked, not idle for too long, and the role has
    /// `permission`. Records activity (for the idle lock). Enforced here, not only in the UI.
    pub fn session(&self, permission: Permission) -> Result<Session, CommandError> {
        let mut auth = self.auth()?;
        let now = now();
        let Some(session) = auth.session.clone() else {
            return Err(CommandError::user("NOT_SIGNED_IN", "Please sign in."));
        };
        if !auth.locked && now - auth.last_activity > auth.idle_lock_secs + IDLE_GRACE_SECS {
            auth.locked = true;
        }
        if auth.locked {
            return Err(CommandError::user("LOCKED", "The screen is locked. Enter your PIN or password to continue."));
        }
        auth.last_activity = now;
        drop(auth);
        // The account may have been deactivated or demoted since sign-in: end the session.
        let still_valid = clinic_services::auth::session_still_valid(&*self.db()?, &session).unwrap_or(false);
        if !still_valid {
            self.sign_out()?;
            return Err(CommandError::user("NOT_SIGNED_IN", "Your account was changed by an administrator. Please sign in again."));
        }
        if !session.role.allows(permission) {
            return Err(CommandError::user("PERMISSION_DENIED", "You don't have permission to do this."));
        }
        Ok(session)
    }

    /// Starts (or refreshes) the signed-in, unlocked session.
    pub fn sign_in(&self, session: Session, idle_lock_minutes: u32) -> Result<(), CommandError> {
        let mut auth = self.auth()?;
        auth.session = Some(session);
        auth.locked = false;
        auth.last_activity = now();
        auth.idle_lock_secs = i64::from(idle_lock_minutes) * 60;
        Ok(())
    }

    pub fn sign_out(&self) -> Result<Option<Session>, CommandError> {
        let mut auth = self.auth()?;
        auth.locked = false;
        Ok(auth.session.take())
    }
}
