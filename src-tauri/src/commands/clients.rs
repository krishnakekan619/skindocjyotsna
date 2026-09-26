//! Clients and their history.

use clinic_core::auth::Permission;
use clinic_services::clients::{self, ClientInput, ClientProfile, DuplicateGroup, DuplicateMatch, DuplicateQuery, MergeResult};
use clinic_sqlite::repo::clients::ClientRow;
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

#[tauri::command(async)]
pub fn search_clients(state: State<'_, AppState>, text: String, include_inactive: bool) -> Result<Vec<ClientRow>, CommandError> {
    let session = state.session(Permission::ManageClients)?;
    Ok(clients::search(&*state.db()?, &session, &text, include_inactive)?)
}

#[tauri::command(async)]
pub fn save_client(state: State<'_, AppState>, input: ClientInput) -> Result<ClientRow, CommandError> {
    let session = state.session(Permission::ManageClients)?;
    Ok(clients::save(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command(async)]
pub fn get_client_profile(state: State<'_, AppState>, client_id: i64, from_date: Option<String>, to_date: Option<String>) -> Result<ClientProfile, CommandError> {
    let session = state.session(Permission::ManageClients)?;
    Ok(clients::profile(&*state.db()?, &session, client_id, from_date.as_deref(), to_date.as_deref())?)
}

/// Possible existing clients for the details typed in the "new client" form.
#[tauri::command(async)]
pub fn check_client_duplicates(state: State<'_, AppState>, query: DuplicateQuery) -> Result<Vec<DuplicateMatch>, CommandError> {
    let session = state.session(Permission::ManageClients)?;
    Ok(clients::possible_duplicates(&*state.db()?, &session, &query)?)
}

#[tauri::command(async)]
pub fn find_duplicate_clients(state: State<'_, AppState>) -> Result<Vec<DuplicateGroup>, CommandError> {
    let session = state.session(Permission::ManageClients)?;
    Ok(clients::duplicate_groups(&*state.db()?, &session)?)
}

/// Administrators: merges `secondary_id` into `primary_id` (bills move; nothing is deleted).
#[tauri::command(async)]
pub fn merge_clients(state: State<'_, AppState>, primary_id: i64, secondary_id: i64) -> Result<MergeResult, CommandError> {
    let session = state.session(Permission::MergeClients)?;
    Ok(clients::merge(&mut *state.db()?, &session, primary_id, secondary_id, now())?)
}
