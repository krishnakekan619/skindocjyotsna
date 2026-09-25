//! Backup and restore.
//!
//! A backup is one `.clinicbak` file: a consistent SQLite snapshot taken with SQLite's online
//! backup API while the app keeps running, plus a small `backup_meta` table describing it.
//! The file format is identical on Windows and macOS, so a backup made on one restores on the other.
//!
//! Restore never destroys data: the current database is backed up first (`pre-restore`), and
//! the replaced file is kept next to the live database until an administrator removes it.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::{Connection, MAIN_DB, OpenFlags, OptionalExtension, params};

use crate::timestamp::utc_stamp;
use crate::{Database, DbError, supported_schema_version, verify_integrity};

pub const BACKUP_EXTENSION: &str = "clinicbak";
const BACKUP_FORMAT: &str = "skindocjyotsna-backup-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKind {
    Manual,
    Automatic,
    PreRestore,
}

impl BackupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BackupKind::Manual => "manual",
            BackupKind::Automatic => "auto",
            BackupKind::PreRestore => "pre-restore",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "manual" => Some(BackupKind::Manual),
            "auto" => Some(BackupKind::Automatic),
            "pre-restore" => Some(BackupKind::PreRestore),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupMeta {
    pub kind: BackupKind,
    pub created_utc: String,
    pub app_version: String,
    pub schema_version: i64,
}

/// A `.clinicbak` file found in a backup folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupFile {
    pub file_name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    /// `None` when the file could not be read; `problem` then says why.
    pub meta: Option<BackupMeta>,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOutcome {
    pub restored_from: BackupFile,
    /// Backup of the database as it was just before the restore.
    pub safety_backup: BackupFile,
    /// The replaced database file, kept for safety.
    pub replaced_file: PathBuf,
}

impl Database {
    /// Writes a verified snapshot of the database into `dir` and returns it.
    pub fn create_backup(&self, dir: &Path, kind: BackupKind, app_version: &str) -> Result<BackupFile, DbError> {
        fs::create_dir_all(dir)?;
        let stamp = utc_stamp(SystemTime::now());
        let final_path = unique_path(dir, &format!("SkinDocJyotsna-{}-{}", stamp.compact, kind.as_str()));
        let file_name = file_name_of(&final_path);
        let tmp_path = dir.join(format!(".{file_name}.tmp"));
        remove_if_exists(&tmp_path)?;

        let meta = BackupMeta {
            kind,
            created_utc: stamp.iso,
            app_version: app_version.to_string(),
            schema_version: self.conn.query_row("PRAGMA user_version", [], |row| row.get(0))?,
        };
        if let Err(error) = self.write_snapshot(&tmp_path, &meta) {
            let _ = fs::remove_file(&tmp_path);
            return Err(error);
        }
        // Rename only after the snapshot is complete and verified: a backup file either
        // exists in full or not at all, even if the PC loses power mid-backup.
        fs::rename(&tmp_path, &final_path)?;
        Ok(BackupFile { file_name, size_bytes: fs::metadata(&final_path)?.len(), path: final_path, meta: Some(meta), problem: None })
    }

    fn write_snapshot(&self, tmp_path: &Path, meta: &BackupMeta) -> Result<(), DbError> {
        self.conn.backup(MAIN_DB, tmp_path, None)?;
        let snapshot = Connection::open(tmp_path)?;
        // The source runs in WAL mode; store the backup as a single self-contained file.
        let _: String = snapshot.pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))?;
        snapshot.execute_batch(
            "CREATE TABLE backup_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL) STRICT;",
        )?;
        let schema_version = meta.schema_version.to_string();
        for (key, value) in [
            ("format", BACKUP_FORMAT),
            ("kind", meta.kind.as_str()),
            ("created_utc", meta.created_utc.as_str()),
            ("app_version", meta.app_version.as_str()),
            ("schema_version", schema_version.as_str()),
        ] {
            snapshot.execute("INSERT INTO backup_meta (key, value) VALUES (?1, ?2)", params![key, value])?;
        }
        verify_integrity(&snapshot)?;
        snapshot.close().map_err(|(_, error)| DbError::from(error))
    }

    /// Replaces the live database with `backup`. See the module docs for the safety steps.
    pub fn restore_backup(&mut self, backup: &Path, backup_dir: &Path, app_version: &str) -> Result<RestoreOutcome, DbError> {
        let live = self.path.clone().ok_or(DbError::NotAFileDatabase)?;
        let restored_from = validate_backup(backup)?;
        let safety_backup = self.create_backup(backup_dir, BackupKind::PreRestore, app_version)?;

        // Stage a copy next to the live file (same folder, so the final rename is atomic).
        let staged = sibling(&live, "restoring");
        remove_if_exists(&staged)?;
        fs::copy(backup, &staged)?;
        let stage_result = (|| -> Result<(), DbError> {
            let conn = Connection::open(&staged)?;
            conn.execute_batch("DROP TABLE backup_meta;")?;
            conn.close().map_err(|(_, error)| DbError::from(error))
        })();
        if let Err(error) = stage_result {
            let _ = fs::remove_file(&staged);
            return Err(error);
        }

        // Close the live connection: Windows cannot replace a file that is open.
        let old = std::mem::replace(&mut self.conn, Connection::open_in_memory()?);
        if let Err((conn, error)) = old.close() {
            self.conn = conn;
            let _ = fs::remove_file(&staged);
            return Err(error.into());
        }

        let replaced = sibling(&live, &format!("replaced-{}", utc_stamp(SystemTime::now()).compact));
        match swap_in(&live, &staged, &replaced) {
            Ok(db) => {
                *self = db;
                Ok(RestoreOutcome { restored_from, safety_backup, replaced_file: replaced })
            }
            Err(error) => {
                // swap_in puts the original back; reopen it and report the failure. Never call
                // `open` on a missing file: that would silently create a new, empty database.
                let _ = fs::remove_file(&staged);
                if !live.exists() && replaced.exists() {
                    let _ = move_with_sidecars(&replaced, &live);
                }
                if live.exists() {
                    *self = Database::open(&live)?;
                }
                Err(error)
            }
        }
    }
}

