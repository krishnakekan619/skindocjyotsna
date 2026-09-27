use rusqlite::{Connection, OptionalExtension, Row, named_params, params};
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
    /// Set when this record was merged into another client (it is then inactive).
    pub merged_into_client_id: Option<i64>,
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

/// Duplicate-detection key: lower case, letters and digits only, single spaces.
/// `" Rahul  Sharma. "` and `"rahul sharma"` give the same key.
pub fn name_key(name: &str) -> String {
    let mut key = String::with_capacity(name.len());
    for word in name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        if !key.is_empty() {
            key.push(' ');
        }
        key.extend(word.chars().flat_map(char::to_lowercase));
    }
    key
}

/// The last 10 digits of a phone number: `"+91 98765-43210"` -> `"9876543210"`.
pub fn phone_digits(phone: &str) -> String {
    let digits: Vec<char> = phone.chars().filter(char::is_ascii_digit).collect();
    digits[digits.len().saturating_sub(10)..].iter().collect()
}

const SELECT: &str = "SELECT id, client_code, full_name, phone, email, date_of_birth, gender, address, emergency_contact,
                             notes, last_visit_at, is_active, created_at, updated_at, merged_into_client_id FROM client";

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
        merged_into_client_id: r.get(14)?,
    })
}

pub fn insert(conn: &Connection, client_code: &str, c: &ClientFields<'_>, now: i64) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO client (client_code, full_name, phone, email, date_of_birth, gender, address, emergency_contact,
                             notes, is_active, created_at, updated_at, name_key, phone_digits)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?12, ?13)",
        params![
            client_code, c.full_name, c.phone, c.email, c.date_of_birth, c.gender, c.address, c.emergency_contact, c.notes, c.is_active, now,
            name_key(c.full_name), phone_digits(c.phone)
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Updates a client that has not been merged away (merged records are kept as they were).
pub fn update(conn: &Connection, id: i64, c: &ClientFields<'_>, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE client SET full_name = ?2, phone = ?3, email = ?4, date_of_birth = ?5, gender = ?6, address = ?7,
                           emergency_contact = ?8, notes = ?9, is_active = ?10, updated_at = ?11, name_key = ?12,
                           phone_digits = ?13
         WHERE id = ?1 AND merged_into_client_id IS NULL",
        params![
            id, c.full_name, c.phone, c.email, c.date_of_birth, c.gender, c.address, c.emergency_contact, c.notes, c.is_active, now,
            name_key(c.full_name), phone_digits(c.phone)
        ],
    )
}

/// Fills `name_key` / `phone_digits` for clients saved before they existed (run at start-up,
/// after migrations; does nothing once every client has them).
pub fn fill_missing_keys(conn: &Connection) -> rusqlite::Result<()> {
    let rows: Vec<(i64, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, full_name, phone FROM client WHERE name_key = '' OR (phone <> '' AND phone_digits = '')")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?
    };
    if rows.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for (id, name, phone) in rows {
        tx.execute("UPDATE client SET name_key = ?2, phone_digits = ?3 WHERE id = ?1", params![id, name_key(&name), phone_digits(&phone)])?;
    }
    tx.commit()
}

pub fn find(conn: &Connection, id: i64) -> rusqlite::Result<Option<ClientRow>> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row).optional()
}

/// By name (normalized, so case, extra spaces and punctuation do not matter), phone digits or
/// client code; names starting with the text first, then recent visitors. Empty text lists the
/// most recent visitors. Merged-away records are never returned.
pub fn search(conn: &Connection, text: &str, include_inactive: bool, limit: u32) -> rusqlite::Result<Vec<ClientRow>> {
    let text = text.trim();
    let key = name_key(text);
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE merged_into_client_id IS NULL AND (:all = 1 OR is_active = 1)
            AND (:text = '' OR (:key <> '' AND name_key LIKE :key_like) OR client_code LIKE :code_like ESCAPE '!'
                 OR (length(:digits) >= 3 AND phone_digits LIKE :digits_like))
         ORDER BY (:key <> '' AND name_key LIKE :key_prefix) DESC, last_visit_at IS NULL, last_visit_at DESC,
                  full_name COLLATE NOCASE
         LIMIT :limit"
    ))?;
    stmt.query_map(
        named_params! {
            ":all": include_inactive, ":text": text, ":key": key, ":key_like": format!("%{key}%"),
            ":code_like": like_pattern(text), ":digits": digits, ":digits_like": format!("%{digits}%"),
            ":key_prefix": format!("{key}%"), ":limit": i64::from(limit),
        },
        from_row,
    )?
    .collect()
}

/// (id, name_key) of every active client, for typo-tolerant matching in the service layer.
pub fn active_name_keys(conn: &Connection) -> rusqlite::Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare("SELECT id, name_key FROM client WHERE merged_into_client_id IS NULL AND is_active = 1 AND name_key <> ''")?;
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
}

