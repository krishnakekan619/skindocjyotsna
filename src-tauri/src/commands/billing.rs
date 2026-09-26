//! Bills, receipts (PDF / print / WhatsApp), returns, cancellation, correction.

use std::path::{Path, PathBuf};

use clinic_core::auth::Permission;
use clinic_core::pricing::Discount;
use clinic_pdf::{ReceiptData, render_receipt_pdf};
use clinic_services::billing::{self, BillDetail, BillFilter, BillInput, BillLineInput, CorrectionInput, Quote, ReturnInput, ReturnResult, ServiceLineInput};
use clinic_services::{Session, share};
use clinic_sqlite::repo::billing::BillRow;
use serde::Serialize;
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

#[tauri::command(async)]
pub fn quote_bill(
    state: State<'_, AppState>,
    lines: Vec<BillLineInput>,
    services: Vec<ServiceLineInput>,
    discount: Discount,
    correcting_bill_id: Option<i64>,
) -> Result<Quote, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(billing::quote(&*state.db()?, &session, &lines, &services, discount, correcting_bill_id, now())?)
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

/// Renders the receipt as an A5 PDF in the exports folder; returns (file name, full path).
fn write_receipt_pdf(state: &AppState, session: &Session, bill_id: i64) -> Result<(String, PathBuf), CommandError> {
    let data = billing::receipt(&*state.db()?, session, bill_id)?;
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
    Ok((file_name, path))
}

/// Saves the receipt as an A5 PDF in the app's exports folder and returns the file name.
#[tauri::command(async)]
pub fn export_receipt_pdf(state: State<'_, AppState>, bill_id: i64) -> Result<String, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(write_receipt_pdf(&state, &session, bill_id)?.0)
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppHandoff {
    pub bill_no: String,
    pub client_name: String,
    pub file_name: String,
    /// The PDF is on the clipboard: paste it into the chat (Ctrl+V / Cmd+V).
    pub pdf_copied: bool,
}

/// Puts the file itself (not its text) on the clipboard, so pasting in WhatsApp attaches it.
/// Uses the operating system's own tools; returns false where that is not possible.
#[cfg(target_os = "macos")]
fn copy_file_to_clipboard(path: &Path) -> bool {
    let escaped = path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    let status = std::process::Command::new("osascript").arg("-e").arg(format!("set the clipboard to (POSIX file \"{escaped}\")")).status();
    clipboard_result(status)
}

#[cfg(windows)]
fn copy_file_to_clipboard(path: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let escaped = path.to_string_lossy().replace('\'', "''");
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &format!("Set-Clipboard -LiteralPath '{escaped}'")])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    clipboard_result(status)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn copy_file_to_clipboard(_path: &Path) -> bool {
    false
}

#[cfg(any(target_os = "macos", windows))]
fn clipboard_result(status: std::io::Result<std::process::ExitStatus>) -> bool {
    match status {
        Ok(status) if status.success() => true,
        Ok(status) => {
            tracing::warn!(%status, "could not copy the receipt to the clipboard");
            false
        }
        Err(error) => {
            tracing::warn!(%error, "could not copy the receipt to the clipboard");
            false
        }
    }
}

/// Prepares a receipt for WhatsApp (DEC-031): saves the PDF, copies it to the clipboard and
/// opens the clinic's WhatsApp on the client's chat with the clinic's message typed in. The
/// receptionist pastes the PDF and presses Send; nothing is sent automatically.
#[tauri::command(async)]
pub fn open_whatsapp(state: State<'_, AppState>, bill_id: i64) -> Result<WhatsAppHandoff, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    let message = share::whatsapp_message(&*state.db()?, &session, bill_id)?;
    let (file_name, path) = write_receipt_pdf(&state, &session, bill_id)?;
    let pdf_copied = copy_file_to_clipboard(&path);
    if !pdf_copied {
        // Show the PDF's folder instead, so it can be dragged into the chat.
        let _ = tauri_plugin_opener::open_path(&state.paths.export_dir, None::<&str>);
    }
    if tauri_plugin_opener::open_url(message.app_url(), None::<&str>).is_err() {
        tauri_plugin_opener::open_url(message.web_url(), None::<&str>).map_err(|error| {
            tracing::error!(%error, "could not open WhatsApp");
            CommandError::user("OPEN_FAILED", "WhatsApp could not be opened. Is WhatsApp installed on this computer?")
        })?;
    }
    tracing::info!(bill = %message.bill_no, pdf_copied, "receipt handed to WhatsApp");
    Ok(WhatsAppHandoff { bill_no: message.bill_no, client_name: message.client_name, file_name, pdf_copied })
}
