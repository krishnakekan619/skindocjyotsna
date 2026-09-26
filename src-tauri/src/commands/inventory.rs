//! Products, categories, suppliers, stock.

use clinic_core::auth::Permission;
use clinic_services::inventory::{
    self, AdjustInput, CategoryInput, ExpiringBatch, ProductDetail, ProductFilter, ProductInput, SaleProduct, StockInInput, SupplierInput,
};
use clinic_sqlite::repo::inventory::{BatchRow, Category, LedgerRow, ProductRow, Supplier};
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

#[tauri::command]
pub fn list_categories(state: State<'_, AppState>) -> Result<Vec<Category>, CommandError> {
    let session = state.session(Permission::ViewInventory)?;
    Ok(inventory::list_categories(&*state.db()?, &session)?)
}

#[tauri::command]
pub fn save_category(state: State<'_, AppState>, input: CategoryInput) -> Result<Vec<Category>, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::save_category(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command]
pub fn list_suppliers(state: State<'_, AppState>) -> Result<Vec<Supplier>, CommandError> {
    let session = state.session(Permission::ViewInventory)?;
    Ok(inventory::list_suppliers(&*state.db()?, &session)?)
}

#[tauri::command]
pub fn save_supplier(state: State<'_, AppState>, input: SupplierInput) -> Result<Vec<Supplier>, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::save_supplier(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command]
pub fn list_products(state: State<'_, AppState>, filter: ProductFilter) -> Result<Vec<ProductRow>, CommandError> {
    let session = state.session(Permission::ViewInventory)?;
    Ok(inventory::list_products(&*state.db()?, &session, filter, now())?)
}

#[tauri::command]
pub fn get_product(state: State<'_, AppState>, product_id: i64) -> Result<ProductDetail, CommandError> {
    let session = state.session(Permission::ViewInventory)?;
    Ok(inventory::get_product(&*state.db()?, &session, product_id, now())?)
}

#[tauri::command]
pub fn save_product(state: State<'_, AppState>, input: ProductInput) -> Result<ProductRow, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::save_product(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command]
pub fn stock_in(state: State<'_, AppState>, input: StockInInput) -> Result<BatchRow, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::stock_in(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command]
pub fn adjust_stock(state: State<'_, AppState>, input: AdjustInput) -> Result<BatchRow, CommandError> {
    let session = state.session(Permission::ManageInventory)?;
    Ok(inventory::adjust_stock(&mut *state.db()?, &session, input, now())?)
}

#[tauri::command]
pub fn list_stock_ledger(state: State<'_, AppState>, product_id: Option<i64>, limit: u32) -> Result<Vec<LedgerRow>, CommandError> {
    let session = state.session(Permission::ViewStockLedger)?;
    Ok(inventory::ledger(&*state.db()?, &session, product_id, limit)?)
}

#[tauri::command]
pub fn list_expiring(state: State<'_, AppState>, within_days: i64) -> Result<Vec<ExpiringBatch>, CommandError> {
    let session = state.session(Permission::ViewInventory)?;
    Ok(inventory::expiring(&*state.db()?, &session, within_days, now())?)
}

#[tauri::command]
pub fn search_products_for_sale(state: State<'_, AppState>, text: String) -> Result<Vec<SaleProduct>, CommandError> {
    let session = state.session(Permission::CreateBills)?;
    Ok(inventory::search_for_sale(&*state.db()?, &session, &text, now())?)
}
