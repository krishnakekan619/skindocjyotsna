//! Domain model and business rules for SkinDocJyotsna.
//!
//! This crate has no dependency on Tauri or SQLite, so every rule can be unit-tested
//! in isolation and reused unchanged by a future clinic-server binary.

pub mod auth;
pub mod fefo;
pub mod money;
pub mod pricing;
pub mod time;
