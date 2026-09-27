//! First-run setup, login, PIN unlock and the signed-in user's own password/PIN.

use std::sync::OnceLock;

use clinic_core::auth::hashing::{hash_secret, verify_secret};
use clinic_core::auth::policy::{self, PIN_MAX_FAILURES};
use clinic_core::auth::{Permission, Role};
use clinic_sqlite::Database;
use clinic_sqlite::repo::users::{self, NewUser, UserRecord};
use clinic_sqlite::rusqlite::Connection;
use serde::Deserialize;
use serde_json::json;

use crate::audit::{self, Actor};
use crate::error::invalid;
use crate::settings::{self, ClinicSettings};
use crate::{ServiceError, Session};

/// A staff account as entered on a form. `pin` is optional everywhere.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAccount {
    pub username: String,
    pub full_name: String,
    pub password: String,
    #[serde(default)]
    pub pin: Option<String>,
}

/// Never prints the password or PIN, even in a debug log.
impl std::fmt::Debug for NewAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewAccount")
            .field("username", &self.username)
            .field("full_name", &self.full_name)
            .field("password", &"<redacted>")
            .field("pin", &self.pin.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Validated account with hashes computed (hashing is slow, so it happens before the
/// database transaction starts).
pub(crate) struct PreparedAccount {
    pub username: String,
    pub full_name: String,
    pub role: Role,
    password_hash: String,
    pin_hash: Option<String>,
}

impl PreparedAccount {
    pub(crate) fn new(account: NewAccount, role: Role) -> Result<Self, ServiceError> {
        let username = policy::normalize_username(&account.username);
        policy::validate_username(&username)?;
        policy::validate_full_name(&account.full_name)?;
        policy::validate_password(&account.password, &username)?;
        let pin = account.pin.filter(|p| !p.is_empty());
        if let Some(pin) = &pin {
            policy::validate_pin(pin)?;
        }
        Ok(Self {
            password_hash: hash_secret(&account.password)?,
            pin_hash: pin.as_deref().map(hash_secret).transpose()?,
            username,
            full_name: account.full_name.trim().to_string(),
            role,
        })
    }

    /// Inserts the account and returns its id; `Conflict` if the username is taken.
    pub(crate) fn insert(&self, conn: &Connection, now: i64) -> Result<i64, ServiceError> {
        if users::find_by_username(conn, &self.username)?.is_some() {
            return Err(ServiceError::Conflict("That username is already taken. Choose another.".to_string()));
        }
        Ok(users::insert(
            conn,
            &NewUser {
                username: &self.username,
                full_name: &self.full_name,
                role: self.role.as_str(),
                password_hash: &self.password_hash,
                pin_hash: self.pin_hash.as_deref(),
                now,
            },
        )?)
    }

    pub(crate) fn has_pin(&self) -> bool {
        self.pin_hash.is_some()
    }
}

pub(crate) fn role_of(user: &UserRecord) -> Result<Role, ServiceError> {
    Role::parse(&user.role).ok_or_else(|| ServiceError::Corrupt(format!("unknown role '{}'", user.role)))
}

pub(crate) fn session_of(user: &UserRecord) -> Result<Session, ServiceError> {
    Ok(Session {
        user_id: user.id,
        username: user.username.clone(),
        full_name: user.full_name.clone(),
        role: role_of(user)?,
        has_pin: user.pin_hash.is_some(),
    })
}

/// Re-checks inside the transaction that the acting user still exists, is active and still has
/// the role their session started with; an administrator may have changed that meanwhile.
/// Every administrative change calls this, so a stale session cannot keep admin powers.
pub(crate) fn verify_actor(conn: &Connection, actor: &Session) -> Result<(), ServiceError> {
    match users::find_by_id(conn, actor.user_id)? {
        Some(user) if user.is_active && role_of(&user)? == actor.role => Ok(()),
        _ => Err(ServiceError::PermissionDenied),
    }
}

fn active_user(db: &Database, user_id: i64) -> Result<UserRecord, ServiceError> {
    let user = db.read(|c| users::find_by_id(c, user_id))?.ok_or(ServiceError::NotFound("user"))?;
    if !user.is_active {
        return Err(ServiceError::AccountDisabled);
    }
    Ok(user)
}

/// Hash compared against when the username does not exist, so a wrong username takes as long
/// as a wrong password (no hint about which usernames exist).
fn dummy_hash() -> Result<&'static str, ServiceError> {
    static DUMMY: OnceLock<String> = OnceLock::new();
    if let Some(hash) = DUMMY.get() {
        return Ok(hash);
    }
    let hash = hash_secret("not-a-real-account-password")?;
    Ok(DUMMY.get_or_init(|| hash))
}

// ---- First-run setup ---------------------------------------------------------------------------

pub fn needs_setup(db: &Database) -> Result<bool, ServiceError> {
    Ok(db.read(users::count)? == 0)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInput {
    pub clinic: ClinicSettings,
    pub admin: NewAccount,
    /// Optional second administrator for password recovery (DEC-025).
    #[serde(default)]
    pub second_admin: Option<NewAccount>,
}

/// Saves the clinic details and creates the first administrator (and optionally a second one).
/// Only possible while no account exists. Returns the first administrator's session.
pub fn complete_setup(db: &mut Database, input: SetupInput, now: i64) -> Result<Session, ServiceError> {
    if !needs_setup(db)? {
        return Err(ServiceError::SetupAlreadyDone);
    }
    let clinic = input.clinic.normalized()?;
    let admin = PreparedAccount::new(input.admin, Role::Admin)?;
    let second = input.second_admin.map(|a| PreparedAccount::new(a, Role::Admin)).transpose()?;
    if second.as_ref().is_some_and(|s| s.username == admin.username) {
        return Err(invalid("secondAdmin.username", "The two administrators need different usernames."));
    }
    db.write(|c| {
        if users::count(c)? > 0 {
            return Err(ServiceError::SetupAlreadyDone); // re-checked inside the transaction
        }
        settings::save(c, &clinic)?;
        let admin_id = admin.insert(c, now)?;
        let actor = || Actor::User { id: admin_id, username: &admin.username };
        audit::record(c, now, actor(), "SETUP_COMPLETE", None, None)?;
        audit::record(c, now, actor(), "USER_CREATE", Some(("user", admin_id.to_string())), Some(json!({ "username": admin.username, "role": "ADMIN" })))?;
        if let Some(second) = &second {
            let second_id = second.insert(c, now)?;
            audit::record(c, now, actor(), "USER_CREATE", Some(("user", second_id.to_string())), Some(json!({ "username": second.username, "role": "ADMIN" })))?;
        }
        Ok(Session {
            user_id: admin_id,
            username: admin.username.clone(),
            full_name: admin.full_name.clone(),
            role: Role::Admin,
            has_pin: admin.has_pin(),
        })
    })
}

// ---- Login and unlock --------------------------------------------------------------------------

/// Checks username and password. After 5 wrong passwords in a row the account is locked for
/// 5 minutes. Every attempt is written to the audit log.
pub fn login(db: &mut Database, username: &str, password: &str, now: i64) -> Result<Session, ServiceError> {
    let username = policy::normalize_username(username);
    let Some(user) = db.read(|c| users::find_by_username(c, &username))? else {
        let _ = verify_secret(password, dummy_hash()?);
        db.write(|c| audit::record(c, now, Actor::Anonymous { username_tried: &username }, "LOGIN_FAILED", None, Some(json!({ "reason": "unknown_user" }))))?;
        return Err(ServiceError::InvalidCredentials);
    };
    let actor = || Actor::User { id: user.id, username: &user.username };
    if let Some(until) = user.locked_until.filter(|until| *until > now) {
        db.write(|c| audit::record(c, now, actor(), "LOGIN_FAILED", None, Some(json!({ "reason": "locked" }))))?;
        return Err(ServiceError::AccountLocked { until });
    }
    if !verify_secret(password, &user.password_hash)? {
        let failures = user.failed_login_count + 1;
        let locked_until = policy::login_lockout(failures, now);
        db.write(|c| {
            // When the lock starts the counter restarts, giving 5 fresh tries after it ends.
            users::record_login_failure(c, user.id, if locked_until.is_some() { 0 } else { failures }, locked_until, now)?;
            audit::record(c, now, actor(), "LOGIN_FAILED", None, Some(json!({ "reason": "wrong_password", "failures": failures })))?;
            if locked_until.is_some() {
                audit::record(c, now, actor(), "ACCOUNT_LOCKED", Some(("user", user.id.to_string())), None)?;
            }
            Ok::<_, ServiceError>(())
        })?;
        return Err(match locked_until {
            Some(until) => ServiceError::AccountLocked { until },
            None => ServiceError::InvalidCredentials,
        });
    }
    if !user.is_active {
        db.write(|c| audit::record(c, now, actor(), "LOGIN_FAILED", None, Some(json!({ "reason": "disabled" }))))?;
        return Err(ServiceError::AccountDisabled);
    }
    let session = session_of(&user)?;
    db.write(|c| {
        users::record_login_success(c, user.id, now)?;
        audit::record(c, now, actor(), "LOGIN", None, None)
    })?;
    Ok(session)
}

/// An administrator's username and password typed at the desk to approve a discount. It counts
/// wrong passwords towards that administrator's lockout, refuses while the account is locked and
/// audits every failure, in its own committed write, so the approval box can never be used to
/// guess an administrator's password (review 2026-09-27, H1). Returns the administrator's id.
pub fn verify_admin_approval(db: &mut Database, username: &str, password: &str, now: i64) -> Result<i64, ServiceError> {
    let username = policy::normalize_username(username);
    let wrong = || invalid("approval", "Administrator username or password is not correct.");
    let Some(user) = db.read(|c| users::find_by_username(c, &username))? else {
        let _ = verify_secret(password, dummy_hash()?);
        db.write(|c| audit::record(c, now, Actor::Anonymous { username_tried: &username }, "APPROVAL_FAILED", None, Some(json!({ "reason": "unknown_user" }))))?;
        return Err(wrong());
    };
    let actor = || Actor::User { id: user.id, username: &user.username };
    if user.locked_until.is_some_and(|until| until > now) {
        db.write(|c| audit::record(c, now, actor(), "APPROVAL_FAILED", None, Some(json!({ "reason": "locked" }))))?;
        return Err(invalid("approval", "This administrator account is locked for a few minutes after wrong passwords."));
    }
    if !verify_secret(password, &user.password_hash)? {
        let failures = user.failed_login_count + 1;
        let locked_until = policy::login_lockout(failures, now);
        db.write(|c| {
            users::record_login_failure(c, user.id, if locked_until.is_some() { 0 } else { failures }, locked_until, now)?;
            audit::record(c, now, actor(), "APPROVAL_FAILED", None, Some(json!({ "reason": "wrong_password", "failures": failures })))?;
            if locked_until.is_some() {
                audit::record(c, now, actor(), "ACCOUNT_LOCKED", Some(("user", user.id.to_string())), None)?;
            }
            Ok::<_, ServiceError>(())
        })?;
        return Err(wrong());
    }
    if !user.is_active || role_of(&user)? != Role::Admin {
        return Err(wrong());
    }
    Ok(user.id)
}

/// The signed-in user still exists, is active and keeps the role the session started with
/// (an administrator may have changed that meanwhile). Checked on every command, reads too.
pub fn session_still_valid(db: &Database, session: &Session) -> Result<bool, ServiceError> {
    Ok(match db.read(|c| users::find_by_id(c, session.user_id))? {
        Some(user) => user.is_active && role_of(&user)? == session.role,
        None => false,
    })
}

/// Asks the signed-in user for their password again before a drastic action (restore). Wrong
/// guesses count towards the lockout like sign-in.
pub fn confirm_password(db: &mut Database, actor: &Session, password: &str, now: i64) -> Result<(), ServiceError> {
    let user = active_user(db, actor.user_id)?;
    check_current_password(db, &user, password, now)
}

/// Unlocks the idle-locked screen for the signed-in user with their PIN. After 5 wrong PINs,
/// PIN unlock is disabled until the full password is used (`unlock_with_password`).
pub fn unlock_with_pin(db: &mut Database, user_id: i64, pin: &str, now: i64) -> Result<Session, ServiceError> {
    let user = active_user(db, user_id)?;
    let actor = || Actor::User { id: user.id, username: &user.username };
    let pin_hash = user.pin_hash.as_deref().ok_or(ServiceError::PinNotSet)?;
    if user.pin_failed_count >= PIN_MAX_FAILURES {
        return Err(ServiceError::PinLocked);
    }
    if verify_secret(pin, pin_hash)? {
        let session = session_of(&user)?;
        db.write(|c| {
            users::reset_pin_failures(c, user.id, now)?;
            audit::record(c, now, actor(), "UNLOCK", None, Some(json!({ "method": "pin" })))
        })?;
        return Ok(session);
    }
    let failures = user.pin_failed_count + 1;
    db.write(|c| {
        users::record_pin_failure(c, user.id, failures, now)?;
        audit::record(c, now, actor(), "PIN_FAILED", None, Some(json!({ "failures": failures })))
    })?;
    Err(if failures >= PIN_MAX_FAILURES { ServiceError::PinLocked } else { ServiceError::WrongPin { remaining: PIN_MAX_FAILURES - failures } })
}

pub fn logout(db: &mut Database, actor: &Session, now: i64) -> Result<(), ServiceError> {
    db.write(|c| audit::record(c, now, Actor::from(actor), "LOGOUT", None, None))
}

/// Unlocks with the full password (always possible; also re-enables PIN unlock).
pub fn unlock_with_password(db: &mut Database, user_id: i64, password: &str, now: i64) -> Result<Session, ServiceError> {
    let user = active_user(db, user_id)?;
    login(db, &user.username, password, now)
}

// ---- The signed-in user's own credentials ------------------------------------------------------

/// Wrong guesses count towards the same lockout as sign-in, so a signed-in screen cannot be
/// used to try passwords without limit.
fn check_current_password(db: &mut Database, user: &UserRecord, current: &str, now: i64) -> Result<(), ServiceError> {
    if let Some(until) = user.locked_until.filter(|until| *until > now) {
        return Err(ServiceError::AccountLocked { until });
    }
    if verify_secret(current, &user.password_hash)? {
        return Ok(());
    }
    let failures = user.failed_login_count + 1;
    let locked_until = policy::login_lockout(failures, now);
    db.write(|c| {
        users::record_login_failure(c, user.id, if locked_until.is_some() { 0 } else { failures }, locked_until, now)?;
        audit::record(c, now, Actor::User { id: user.id, username: &user.username }, "PASSWORD_CHECK_FAILED", None, Some(json!({ "failures": failures })))?;
        if locked_until.is_some() {
            audit::record(c, now, Actor::User { id: user.id, username: &user.username }, "ACCOUNT_LOCKED", Some(("user", user.id.to_string())), None)?;
        }
        Ok::<_, ServiceError>(())
    })?;
    Err(match locked_until {
        Some(until) => ServiceError::AccountLocked { until },
        None => invalid("currentPassword", "Your current password is not correct."),
    })
}

pub fn change_own_password(db: &mut Database, actor: &Session, current: &str, new_password: &str, now: i64) -> Result<(), ServiceError> {
    actor.require(Permission::ManageOwnSecurity)?;
    let user = active_user(db, actor.user_id)?;
    check_current_password(db, &user, current, now)?;
    policy::validate_password(new_password, &user.username)?;
    if current == new_password {
        return Err(invalid("password", "The new password must be different from the current one."));
    }
    let hash = hash_secret(new_password)?;
    db.write(|c| {
        users::set_password(c, user.id, &hash, now)?;
        audit::record(c, now, Actor::from(actor), "PASSWORD_CHANGE", Some(("user", user.id.to_string())), None)
    })
}

/// Sets, changes or (with `None`) removes the signed-in user's PIN. Needs the current password.
pub fn set_own_pin(db: &mut Database, actor: &Session, current_password: &str, pin: Option<&str>, now: i64) -> Result<Session, ServiceError> {
    actor.require(Permission::ManageOwnSecurity)?;
    let user = active_user(db, actor.user_id)?;
    check_current_password(db, &user, current_password, now)?;
    let pin = pin.filter(|p| !p.is_empty());
    if let Some(pin) = pin {
        policy::validate_pin(pin)?;
    }
    let hash = pin.map(hash_secret).transpose()?;
    db.write(|c| {
        users::set_pin(c, user.id, hash.as_deref(), now)?;
        let action = if hash.is_some() { "PIN_SET" } else { "PIN_REMOVED" };
        audit::record(c, now, Actor::from(actor), action, Some(("user", user.id.to_string())), None)
    })?;
    Ok(Session { has_pin: hash.is_some(), ..actor.clone() })
}
