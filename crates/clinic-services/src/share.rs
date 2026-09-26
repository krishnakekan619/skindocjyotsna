//! Handing a finalized receipt to the clinic's own WhatsApp (DEC-031). The app opens the
//! client's chat with the clinic's message typed in and the PDF ready to paste; the
//! receptionist presses Send. Nothing is sent automatically, and the message never lists
//! medicines or other health details (privacy).

use clinic_core::auth::Permission;
use clinic_core::money::Paise;
use clinic_sqlite::Database;
use clinic_sqlite::repo::billing as bills;
use clinic_sqlite::repo::clients;
use serde::Serialize;

use crate::settings::clinic_today;
use crate::{ServiceError, Session};

/// What the shell needs to open WhatsApp on the client's chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppMessage {
    /// Digits with country code, e.g. `919876543210`.
    pub phone: String,
    pub text: String,
    pub bill_no: String,
    pub client_name: String,
}

impl WhatsAppMessage {
    /// Opens the installed WhatsApp app directly.
    pub fn app_url(&self) -> String {
        format!("whatsapp://send?phone={}&text={}", self.phone, percent_encode(&self.text))
    }

    /// Fallback when the app is not installed: WhatsApp's own link (opens WhatsApp Web).
    pub fn web_url(&self) -> String {
        format!("https://wa.me/{}?text={}", self.phone, percent_encode(&self.text))
    }
}

/// A WhatsApp number from what staff typed: 10 digits get India's code (91); a leading 0 is
/// dropped; 12-15 digits are taken to include a country code already.
pub fn whatsapp_number(phone: &str) -> Option<String> {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        10 => Some(format!("91{digits}")),
        11 if digits.starts_with('0') => Some(format!("91{}", &digits[1..])),
        12..=15 => Some(digits),
        _ => None,
    }
}

/// URL-encodes everything except unreserved characters (RFC 3986).
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn fill_template(template: &str, name: &str, clinic: &str, bill_no: &str, total: &str) -> String {
    template.replace("{name}", name).replace("{clinic}", clinic).replace("{billNo}", bill_no).replace("{total}", total)
}

/// The message and number for sharing a bill's receipt on WhatsApp.
pub fn whatsapp_message(db: &Database, actor: &Session, bill_id: i64) -> Result<WhatsAppMessage, ServiceError> {
    actor.require(Permission::CreateBills)?;
    db.read(|c| {
        let bill = bills::find_bill(c, bill_id)?.ok_or(ServiceError::NotFound("bill"))?;
        let client_id = bill.client_id.ok_or_else(|| ServiceError::NoPhone("This bill has no client, so there is no WhatsApp number to send it to.".into()))?;
        let client = clients::find(c, client_id)?.ok_or(ServiceError::NotFound("client"))?;
        let phone = whatsapp_number(&client.phone).ok_or_else(|| ServiceError::NoPhone(format!("{} does not have a mobile number.", client.full_name)))?;
        let (clinic, _) = clinic_today(c, 0)?;
        let first_name = client.full_name.split_whitespace().next().unwrap_or(&client.full_name).to_string();
        let total = format!("₹{}", Paise::new(bill.total_paise).to_indian_string());
        Ok(WhatsAppMessage {
            phone,
            text: fill_template(&clinic.whatsapp_message, &first_name, &clinic.name, &bill.bill_no, &total),
            bill_no: bill.bill_no,
            client_name: client.full_name,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_numbers_get_the_country_code() {
        assert_eq!(whatsapp_number("98765 43210").as_deref(), Some("919876543210"));
        assert_eq!(whatsapp_number("098765-43210").as_deref(), Some("919876543210"));
        assert_eq!(whatsapp_number("+91 98765 43210").as_deref(), Some("919876543210"));
        assert_eq!(whatsapp_number("+44 7700 900123").as_deref(), Some("447700900123"));
        assert_eq!(whatsapp_number("12345"), None);
        assert_eq!(whatsapp_number(""), None);
    }

    #[test]
    fn messages_are_url_encoded_and_filled_in() {
        assert_eq!(percent_encode("Total: ₹950\nThanks"), "Total%3A%20%E2%82%B9950%0AThanks");
        let text = fill_template("Hello {name}, bill {billNo} ({total}) - {clinic}", "Rahul", "SkinDoc", "INV/26-27/000123", "₹950");
        assert_eq!(text, "Hello Rahul, bill INV/26-27/000123 (₹950) - SkinDoc");
        let message = WhatsAppMessage { phone: "919876543210".into(), text: "Hi there".into(), bill_no: String::new(), client_name: String::new() };
        assert_eq!(message.app_url(), "whatsapp://send?phone=919876543210&text=Hi%20there");
        assert_eq!(message.web_url(), "https://wa.me/919876543210?text=Hi%20there");
    }
}
