//! Dashboard and reports.

use clinic_core::auth::Permission;
use clinic_services::reports::{self, Dashboard, DateRange, SalesReport, StockReport};
use clinic_sqlite::repo::billing::ProductSales;
use tauri::State;

use crate::error::CommandError;
use crate::state::{AppState, now};

#[tauri::command]
pub fn get_dashboard(state: State<'_, AppState>) -> Result<Dashboard, CommandError> {
    let session = state.session(Permission::UseApp)?;
    Ok(reports::dashboard(&*state.db()?, &session, now())?)
}

#[tauri::command]
pub fn sales_report(state: State<'_, AppState>, range: DateRange) -> Result<SalesReport, CommandError> {
    let session = state.session(Permission::ViewReports)?;
    Ok(reports::sales(&*state.db()?, &session, &range)?)
}

#[tauri::command]
pub fn product_sales_report(state: State<'_, AppState>, range: DateRange) -> Result<Vec<ProductSales>, CommandError> {
    let session = state.session(Permission::ViewReports)?;
    Ok(reports::product_sales(&*state.db()?, &session, &range)?)
}

#[tauri::command]
pub fn stock_report(state: State<'_, AppState>) -> Result<StockReport, CommandError> {
    let session = state.session(Permission::ViewReports)?;
    Ok(reports::stock(&*state.db()?, &session, now())?)
}
