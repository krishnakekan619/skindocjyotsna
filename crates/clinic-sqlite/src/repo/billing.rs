//! Bills, items, the batches each item came from, payments, returns, number sequences and the
//! sales figures used by reports.

use rusqlite::{Connection, OptionalExtension, Row, named_params, params};
use serde::Serialize;

use super::inventory::like_pattern;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillRow {
    pub id: i64,
    pub bill_no: String,
    pub client_id: Option<i64>,
    pub client_name: Option<String>,
    pub client_code: Option<String>,
    pub status: String,
    pub subtotal_paise: i64,
    pub discount_paise: i64,
    pub tax_paise: i64,
    pub round_off_paise: i64,
    pub total_paise: i64,
    pub amount_received_paise: Option<i64>,
    pub change_paise: Option<i64>,
    pub returned_paise: i64,
    pub note: String,
    pub created_by: i64,
    pub created_by_name: String,
    pub finalized_at: i64,
    pub cancelled_at: Option<i64>,
    pub cancel_reason: Option<String>,
    pub replaces_bill_id: Option<i64>,
    pub replaces_bill_no: Option<String>,
    pub corrected_by_bill_id: Option<i64>,
    pub corrected_by_bill_no: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillItemRow {
    pub id: i64,
    pub bill_id: i64,
    pub line_no: i64,
    pub product_id: i64,
    pub product_name: String,
    pub sku: String,
    pub unit: String,
    pub qty: i64,
    pub not_supplied_qty: i64,
    pub unit_price_paise: i64,
    pub discount_share_paise: i64,
    pub gst_rate_bp: i64,
    pub tax_paise: i64,
    pub line_total_paise: i64,
    pub returned_qty: i64,
    pub batches: Vec<ItemBatchRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemBatchRow {
    pub id: i64,
    pub bill_item_id: i64,
    pub batch_id: i64,
    pub batch_no: String,
    pub expiry_date: Option<String>,
    pub qty: i64,
    pub returned_qty: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRow {
    pub id: i64,
    pub method: String,
    pub amount_paise: i64,
    pub direction: String,
    pub reference: String,
    pub created_at: i64,
}

// ---- Numbers ---------------------------------------------------------------------------------

/// Next number in a gapless series: 1, 2, 3 ... per (series, period). Inside the caller's
/// transaction, so a failed bill does not use up a number.
pub fn next_number(conn: &Connection, series: &str, period: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "INSERT INTO number_sequence (series, period, next_value) VALUES (?1, ?2, 2)
         ON CONFLICT (series, period) DO UPDATE SET next_value = next_value + 1
         RETURNING next_value - 1",
        params![series, period],
        |r| r.get(0),
    )
}

// ---- Writing ---------------------------------------------------------------------------------

pub struct NewBill<'a> {
    pub bill_no: &'a str,
    pub idempotency_key: &'a str,
    pub client_id: Option<i64>,
    pub subtotal_paise: i64,
    pub discount_paise: i64,
    pub tax_paise: i64,
    pub round_off_paise: i64,
    pub total_paise: i64,
    pub amount_received_paise: Option<i64>,
    pub change_paise: Option<i64>,
    pub note: &'a str,
    pub created_by: i64,
    pub discount_approved_by: Option<i64>,
    pub replaces_bill_id: Option<i64>,
    pub now: i64,
}

pub fn insert_bill(conn: &Connection, b: &NewBill<'_>) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO bill (bill_no, idempotency_key, client_id, status, subtotal_paise, discount_paise, tax_paise,
                           round_off_paise, total_paise, amount_received_paise, change_paise, note, created_by,
                           discount_approved_by, finalized_at, replaces_bill_id)
         VALUES (?1, ?2, ?3, 'FINALIZED', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            b.bill_no, b.idempotency_key, b.client_id, b.subtotal_paise, b.discount_paise, b.tax_paise,
            b.round_off_paise, b.total_paise, b.amount_received_paise, b.change_paise, b.note, b.created_by,
            b.discount_approved_by, b.now, b.replaces_bill_id
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub struct NewBillItem<'a> {
    pub bill_id: i64,
    pub line_no: i64,
    pub product_id: i64,
    pub product_name: &'a str,
    pub sku: &'a str,
    pub unit: &'a str,
    pub qty: i64,
    pub not_supplied_qty: i64,
    pub unit_price_paise: i64,
    pub discount_share_paise: i64,
    pub gst_rate_bp: i64,
    pub tax_paise: i64,
    pub line_total_paise: i64,
}

pub fn insert_item(conn: &Connection, i: &NewBillItem<'_>) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO bill_item (bill_id, line_no, product_id, product_name, sku, unit, qty, not_supplied_qty,
                                unit_price_paise, discount_share_paise, gst_rate_bp, tax_paise, line_total_paise)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            i.bill_id, i.line_no, i.product_id, i.product_name, i.sku, i.unit, i.qty, i.not_supplied_qty,
            i.unit_price_paise, i.discount_share_paise, i.gst_rate_bp, i.tax_paise, i.line_total_paise
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_item_batch(conn: &Connection, bill_item_id: i64, batch_id: i64, qty: i64) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO bill_item_batch (bill_item_id, batch_id, qty) VALUES (?1, ?2, ?3)",
        params![bill_item_id, batch_id, qty],
    )?;
    Ok(conn.last_insert_rowid())
}

