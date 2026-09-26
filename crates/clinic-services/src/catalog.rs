//! Consultation types and procedures that can be charged on a bill (DEC-030). Receptionists see
//! the active list; administrators maintain it. Entries are deactivated, never deleted, because
//! past bills refer to them.

use clinic_core::auth::Permission;
use clinic_sqlite::Database;
use clinic_sqlite::repo::services::{self as repo, ServiceFields, ServiceRow};
use serde::Deserialize;
use serde_json::json;

use crate::audit::{self, Actor};
use crate::auth::verify_actor;
use crate::error::invalid;
use crate::{ServiceError, Session};

pub const SERVICE_KINDS: [&str; 2] = ["CONSULTATION", "PROCEDURE"];
/// Same ceiling as product prices (₹10,00,000).
pub const MAX_PRICE_PAISE: i64 = 100_000_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInput {
    pub id: Option<i64>,
    pub kind: String,
    pub name: String,
    pub default_price_paise: i64,
    #[serde(default)]
    pub gst_rate_bp: i64,
    #[serde(default)]
    pub discount_eligible: bool,
    #[serde(default = "yes")]
    pub is_active: bool,
    #[serde(default)]
    pub sort_order: i64,
}

fn yes() -> bool {
    true
}

/// Active entries for billing; administrators may also ask for inactive ones.
pub fn list(db: &Database, actor: &Session, include_inactive: bool) -> Result<Vec<ServiceRow>, ServiceError> {
    actor.require(Permission::CreateBills)?;
    let include_inactive = include_inactive && actor.role.allows(Permission::ManageClinicSettings);
    Ok(db.read(|c| repo::list(c, include_inactive))?)
}

fn validate(input: &ServiceInput) -> Result<(), ServiceError> {
    if !SERVICE_KINDS.contains(&input.kind.as_str()) {
        return Err(invalid("kind", "Choose consultation or procedure."));
    }
    let name = input.name.trim().chars().count();
    if name == 0 || name > 80 {
        return Err(invalid("name", "Name is required (at most 80 characters)."));
    }
    if !(0..=MAX_PRICE_PAISE).contains(&input.default_price_paise) {
        return Err(invalid("defaultPricePaise", "Price must be between ₹0 and ₹10,00,000."));
    }
    if !(0..=2_800).contains(&input.gst_rate_bp) {
        return Err(invalid("gstRateBp", "GST must be between 0% and 28%."));
    }
    if !(0..=9_999).contains(&input.sort_order) {
        return Err(invalid("sortOrder", "Order must be between 0 and 9999."));
    }
    Ok(())
}

/// Adds or edits an entry (administrators). The kind of an existing entry cannot change.
pub fn save(db: &mut Database, actor: &Session, input: ServiceInput, now: i64) -> Result<ServiceRow, ServiceError> {
    actor.require(Permission::ManageClinicSettings)?;
    validate(&input)?;
    db.write(|c| {
        verify_actor(c, actor)?;
        let name = input.name.trim();
        let kind = match input.id {
            Some(id) => repo::find(c, id)?.ok_or(ServiceError::NotFound("consultation or procedure"))?.kind,
            None => input.kind.clone(),
        };
        if repo::name_taken(c, &kind, name, input.id)? {
            return Err(ServiceError::Conflict(format!("\"{name}\" already exists.")));
        }
        let fields = ServiceFields {
            kind: &kind,
            name,
            default_price_paise: input.default_price_paise,
            gst_rate_bp: input.gst_rate_bp,
            discount_eligible: input.discount_eligible,
            is_active: input.is_active,
            sort_order: input.sort_order,
        };
        let (id, action) = match input.id {
            Some(id) => {
                repo::update(c, id, &fields, now)?;
                (id, "SERVICE_UPDATE")
            }
            None => (repo::insert(c, &fields, now)?, "SERVICE_CREATE"),
        };
        audit::record(
            c,
            now,
            Actor::from(actor),
            action,
            Some(("service", id.to_string())),
            Some(json!({ "kind": kind, "name": name, "price": input.default_price_paise, "active": input.is_active })),
        )?;
        repo::find(c, id)?.ok_or(ServiceError::NotFound("consultation or procedure"))
    })
}
