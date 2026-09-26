//! First-run setup, sign-in, idle lock and the signed-in user's own password/PIN.

use clinic_core::auth::Permission;
use clinic_services::auth::{self, SetupInput};
use clinic_services::{Session, settings};
use serde::Serialize;
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

/// Everything the UI needs to decide which screen to show.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    needs_setup: bool,
    clinic_name: String,
    session: Option<Session>,
    locked: bool,
    idle_lock_minutes: u32,
    app_version: String,
}

fn status(state: &AppState) -> Result<AppStatus, CommandError> {
    let (needs_setup, clinic) = {
        let db = state.db()?;
        (auth::needs_setup(&db)?, settings::get_clinic(&db)?)
    };
    let mut guard = state.auth()?;
    if guard.session.is_some() && !guard.locked && now() - guard.last_activity > guard.idle_lock_secs {
        guard.locked = true;
    }
    Ok(AppStatus {
        needs_setup,
        clinic_name: clinic.name,
        session: guard.session.clone(),
        locked: guard.locked,
        idle_lock_minutes: clinic.idle_lock_minutes,
        app_version: state.app_version.clone(),
    })
}

fn idle_minutes(state: &AppState) -> Result<u32, CommandError> {
    let db = state.db()?;
    Ok(settings::get_clinic(&db)?.idle_lock_minutes)
}

#[tauri::command(async)]
pub fn get_app_status(state: State<'_, AppState>) -> Result<AppStatus, CommandError> {
    status(&state)
}

#[tauri::command(async)]
pub fn complete_setup(state: State<'_, AppState>, input: SetupInput) -> Result<AppStatus, CommandError> {
    let session = {
        let mut db = state.db()?;
        auth::complete_setup(&mut db, input, now())?
    };
    tracing::info!(user = %session.username, "first-run setup completed");
    state.sign_in(session, idle_minutes(&state)?)?;
    status(&state)
}

#[tauri::command(async)]
pub fn login(state: State<'_, AppState>, username: String, password: String) -> Result<AppStatus, CommandError> {
    let session = {
        let mut db = state.db()?;
        auth::login(&mut db, &username, &password, now())?
    };
    tracing::info!(user = %session.username, "signed in");
    state.sign_in(session, idle_minutes(&state)?)?;
    status(&state)
}

#[tauri::command(async)]
pub fn logout(state: State<'_, AppState>) -> Result<AppStatus, CommandError> {
    if let Some(session) = state.sign_out()? {
        let mut db = state.db()?;
        auth::logout(&mut db, &session, now())?;
        tracing::info!(user = %session.username, "signed out");
    }
    status(&state)
}

/// Locks the screen now (the UI calls this when it detects inactivity or on "Lock").
#[tauri::command(async)]
pub fn lock_screen(state: State<'_, AppState>) -> Result<AppStatus, CommandError> {
    {
        let mut guard = state.auth()?;
        if guard.session.is_some() {
            guard.locked = true;
        }
    }
    status(&state)
}

fn locked_user_id(state: &AppState) -> Result<i64, CommandError> {
    state.auth()?.session.as_ref().map(|s| s.user_id).ok_or_else(|| CommandError::user("NOT_SIGNED_IN", "Please sign in."))
}

#[tauri::command(async)]
pub fn unlock_with_pin(state: State<'_, AppState>, pin: String) -> Result<AppStatus, CommandError> {
    let user_id = locked_user_id(&state)?;
    let session = {
        let mut db = state.db()?;
        auth::unlock_with_pin(&mut db, user_id, &pin, now())?
    };
    state.sign_in(session, idle_minutes(&state)?)?;
    status(&state)
}

#[tauri::command(async)]
pub fn unlock_with_password(state: State<'_, AppState>, password: String) -> Result<AppStatus, CommandError> {
    let user_id = locked_user_id(&state)?;
    let session = {
        let mut db = state.db()?;
        auth::unlock_with_password(&mut db, user_id, &password, now())?
    };
    state.sign_in(session, idle_minutes(&state)?)?;
    status(&state)
}

/// Keeps the session alive while the user is working on one screen without saving anything.
#[tauri::command(async)]
pub fn heartbeat(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.session(Permission::UseApp).map(|_| ())
}

#[tauri::command(async)]
pub fn change_own_password(state: State<'_, AppState>, current_password: String, new_password: String) -> Result<(), CommandError> {
    let session = state.session(Permission::ManageOwnSecurity)?;
    let mut db = state.db()?;
    Ok(auth::change_own_password(&mut db, &session, &current_password, &new_password, now())?)
}

#[tauri::command(async)]
pub fn set_own_pin(state: State<'_, AppState>, current_password: String, pin: Option<String>) -> Result<AppStatus, CommandError> {
    let session = state.session(Permission::ManageOwnSecurity)?;
    let updated = {
        let mut db = state.db()?;
        auth::set_own_pin(&mut db, &session, &current_password, pin.as_deref(), now())?
    };
    {
        // Only if the same user is still signed in (commands can now run concurrently).
        let mut auth = state.auth()?;
        if auth.session.as_ref().is_some_and(|current| current.user_id == updated.user_id) {
            auth.session = Some(updated);
        }
    }
    status(&state)
}