/// Moves `live` aside to `replaced`, moves `staged` into place and opens it (running any
/// migrations for older backups). On any failure the original file is put back.
fn swap_in(live: &Path, staged: &Path, replaced: &Path) -> Result<Database, DbError> {
    move_with_sidecars(live, replaced)?;
    if let Err(error) = fs::rename(staged, live) {
        move_with_sidecars(replaced, live)?;
        return Err(error.into());
    }
    match Database::open(live).and_then(|db| db.verify_integrity().map(|()| db)) {
        Ok(db) => Ok(db),
        Err(error) => {
            move_with_sidecars(live, &sibling(live, "failed-restore"))?;
            move_with_sidecars(replaced, live)?;
            Err(error)
        }
    }
}

/// Fully checks a backup file without changing it: it must be a SkinDocJyotsna backup,
/// pass SQLite's integrity check, and not come from a newer app version.
pub fn validate_backup(path: &Path) -> Result<BackupFile, DbError> {
    let file = read_backup_file(path);
    if let Some(problem) = &file.problem {
        return Err(DbError::InvalidBackup(problem.clone()));
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    verify_integrity(&conn).map_err(|error| DbError::InvalidBackup(error.to_string()))?;
    let backup_version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if backup_version > supported_schema_version() {
        return Err(DbError::BackupFromNewerVersion { backup: backup_version, supported: supported_schema_version() });
    }
    Ok(file)
}

/// Lists backups in `dir`, newest first. Unreadable files are listed with a `problem`
/// rather than hidden, so staff can see that something is wrong.
pub fn list_backups(dir: &Path) -> Result<Vec<BackupFile>, DbError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let is_backup = path.extension().is_some_and(|ext| ext == BACKUP_EXTENSION);
        let hidden = path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.'));
        if path.is_file() && is_backup && !hidden {
            files.push(read_backup_file(&path));
        }
    }
    files.sort_by(|a, b| b.file_name.cmp(&a.file_name)); // names start with a UTC timestamp
    Ok(files)
}

fn read_backup_file(path: &Path) -> BackupFile {
    let file_name = file_name_of(path);
    let size_bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    match read_meta(path) {
        Ok(meta) => BackupFile { file_name, path: path.to_path_buf(), size_bytes, meta: Some(meta), problem: None },
        Err(problem) => BackupFile { file_name, path: path.to_path_buf(), size_bytes, meta: None, problem: Some(problem) },
    }
}

fn read_meta(path: &Path) -> Result<BackupMeta, String> {
    let not_ours = || "this file is not a SkinDocJyotsna backup".to_string();
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|_| not_ours())?;
    let get = |key: &str| -> Result<String, String> {
        conn.query_row("SELECT value FROM backup_meta WHERE key = ?1", [key], |row| row.get::<_, String>(0))
            .optional()
            .map_err(|_| not_ours())?
            .ok_or_else(|| format!("backup information '{key}' is missing"))
    };
    if get("format")? != BACKUP_FORMAT {
        return Err("unsupported backup format".to_string());
    }
    let kind = get("kind")?;
    Ok(BackupMeta {
        kind: BackupKind::parse(&kind).ok_or_else(|| format!("unknown backup kind '{kind}'"))?,
        created_utc: get("created_utc")?,
        app_version: get("app_version")?,
        schema_version: get("schema_version")?.parse().map_err(|_| "invalid schema version".to_string())?,
    })
}

fn file_name_of(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `<dir>/<stem>.clinicbak`, or `<stem>-2.clinicbak`, ... if that name is taken.
fn unique_path(dir: &Path, stem: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{BACKUP_EXTENSION}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{n}.{BACKUP_EXTENSION}"));
        n += 1;
    }
    candidate
}

/// `clinic.db` + `suffix` -> `clinic.db.<suffix>` in the same folder.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".{suffix}"));
    path.with_file_name(name)
}

