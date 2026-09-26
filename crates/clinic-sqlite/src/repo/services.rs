//! Consultation types and procedures (the `service` catalog). Not stock items.

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRow {
    pub id: i64,
    /// `CONSULTATION` or `PROCEDURE`.
    pub kind: String,
    pub name: String,
    pub default_price_paise: i64,
    pub gst_rate_bp: i64,
    pub discount_eligible: bool,
    pub is_active: bool,
    pub sort_order: i64,
}

pub struct ServiceFields<'a> {
    pub kind: &'a str,
    pub name: &'a str,
    pub default_price_paise: i64,
    pub gst_rate_bp: i64,
    pub discount_eligible: bool,
    pub is_active: bool,
    pub sort_order: i64,
}

const SELECT: &str = "SELECT id, kind, name, default_price_paise, gst_rate_bp, discount_eligible, is_active, sort_order FROM service";

fn from_row(r: &Row<'_>) -> rusqlite::Result<ServiceRow> {
    Ok(ServiceRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        name: r.get(2)?,
        default_price_paise: r.get(3)?,
        gst_rate_bp: r.get(4)?,
        discount_eligible: r.get(5)?,
        is_active: r.get(6)?,
        sort_order: r.get(7)?,
    })
}

/// Consultations first, then procedures, each in the admin's order.
pub fn list(conn: &Connection, include_inactive: bool) -> rusqlite::Result<Vec<ServiceRow>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE (?1 = 1 OR is_active = 1)
         ORDER BY kind = 'PROCEDURE', sort_order, name COLLATE NOCASE"
    ))?;
    stmt.query_map([include_inactive], from_row)?.collect()
}

pub fn find(conn: &Connection, id: i64) -> rusqlite::Result<Option<ServiceRow>> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row).optional()
}

/// Another entry of the same kind already uses this name (names are case-insensitive).
pub fn name_taken(conn: &Connection, kind: &str, name: &str, exclude: Option<i64>) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM service WHERE kind = ?1 AND name = ?2 AND (?3 IS NULL OR id <> ?3))",
        params![kind, name, exclude],
        |r| r.get(0),
    )
}

pub fn insert(conn: &Connection, s: &ServiceFields<'_>, now: i64) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO service (kind, name, default_price_paise, gst_rate_bp, discount_eligible, is_active, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![s.kind, s.name, s.default_price_paise, s.gst_rate_bp, s.discount_eligible, s.is_active, s.sort_order, now],
    )?;
    Ok(conn.last_insert_rowid())
}

/// The kind of an entry never changes (bills refer to it).
pub fn update(conn: &Connection, id: i64, s: &ServiceFields<'_>, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE service SET name = ?2, default_price_paise = ?3, gst_rate_bp = ?4, discount_eligible = ?5, is_active = ?6,
                            sort_order = ?7, updated_at = ?8
         WHERE id = ?1",
        params![id, s.name, s.default_price_paise, s.gst_rate_bp, s.discount_eligible, s.is_active, s.sort_order, now],
    )
}
