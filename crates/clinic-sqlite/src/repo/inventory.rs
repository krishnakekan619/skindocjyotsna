//! Categories, suppliers, products, batches and the stock ledger.
//!
//! Stock quantities change ONLY through `apply_movement`, which updates the batch and writes the
//! ledger row together, and cannot take a batch below zero.

use rusqlite::{Connection, OptionalExtension, Row, named_params, params};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Supplier {
    pub id: i64,
    pub name: String,
    pub phone: String,
    pub gstin: String,
    pub is_active: bool,
}

/// A product with its current stock. `sellable_qty` excludes expired batches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductRow {
    pub id: i64,
    pub sku: String,
    pub name: String,
    pub generic_name: String,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub product_type: String,
    pub manufacturer: String,
    pub unit: String,
    pub gst_rate_bp: i64,
    pub default_selling_price_paise: i64,
    pub default_purchase_price_paise: i64,
    pub min_stock: i64,
    pub requires_expiry: bool,
    pub is_active: bool,
    pub notes: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub total_qty: i64,
    pub sellable_qty: i64,
    pub next_expiry: Option<String>,
}

/// Editable product fields (insert and update).
pub struct ProductFields<'a> {
    pub sku: &'a str,
    pub name: &'a str,
    pub generic_name: &'a str,
    pub category_id: Option<i64>,
    pub product_type: &'a str,
    pub manufacturer: &'a str,
    pub unit: &'a str,
    pub gst_rate_bp: i64,
    pub default_selling_price_paise: i64,
    pub default_purchase_price_paise: i64,
    pub min_stock: i64,
    pub requires_expiry: bool,
    pub is_active: bool,
    pub notes: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRow {
    pub id: i64,
    pub product_id: i64,
    pub product_name: String,
    pub batch_no: String,
    pub expiry_date: Option<String>,
    pub supplier_id: Option<i64>,
    pub supplier_name: Option<String>,
    pub purchase_price_paise: i64,
    pub selling_price_paise: i64,
    pub quantity: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerRow {
    pub id: i64,
    pub occurred_at: i64,
    pub product_id: i64,
    pub product_name: String,
    pub batch_id: i64,
    pub batch_no: String,
    pub kind: String,
    pub qty_change: i64,
    pub previous_qty: i64,
    pub new_qty: i64,
    pub reason: String,
    pub bill_id: Option<i64>,
    pub bill_no: Option<String>,
    pub username: String,
}

// ---- Categories and suppliers ----------------------------------------------------------------

pub fn list_categories(conn: &Connection) -> rusqlite::Result<Vec<Category>> {
    let mut stmt = conn.prepare("SELECT id, name, is_active FROM category ORDER BY is_active DESC, name")?;
    stmt.query_map([], |r| Ok(Category { id: r.get(0)?, name: r.get(1)?, is_active: r.get(2)? }))?.collect()
}

pub fn category_name_taken(conn: &Connection, name: &str, except_id: Option<i64>) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM category WHERE name = ?1 AND (?2 IS NULL OR id <> ?2))",
        params![name, except_id],
        |r| r.get(0),
    )
}

pub fn insert_category(conn: &Connection, name: &str, is_active: bool) -> rusqlite::Result<i64> {
    conn.execute("INSERT INTO category (name, is_active) VALUES (?1, ?2)", params![name, is_active])?;
    Ok(conn.last_insert_rowid())
}

pub fn update_category(conn: &Connection, id: i64, name: &str, is_active: bool) -> rusqlite::Result<usize> {
    conn.execute("UPDATE category SET name = ?2, is_active = ?3 WHERE id = ?1", params![id, name, is_active])
}

pub fn list_suppliers(conn: &Connection) -> rusqlite::Result<Vec<Supplier>> {
    let mut stmt = conn.prepare("SELECT id, name, phone, gstin, is_active FROM supplier ORDER BY is_active DESC, name")?;
    stmt.query_map([], |r| Ok(Supplier { id: r.get(0)?, name: r.get(1)?, phone: r.get(2)?, gstin: r.get(3)?, is_active: r.get(4)? }))?
        .collect()
}

pub fn supplier_name_taken(conn: &Connection, name: &str, except_id: Option<i64>) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM supplier WHERE name = ?1 AND (?2 IS NULL OR id <> ?2))",
        params![name, except_id],
        |r| r.get(0),
    )
}

pub fn insert_supplier(conn: &Connection, name: &str, phone: &str, gstin: &str, is_active: bool) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO supplier (name, phone, gstin, is_active) VALUES (?1, ?2, ?3, ?4)",
        params![name, phone, gstin, is_active],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_supplier(conn: &Connection, id: i64, name: &str, phone: &str, gstin: &str, is_active: bool) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE supplier SET name = ?2, phone = ?3, gstin = ?4, is_active = ?5 WHERE id = ?1",
        params![id, name, phone, gstin, is_active],
    )
}

