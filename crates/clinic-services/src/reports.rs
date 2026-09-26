//! Dashboard and reports (Phase 6). Figures come straight from SQL aggregates, so they stay
//! fast with years of bills.

use clinic_core::auth::Permission;
use clinic_core::time::{Date, local_day_start_utc};
use clinic_sqlite::Database;
use clinic_sqlite::repo::billing::{self as bills, BillQuery, BillRow, MethodTotal, ProductSales, SalesTotals};
use clinic_sqlite::repo::clients::{self, ClientRow};
use clinic_sqlite::repo::inventory::{self as stock, BatchRow, ProductQuery, ProductRow};
use serde::{Deserialize, Serialize};

use crate::error::invalid;
use crate::inventory::EXPIRY_WARNING_DAYS;
use crate::settings::clinic_today;
use crate::{ServiceError, Session, users};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    /// e.g. "25-Sep-2026".
    pub today: String,
    pub sales_today: SalesTotals,
    /// Total minus refunds.
    pub net_sales_today_paise: i64,
    pub active_products: i64,
    pub low_stock: i64,
    pub out_of_stock: i64,
    pub expiring_soon: i64,
    pub recent_bills: Vec<BillRow>,
    pub recent_clients: Vec<ClientRow>,
    /// For administrators: false means no second admin exists for password recovery (DEC-025).
    pub has_backup_admin: bool,
    /// For administrators: batches whose stock does not match the ledger (should be 0).
    pub ledger_problems: usize,
}

pub fn dashboard(db: &Database, actor: &Session, now: i64) -> Result<Dashboard, ServiceError> {
    actor.require(Permission::UseApp)?;
    let has_backup_admin = users::has_backup_admin(db)?;
    db.read(|c| {
        let (clinic, today) = clinic_today(c, now)?;
        let start = local_day_start_utc(today, clinic.utc_offset_minutes);
        let sales = bills::sales_totals(c, start, start + 86_400)?;
        let today_text = today.to_string();
        let count = |filter: &str| -> Result<i64, ServiceError> {
            let rows = stock::query_products(
                c,
                &ProductQuery { text: "", category_id: None, active_only: true, stock: filter, product_id: None, today: &today_text, limit: 100_000 },
            )?;
            Ok(rows.len() as i64)
        };
        Ok(Dashboard {
            today: today.display(),
            net_sales_today_paise: sales.total_paise - sales.returned_paise,
            sales_today: sales,
            active_products: stock::count_active_products(c)?,
            low_stock: count("LOW")?,
            out_of_stock: count("OUT")?,
            expiring_soon: stock::count_expiring(c, &today_text, &today.add_days(EXPIRY_WARNING_DAYS).to_string())?,
            recent_bills: bills::list_bills(c, &BillQuery { from: None, to: None, status: None, client_id: None, text: "", limit: 8 })?,
            recent_clients: clients::search(c, "", false, 8)?,
            has_backup_admin,
            ledger_problems: if actor.role.allows(Permission::ViewSystemInfo) { stock::ledger_mismatches(c)?.len() } else { 0 },
        })
    })
}

/// Local dates, both inclusive.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRange {
    pub from: String,
    pub to: String,
}

fn range_bounds(range: &DateRange, utc_offset_minutes: i32) -> Result<(i64, i64), ServiceError> {
    let from = Date::parse(range.from.trim()).ok_or_else(|| invalid("from", "Please enter a valid start date."))?;
    let to = Date::parse(range.to.trim()).ok_or_else(|| invalid("to", "Please enter a valid end date."))?;
    if to < from {
        return Err(invalid("to", "The end date is before the start date."));
    }
    if to.days_since_epoch() - from.days_since_epoch() > 3_660 {
        return Err(invalid("to", "Please choose a range of at most 10 years."));
    }
    Ok((local_day_start_utc(from, utc_offset_minutes), local_day_start_utc(to.add_days(1), utc_offset_minutes)))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesReport {
    pub totals: SalesTotals,
    pub net_paise: i64,
    pub by_method: Vec<MethodTotal>,
}

pub fn sales(db: &Database, actor: &Session, range: &DateRange) -> Result<SalesReport, ServiceError> {
    actor.require(Permission::ViewReports)?;
    db.read(|c| {
        let (clinic, _) = clinic_today(c, 0)?;
        let (from, to) = range_bounds(range, clinic.utc_offset_minutes)?;
        let totals = bills::sales_totals(c, from, to)?;
        Ok(SalesReport { net_paise: totals.total_paise - totals.returned_paise, totals, by_method: bills::payment_totals(c, from, to)? })
    })
}

pub fn product_sales(db: &Database, actor: &Session, range: &DateRange) -> Result<Vec<ProductSales>, ServiceError> {
    actor.require(Permission::ViewReports)?;
    db.read(|c| {
        let (clinic, _) = clinic_today(c, 0)?;
        let (from, to) = range_bounds(range, clinic.utc_offset_minutes)?;
        Ok(bills::product_sales(c, from, to)?)
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockReport {
    /// Every batch with stock (current stock report).
    pub batches: Vec<BatchRow>,
    pub low_stock: Vec<ProductRow>,
    pub out_of_stock: Vec<ProductRow>,
}

pub fn stock(db: &Database, actor: &Session, now: i64) -> Result<StockReport, ServiceError> {
    actor.require(Permission::ViewReports)?;
    db.read(|c| {
        let (_, today) = clinic_today(c, now)?;
        let today_text = today.to_string();
        let query = |filter: &str| {
            stock::query_products(c, &ProductQuery { text: "", category_id: None, active_only: true, stock: filter, product_id: None, today: &today_text, limit: 100_000 })
        };
        Ok(StockReport { batches: stock::batches_in_stock(c)?, low_stock: query("LOW")?, out_of_stock: query("OUT")? })
    })
}
