//! Backups, restore and health checks (Phase 8), with audit entries.

use std::path::Path;

use clinic_core::auth::Permission;
use clinic_sqlite::repo::inventory::ledger_mismatches;
use clinic_sqlite::{BackupFile, BackupKind, Database, RestoreOutcome, latest_backup_time, list_backups, prune_backups};
use serde::Serialize;
use serde_json::json;

use crate::audit::{self, Actor};
use crate::auth::verify_actor;
use crate::{ServiceError, Session};

/// Automatic backups kept (older ones are removed; manual and pre-restore backups never are).
pub const AUTO_BACKUPS_KEPT: usize = 14;
/// An automatic backup is due when the last one is older than this (daily, D22).
pub const AUTO_BACKUP_INTERVAL_SECS: i64 = 23 * 3600 + 30 * 60;
/// When the app is closed, back up if the last automatic backup is older than this (D22).
pub const EXIT_BACKUP_INTERVAL_SECS: i64 = 12 * 3600;

pub fn backups(actor: &Session, dir: &Path) -> Result<Vec<BackupFile>, ServiceError> {
    actor.require(Permission::ManageBackups)?;
    Ok(list_backups(dir)?)
}

pub fn backup_now(db: &mut Database, actor: &Session, dir: &Path, app_version: &str, now: i64) -> Result<BackupFile, ServiceError> {
    actor.require(Permission::ManageBackups)?;
    db.read(|c| verify_actor(c, actor))?;
    ensure_database_loaded(db)?;
    let file = db.create_backup(dir, BackupKind::Manual, app_version)?;
    db.write(|c| audit::record(c, now, Actor::from(actor), "BACKUP_CREATE", None, Some(json!({ "file": file.file_name }))))?;
    Ok(file)
}

/// Replaces all data with the backup. The caller must sign everyone out afterwards: the restored
/// data may have different accounts.
pub fn restore(db: &mut Database, actor: &Session, backup: &Path, dir: &Path, app_version: &str, now: i64) -> Result<RestoreOutcome, ServiceError> {
    actor.require(Permission::ManageBackups)?;
    // A deactivated or demoted administrator with an old session must not replace all data.
    db.read(|c| verify_actor(c, actor))?;
    let outcome = db.restore_backup(backup, dir, app_version)?;
    // Recorded in the RESTORED database; the account may not exist there, so by name only.
    db.write(|c| {
        audit::record(
            c,
            now,
            Actor::Anonymous { username_tried: &actor.username },
            "RESTORE",
            None,
            Some(json!({ "from": outcome.restored_from.file_name, "safetyBackup": outcome.safety_backup.file_name })),
        )
    })?;
    Ok(outcome)
}

/// Refuses to back up an empty database (e.g. after a restore failed half-way), so a useless
/// backup can never push a good automatic backup out of the kept set.
fn ensure_database_loaded(db: &Database) -> Result<(), ServiceError> {
    if db.status()?.schema_version == 0 {
        return Err(ServiceError::Corrupt("the database is not loaded; restart the app".into()));
    }
    Ok(())
}

/// Makes an automatic backup if the last one is older than `min_age_secs` (called at start-up and
/// periodically with `AUTO_BACKUP_INTERVAL_SECS`, and at exit with `EXIT_BACKUP_INTERVAL_SECS`).
pub fn auto_backup_if_due(db: &mut Database, dir: &Path, app_version: &str, now: i64, min_age_secs: i64) -> Result<Option<BackupFile>, ServiceError> {
    // A backup stamped in the future (clock was wrong) must not stop automatic backups.
    let due = latest_backup_time(dir, BackupKind::Automatic)?.is_none_or(|last| last > now || now - last >= min_age_secs);
    if !due {
        return Ok(None);
    }
    ensure_database_loaded(db)?;
    let file = db.create_backup(dir, BackupKind::Automatic, app_version)?;
    let removed = prune_backups(dir, BackupKind::Automatic, AUTO_BACKUPS_KEPT)?;
    db.write(|c| audit::record(c, now, Actor::System, "BACKUP_AUTO", None, Some(json!({ "file": file.file_name, "removedOld": removed }))))?;
    Ok(Some(file))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub integrity_ok: bool,
    /// Batches whose quantity does not match their ledger (design D24; never auto-corrected).
    pub ledger_mismatch_batch_ids: Vec<i64>,
}

pub fn health(db: &Database, actor: &Session) -> Result<HealthReport, ServiceError> {
    actor.require(Permission::ViewSystemInfo)?;
    let integrity_ok = db.verify_integrity().is_ok();
    Ok(HealthReport { integrity_ok, ledger_mismatch_batch_ids: db.read(ledger_mismatches)? })
}