// ---- Products --------------------------------------------------------------------------------

/// Filters for the product list. `stock`: "ALL", "LOW" (sellable but at/below minimum) or "OUT".
pub struct ProductQuery<'a> {
    pub text: &'a str,
    pub category_id: Option<i64>,
    pub active_only: bool,
    pub stock: &'a str,
    pub product_id: Option<i64>,
    /// Today's local date `YYYY-MM-DD` (decides what counts as expired).
    pub today: &'a str,
    pub limit: u32,
}

const PRODUCT_SELECT: &str = "
SELECT * FROM (
    SELECT p.id, p.sku, p.name, p.generic_name, p.category_id, c.name AS category_name, p.product_type,
           p.manufacturer, p.unit, p.gst_rate_bp, p.default_selling_price_paise, p.default_purchase_price_paise,
           p.min_stock, p.requires_expiry, p.is_active, p.notes, p.created_at, p.updated_at,
           COALESCE((SELECT SUM(b.quantity) FROM inventory_batch b WHERE b.product_id = p.id), 0) AS total_qty,
           COALESCE((SELECT SUM(b.quantity) FROM inventory_batch b
                     WHERE b.product_id = p.id AND (b.expiry_date IS NULL OR b.expiry_date >= :today)), 0) AS sellable_qty,
           (SELECT MIN(b.expiry_date) FROM inventory_batch b
             WHERE b.product_id = p.id AND b.quantity > 0 AND b.expiry_date >= :today) AS next_expiry
    FROM product p LEFT JOIN category c ON c.id = p.category_id
    WHERE (:product_id IS NULL OR p.id = :product_id)
      AND (:category_id IS NULL OR p.category_id = :category_id)
      AND (:active_only = 0 OR p.is_active = 1)
      AND (:text = '' OR p.name LIKE :like OR p.generic_name LIKE :like OR p.sku LIKE :like
           OR p.manufacturer LIKE :like
           OR EXISTS (SELECT 1 FROM inventory_batch b WHERE b.product_id = p.id AND b.batch_no LIKE :like))
)
WHERE :stock = 'ALL'
   OR (:stock = 'LOW' AND sellable_qty > 0 AND sellable_qty <= min_stock)
   OR (:stock = 'OUT' AND sellable_qty = 0)
ORDER BY name COLLATE NOCASE
LIMIT :limit";

fn product_from_row(r: &Row<'_>) -> rusqlite::Result<ProductRow> {
    Ok(ProductRow {
        id: r.get(0)?,
        sku: r.get(1)?,
        name: r.get(2)?,
        generic_name: r.get(3)?,
        category_id: r.get(4)?,
        category_name: r.get(5)?,
        product_type: r.get(6)?,
        manufacturer: r.get(7)?,
        unit: r.get(8)?,
        gst_rate_bp: r.get(9)?,
        default_selling_price_paise: r.get(10)?,
        default_purchase_price_paise: r.get(11)?,
        min_stock: r.get(12)?,
        requires_expiry: r.get(13)?,
        is_active: r.get(14)?,
        notes: r.get(15)?,
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
        total_qty: r.get(18)?,
        sellable_qty: r.get(19)?,
        next_expiry: r.get(20)?,
    })
}

/// `%text%` for LIKE, with LIKE wildcards in the user's text escaped away.
pub fn like_pattern(text: &str) -> String {
    format!("%{}%", text.trim().replace(['%', '_'], " "))
}

pub fn query_products(conn: &Connection, q: &ProductQuery<'_>) -> rusqlite::Result<Vec<ProductRow>> {
    let mut stmt = conn.prepare(PRODUCT_SELECT)?;
    let text = q.text.trim();
    let like = like_pattern(text);
    stmt.query_map(
        named_params! {
            ":today": q.today, ":product_id": q.product_id, ":category_id": q.category_id,
            ":active_only": q.active_only, ":text": text, ":like": like, ":stock": q.stock, ":limit": i64::from(q.limit),
        },
        product_from_row,
    )?
    .collect()
}

pub fn find_product(conn: &Connection, id: i64, today: &str) -> rusqlite::Result<Option<ProductRow>> {
    let rows = query_products(
        conn,
        &ProductQuery { text: "", category_id: None, active_only: false, stock: "ALL", product_id: Some(id), today, limit: 1 },
    )?;
    Ok(rows.into_iter().next())
}

pub fn sku_taken(conn: &Connection, sku: &str, except_id: Option<i64>) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM product WHERE sku = ?1 AND (?2 IS NULL OR id <> ?2))",
        params![sku, except_id],
        |r| r.get(0),
    )
}

