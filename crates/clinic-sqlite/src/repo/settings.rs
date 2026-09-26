use rusqlite::{Connection, OptionalExtension, params};

/// The stored JSON text for `key`, if set.
pub fn get(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value_json FROM app_setting WHERE key = ?1", [key], |row| row.get(0))
        .optional()
}

pub fn set(conn: &Connection, key: &str, value_json: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO app_setting (key, value_json, updated_at)
         VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![key, value_json],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, DbError};

    #[test]
    fn set_inserts_then_updates() -> Result<(), DbError> {
        let mut db = Database::open_in_memory()?;
        assert_eq!(db.read(|c| get(c, "x.test"))?, None);
        db.write(|c| set(c, "x.test", "1"))?;
        db.write(|c| set(c, "x.test", "2"))?;
        assert_eq!(db.read(|c| get(c, "x.test"))?.as_deref(), Some("2"));
        Ok(())
    }
}