/// Active clients with exactly these phone digits (at least 6 digits), other than `exclude`.
pub fn with_phone_digits(conn: &Connection, digits: &str, exclude: Option<i64>) -> rusqlite::Result<Vec<ClientRow>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE phone_digits = ?1 AND length(?1) >= 6 AND merged_into_client_id IS NULL AND is_active = 1
                    AND (?2 IS NULL OR id <> ?2)
         ORDER BY last_visit_at IS NULL, last_visit_at DESC LIMIT 20"
    ))?;
    stmt.query_map(params![digits, exclude], from_row)?.collect()
}

/// Active clients with exactly this name key, other than `exclude`.
pub fn with_name_key(conn: &Connection, key: &str, exclude: Option<i64>) -> rusqlite::Result<Vec<ClientRow>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE name_key = ?1 AND ?1 <> '' AND merged_into_client_id IS NULL AND is_active = 1
                    AND (?2 IS NULL OR id <> ?2)
         ORDER BY last_visit_at IS NULL, last_visit_at DESC LIMIT 20"
    ))?;
    stmt.query_map(params![key, exclude], from_row)?.collect()
}

/// Phone numbers shared by more than one active client (possible duplicates).
pub fn shared_phone_digits(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT phone_digits FROM client WHERE merged_into_client_id IS NULL AND is_active = 1 AND length(phone_digits) >= 6
         GROUP BY phone_digits HAVING count(*) > 1 ORDER BY phone_digits LIMIT ?1",
    )?;
    stmt.query_map([i64::from(limit)], |r| r.get(0))?.collect()
}

/// Name keys shared by more than one active client (possible duplicates).
pub fn shared_name_keys(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name_key FROM client WHERE merged_into_client_id IS NULL AND is_active = 1 AND name_key <> ''
         GROUP BY name_key HAVING count(*) > 1 ORDER BY name_key LIMIT ?1",
    )?;
    stmt.query_map([i64::from(limit)], |r| r.get(0))?.collect()
}

/// Finalized bills and the amount kept (total minus refunds) for a client, bills finalized in
/// [from, to) when given. Computed over all bills, not only the ones listed on screen.
pub fn finalized_totals(conn: &Connection, client_id: i64, from: Option<i64>, to: Option<i64>) -> rusqlite::Result<(i64, i64)> {
    conn.query_row(
        "SELECT count(*), COALESCE(SUM(total_paise - returned_paise), 0) FROM bill
         WHERE client_id = ?1 AND status = 'FINALIZED' AND (?2 IS NULL OR finalized_at >= ?2) AND (?3 IS NULL OR finalized_at < ?3)",
        params![client_id, from, to],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

pub fn bill_count(conn: &Connection, client_id: i64) -> rusqlite::Result<i64> {
    conn.query_row("SELECT count(*) FROM bill WHERE client_id = ?1", [client_id], |r| r.get(0))
}

pub fn touch_visit(conn: &Connection, id: i64, now: i64) -> rusqlite::Result<usize> {
    conn.execute("UPDATE client SET last_visit_at = ?2 WHERE id = ?1", params![id, now])
}

/// Marks `secondary` as merged into `primary` (inactive, kept for the record). Must come
/// before `move_bills`: the database lets a bill change client only after this.
pub fn mark_merged(conn: &Connection, secondary: i64, primary: i64, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE client SET merged_into_client_id = ?2, merged_at = ?3, is_active = 0, updated_at = ?3
         WHERE id = ?1 AND merged_into_client_id IS NULL",
        params![secondary, primary, now],
    )
}

/// Moves every bill of `from` to `to` and returns the moved bill ids.
pub fn move_bills(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<i64>> {
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT id FROM bill WHERE client_id = ?1 ORDER BY id")?;
        stmt.query_map([from], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
    };
    conn.execute("UPDATE bill SET client_id = ?2 WHERE client_id = ?1", params![from, to])?;
    Ok(ids)
}

/// The primary's last visit becomes the later of the two records' last visits.
pub fn keep_latest_visit(conn: &Connection, primary: i64, secondary: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE client SET last_visit_at = (SELECT MAX(last_visit_at) FROM client WHERE id IN (?1, ?2)) WHERE id = ?1",
        params![primary, secondary],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_phones_are_normalized_for_duplicate_checks() {
        assert_eq!(name_key(" Rahul  Sharma. "), "rahul sharma");
        assert_eq!(name_key("rahul sharma"), "rahul sharma");
        assert_eq!(name_key("D'Souza-Pinto, Anne"), "d souza pinto anne");
        assert_eq!(name_key("  ...  "), "");
        assert_eq!(phone_digits("+91 98765-43210"), "9876543210");
        assert_eq!(phone_digits("020 1234"), "0201234");
        assert_eq!(phone_digits(""), "");
    }
}