pub struct NewPayment<'a> {
    pub bill_id: i64,
    pub method: &'a str,
    pub amount_paise: i64,
    /// "IN" (paid) or "REFUND".
    pub direction: &'a str,
    pub reference: &'a str,
    pub sales_return_id: Option<i64>,
    pub now: i64,
}

pub fn insert_payment(conn: &Connection, p: &NewPayment<'_>) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO payment (bill_id, method, amount_paise, direction, reference, sales_return_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![p.bill_id, p.method, p.amount_paise, p.direction, p.reference, p.sales_return_id, p.now],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Marks a bill CANCELLED or CORRECTED (never deletes it).
pub fn close_bill(conn: &Connection, bill_id: i64, status: &str, user_id: i64, reason: &str, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE bill SET status = ?2, cancelled_at = ?5, cancelled_by = ?3, cancel_reason = ?4
         WHERE id = ?1 AND status = 'FINALIZED'",
        params![bill_id, status, user_id, reason, now],
    )
}

pub fn set_corrected_by(conn: &Connection, bill_id: i64, new_bill_id: i64) -> rusqlite::Result<usize> {
    conn.execute("UPDATE bill SET corrected_by_bill_id = ?2 WHERE id = ?1", params![bill_id, new_bill_id])
}

pub struct NewReturn<'a> {
    pub return_no: &'a str,
    pub bill_id: i64,
    pub reason: &'a str,
    pub refund_paise: i64,
    pub refund_method: &'a str,
    pub user_id: i64,
    pub now: i64,
}

pub fn insert_return(conn: &Connection, r: &NewReturn<'_>) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO sales_return (return_no, bill_id, reason, refund_paise, refund_method, user_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![r.return_no, r.bill_id, r.reason, r.refund_paise, r.refund_method, r.user_id, r.now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_return_item(conn: &Connection, sales_return_id: i64, bill_item_id: i64, batch_id: i64, qty: i64, refund_paise: i64, restocked: bool) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO return_item (sales_return_id, bill_item_id, batch_id, qty, refund_paise, restocked)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![sales_return_id, bill_item_id, batch_id, qty, refund_paise, restocked],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Records returned quantities on the item, the batch share and the bill total.
pub fn record_returned(conn: &Connection, bill_id: i64, bill_item_id: i64, item_batch_id: i64, qty: i64, refund_paise: i64) -> rusqlite::Result<()> {
    conn.execute("UPDATE bill_item SET returned_qty = returned_qty + ?2 WHERE id = ?1", params![bill_item_id, qty])?;
    conn.execute("UPDATE bill_item_batch SET returned_qty = returned_qty + ?2 WHERE id = ?1", params![item_batch_id, qty])?;
    conn.execute("UPDATE bill SET returned_paise = returned_paise + ?2 WHERE id = ?1", params![bill_id, refund_paise])?;
    Ok(())
}

/// Money already refunded for one bill item (across all its returns).
pub fn refunded_for_item(conn: &Connection, bill_item_id: i64) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COALESCE(SUM(refund_paise), 0) FROM return_item WHERE bill_item_id = ?1", [bill_item_id], |r| r.get(0))
}

// ---- Reading ---------------------------------------------------------------------------------

const BILL_SELECT: &str = "
SELECT b.id, b.bill_no, b.client_id, c.full_name, c.client_code, b.status, b.subtotal_paise, b.discount_paise,
       b.tax_paise, b.round_off_paise, b.total_paise, b.amount_received_paise, b.change_paise, b.returned_paise,
       b.note, b.created_by, u.full_name, b.finalized_at, b.cancelled_at, b.cancel_reason, b.replaces_bill_id,
       rb.bill_no, b.corrected_by_bill_id, cb.bill_no
