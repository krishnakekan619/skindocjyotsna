//! Users, clinic settings and the audit log.

use clinic_core::auth::{Permission, Role};
use clinic_services::audit::{self, AuditEntry};
use clinic_services::auth::NewAccount;
use clinic_services::settings::{self, ClinicSettings};
use clinic_services::users::{self, UserSummary, UserUpdate};
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

#[tauri::command]
pub fn list_users(state: State<'_, AppState>) -> Result<Vec<UserSummary>, CommandError> {
    let session = state.session(Permission::ManageUsers)?;
    Ok(users::list(&*state.db()?, &session, now())?)
}

#[tauri::command]
pub fn create_user(state: State<'_, AppState>, account: NewAccount, role: Role) -> Result<UserSummary, CommandError> {
    let session = state.session(Permission::ManageUsers)?;
    Ok(users::create(&mut *state.db()?, &session, account, role, now())?)
}

#[tauri::command]
pub fn update_user(state: State<'_, AppState>, user_id: i64, change: UserUpdate) -> Result<UserSummary, CommandError> {
    let session = state.session(Permission::ManageUsers)?;
    Ok(users::update(&mut *state.db()?, &session, user_id, change, now())?)
}

#[tauri::command]
pub fn reset_user_password(state: State<'_, AppState>, user_id: i64, new_password: String) -> Result<(), CommandError> {
    let session = state.session(Permission::ManageUsers)?;
    Ok(users::reset_password(&mut *state.db()?, &session, user_id, &new_password, now())?)
}

#[tauri::command]
pub fn get_clinic_settings(state: State<'_, AppState>) -> Result<ClinicSettings, CommandError> {
    state.session(Permission::UseApp)?;
    Ok(settings::get_clinic(&*state.db()?)?)
}

#[tauri::command]
pub fn update_clinic_settings(state: State<'_, AppState>, settings: ClinicSettings) -> Result<ClinicSettings, CommandError> {
    let session = state.session(Permission::ManageClinicSettings)?;
    let saved = settings::update_clinic(&mut *state.db()?, &session, settings, now())?;
    state.auth()?.idle_lock_secs = i64::from(saved.idle_lock_minutes) * 60;
    Ok(saved)
}

#[tauri::command]
pub fn list_audit(state: State<'_, AppState>, limit: u32, before_id: Option<i64>) -> Result<Vec<AuditEntry>, CommandError> {
    let session = state.session(Permission::ViewAuditLog)?;
    Ok(audit::list(&*state.db()?, &session, limit, before_id)?)
}