pub fn insert_product(conn: &Connection, p: &ProductFields<'_>, now: i64) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO product (sku, name, generic_name, category_id, product_type, manufacturer, unit, gst_rate_bp,
                              default_selling_price_paise, default_purchase_price_paise, min_stock, requires_expiry,
                              is_active, notes, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
        params![
            p.sku, p.name, p.generic_name, p.category_id, p.product_type, p.manufacturer, p.unit, p.gst_rate_bp,
            p.default_selling_price_paise, p.default_purchase_price_paise, p.min_stock, p.requires_expiry,
            p.is_active, p.notes, now
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_product(conn: &Connection, id: i64, p: &ProductFields<'_>, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE product SET sku = ?2, name = ?3, generic_name = ?4, category_id = ?5, product_type = ?6,
                            manufacturer = ?7, unit = ?8, gst_rate_bp = ?9, default_selling_price_paise = ?10,
                            default_purchase_price_paise = ?11, min_stock = ?12, requires_expiry = ?13,
                            is_active = ?14, notes = ?15, updated_at = ?16
         WHERE id = ?1",
        params![
            id, p.sku, p.name, p.generic_name, p.category_id, p.product_type, p.manufacturer, p.unit, p.gst_rate_bp,
            p.default_selling_price_paise, p.default_purchase_price_paise, p.min_stock, p.requires_expiry,
            p.is_active, p.notes, now
        ],
    )
}

pub fn count_active_products(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT count(*) FROM product WHERE is_active = 1", [], |r| r.get(0))
}

// ---- Batches ---------------------------------------------------------------------------------

const BATCH_SELECT: &str = "
SELECT b.id, b.product_id, p.name, b.batch_no, b.expiry_date, b.supplier_id, s.name, b.purchase_price_paise,
       b.selling_price_paise, b.quantity, b.created_at
FROM inventory_batch b JOIN product p ON p.id = b.product_id LEFT JOIN supplier s ON s.id = b.supplier_id";

fn batch_from_row(r: &Row<'_>) -> rusqlite::Result<BatchRow> {
    Ok(BatchRow {
        id: r.get(0)?,
        product_id: r.get(1)?,
        product_name: r.get(2)?,
        batch_no: r.get(3)?,
        expiry_date: r.get(4)?,
        supplier_id: r.get(5)?,
        supplier_name: r.get(6)?,
        purchase_price_paise: r.get(7)?,
        selling_price_paise: r.get(8)?,
        quantity: r.get(9)?,
        created_at: r.get(10)?,
    })
}

/// All batches of a product, soonest expiry first (no-expiry batches last).
pub fn batches_for_product(conn: &Connection, product_id: i64) -> rusqlite::Result<Vec<BatchRow>> {
    let mut stmt = conn.prepare(&format!(
        "{BATCH_SELECT} WHERE b.product_id = ?1 ORDER BY b.expiry_date IS NULL, b.expiry_date, b.id"
    ))?;
    stmt.query_map([product_id], batch_from_row)?.collect()
}

pub fn find_batch(conn: &Connection, id: i64) -> rusqlite::Result<Option<BatchRow>> {
    conn.query_row(&format!("{BATCH_SELECT} WHERE b.id = ?1"), [id], batch_from_row).optional()
}

pub fn find_batch_by_no(conn: &Connection, product_id: i64, batch_no: &str) -> rusqlite::Result<Option<BatchRow>> {
    conn.query_row(&format!("{BATCH_SELECT} WHERE b.product_id = ?1 AND b.batch_no = ?2"), params![product_id, batch_no], batch_from_row)
        .optional()
}

pub struct NewBatch<'a> {
    pub product_id: i64,
    pub batch_no: &'a str,
    pub expiry_date: Option<&'a str>,
    pub supplier_id: Option<i64>,
    pub purchase_price_paise: i64,
    pub selling_price_paise: i64,
    pub now: i64,
}

/// Creates an empty batch; stock is added with `apply_movement`.
pub fn insert_batch(conn: &Connection, b: &NewBatch<'_>) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO inventory_batch (product_id, batch_no, expiry_date, supplier_id, purchase_price_paise,
                                      selling_price_paise, quantity, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7)",
        params![b.product_id, b.batch_no, b.expiry_date, b.supplier_id, b.purchase_price_paise, b.selling_price_paise, b.now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_batch_details(conn: &Connection, id: i64, expiry_date: Option<&str>, supplier_id: Option<i64>, purchase: i64, selling: i64, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE inventory_batch SET expiry_date = ?2, supplier_id = ?3, purchase_price_paise = ?4,
                                    selling_price_paise = ?5, updated_at = ?6
         WHERE id = ?1",
        params![id, expiry_date, supplier_id, purchase, selling, now],
    )
}

