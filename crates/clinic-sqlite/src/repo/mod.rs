//! Plain data access: one module per table group. No business rules here; those live in
//! clinic-core (rules) and clinic-services (flows). Functions take `&Connection`, so they work
//! both inside and outside a transaction (`Database::write` / `Database::read`).

pub mod audit;
pub mod billing;
pub mod clients;
pub mod inventory;
pub mod services;
pub mod settings;
pub mod users;
