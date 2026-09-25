//! OS-appropriate data locations, resolved at runtime (nothing is hard-coded).
//!
//! | OS      | Base folder                                                    |
//! |---------|----------------------------------------------------------------|
//! | Windows | %LOCALAPPDATA%\in.skindocjyotsna.clinic\                       |
//! | macOS   | ~/Library/Application Support/in.skindocjyotsna.clinic/        |
//!
//! Local (not Roaming) AppData is used on purpose: a roaming profile would sync the live
//! database file and corrupt it. `SKINDOC_DATA_DIR` overrides the base folder, for tests
//! and for recovery on a rebuilt machine.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

pub const DATA_DIR_OVERRIDE_ENV: &str = "SKINDOC_DATA_DIR";

pub struct AppPaths {
    pub data_dir: PathBuf,
    pub backup_dir: PathBuf,
    /// Generated PDFs. Inside the app folder (not Documents) because receipts contain
    /// patient names and Documents is often synced to OneDrive/iCloud.
    pub export_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let (base, log_dir) = match std::env::var_os(DATA_DIR_OVERRIDE_ENV) {
            Some(base) => {
                let base = PathBuf::from(base);
                let logs = base.join("logs");
                (base, logs)
            }
            None => (app.path().app_local_data_dir()?, app.path().app_log_dir()?),
        };
        let paths = Self { data_dir: base.join("data"), backup_dir: base.join("backups"), export_dir: base.join("exports"), log_dir };
        for dir in [&paths.data_dir, &paths.backup_dir, &paths.export_dir, &paths.log_dir] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(paths)
    }

    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("clinic.db")
    }
}
