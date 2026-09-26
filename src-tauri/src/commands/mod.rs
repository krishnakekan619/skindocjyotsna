// Every command here must also be listed in build.rs (which generates the permission for it
// and grants it in capabilities/default.json), or the UI cannot call it.
pub mod admin;
pub mod billing;
pub mod catalog;
pub mod clients;
pub mod inventory;
pub mod reports;
pub mod session;
pub mod system;
