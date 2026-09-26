use rusqlite::{Connection, OptionalExtension, Row, params};

/// A row of `app_user`. `role` is the stored text ('ADMIN' / 'RECEPTIONIST').
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRecord {
    pub id: i64,
    pub username: String,
    pub full_name: String,
    pub role: String,
    pub password_hash: String,
    pub pin_hash: Option<String>,
    pub is_active: bool,
    pub failed_login_count: i64,
    pub locked_until: Option<i64>,
    pub pin_failed_count: i64,
    pub last_login_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct NewUser<'a> {
    pub username: &'a str,
    pub full_name: &'a str,
    pub role: &'a str,
    pub password_hash: &'a str,
    pub pin_hash: Option<&'a str>,
    pub now: i64,
}

const COLUMNS: &str = "id, username, full_name, role, password_hash, pin_hash, is_active, failed_login_count, \
                       locked_until, pin_failed_count, last_login_at, created_at, updated_at";

fn from_row(row: &Row<'_>) -> rusqlite::Result<UserRecord> {
    Ok(UserRecord {
        id: row.get(0)?,
        username: row.get(1)?,
        full_name: row.get(2)?,
        role: row.get(3)?,
        password_hash: row.get(4)?,
        pin_hash: row.get(5)?,
        is_active: row.get(6)?,
        failed_login_count: row.get(7)?,
        locked_until: row.get(8)?,
        pin_failed_count: row.get(9)?,
        last_login_at: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

pub fn insert(conn: &Connection, user: &NewUser<'_>) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO app_user (username, full_name, role, password_hash, pin_hash, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![user.username, user.full_name, user.role, user.password_hash, user.pin_hash, user.now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT count(*) FROM app_user", [], |row| row.get(0))
}

pub fn count_active_admins(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT count(*) FROM app_user WHERE role = 'ADMIN' AND is_active = 1", [], |row| row.get(0))
}

/// Case-insensitive (the column is `COLLATE NOCASE`).
pub fn find_by_username(conn: &Connection, username: &str) -> rusqlite::Result<Option<UserRecord>> {
    conn.query_row(&format!("SELECT {COLUMNS} FROM app_user WHERE username = ?1"), [username], from_row)
        .optional()
}

pub fn find_by_id(conn: &Connection, id: i64) -> rusqlite::Result<Option<UserRecord>> {
    conn.query_row(&format!("SELECT {COLUMNS} FROM app_user WHERE id = ?1"), [id], from_row).optional()
}

/// Active users first, then admins, then by name.
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<UserRecord>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM app_user ORDER BY is_active DESC, role, full_name COLLATE NOCASE"
    ))?;
    stmt.query_map([], from_row)?.collect()
}

pub fn update_profile(conn: &Connection, id: i64, full_name: &str, role: &str, is_active: bool, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE app_user SET full_name = ?2, role = ?3, is_active = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, full_name, role, is_active, now],
    )?;
    Ok(())
}

/// New password; also clears any login lockout and PIN failures.
pub fn set_password(conn: &Connection, id: i64, password_hash: &str, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE app_user SET password_hash = ?2, failed_login_count = 0, locked_until = NULL,
                             pin_failed_count = 0, updated_at = ?3
         WHERE id = ?1",
        params![id, password_hash, now],
    )?;
    Ok(())
}

/// Sets (or with `None` removes) the unlock PIN; resets PIN failures.
pub fn set_pin(conn: &Connection, id: i64, pin_hash: Option<&str>, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE app_user SET pin_hash = ?2, pin_failed_count = 0, updated_at = ?3 WHERE id = ?1",
        params![id, pin_hash, now],
    )?;
    Ok(())
}

pub fn record_login_failure(conn: &Connection, id: i64, failures: i64, locked_until: Option<i64>, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE app_user SET failed_login_count = ?2, locked_until = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, failures, locked_until, now],
    )?;
    Ok(())
}

/// A successful password login also re-enables PIN unlock (DEC-023).
pub fn record_login_success(conn: &Connection, id: i64, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE app_user SET failed_login_count = 0, locked_until = NULL, pin_failed_count = 0,
                             last_login_at = ?2, updated_at = ?2
         WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

pub fn record_pin_failure(conn: &Connection, id: i64, failures: i64, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE app_user SET pin_failed_count = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, failures, now],
    )?;
    Ok(())
}

pub fn reset_pin_failures(conn: &Connection, id: i64, now: i64) -> rusqlite::Result<()> {
    conn.execute("UPDATE app_user SET pin_failed_count = 0, updated_at = ?2 WHERE id = ?1", params![id, now])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, DbError};

    fn new_user<'a>(username: &'a str, role: &'a str) -> NewUser<'a> {
        NewUser { username, full_name: "Test User", role, password_hash: "$argon2id$x", pin_hash: None, now: 100 }
    }

    #[test]
    fn insert_find_and_count() -> Result<(), DbError> {
        let mut db = Database::open_in_memory()?;
        let id = db.write(|c| insert(c, &new_user("priya", "RECEPTIONIST")))?;
        db.write(|c| insert(c, &new_user("owner", "ADMIN")))?;
        db.read(|c| {
            assert_eq!(count(c)?, 2);
            assert_eq!(count_active_admins(c)?, 1);
            let found = find_by_username(c, "PRIYA")?.expect("case-insensitive lookup");
            assert_eq!(found.id, id);
            assert!(found.is_active);
            assert_eq!(list(c)?.len(), 2);
            Ok::<_, DbError>(())
        })
    }

    #[test]
    fn usernames_are_unique_ignoring_case() -> Result<(), DbError> {
        let mut db = Database::open_in_memory()?;
        db.write(|c| insert(c, &new_user("priya", "RECEPTIONIST")))?;
        let duplicate = db.write(|c| insert(c, &new_user("Priya", "RECEPTIONIST")));
        assert!(duplicate.is_err());
        Ok(())
    }

    #[test]
    fn the_database_rejects_unknown_roles() -> Result<(), DbError> {
        let mut db = Database::open_in_memory()?;
        assert!(db.write(|c| insert(c, &new_user("someone", "SUPERUSER"))).is_err());
        Ok(())
    }

    #[test]
    fn a_failed_write_is_rolled_back() -> Result<(), DbError> {
        let mut db = Database::open_in_memory()?;
        let result: Result<(), DbError> = db.write(|c| {
            insert(c, &new_user("first", "ADMIN"))?;
            insert(c, &new_user("first", "ADMIN"))?; // duplicate: fails
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(db.read(|c| count(c))?, 0, "the first insert must be rolled back too");
        Ok(())
    }
}
