//! Bills, receipts (PDF / print), returns, cancellation, correction.

use clinic_core::auth::Permission;
use clinic_core::pricing::Discount;
use clinic_pdf::{ReceiptData, render_receipt_pdf};
use clinic_services::billing::{self, BillDetail, BillFilter, BillInput, BillLineInput, CorrectionInput, Quote, ReturnInput, ReturnResult};
use clinic_sqlite::repo::billing::BillRow;
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

#[tauri::command(async)]
pub fn quote_bill(state: State<'_, AppState>, lines: Vec<BillLineInput>, discount: Discount, correcting_bill_id: Option<i64>) -> Result<Quote, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(billing::quote(&*state.db()?, &session, &lines, discount, correcting_bill_id, now())?)
}

#[tauri::command(async)]
pub fn finalize_bill(state: State<'_, AppState>, input: BillInput) -> Result<BillDetail, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    let detail = billing::finalize(&mut *state.db()?, &session, input, now())?;
    tracing::info!(bill = %detail.bill.bill_no, "bill finalized");
    Ok(detail)
}

#[tauri::command(async)]
pub fn list_bills(state: State<'_, AppState>, filter: BillFilter) -> Result<Vec<BillRow>, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(billing::list(&*state.db()?, &session, filter)?)
}

#[tauri::command(async)]
pub fn get_bill(state: State<'_, AppState>, bill_id: i64) -> Result<BillDetail, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(billing::get(&*state.db()?, &session, bill_id)?)
}

#[tauri::command(async)]
pub fn cancel_bill(state: State<'_, AppState>, bill_id: i64, reason: String) -> Result<BillDetail, CommandError> {
    let session = state.session(Permission::CancelBills)?;
    Ok(billing::cancel(&mut *state.db()?, &session, bill_id, &reason, now())?)
}

#[tauri::command(async)]
pub fn return_bill_items(state: State<'_, AppState>, input: ReturnInput) -> Result<ReturnResult, CommandError> {
    let session = state.session(Permission::ProcessReturns)?;
    Ok(billing::return_items(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command(async)]
pub fn correct_bill(state: State<'_, AppState>, input: CorrectionInput) -> Result<BillDetail, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(billing::correct(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command(async)]
pub fn get_receipt(state: State<'_, AppState>, bill_id: i64) -> Result<ReceiptData, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(billing::receipt(&*state.db()?, &session, bill_id)?)
}

/// Saves the receipt as an A5 PDF in the app's exports folder and returns the file name.
#[tauri::command(async)]
pub fn export_receipt_pdf(state: State<'_, AppState>, bill_id: i64) -> Result<String, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    let data = billing::receipt(&*state.db()?, &session, bill_id)?;
    let bytes = render_receipt_pdf(&data).map_err(|error| {
        tracing::error!(%error, "receipt PDF failed");
        CommandError::internal()
    })?;
    let file_name = format!("Receipt-{}.pdf", data.bill_no.replace(['/', '\\', ':'], "-"));
    let path = state.paths.export_dir.join(&file_name);
    std::fs::write(&path, bytes).map_err(|error| {
        tracing::error!(%error, path = %path.display(), "could not write receipt PDF");
        CommandError::user("EXPORT_FAILED", "The PDF could not be saved. Check that the disk is not full.")
    })?;
    Ok(file_name)
}

/// Opens a file from the exports folder (e.g. a receipt PDF) with the default app. Only plain
/// file names inside that folder are accepted.
#[tauri::command(async)]
pub fn open_export(state: State<'_, AppState>, file_name: String) -> Result<(), CommandError> {
    state.session(Permission::CreateBills)?;
    let plain = !file_name.is_empty() && !file_name.contains(['/', '\\', ':']) && !file_name.starts_with('.') && file_name.ends_with(".pdf");
    let path = state.paths.export_dir.join(&file_name);
    if !plain || !path.is_file() {
        return Err(CommandError::user("NOT_FOUND", "That file could not be found."));
    }
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|error| {
        tracing::error!(%error, "could not open file");
        CommandError::user("OPEN_FAILED", "The file could not be opened. Is a PDF viewer installed?")
    })
}