fn remove_if_exists(path: &Path) -> Result<(), DbError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Renames a database file together with its `-wal` / `-shm` companions, if present.
fn move_with_sidecars(from: &Path, to: &Path) -> Result<(), DbError> {
    fs::rename(from, to)?;
    for suffix in ["-wal", "-shm"] {
        let mut from_side = from.as_os_str().to_os_string();
        from_side.push(suffix);
        let from_side = PathBuf::from(from_side);
        if from_side.exists() {
            let mut to_side = to.as_os_str().to_os_string();
            to_side.push(suffix);
            fs::rename(&from_side, PathBuf::from(to_side))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    fn set_clinic_name(db: &Database, name: &str) -> Result<(), DbError> {
        db.conn.execute("UPDATE app_setting SET value_json = ?1 WHERE key = 'clinic.name'", [format!("\"{name}\"")])?;
        Ok(())
    }

    fn clinic_name(db: &Database) -> Result<String, DbError> {
        Ok(db.conn.query_row("SELECT value_json FROM app_setting WHERE key = 'clinic.name'", [], |row| row.get(0))?)
    }

    #[test]
    fn backup_is_verified_listed_and_valid() -> Result<(), DbError> {
        let dir = TempDir::new("backup-list");
        let db = Database::open(&dir.0.join("clinic.db"))?;
        let backups = dir.0.join("backups");

        let first = db.create_backup(&backups, BackupKind::Manual, "0.1.0")?;
        let second = db.create_backup(&backups, BackupKind::Automatic, "0.1.0")?;
        assert_ne!(first.file_name, second.file_name);
        assert!(first.file_name.ends_with(".clinicbak"));

        let listed = list_backups(&backups)?;
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|b| b.problem.is_none()));

        let valid = validate_backup(&first.path)?;
        assert_eq!(valid.meta.map(|m| m.kind), Some(BackupKind::Manual));
        Ok(())
    }

    #[test]
    fn restore_brings_back_old_data_and_keeps_a_safety_copy() -> Result<(), DbError> {
        let dir = TempDir::new("restore");
        let backups = dir.0.join("backups");
        let mut db = Database::open(&dir.0.join("clinic.db"))?;

        set_clinic_name(&db, "Before")?;
        let backup = db.create_backup(&backups, BackupKind::Manual, "0.1.0")?;
        set_clinic_name(&db, "After")?;

        let outcome = db.restore_backup(&backup.path, &backups, "0.1.0")?;
        assert_eq!(clinic_name(&db)?, "\"Before\"");
        assert!(outcome.replaced_file.exists(), "replaced database must be kept");
        assert_eq!(outcome.safety_backup.meta.as_ref().map(|m| m.kind), Some(BackupKind::PreRestore));

        // The safety backup holds the data from just before the restore.
        db.restore_backup(&outcome.safety_backup.path, &backups, "0.1.0")?;
        assert_eq!(clinic_name(&db)?, "\"After\"");

        // backup_meta must not leak into the live schema.
        let leaked: i64 =
            db.conn.query_row("SELECT count(*) FROM sqlite_master WHERE name = 'backup_meta'", [], |row| row.get(0))?;
        assert_eq!(leaked, 0);
        Ok(())
    }

    #[test]
    fn corrupt_or_foreign_files_are_rejected_and_live_data_is_untouched() -> Result<(), DbError> {
        let dir = TempDir::new("corrupt");
        let backups = dir.0.join("backups");
        let mut db = Database::open(&dir.0.join("clinic.db"))?;
        set_clinic_name(&db, "Live")?;
        fs::create_dir_all(&backups)?;

        let garbage = backups.join("garbage.clinicbak");
        fs::write(&garbage, b"this is not a database")?;
        assert!(matches!(db.restore_backup(&garbage, &backups, "0.1.0"), Err(DbError::InvalidBackup(_))));

        let foreign = backups.join("foreign.clinicbak");
        Connection::open(&foreign)?.execute_batch("CREATE TABLE t (x INTEGER);")?;
        assert!(matches!(db.restore_backup(&foreign, &backups, "0.1.0"), Err(DbError::InvalidBackup(_))));

        assert_eq!(clinic_name(&db)?, "\"Live\"");
        let listed = list_backups(&backups)?;
        assert_eq!(listed.iter().filter(|b| b.problem.is_some()).count(), 2, "bad files are listed with a problem");
        Ok(())
    }

    #[test]
    fn backups_from_a_newer_app_are_refused() -> Result<(), DbError> {
        let dir = TempDir::new("newer");
        let backups = dir.0.join("backups");
        let db = Database::open(&dir.0.join("clinic.db"))?;
        let backup = db.create_backup(&backups, BackupKind::Manual, "0.1.0")?;
        Connection::open(&backup.path)?.pragma_update(None, "user_version", supported_schema_version() + 1)?;
        assert!(matches!(validate_backup(&backup.path), Err(DbError::BackupFromNewerVersion { .. })));
        Ok(())
    }
}
