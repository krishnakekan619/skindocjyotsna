use clinic_pdf::{ReceiptData, render_receipt_pdf, sample_receipt};
use tauri::State;

use crate::error::CommandError;
use crate::state::AppState;

/// Phase 0: the demo receipt shown in the preview (real bills arrive in Phase 4).
#[tauri::command]
pub fn get_sample_receipt() -> ReceiptData {
    sample_receipt()
}

/// Writes the demo receipt as a PDF into the app's exports folder and returns its path.
#[tauri::command]
pub fn export_sample_receipt_pdf(state: State<'_, AppState>) -> Result<String, CommandError> {
    let data = sample_receipt();
    let bytes = render_receipt_pdf(&data).map_err(|error| {
        eprintln!("receipt PDF failed: {error}");
        CommandError::internal()
    })?;
    let file_name = format!("Receipt-{}.pdf", data.bill_no.replace(['/', '\\', ':'], "-"));
    let path = state.paths.export_dir.join(file_name);
    std::fs::write(&path, bytes).map_err(|error| {
        eprintln!("could not write {}: {error}", path.display());
        CommandError::user("EXPORT_FAILED", "The PDF could not be saved. Check that the disk is not full.")
    })?;
    Ok(path.display().to_string())
}
