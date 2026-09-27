//! SQLite persistence for SkinDocJyotsna.
//!
//! Opening the database creates the file if needed, applies the connection settings that
//! protect against crashes and power loss, and runs any pending schema migrations.

mod backup;
pub mod repo;
mod timestamp;

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};
use rusqlite_migration::{M, Migrations};

/// Re-exported so higher layers use exactly this crate's rusqlite version.
pub use rusqlite;

pub use backup::{
    BACKUP_EXTENSION, BackupFile, BackupKind, BackupMeta, RestoreOutcome, latest_backup_time, list_backups, prune_backups,
    validate_backup,
};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("schema migration failed: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database setting '{setting}' is '{actual}', expected '{expected}'")]
    UnexpectedSetting { setting: &'static str, expected: &'static str, actual: String },
    #[error("integrity check failed: {0:?}")]
    IntegrityFailed(Vec<String>),
    #[error("not a valid backup: {0}")]
    InvalidBackup(String),
    #[error("backup has schema v{backup}, but this app only supports up to v{supported}; update the app first")]
    BackupFromNewerVersion { backup: i64, supported: i64 },
    #[error("operation needs a file-based database")]
    NotAFileDatabase,
}

const MIGRATIONS: &[M<'static>] = &[
    M::up(include_str!("../migrations/0001_init.sql")),
    M::up(include_str!("../migrations/0002_users_audit.sql")),
    M::up(include_str!("../migrations/0003_clinic.sql")),
    M::up(include_str!("../migrations/0004_bill_guards.sql")),
    M::up(include_str!("../migrations/0005_services_clients.sql")),
    M::up(include_str!("../migrations/0006_product_types.sql")),
];

fn migrations() -> Migrations<'static> {
    Migrations::from_slice(MIGRATIONS)
}

/// Newest schema version this build understands.
pub fn supported_schema_version() -> i64 {
    MIGRATIONS.len() as i64
}

/// Snapshot of database health, shown on the System screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbStatus {
    pub sqlite_version: String,
    pub schema_version: i64,
    pub journal_mode: String,
    pub integrity_ok: bool,
    pub fts5_available: bool,
}

pub struct Database {
    conn: Connection,
    /// `None` for in-memory databases.
    path: Option<PathBuf>,
}

impl Database {
    /// Opens (or creates) the database file and brings the schema up to date.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let mut conn = Connection::open(path)?;
        configure_file_connection(&conn)?;
        migrations().to_latest(&mut conn)?;
        repo::clients::fill_missing_keys(&conn)?;
        Ok(Self { conn, path: Some(path.to_path_buf()) })
    }

    /// Like `open`, but if this app version brings schema changes for an existing database, a
    /// `pre-upgrade` backup is written to `backup_dir` first (returned), so an upgrade can always
    /// be undone by restoring it. Client search keys are filled in afterwards.
    pub fn open_with_upgrade_backup(path: &Path, backup_dir: &Path, app_version: &str) -> Result<(Self, Option<BackupFile>), DbError> {
        let conn = Connection::open(path)?;
        configure_file_connection(&conn)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let mut db = Self { conn, path: Some(path.to_path_buf()) };
        let backup = if version > 0 && version < supported_schema_version() {
            Some(db.create_backup(backup_dir, BackupKind::PreUpgrade, app_version)?)
        } else {
            None
        };
        migrations().to_latest(&mut db.conn)?;
        repo::clients::fill_missing_keys(&db.conn)?;
        Ok((db, backup))
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> Result<Self, DbError> {
        let mut conn = Connection::open_in_memory()?;
        configure_common(&conn)?;
        migrations().to_latest(&mut conn)?;
        Ok(Self { conn, path: None })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn status(&self) -> Result<DbStatus, DbError> {
        let conn = &self.conn;
        Ok(DbStatus {
            sqlite_version: rusqlite::version().to_string(),
            schema_version: conn.query_row("PRAGMA user_version", [], |row| row.get(0))?,
            journal_mode: conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?,
            integrity_ok: conn.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))? == "ok",
            fts5_available: conn.query_row("SELECT sqlite_compileoption_used('ENABLE_FTS5')", [], |row| row.get(0))?,
        })
    }

    /// Full `PRAGMA integrity_check` (slower than the quick check in `status`).
    pub fn verify_integrity(&self) -> Result<(), DbError> {
        verify_integrity(&self.conn)
    }

    /// Runs read-only work on the connection.
    pub fn read<T, E>(&self, work: impl FnOnce(&Connection) -> Result<T, E>) -> Result<T, E> {
        work(&self.conn)
    }

    /// Runs `work` in ONE transaction: everything it writes is committed together, or, if it
    /// returns an error (or the app crashes), nothing is. `BEGIN IMMEDIATE` takes the write lock
    /// up front, so no other write can interleave.
    pub fn write<T, E: From<rusqlite::Error>>(&mut self, work: impl FnOnce(&Connection) -> Result<T, E>) -> Result<T, E> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = work(&tx)?;
        tx.commit()?;
        Ok(value)
    }
}

