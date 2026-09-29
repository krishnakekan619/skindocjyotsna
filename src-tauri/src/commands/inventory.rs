//! Products, categories, suppliers, stock.

use clinic_core::auth::Permission;
use clinic_services::inventory::{
    self, AddInventoryInput, AdjustInput, CategoryInput, DeleteOutcome, ExpiringBatch, ProductDetail, ProductFilter, ProductInput, SaleProduct, StockInInput,
    SupplierInput,
};
use clinic_services::stock_import::{self, ImportMode, ImportPreview, ImportResult};
use clinic_sqlite::repo::inventory::{BatchRow, Category, LedgerRow, ProductRow, Supplier};
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

#[tauri::command(async)]
pub fn list_categories(state: State<'_, AppState>) -> Result<Vec<Category>, CommandError> {
    let session = state.session(Permission::ViewInventory)?;
    Ok(inventory::list_categories(&*state.db()?, &session)?)
}

#[tauri::command(async)]
pub fn save_category(state: State<'_, AppState>, input: CategoryInput) -> Result<Vec<Category>, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::save_category(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command(async)]
pub fn list_suppliers(state: State<'_, AppState>) -> Result<Vec<Supplier>, CommandError> {
    let session = state.session(Permission::ViewInventory)?;
    Ok(inventory::list_suppliers(&*state.db()?, &session)?)
}

#[tauri::command(async)]
pub fn save_supplier(state: State<'_, AppState>, input: SupplierInput) -> Result<Vec<Supplier>, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::save_supplier(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command(async)]
pub fn list_products(state: State<'_, AppState>, filter: ProductFilter) -> Result<Vec<ProductRow>, CommandError> {
    let session = state.session(Permission::ViewInventory)?;
    Ok(inventory::list_products(&*state.db()?, &session, filter, now())?)
}

#[tauri::command(async)]
pub fn get_product(state: State<'_, AppState>, product_id: i64) -> Result<ProductDetail, CommandError> {
    let session = state.session(Permission::ViewInventory)?;
    Ok(inventory::get_product(&*state.db()?, &session, product_id, now())?)
}

#[tauri::command(async)]
pub fn save_product(state: State<'_, AppState>, input: ProductInput) -> Result<ProductRow, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::save_product(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command(async)]
pub fn stock_in(state: State<'_, AppState>, input: StockInInput) -> Result<BatchRow, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::stock_in(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command(async)]
pub fn adjust_stock(state: State<'_, AppState>, input: AdjustInput) -> Result<BatchRow, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::adjust_stock(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command(async)]
pub fn list_stock_ledger(state: State<'_, AppState>, product_id: Option<i64>, limit: u32) -> Result<Vec<LedgerRow>, CommandError> {
    let session = state.session(Permission::ViewStockLedger)?;
    Ok(inventory::ledger(&*state.db()?, &session, product_id, limit)?)
}

#[tauri::command(async)]
pub fn list_expiring(state: State<'_, AppState>, within_days: i64) -> Result<Vec<ExpiringBatch>, CommandError> {
    let session = state.session(Permission::ViewInventory)?;
    Ok(inventory::expiring(&*state.db()?, &session, within_days, now())?)
}

#[tauri::command(async)]
pub fn search_products_for_sale(state: State<'_, AppState>, text: String) -> Result<Vec<SaleProduct>, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(inventory::search_for_sale(&*state.db()?, &session, &text, now())?)
}

#[tauri::command(async)]
pub fn recent_products_for_sale(state: State<'_, AppState>) -> Result<Vec<SaleProduct>, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(inventory::recent_for_sale(&*state.db()?, &session, now())?)
}

/// Inventory → Add Inventory (receptionists too): the simple one-screen stock form.
#[tauri::command(async)]
pub fn add_inventory(state: State<'_, AppState>, input: AddInventoryInput) -> Result<BatchRow, CommandError> {
    let session = state.session(Permission::AddStock)?;
    Ok(inventory::add_inventory(&mut *state.db()?, &session, input, now())?)
}

/// Inventory → Import from CSV (administrators): checks every row of the file, changes nothing.
#[tauri::command(async)]
pub fn preview_stock_import(state: State<'_, AppState>, text: String, mode: ImportMode) -> Result<ImportPreview, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(stock_import::preview(&*state.db()?, &session, &text, mode, now())?)
}

/// Imports every row of a checked file in one transaction: all rows or none.
#[tauri::command(async)]
pub fn import_stock(state: State<'_, AppState>, text: String, mode: ImportMode, request_key: String, import_again: bool) -> Result<ImportResult, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(stock_import::import(&mut *state.db()?, &session, &text, mode, &request_key, import_again, now())?)
}

/// Inventory → Export to CSV (administrators): the stock in hand, one row per lot, saved in the
/// exports folder and opened (normally in Excel). Returns where it was saved.
#[tauri::command(async)]
pub fn export_stock_csv(state: State<'_, AppState>) -> Result<String, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    let export = stock_import::export(&*state.db()?, &session, now())?;
    let path = state.paths.export_dir.join(&export.file_name);
    std::fs::write(&path, export.csv).map_err(|error| {
        tracing::error!(%error, path = %path.display(), "could not write the stock export");
        CommandError::user("EXPORT_FAILED", "The file could not be saved. Check that the disk is not full, and close it in Excel if it is open.")
    })?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|error| {
        tracing::error!(%error, "could not open the stock export");
        CommandError::user("OPEN_FAILED", "The file was saved in the exports folder but could not be opened. Is Excel installed?")
    })?;
    Ok(path.display().to_string())
}

/// Saves the empty import sheet (just the column names) in the exports folder and opens it,
/// normally in Excel. Returns where it was saved.
#[tauri::command(async)]
pub fn open_stock_import_template(state: State<'_, AppState>) -> Result<String, CommandError> {
    state.session(Permission::ManageInventory)?;
    let path = state.paths.export_dir.join(stock_import::TEMPLATE_FILE_NAME);
    std::fs::write(&path, stock_import::TEMPLATE_CSV).map_err(|error| {
        tracing::error!(%error, path = %path.display(), "could not write the import template");
        CommandError::user("EXPORT_FAILED", "The sheet could not be saved. Check that the disk is not full.")
    })?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|error| {
        tracing::error!(%error, "could not open the import template");
        CommandError::user("OPEN_FAILED", "The sheet was saved in the exports folder but could not be opened. Is Excel installed?")
    })?;
    Ok(path.display().to_string())
}

/// Inventory → Delete (administrators): archives a product with history, removes an unused one.
#[tauri::command(async)]
pub fn delete_product(state: State<'_, AppState>, product_id: i64) -> Result<DeleteOutcome, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::delete_product(&mut *state.db()?, &session, product_id, now())?)
}
