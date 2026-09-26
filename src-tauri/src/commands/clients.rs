//! Clients and their history.

use clinic_core::auth::Permission;
use clinic_services::clients::{self, ClientInput, ClientProfile};
use clinic_sqlite::repo::clients::ClientRow;
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

#[tauri::command]
pub fn search_clients(state: State<'_, AppState>, text: String, include_inactive: bool) -> Result<Vec<ClientRow>, CommandError> {
    let session = state.session(Permission::ManageClients)?;
    Ok(clients::search(&state.db()?, &session, &text, include_inactive)?)
}

#[tauri::command]
pub fn save_client(state: State<'_, AppState>, input: ClientInput) -> Result<ClientRow, CommandError> {
    let session = state.session(Permission::ManageClients)?;
    Ok(clients::save(&mut state.db()?, &session, input, now())?)
}

#[tauri::command]
pub fn get_client_profile(state: State<'_, AppState>, client_id: i64, from_date: Option<String>, to_date: Option<String>) -> Result<ClientProfile, CommandError> {
    let session = state.session(Permission::ManageClients)?;
    Ok(clients::profile(&state.db()?, &session, client_id, from_date.as_deref(), to_date.as_deref())?)
}
