use clinic_core::auth::Permission;
use clinic_core::auth::policy::{IDLE_LOCK_DEFAULT_MINUTES, validate_idle_lock_minutes};
use clinic_core::time::DEFAULT_UTC_OFFSET_MINUTES;
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

/// The WhatsApp message typed in for the client when a receipt is shared (editable in Clinic
/// details). Placeholders: {name} (client's first name), {clinic}, {billNo}, {total}.
/// No medicines or other health details (privacy).
pub const DEFAULT_WHATSAPP_MESSAGE: &str = "Hello {name},\n\nThank you for visiting {clinic}.\n\nPlease find your e-receipt {billNo} attached.\nTotal: {total}\n\nThis is a computer-generated e-receipt and does not require a signature or stamp.\n\nThank you,\n{clinic}";

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
    /// Bill numbers look like <prefix>/<FY>/000123.
    pub invoice_prefix: String,
    /// Receptionists may give up to this discount (% of the bill) without admin approval (D7).
    pub receptionist_discount_cap_percent: u32,
    /// Round bill totals to the nearest rupee, with a visible round-off line (D4).
    pub round_to_rupee: bool,
    /// Clinic time zone as minutes from UTC (India: +330).
    pub utc_offset_minutes: i32,
    /// Receptionists may process returns up to this many days after the bill (D18).
    pub return_window_days: u32,
    /// Message typed in when a receipt is shared on WhatsApp (see `DEFAULT_WHATSAPP_MESSAGE`).
    pub whatsapp_message: String,
    /// Discount (%) ticked by default on every bill's medicines; 0 = none (DEC-034). Never more
    /// than the receptionist limit, so it never needs an approval.
    pub default_medicine_discount_percent: u32,
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
            invoice_prefix: "INV".to_string(),
            receptionist_discount_cap_percent: 10,
            round_to_rupee: true,
            utc_offset_minutes: DEFAULT_UTC_OFFSET_MINUTES,
            return_window_days: 7,
            whatsapp_message: DEFAULT_WHATSAPP_MESSAGE.to_string(),
            default_medicine_discount_percent: 10,
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
            invoice_prefix: self.invoice_prefix.trim().to_uppercase(),
            receptionist_discount_cap_percent: self.receptionist_discount_cap_percent,
            round_to_rupee: self.round_to_rupee,
            utc_offset_minutes: self.utc_offset_minutes,
            return_window_days: self.return_window_days,
            whatsapp_message: match self.whatsapp_message.trim() {
                "" => DEFAULT_WHATSAPP_MESSAGE.to_string(),
                text => text.to_string(),
            },
            default_medicine_discount_percent: self.default_medicine_discount_percent,
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
        let prefix_ok = (1..=8).contains(&self.invoice_prefix.len()) && self.invoice_prefix.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        if !prefix_ok {
            return Err(invalid("invoicePrefix", "Invoice prefix must be 1 to 8 letters or digits, like INV."));
        }
        if self.receptionist_discount_cap_percent > 100 {
            return Err(invalid("receptionistDiscountCapPercent", "Discount limit must be between 0 and 100%."));
        }
        if !(-720..=840).contains(&self.utc_offset_minutes) {
            return Err(invalid("utcOffsetMinutes", "Time zone offset must be between -12:00 and +14:00."));
        }
        if !(0..=365).contains(&self.return_window_days) {
            return Err(invalid("returnWindowDays", "Return window must be 0 to 365 days."));
        }
        if self.default_medicine_discount_percent > self.receptionist_discount_cap_percent {
            return Err(invalid(
                "defaultMedicineDiscountPercent",
                "The standard medicine discount cannot be higher than the receptionist discount limit.",
            ));
        }
        if chars(&self.whatsapp_message) > 1_000 {
            return Err(invalid("whatsappMessage", "The WhatsApp message must be at most 1000 characters."));
        }
        Ok(())
    }
}

pub(crate) fn load(conn: &Connection) -> Result<ClinicSettings, ServiceError> {
    let mut clinic: ClinicSettings = match settings::get(conn, CLINIC_KEY)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| ServiceError::Corrupt(format!("clinic settings: {e}")))?,
        None => ClinicSettings::default(),
    };
    // Settings saved before v0.3.1 have no standard discount and get the default 10%: never more
    // than the clinic's receptionist limit, or every bill would need an approval.
    clinic.default_medicine_discount_percent = clinic.default_medicine_discount_percent.min(clinic.receptionist_discount_cap_percent);
    Ok(clinic)
}

/// Clinic settings plus today's date in the clinic's time zone.
pub(crate) fn clinic_today(conn: &Connection, now: i64) -> Result<(ClinicSettings, clinic_core::time::Date), ServiceError> {
    let clinic = load(conn)?;
    let today = clinic_core::time::local_date(now, clinic.utc_offset_minutes);
    Ok((clinic, today))
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
        let before = load(c)?;
        save(c, &clinic)?;
        // Before and after for the settings that change money, time or access rules.
        let snapshot = |s: &ClinicSettings| {
            json!({
                "idleLockMinutes": s.idle_lock_minutes,
                "receptionistDiscountCapPercent": s.receptionist_discount_cap_percent,
                "defaultMedicineDiscountPercent": s.default_medicine_discount_percent,
                "roundToRupee": s.round_to_rupee,
                "utcOffsetMinutes": s.utc_offset_minutes,
                "returnWindowDays": s.return_window_days,
                "invoicePrefix": s.invoice_prefix,
            })
        };
        audit::record(c, now, Actor::from(actor), "SETTINGS_UPDATE", Some(("settings", CLINIC_KEY.to_string())), Some(json!({ "before": snapshot(&before), "after": snapshot(&clinic) })))
    })?;
    Ok(clinic)
}