FROM bill b
JOIN app_user u ON u.id = b.created_by
LEFT JOIN client c ON c.id = b.client_id
LEFT JOIN bill rb ON rb.id = b.replaces_bill_id
LEFT JOIN bill cb ON cb.id = b.corrected_by_bill_id";

fn bill_from_row(r: &Row<'_>) -> rusqlite::Result<BillRow> {
    Ok(BillRow {
        id: r.get(0)?,
        bill_no: r.get(1)?,
        client_id: r.get(2)?,
        client_name: r.get(3)?,
        client_code: r.get(4)?,
        status: r.get(5)?,
        subtotal_paise: r.get(6)?,
        discount_paise: r.get(7)?,
        tax_paise: r.get(8)?,
        round_off_paise: r.get(9)?,
        total_paise: r.get(10)?,
        amount_received_paise: r.get(11)?,
        change_paise: r.get(12)?,
        returned_paise: r.get(13)?,
        note: r.get(14)?,
        created_by: r.get(15)?,
        created_by_name: r.get(16)?,
        finalized_at: r.get(17)?,
        cancelled_at: r.get(18)?,
        cancel_reason: r.get(19)?,
        replaces_bill_id: r.get(20)?,
        replaces_bill_no: r.get(21)?,
        corrected_by_bill_id: r.get(22)?,
        corrected_by_bill_no: r.get(23)?,
    })
}

pub fn find_bill(conn: &Connection, id: i64) -> rusqlite::Result<Option<BillRow>> {
    conn.query_row(&format!("{BILL_SELECT} WHERE b.id = ?1"), [id], bill_from_row).optional()
}

pub fn find_bill_id_by_key(conn: &Connection, idempotency_key: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row("SELECT id FROM bill WHERE idempotency_key = ?1", [idempotency_key], |r| r.get(0)).optional()
}

pub struct BillQuery<'a> {
    /// Unix seconds, inclusive / exclusive; `None` = no limit.
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub status: Option<&'a str>,
    pub client_id: Option<i64>,
    /// Bill number or client name.
    pub text: &'a str,
    pub limit: u32,
}

pub fn list_bills(conn: &Connection, q: &BillQuery<'_>) -> rusqlite::Result<Vec<BillRow>> {
    let mut stmt = conn.prepare(&format!(
        "{BILL_SELECT}
         WHERE (:from IS NULL OR b.finalized_at >= :from) AND (:to IS NULL OR b.finalized_at < :to)
           AND (:status IS NULL OR b.status = :status) AND (:client_id IS NULL OR b.client_id = :client_id)
           AND (:text = '' OR b.bill_no LIKE :like OR c.full_name LIKE :like)
         ORDER BY b.finalized_at DESC, b.id DESC LIMIT :limit"
    ))?;
    let text = q.text.trim();
    let like = like_pattern(text);
    stmt.query_map(
        named_params! {
            ":from": q.from, ":to": q.to, ":status": q.status, ":client_id": q.client_id,
            ":text": text, ":like": like, ":limit": i64::from(q.limit),
        },
        bill_from_row,
    )?
    .collect()
}

