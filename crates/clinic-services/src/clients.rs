//! Clients (patients) and their purchase history (Phase 3). Only what the clinic needs is
//! stored (brief §15, D16): no diagnoses or prescriptions.

use clinic_core::auth::Permission;
use clinic_core::time::Date;
use clinic_sqlite::Database;
use clinic_sqlite::repo::billing::{self as bills, BillItemRow, BillQuery, BillRow, next_number};
use clinic_sqlite::repo::clients::{self as repo, ClientFields, ClientRow};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::{self, Actor};
use crate::auth::verify_actor;
use crate::error::invalid;
use crate::settings::clinic_today;
use crate::{ServiceError, Session};

pub const GENDERS: [&str; 4] = ["MALE", "FEMALE", "OTHER", "UNDISCLOSED"];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInput {
    pub id: Option<i64>,
    pub full_name: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    pub date_of_birth: Option<String>,
    #[serde(default = "undisclosed")]
    pub gender: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub emergency_contact: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "yes")]
    pub is_active: bool,
}

fn undisclosed() -> String {
    "UNDISCLOSED".to_string()
}

fn yes() -> bool {
    true
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

fn validate(input: &ClientInput, today: Date) -> Result<Option<String>, ServiceError> {
    let name = chars(input.full_name.trim());
    if name == 0 || name > 100 {
        return Err(invalid("fullName", "Client name is required (at most 100 characters)."));
    }
    let phone = input.phone.trim();
    if chars(phone) > 20 || !phone.chars().all(|c| c.is_ascii_digit() || matches!(c, ' ' | '+' | '-' | '(' | ')')) {
        return Err(invalid("phone", "Phone can contain only digits, spaces and + - ( ) (at most 20)."));
    }
    let email = input.email.trim();
    if !(email.is_empty() || (chars(email) <= 100 && email.contains('@') && !email.contains(char::is_whitespace))) {
        return Err(invalid("email", "Please enter a valid email address, or leave it empty."));
    }
    if !GENDERS.contains(&input.gender.as_str()) {
        return Err(invalid("gender", "Please choose an option."));
    }
    if chars(input.address.trim()) > 200 || chars(input.emergency_contact.trim()) > 100 {
        return Err(invalid("address", "Address must be at most 200 characters and emergency contact at most 100."));
    }
    if chars(input.notes.trim()) > 500 {
        return Err(invalid("notes", "Notes must be at most 500 characters (do not record diagnoses here)."));
    }
    match input.date_of_birth.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        None => Ok(None),
        Some(text) => match Date::parse(text) {
            Some(dob) if dob <= today => Ok(Some(dob.to_string())),
            _ => Err(invalid("dateOfBirth", "Date of birth must be a valid date, not in the future.")),
        },
    }
}

pub fn save(db: &mut Database, actor: &Session, input: ClientInput, now: i64) -> Result<ClientRow, ServiceError> {
    actor.require(Permission::ManageClients)?;
    db.write(|c| {
        verify_actor(c, actor)?;
        let (_, today) = clinic_today(c, now)?;
        let dob = validate(&input, today)?;
        let fields = ClientFields {
            full_name: input.full_name.trim(),
            phone: input.phone.trim(),
            email: input.email.trim(),
            date_of_birth: dob.as_deref(),
            gender: &input.gender,
            address: input.address.trim(),
            emergency_contact: input.emergency_contact.trim(),
            notes: input.notes.trim(),
            is_active: input.is_active,
        };
        let (id, action) = match input.id {
            Some(id) if repo::update(c, id, &fields, now)? == 1 => (id, "CLIENT_UPDATE"),
            Some(_) => return Err(ServiceError::NotFound("client")),
            None => {
                let code = format!("CL-{:06}", next_number(c, "CLIENT", "ALL")?);
                (repo::insert(c, &code, &fields, now)?, "CLIENT_CREATE")
            }
        };
        let client = repo::find(c, id)?.ok_or(ServiceError::NotFound("client"))?;
        // Audit by client code only: no names or phone numbers in the audit log.
        audit::record(c, now, Actor::from(actor), action, Some(("client", id.to_string())), Some(json!({ "code": client.client_code })))?;
        Ok(client)
    })
}

/// By name, phone or client code. Empty text: most recent visitors.
pub fn search(db: &Database, actor: &Session, text: &str, include_inactive: bool) -> Result<Vec<ClientRow>, ServiceError> {
    actor.require(Permission::ManageClients)?;
    Ok(db.read(|c| repo::search(c, text, include_inactive, 50))?)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Visit {
    pub bill: BillRow,
    pub items: Vec<BillItemRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientProfile {
    pub client: ClientRow,
    /// Newest first; cancelled and corrected bills are included for the record.
    pub visits: Vec<Visit>,
    pub visit_count: i64,
    /// Kept by the client: finalized bills minus refunds.
    pub total_spent_paise: i64,
}

/// Profile and history, optionally limited to local dates `from`..=`to` (YYYY-MM-DD).
pub fn profile(db: &Database, actor: &Session, client_id: i64, from: Option<&str>, to: Option<&str>) -> Result<ClientProfile, ServiceError> {
    actor.require(Permission::ManageClients)?;
    db.read(|c| {
        let client = repo::find(c, client_id)?.ok_or(ServiceError::NotFound("client"))?;
        let (clinic, _) = clinic_today(c, 0)?;
        let bound = |text: Option<&str>, field: &'static str, extra_days: i64| -> Result<Option<i64>, ServiceError> {
            match text.map(str::trim).filter(|t| !t.is_empty()) {
                None => Ok(None),
                Some(t) => Date::parse(t)
                    .map(|d| Some(clinic_core::time::local_day_start_utc(d.add_days(extra_days), clinic.utc_offset_minutes)))
                    .ok_or_else(|| invalid(field, "Please enter a valid date.")),
            }
        };
        let rows = bills::list_bills(
            c,
            &BillQuery { from: bound(from, "from", 0)?, to: bound(to, "to", 1)?, status: None, client_id: Some(client_id), text: "", limit: 200 },
        )?;
        let mut visits = Vec::with_capacity(rows.len());
        for bill in rows {
            visits.push(Visit { items: bills::items(c, bill.id)?, bill });
        }
        let kept = visits.iter().filter(|v| v.bill.status == "FINALIZED");
        let total_spent_paise = kept.clone().map(|v| v.bill.total_paise - v.bill.returned_paise).sum();
        let visit_count = kept.count() as i64;
        Ok(ClientProfile { client, visits, visit_count, total_spent_paise })
    })
}
