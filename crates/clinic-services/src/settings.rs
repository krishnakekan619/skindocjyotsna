use clinic_core::auth::Permission;
use clinic_core::auth::policy::{IDLE_LOCK_DEFAULT_MINUTES, validate_idle_lock_minutes};
use clinic_sqlite::Database;
use clinic_sqlite::repo::settings;
use clinic_sqlite::rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::{self, Actor};
use crate::auth::verify_actor;
use crate::error::invalid;
use crate::{ServiceError, Session};

const CLINIC_KEY: &str = "clinic.profile";

/// Clinic details printed on receipts, plus clinic-wide preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClinicSettings {
    pub name: String,
    pub address_lines: Vec<String>,
    pub phone: String,
    pub email: String,
    pub gstin: String,
    pub receipt_footer: String,
    pub idle_lock_minutes: u32,
}

impl Default for ClinicSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            address_lines: Vec::new(),
            phone: String::new(),
            email: String::new(),
            gstin: String::new(),
            receipt_footer: "Thank you. Get well soon!".to_string(),
            idle_lock_minutes: IDLE_LOCK_DEFAULT_MINUTES,
        }
    }
}

impl ClinicSettings {
    /// Trims text, drops empty address lines, upper-cases the GSTIN, then checks every field.
    pub fn normalized(self) -> Result<Self, ServiceError> {
        let settings = Self {
            name: self.name.trim().to_string(),
            address_lines: self.address_lines.iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(),
            phone: self.phone.trim().to_string(),
            email: self.email.trim().to_string(),
            gstin: self.gstin.trim().to_uppercase(),
            receipt_footer: self.receipt_footer.trim().to_string(),
            idle_lock_minutes: self.idle_lock_minutes,
        };
        settings.validate()?;
        Ok(settings)
    }

    fn validate(&self) -> Result<(), ServiceError> {
        let chars = |s: &str| s.chars().count();
        if self.name.is_empty() || chars(&self.name) > 100 {
            return Err(invalid("name", "Clinic name is required (at most 100 characters)."));
        }
        if self.address_lines.len() > 4 || self.address_lines.iter().any(|l| chars(l) > 100) {
            return Err(invalid("addressLines", "Address can have at most 4 lines of 100 characters."));
        }
        let phone_ok = chars(&self.phone) <= 20
            && self.phone.chars().all(|c| c.is_ascii_digit() || matches!(c, ' ' | '+' | '-' | '(' | ')' | '/'));
        if !phone_ok {
            return Err(invalid("phone", "Phone can contain only digits, spaces and + - ( ) / (at most 20)."));
        }
        let email_ok = self.email.is_empty()
            || (chars(&self.email) <= 100 && self.email.contains('@') && !self.email.contains(char::is_whitespace));
        if !email_ok {
            return Err(invalid("email", "Please enter a valid email address, or leave it empty."));
        }
        let gstin_ok = self.gstin.is_empty() || (self.gstin.len() == 15 && self.gstin.chars().all(|c| c.is_ascii_alphanumeric()));
        if !gstin_ok {
            return Err(invalid("gstin", "GSTIN must be 15 letters and digits, or left empty."));
        }
        if chars(&self.receipt_footer) > 200 {
            return Err(invalid("receiptFooter", "Receipt footer must be at most 200 characters."));
        }
        validate_idle_lock_minutes(self.idle_lock_minutes)?;
        Ok(())
    }
}

pub(crate) fn load(conn: &Connection) -> Result<ClinicSettings, ServiceError> {
    match settings::get(conn, CLINIC_KEY)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| ServiceError::Corrupt(format!("clinic settings: {e}"))),
        None => Ok(ClinicSettings::default()),
    }
}

/// Expects already-normalized settings.
pub(crate) fn save(conn: &Connection, clinic: &ClinicSettings) -> Result<(), ServiceError> {
    let json = serde_json::to_string(clinic).map_err(|e| ServiceError::Corrupt(e.to_string()))?;
    settings::set(conn, CLINIC_KEY, &json)?;
    Ok(())
}

/// Readable without a login: the login and lock screens show the clinic name, and the shell
/// needs the idle lock time.
pub fn get_clinic(db: &Database) -> Result<ClinicSettings, ServiceError> {
    db.read(load)
}

pub fn update_clinic(db: &mut Database, actor: &Session, clinic: ClinicSettings, now: i64) -> Result<ClinicSettings, ServiceError> {
    actor.require(Permission::ManageClinicSettings)?;
    let clinic = clinic.normalized()?;
    db.write(|c| {
        verify_actor(c, actor)?;
        save(c, &clinic)?;
        audit::record(c, now, Actor::from(actor), "SETTINGS_UPDATE", Some(("settings", CLINIC_KEY.to_string())), Some(json!({ "idleLockMinutes": clinic.idle_lock_minutes })))
    })?;
    Ok(clinic)
}
