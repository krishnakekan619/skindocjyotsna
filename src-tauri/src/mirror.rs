//! The second backup copy (DEC-037): every backup is also copied to `~/skindocjyotsnaBackup`.
//! Copying never fails the backup itself; problems are logged and shown on Backup & system.

use std::path::Path;

use clinic_sqlite::{BackupFile, BackupKind, prune_backups};

/// Automatic backups kept in the second folder (the app folder keeps 14).
const MIRROR_AUTO_KEPT: usize = 30;

pub fn copy_backup(file: &BackupFile, mirror_dir: &Path) {
    if let Err(error) = try_copy(file, mirror_dir) {
        tracing::warn!(%error, file = %file.file_name, dir = %mirror_dir.display(), "could not copy the backup to the second folder");
    }
}

fn try_copy(file: &BackupFile, mirror_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(mirror_dir)?;
    let target = mirror_dir.join(&file.file_name);
    // Copy under a temporary name, then rename: the folder never holds a half-written backup.
    let partial = mirror_dir.join(format!(".{}.partial", file.file_name));
    std::fs::copy(&file.path, &partial)?;
    std::fs::rename(&partial, &target)?;
    if file.meta.as_ref().is_some_and(|m| m.kind == BackupKind::Automatic) {
        prune_backups(mirror_dir, BackupKind::Automatic, MIRROR_AUTO_KEPT)?;
    }
    Ok(())
}
