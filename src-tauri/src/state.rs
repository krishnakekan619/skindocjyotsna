use std::sync::Mutex;

use clinic_sqlite::Database;

use crate::paths::AppPaths;

/// Shared application state. The single database connection is the only writer,
/// so all writes are serialised (no "database is busy" errors during billing).
pub struct AppState {
    pub paths: AppPaths,
    pub db: Mutex<Database>,
}
