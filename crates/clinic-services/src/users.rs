//! Staff accounts, managed by administrators.

use clinic_core::auth::hashing::hash_secret;
use clinic_core::auth::policy;
use clinic_core::auth::{Permission, Role};
use clinic_sqlite::Database;
use clinic_sqlite::repo::users::{self, UserRecord};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::{self, Actor};
use crate::auth::{NewAccount, PreparedAccount, role_of, verify_actor};
use crate::error::invalid;
use crate::{ServiceError, Session};

/// What the Users screen shows. Never includes hashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSummary {
    pub id: i64,
    pub username: String,
    pub full_name: String,
    pub role: Role,
    pub is_active: bool,
    pub has_pin: bool,
    /// Temporarily locked after too many wrong passwords.
    pub is_locked: bool,
    pub last_login_at: Option<i64>,
    pub created_at: i64,
}

fn summary(user: &UserRecord, now: i64) -> Result<UserSummary, ServiceError> {
    Ok(UserSummary {
        id: user.id,
        username: user.username.clone(),
        full_name: user.full_name.clone(),
        role: role_of(user)?,
        is_active: user.is_active,
        has_pin: user.pin_hash.is_some(),
        is_locked: user.locked_until.is_some_and(|until| until > now),
        last_login_at: user.last_login_at,
        created_at: user.created_at,
    })
}

pub fn list(db: &Database, actor: &Session, now: i64) -> Result<Vec<UserSummary>, ServiceError> {
    actor.require(Permission::ManageUsers)?;
    db.read(users::list)?.iter().map(|u| summary(u, now)).collect()
}

/// True when at least two administrators are active, so one can reset the other's password
/// (DEC-025). The UI warns admins while this is false.
pub fn has_backup_admin(db: &Database) -> Result<bool, ServiceError> {
    Ok(db.read(users::count_active_admins)? >= 2)
}

pub fn create(db: &mut Database, actor: &Session, account: NewAccount, role: Role, now: i64) -> Result<UserSummary, ServiceError> {
    actor.require(Permission::ManageUsers)?;
    let prepared = PreparedAccount::new(account, role)?;
    db.write(|c| {
        verify_actor(c, actor)?;
        let id = prepared.insert(c, now)?;
        audit::record(c, now, Actor::from(actor), "USER_CREATE", Some(("user", id.to_string())), Some(json!({ "username": prepared.username, "role": role.as_str() })))?;
        let user = users::find_by_id(c, id)?.ok_or(ServiceError::NotFound("user"))?;
        summary(&user, now)
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserUpdate {
    pub full_name: String,
    pub role: Role,
    pub is_active: bool,
}

/// Changes name, role or active state. Refuses to leave the clinic without an active
/// administrator, and refuses to let admins deactivate or demote themselves.
pub fn update(db: &mut Database, actor: &Session, user_id: i64, change: UserUpdate, now: i64) -> Result<UserSummary, ServiceError> {
    actor.require(Permission::ManageUsers)?;
    policy::validate_full_name(&change.full_name)?;
    let full_name = change.full_name.trim().to_string();
    db.write(|c| {
        verify_actor(c, actor)?;
        let target = users::find_by_id(c, user_id)?.ok_or(ServiceError::NotFound("user"))?;
        let was_active_admin = target.is_active && role_of(&target)? == Role::Admin;
        let stays_active_admin = change.is_active && change.role == Role::Admin;
        if target.id == actor.user_id && !stays_active_admin {
            return Err(invalid("isActive", "You can't deactivate or demote your own account. Ask the other administrator."));
        }
        if was_active_admin && !stays_active_admin && users::count_active_admins(c)? <= 1 {
            return Err(ServiceError::LastAdmin);
        }
        users::update_profile(c, target.id, &full_name, change.role.as_str(), change.is_active, now)?;
        audit::record(
            c,
            now,
            Actor::from(actor),
            "USER_UPDATE",
            Some(("user", target.id.to_string())),
            Some(json!({
                "username": target.username,
                "role": { "from": target.role, "to": change.role.as_str() },
                "active": { "from": target.is_active, "to": change.is_active },
            })),
        )?;
        let updated = users::find_by_id(c, target.id)?.ok_or(ServiceError::NotFound("user"))?;
        summary(&updated, now)
    })
}

/// An administrator sets a new password for someone who forgot theirs (DEC-025). Also clears
/// any lockout; the user's PIN is kept.
pub fn reset_password(db: &mut Database, actor: &Session, user_id: i64, new_password: &str, now: i64) -> Result<(), ServiceError> {
    actor.require(Permission::ManageUsers)?;
    let target = db.read(|c| users::find_by_id(c, user_id))?.ok_or(ServiceError::NotFound("user"))?;
    policy::validate_password(new_password, &target.username)?;
    let hash = hash_secret(new_password)?;
    db.write(|c| {
        verify_actor(c, actor)?;
        users::set_password(c, target.id, &hash, now)?;
        audit::record(c, now, Actor::from(actor), "PASSWORD_RESET", Some(("user", target.id.to_string())), Some(json!({ "username": target.username })))
    })
}