pub(crate) fn verify_integrity(conn: &Connection) -> Result<(), DbError> {
    let mut stmt = conn.prepare("PRAGMA integrity_check")?;
    let problems: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;
    if problems.len() == 1 && problems[0] == "ok" { Ok(()) } else { Err(DbError::IntegrityFailed(problems)) }
}

/// Settings for the on-disk database. WAL + synchronous=FULL means a committed bill
/// survives a crash or power cut, and a half-finished one is rolled back automatically.
fn configure_file_connection(conn: &Connection) -> Result<(), DbError> {
    let mode: String = conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(DbError::UnexpectedSetting { setting: "journal_mode", expected: "wal", actual: mode });
    }
    conn.pragma_update(None, "synchronous", "FULL")?;
    configure_common(conn)
}

fn configure_common(conn: &Connection) -> Result<(), DbError> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "trusted_schema", "OFF")?;
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;

    /// A unique, empty temp folder per test; removed when dropped.
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("clinic-sqlite-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("create temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::TempDir;
    use super::*;

    #[test]
    fn an_upgrade_saves_a_backup_of_the_old_database_first() -> Result<(), DbError> {
        let dir = TempDir::new("upgrade");
        let path = dir.0.join("clinic.db");
        let backups = dir.0.join("backups");
        {
            let mut conn = Connection::open(&path)?;
            migrations().to_version(&mut conn, 4)?;
        }
        let (db, backup) = Database::open_with_upgrade_backup(&path, &backups, "0.3.0")?;
        assert_eq!(db.status()?.schema_version, supported_schema_version());
        let meta = backup.and_then(|b| b.meta).ok_or(DbError::NotAFileDatabase)?;
        assert_eq!((meta.kind, meta.schema_version), (BackupKind::PreUpgrade, 4));
        drop(db);
        let (_, again) = Database::open_with_upgrade_backup(&path, &backups, "0.3.0")?;
        assert!(again.is_none(), "no backup when the schema is already current");
        Ok(())
    }

    #[test]
    fn migrations_are_valid() {
        assert!(migrations().validate().is_ok());
    }

    #[test]
    fn in_memory_database_is_migrated_and_healthy() -> Result<(), DbError> {
        let db = Database::open_in_memory()?;
        let status = db.status()?;
        assert_eq!(status.schema_version, supported_schema_version());
        assert!(status.integrity_ok);
        assert!(status.fts5_available, "bundled SQLite must include FTS5 for fast search");
        db.verify_integrity()
    }

    #[test]
    fn file_database_uses_wal_and_survives_reopen() -> Result<(), DbError> {
        let dir = TempDir::new("reopen");
        let path = dir.0.join("clinic.db");

        let first = Database::open(&path)?.status()?;
        assert_eq!(first.journal_mode.to_lowercase(), "wal");

        let reopened = Database::open(&path)?.status()?; // migrations must not re-run
        assert_eq!(reopened.schema_version, first.schema_version);
        Ok(())
    }

    #[test]
    fn fts5_prefix_search_works() -> Result<(), DbError> {
        let db = Database::open_in_memory()?;
        db.conn.execute_batch(
            "CREATE VIRTUAL TABLE product_search USING fts5(name, generic_name);
             INSERT INTO product_search VALUES ('Paracetamol 500mg', 'acetaminophen'),
                                               ('Pain Relief Gel', 'diclofenac'),
                                               ('Amoxicillin 500mg', 'amoxicillin');",
        )?;
        let mut stmt = db.conn.prepare("SELECT name FROM product_search WHERE product_search MATCH ? ORDER BY rank")?;
        let hits: Vec<String> = stmt.query_map(["pa*"], |row| row.get(0))?.collect::<Result<_, _>>()?;
        assert_eq!(hits.len(), 2, "prefix 'pa' should match Paracetamol and Pain Relief Gel: {hits:?}");
        Ok(())
    }
}
