//! Application services: the flows behind every screen (setup, login, users, settings, audit).
//!
//! Each public function checks the caller's permission, applies the rules from clinic-core and
//! writes everything, including its audit entry, in ONE database transaction. The desktop
//! shell only translates between UI commands and these functions; a future clinic server could
//! call them unchanged.

pub mod audit;
pub mod auth;
pub mod billing;
pub mod catalog;
pub mod clients;
mod error;
pub mod inventory;
pub mod maintenance;
pub mod reports;
pub mod settings;
pub mod share;
pub mod users;

use clinic_core::auth::{Permission, Role};
use serde::Serialize;

pub use error::ServiceError;

/// The signed-in staff member. Held by the shell while the app is unlocked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub user_id: i64,
    pub username: String,
    pub full_name: String,
    pub role: Role,
    pub has_pin: bool,
}

impl Session {
    pub fn require(&self, permission: Permission) -> Result<(), ServiceError> {
        if self.role.allows(permission) { Ok(()) } else { Err(ServiceError::PermissionDenied) }
    }
}
