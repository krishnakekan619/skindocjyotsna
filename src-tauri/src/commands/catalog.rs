//! Consultation types and procedures that can be charged on bills.

use clinic_core::auth::Permission;
use clinic_services::catalog::{self, ServiceInput};
use clinic_sqlite::repo::services::ServiceRow;
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

#[tauri::command(async)]
pub fn list_services(state: State<'_, AppState>, include_inactive: bool) -> Result<Vec<ServiceRow>, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(catalog::list(&*state.db()?, &session, include_inactive)?)
}

#[tauri::command(async)]
pub fn save_service(state: State<'_, AppState>, input: ServiceInput) -> Result<ServiceRow, CommandError> {
    let session = state.session(Permission::ManageClinicSettings)?;
    Ok(catalog::save(&mut *state.db()?, &session, input, now())?)
}
