use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use super::inventory::like_pattern;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientRow {
    pub id: i64,
    pub client_code: String,
    pub full_name: String,
    pub phone: String,
    pub email: String,
    pub date_of_birth: Option<String>,
    pub gender: String,
    pub address: String,
    pub emergency_contact: String,
    pub notes: String,
    pub last_visit_at: Option<i64>,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct ClientFields<'a> {
    pub full_name: &'a str,
    pub phone: &'a str,
    pub email: &'a str,
    pub date_of_birth: Option<&'a str>,
    pub gender: &'a str,
    pub address: &'a str,
    pub emergency_contact: &'a str,
    pub notes: &'a str,
    pub is_active: bool,
}

const SELECT: &str = "SELECT id, client_code, full_name, phone, email, date_of_birth, gender, address, emergency_contact,
                             notes, last_visit_at, is_active, created_at, updated_at FROM client";

fn from_row(r: &Row<'_>) -> rusqlite::Result<ClientRow> {
    Ok(ClientRow {
        id: r.get(0)?,
        client_code: r.get(1)?,
        full_name: r.get(2)?,
        phone: r.get(3)?,
        email: r.get(4)?,
        date_of_birth: r.get(5)?,
        gender: r.get(6)?,
        address: r.get(7)?,
        emergency_contact: r.get(8)?,
        notes: r.get(9)?,
        last_visit_at: r.get(10)?,
        is_active: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
    })
}

pub fn insert(conn: &Connection, client_code: &str, c: &ClientFields<'_>, now: i64) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO client (client_code, full_name, phone, email, date_of_birth, gender, address, emergency_contact,
                             notes, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
        params![client_code, c.full_name, c.phone, c.email, c.date_of_birth, c.gender, c.address, c.emergency_contact, c.notes, c.is_active, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, c: &ClientFields<'_>, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE client SET full_name = ?2, phone = ?3, email = ?4, date_of_birth = ?5, gender = ?6, address = ?7,
                           emergency_contact = ?8, notes = ?9, is_active = ?10, updated_at = ?11
         WHERE id = ?1",
        params![id, c.full_name, c.phone, c.email, c.date_of_birth, c.gender, c.address, c.emergency_contact, c.notes, c.is_active, now],
    )
}

pub fn find(conn: &Connection, id: i64) -> rusqlite::Result<Option<ClientRow>> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row).optional()
}

/// By name, phone or client code; most recent visitors first. Empty text lists recent clients.
pub fn search(conn: &Connection, text: &str, include_inactive: bool, limit: u32) -> rusqlite::Result<Vec<ClientRow>> {
    let text = text.trim();
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE (?1 = '' OR full_name LIKE ?2 OR phone LIKE ?2 OR client_code LIKE ?2)
                    AND (?3 = 1 OR is_active = 1)
         ORDER BY last_visit_at IS NULL, last_visit_at DESC, full_name COLLATE NOCASE LIMIT ?4"
    ))?;
    stmt.query_map(params![text, like_pattern(text), include_inactive, i64::from(limit)], from_row)?.collect()
}

pub fn touch_visit(conn: &Connection, id: i64, now: i64) -> rusqlite::Result<usize> {
    conn.execute("UPDATE client SET last_visit_at = ?2 WHERE id = ?1", params![id, now])
}