pub fn items(conn: &Connection, bill_id: i64) -> rusqlite::Result<Vec<BillItemRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, bill_id, line_no, product_id, product_name, sku, unit, qty, not_supplied_qty, unit_price_paise,
                discount_share_paise, gst_rate_bp, tax_paise, line_total_paise, returned_qty
         FROM bill_item WHERE bill_id = ?1 ORDER BY line_no",
    )?;
    let mut rows: Vec<BillItemRow> = stmt
        .query_map([bill_id], |r| {
            Ok(BillItemRow {
                id: r.get(0)?,
                bill_id: r.get(1)?,
                line_no: r.get(2)?,
                product_id: r.get(3)?,
                product_name: r.get(4)?,
                sku: r.get(5)?,
                unit: r.get(6)?,
                qty: r.get(7)?,
                not_supplied_qty: r.get(8)?,
                unit_price_paise: r.get(9)?,
                discount_share_paise: r.get(10)?,
                gst_rate_bp: r.get(11)?,
                tax_paise: r.get(12)?,
                line_total_paise: r.get(13)?,
                returned_qty: r.get(14)?,
                batches: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut batch_stmt = conn.prepare(
        "SELECT ib.id, ib.bill_item_id, ib.batch_id, b.batch_no, b.expiry_date, ib.qty, ib.returned_qty
         FROM bill_item_batch ib JOIN inventory_batch b ON b.id = ib.batch_id
         WHERE ib.bill_item_id = ?1 ORDER BY ib.id",
    )?;
    for row in &mut rows {
        row.batches = batch_stmt
            .query_map([row.id], |r| {
                Ok(ItemBatchRow {
                    id: r.get(0)?,
                    bill_item_id: r.get(1)?,
                    batch_id: r.get(2)?,
                    batch_no: r.get(3)?,
                    expiry_date: r.get(4)?,
                    qty: r.get(5)?,
                    returned_qty: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
    }
    Ok(rows)
}

pub fn payments(conn: &Connection, bill_id: i64) -> rusqlite::Result<Vec<PaymentRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, method, amount_paise, direction, reference, created_at FROM payment WHERE bill_id = ?1 ORDER BY id",
    )?;
    stmt.query_map([bill_id], |r| {
        Ok(PaymentRow { id: r.get(0)?, method: r.get(1)?, amount_paise: r.get(2)?, direction: r.get(3)?, reference: r.get(4)?, created_at: r.get(5)? })
    })?
    .collect()
}

// ---- Reports ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesTotals {
    pub bill_count: i64,
    pub subtotal_paise: i64,
    pub discount_paise: i64,
    pub tax_paise: i64,
    pub total_paise: i64,
    pub returned_paise: i64,
    pub clients_served: i64,
}

/// Figures for bills finalized in [from, to). Cancelled bills, and bills replaced by a
/// correction, are not sales.
pub fn sales_totals(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<SalesTotals> {
    conn.query_row(
        "SELECT count(*), COALESCE(SUM(subtotal_paise), 0), COALESCE(SUM(discount_paise), 0), COALESCE(SUM(tax_paise), 0),
                COALESCE(SUM(total_paise), 0), COALESCE(SUM(returned_paise), 0), count(DISTINCT client_id)
         FROM bill WHERE status = 'FINALIZED' AND finalized_at >= ?1 AND finalized_at < ?2",
        params![from, to],
        |r| {
            Ok(SalesTotals {
                bill_count: r.get(0)?,
                subtotal_paise: r.get(1)?,
                discount_paise: r.get(2)?,
                tax_paise: r.get(3)?,
                total_paise: r.get(4)?,
                returned_paise: r.get(5)?,
                clients_served: r.get(6)?,
            })
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodTotal {
    pub method: String,
    pub received_paise: i64,
    pub refunded_paise: i64,
}

/// Money in and refunds out per payment method, by payment date, in [from, to).
pub fn payment_totals(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<MethodTotal>> {
    let mut stmt = conn.prepare(
        "SELECT method,
                COALESCE(SUM(CASE WHEN direction = 'IN' THEN amount_paise END), 0),
                COALESCE(SUM(CASE WHEN direction = 'REFUND' THEN amount_paise END), 0)
         FROM payment WHERE created_at >= ?1 AND created_at < ?2 GROUP BY method ORDER BY method",
    )?;
    stmt.query_map(params![from, to], |r| Ok(MethodTotal { method: r.get(0)?, received_paise: r.get(1)?, refunded_paise: r.get(2)? }))?
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSales {
    pub product_id: i64,
    pub product_name: String,
    pub qty_sold: i64,
    pub revenue_paise: i64,
}

/// Quantity kept by clients (sold minus returned) and revenue after refunds, per product.
pub fn product_sales(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<ProductSales>> {
    let mut stmt = conn.prepare(
        "SELECT i.product_id, i.product_name, SUM(i.qty - i.returned_qty),
                SUM(i.line_total_paise) - COALESCE(SUM((SELECT SUM(ri.refund_paise) FROM return_item ri WHERE ri.bill_item_id = i.id)), 0)
         FROM bill_item i JOIN bill b ON b.id = i.bill_id
         WHERE b.status = 'FINALIZED' AND b.finalized_at >= ?1 AND b.finalized_at < ?2 AND i.qty > 0
         GROUP BY i.product_id, i.product_name
         ORDER BY 4 DESC",
    )?;
    stmt.query_map(params![from, to], |r| Ok(ProductSales { product_id: r.get(0)?, product_name: r.get(1)?, qty_sold: r.get(2)?, revenue_paise: r.get(3)? }))?
        .collect()
}
