use rusqlite::{Connection, params};

pub struct NewAuditEntry<'a> {
    pub occurred_at: i64,
    pub user_id: Option<i64>,
    pub username: Option<&'a str>,
    pub action: &'a str,
    pub entity_type: Option<&'a str>,
    pub entity_id: Option<&'a str>,
    pub details_json: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub id: i64,
    pub occurred_at: i64,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub action: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub details_json: Option<String>,
}

pub fn append(conn: &Connection, entry: &NewAuditEntry<'_>) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO audit_log (occurred_at, user_id, username, action, entity_type, entity_id, details_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            entry.occurred_at,
            entry.user_id,
            entry.username,
            entry.action,
            entry.entity_type,
            entry.entity_id,
            entry.details_json
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Newest first; pass the smallest `id` of the previous page as `before_id` for the next page.
pub fn list(conn: &Connection, limit: u32, before_id: Option<i64>) -> rusqlite::Result<Vec<AuditRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, occurred_at, user_id, username, action, entity_type, entity_id, details_json
         FROM audit_log WHERE (?1 IS NULL OR id < ?1) ORDER BY id DESC LIMIT ?2",
    )?;
    stmt.query_map(params![before_id, i64::from(limit)], |row| {
        Ok(AuditRecord {
            id: row.get(0)?,
            occurred_at: row.get(1)?,
            user_id: row.get(2)?,
            username: row.get(3)?,
            action: row.get(4)?,
            entity_type: row.get(5)?,
            entity_id: row.get(6)?,
            details_json: row.get(7)?,
        })
    })?
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, DbError};

    fn entry(action: &str) -> NewAuditEntry<'_> {
        NewAuditEntry { occurred_at: 1, user_id: None, username: Some("tester"), action, entity_type: None, entity_id: None, details_json: None }
    }

    #[test]
    fn entries_page_newest_first() -> Result<(), DbError> {
        let mut db = Database::open_in_memory()?;
        for action in ["A", "B", "C"] {
            db.write(|c| append(c, &entry(action)))?;
        }
        let page = db.read(|c| list(c, 2, None))?;
        assert_eq!(page.iter().map(|e| e.action.as_str()).collect::<Vec<_>>(), ["C", "B"]);
        let next = db.read(|c| list(c, 2, Some(page[1].id)))?;
        assert_eq!(next.iter().map(|e| e.action.as_str()).collect::<Vec<_>>(), ["A"]);
        Ok(())
    }

    #[test]
    fn the_audit_log_cannot_be_edited_or_deleted() -> Result<(), DbError> {
        let mut db = Database::open_in_memory()?;
        db.write(|c| append(c, &entry("LOGIN")))?;
        let edit: Result<usize, DbError> = db.write(|c| Ok(c.execute("UPDATE audit_log SET action = 'X'", [])?));
        let delete: Result<usize, DbError> = db.write(|c| Ok(c.execute("DELETE FROM audit_log", [])?));
        assert!(edit.is_err() && delete.is_err(), "triggers must block changes");
        assert_eq!(db.read(|c| list(c, 10, None))?[0].action, "LOGIN");
        Ok(())
    }
}