/// Batches with stock that expire on or before `until` (already expired ones included).
pub fn expiring_batches(conn: &Connection, until: &str) -> rusqlite::Result<Vec<BatchRow>> {
    let mut stmt = conn.prepare(&format!(
        "{BATCH_SELECT} WHERE b.quantity > 0 AND b.expiry_date IS NOT NULL AND b.expiry_date <= ?1
         ORDER BY b.expiry_date, p.name COLLATE NOCASE"
    ))?;
    stmt.query_map([until], batch_from_row)?.collect()
}

/// Every batch with stock, for the stock report.
pub fn batches_in_stock(conn: &Connection) -> rusqlite::Result<Vec<BatchRow>> {
    let mut stmt = conn.prepare(&format!(
        "{BATCH_SELECT} WHERE b.quantity > 0 ORDER BY p.name COLLATE NOCASE, b.expiry_date IS NULL, b.expiry_date"
    ))?;
    stmt.query_map([], batch_from_row)?.collect()
}

pub fn count_expiring(conn: &Connection, today: &str, until: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT count(*) FROM inventory_batch WHERE quantity > 0 AND expiry_date >= ?1 AND expiry_date <= ?2",
        params![today, until],
        |r| r.get(0),
    )
}

// ---- Stock movements (the only way stock changes) --------------------------------------------

pub struct Movement<'a> {
    pub batch_id: i64,
    /// INITIAL_STOCK, PURCHASE, SALE, RETURN, ADJUSTMENT, DAMAGE, EXPIRY or CANCELLATION.
    pub kind: &'a str,
    pub qty_change: i64,
    pub reason: &'a str,
    pub bill_id: Option<i64>,
    pub sales_return_id: Option<i64>,
    pub user_id: i64,
    pub now: i64,
}

/// Changes a batch's quantity and records it in the ledger, in the caller's transaction.
/// Returns the new quantity, or `None` (nothing changed) if the batch would go below zero.
pub fn apply_movement(conn: &Connection, m: &Movement<'_>) -> rusqlite::Result<Option<i64>> {
    let updated: Option<(i64, i64)> = conn
        .query_row(
            "UPDATE inventory_batch SET quantity = quantity + ?2, updated_at = ?3
             WHERE id = ?1 AND quantity + ?2 >= 0
             RETURNING quantity, product_id",
            params![m.batch_id, m.qty_change, m.now],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((new_qty, product_id)) = updated else { return Ok(None) };
    conn.execute(
        "INSERT INTO inventory_transaction (occurred_at, product_id, batch_id, kind, qty_change, previous_qty, new_qty,
                                            reason, bill_id, sales_return_id, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            m.now, product_id, m.batch_id, m.kind, m.qty_change, new_qty - m.qty_change, new_qty, m.reason,
            m.bill_id, m.sales_return_id, m.user_id
        ],
    )?;
    Ok(Some(new_qty))
}

/// Ledger entries, newest first, optionally for one product.
pub fn ledger(conn: &Connection, product_id: Option<i64>, limit: u32) -> rusqlite::Result<Vec<LedgerRow>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.occurred_at, t.product_id, p.name, t.batch_id, b.batch_no, t.kind, t.qty_change,
                t.previous_qty, t.new_qty, t.reason, t.bill_id, bl.bill_no, u.username
         FROM inventory_transaction t
         JOIN product p ON p.id = t.product_id
         JOIN inventory_batch b ON b.id = t.batch_id
         JOIN app_user u ON u.id = t.user_id
         LEFT JOIN bill bl ON bl.id = t.bill_id
         WHERE (?1 IS NULL OR t.product_id = ?1)
         ORDER BY t.id DESC LIMIT ?2",
    )?;
    stmt.query_map(params![product_id, i64::from(limit)], |r| {
        Ok(LedgerRow {
            id: r.get(0)?,
            occurred_at: r.get(1)?,
            product_id: r.get(2)?,
            product_name: r.get(3)?,
            batch_id: r.get(4)?,
            batch_no: r.get(5)?,
            kind: r.get(6)?,
            qty_change: r.get(7)?,
            previous_qty: r.get(8)?,
            new_qty: r.get(9)?,
            reason: r.get(10)?,
            bill_id: r.get(11)?,
            bill_no: r.get(12)?,
            username: r.get(13)?,
        })
    })?
    .collect()
}

/// Checks the stock invariant (design D24): each batch's quantity equals the sum of its ledger.
/// Returns the ids of batches where it does not hold (should always be empty).
pub fn ledger_mismatches(conn: &Connection) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT b.id FROM inventory_batch b
         WHERE b.quantity <> COALESCE((SELECT SUM(t.qty_change) FROM inventory_transaction t WHERE t.batch_id = b.id), 0)",
    )?;
    stmt.query_map([], |r| r.get(0))?.collect()
}
