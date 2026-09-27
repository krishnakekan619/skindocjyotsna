//! Dashboard and reports (Phase 6). Figures come straight from SQL aggregates, so they stay
//! fast with years of bills.

use clinic_core::auth::Permission;
use clinic_core::time::{Date, local_day_start_utc};
use clinic_sqlite::Database;
use clinic_sqlite::repo::billing::{self as bills, BillQuery, BillRow, MethodTotal, ProductSales, SalesTotals, SectionSales, ServiceSales};
use clinic_sqlite::repo::clients::{self, ClientRow};
use clinic_sqlite::repo::inventory::{self as stock, BatchRow, ProductQuery, ProductRow};
use serde::{Deserialize, Serialize};

use crate::error::invalid;
use crate::inventory::{EXPIRY_WARNING_DAYS, ExpiringBatch};
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
    /// Today's sales split into consultation / procedures / medicines.
    pub sales_split_today: SectionSales,
    pub active_products: i64,
    pub low_stock: i64,
    pub out_of_stock: i64,
    pub expiring_soon: i64,
    /// Out-of-stock products first, then low stock (a few of each, for the dashboard list).
    pub stock_alerts: Vec<ProductRow>,
    /// Expired or expiring within 30 days, soonest first (a few, for the dashboard list).
    pub expiring_batches: Vec<ExpiringBatch>,
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
        let products = |filter: &str| -> Result<Vec<ProductRow>, ServiceError> {
            Ok(stock::query_products(
                c,
                &ProductQuery { text: "", category_id: None, active_only: true, stock: filter, product_id: None, today: &today_text, limit: 100_000 },
            )?)
        };
        let out = products("OUT")?;
        let low = products("LOW")?;
        let until = today.add_days(EXPIRY_WARNING_DAYS).to_string();
        let expiring_batches = stock::expiring_batches(c, &until)?
            .into_iter()
            .take(8)
            .map(|batch| {
                let days_left = batch.expiry_date.as_deref().and_then(Date::parse).map_or(0, |e| e.days_since_epoch() - today.days_since_epoch());
                ExpiringBatch { batch, days_left }
            })
            .collect();
        Ok(Dashboard {
            today: today.display(),
            net_sales_today_paise: sales.total_paise - sales.returned_paise,
            sales_today: sales,
            sales_split_today: bills::section_sales(c, start, start + 86_400)?,
            active_products: stock::count_active_products(c)?,
            low_stock: low.len() as i64,
            out_of_stock: out.len() as i64,
            expiring_soon: stock::count_expiring(c, &today_text, &until)?,
            stock_alerts: out.iter().take(5).chain(low.iter().take(5)).cloned().collect(),
            expiring_batches,
            recent_bills: bills::list_bills(c, &BillQuery { from: None, to: None, status: None, client_id: None, text: "", limit: 8 })?,
            recent_clients: clients::search(c, "", false, 8)?,
            has_backup_admin,
            ledger_problems: if actor.role.allows(Permission::ViewSystemInfo) { stock::ledger_mismatches(c)?.len() } else { 0 },
        })
    })
}

/// What sells most in a period: medicines, procedures and consultations (top 10 each).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopSellers {
    pub split: SectionSales,
    pub medicines: Vec<ProductSales>,
    pub procedures: Vec<ServiceSales>,
    pub consultations: Vec<ServiceSales>,
}

/// For the dashboard (every signed-in user): Today / Last 7 days / This month.
pub fn top_sellers(db: &Database, actor: &Session, range: &DateRange) -> Result<TopSellers, ServiceError> {
    actor.require(Permission::UseApp)?;
    db.read(|c| {
        let (clinic, _) = clinic_today(c, 0)?;
        let (from, to) = range_bounds(range, clinic.utc_offset_minutes)?;
        let services = bills::service_sales(c, from, to)?;
        let of_kind = |kind: &str| services.iter().filter(|s| s.kind == kind).take(10).cloned().collect::<Vec<_>>();
        Ok(TopSellers {
            split: bills::section_sales(c, from, to)?,
            medicines: bills::product_sales(c, from, to)?.into_iter().take(10).collect(),
            procedures: of_kind("PROCEDURE"),
            consultations: of_kind("CONSULTATION"),
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
