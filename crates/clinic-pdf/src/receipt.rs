use clinic_core::money::Paise;
use serde::{Deserialize, Serialize};

/// Everything printed on a receipt, already formatted where formatting is locale-specific
/// (date/time). The same data drives the PDF and the on-screen/print preview in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptData {
    pub clinic_name: String,
    pub clinic_address_lines: Vec<String>,
    pub clinic_phone: Option<String>,
    pub clinic_gstin: Option<String>,
    /// e.g. "CANCELLED" or "CORRECTED - see INV/26-27/000124"; shown prominently at the top.
    pub status_banner: Option<String>,
    pub bill_no: String,
    pub date_time: String,
    pub client_label: Option<String>,
    pub lines: Vec<ReceiptLine>,
    pub subtotal: Paise,
    /// Section subtotals shown instead of "Subtotal" (Consultation, Procedures, Medicines...);
    /// empty for a bill with medicines/products only.
    #[serde(default)]
    pub breakdown: Vec<ReceiptTotal>,
    /// e.g. "Discount on medicines" when the discount did not apply to consultation/procedures.
    #[serde(default = "default_discount_label")]
    pub discount_label: String,
    pub discount: Paise,
    pub tax_label: String,
    pub tax: Paise,
    pub round_off: Paise,
    pub total: Paise,
    pub payments: Vec<ReceiptPayment>,
    pub amount_received: Option<Paise>,
    pub change_due: Option<Paise>,
    pub billed_by: Option<String>,
    pub footer: Option<String>,
    /// Small print at the very bottom, e.g. that an e-receipt needs no signature or stamp.
    #[serde(default)]
    pub notice: Option<String>,
}

fn default_discount_label() -> String {
    "Discount".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptTotal {
    pub label: String,
    pub amount: Paise,
}

/// Printed on every receipt (the PDF is sent electronically, e.g. on WhatsApp).
pub const E_RECEIPT_NOTICE: &str = "This is a computer-generated e-receipt and does not require a signature or stamp.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptLine {
    pub name: String,
    /// Small second line under the item; not used for batch numbers or expiry dates (not shown to
    /// patients). "Not supplied" is printed from `not_supplied_qty`.
    pub detail: Option<String>,
    pub qty: u32,
    /// MRP per unit (before the discount).
    pub unit_price: Paise,
    /// This line's share of the bill discount.
    #[serde(default)]
    pub discount: Paise,
    /// What is paid for the line: qty × MRP − discount.
    pub amount: Paise,
    /// Prescribed but not given because it was out of stock (qty and amount are then 0).
    pub not_supplied_qty: u32,
    /// Heading this line belongs under ("Consultation", "Procedures", "Medicines & Products");
    /// empty = no headings (a bill with medicines/products only).
    #[serde(default)]
    pub section: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptPayment {
    pub method: String,
    pub amount: Paise,
}

/// Demo receipt used by the Phase 0 spike screen and tests.
pub fn sample_receipt() -> ReceiptData {
    // Bill discount ₹10 spread over ₹160: Paracetamol 40 -> 37.50, cream 120 -> 112.50.
    ReceiptData {
        clinic_name: "Dr Jyotsna's SkinDoc Clinic".into(),
        clinic_address_lines: vec!["12 MG Road, Pune 411001".into()],
        clinic_phone: Some("020-12345678".into()),
        clinic_gstin: None,
        status_banner: None,
        bill_no: "INV/26-27/000123".into(),
        date_time: "25-Sep-2026 10:42".into(),
        client_label: Some("John Doe (CL-000045)".into()),
        lines: vec![
            ReceiptLine {
                name: "Paracetamol 500mg".into(),
                detail: None,
                qty: 2,
                unit_price: Paise::new(2_000),
                discount: Paise::new(250),
                amount: Paise::new(3_750),
                not_supplied_qty: 0,
                section: String::new(),
            },
            ReceiptLine {
                name: "Pain Relief Cream".into(),
                detail: None,
                qty: 1,
                unit_price: Paise::new(12_000),
                discount: Paise::new(750),
                amount: Paise::new(11_250),
                not_supplied_qty: 0,
                section: String::new(),
            },
            ReceiptLine {
                name: "Sunscreen SPF 50".into(),
                detail: None,
                qty: 0,
                unit_price: Paise::ZERO,
                discount: Paise::ZERO,
                amount: Paise::ZERO,
                not_supplied_qty: 1,
                section: String::new(),
            },
        ],
        subtotal: Paise::new(16_000),
        breakdown: Vec::new(),
        discount_label: default_discount_label(),
        discount: Paise::new(1_000),
        tax_label: String::new(),
        tax: Paise::ZERO,
        round_off: Paise::ZERO,
        total: Paise::new(15_000),
        payments: vec![ReceiptPayment { method: "Cash".into(), amount: Paise::new(15_000) }],
        amount_received: Some(Paise::new(20_000)),
        change_due: Some(Paise::new(5_000)),
        billed_by: Some("Priya".into()),
        footer: Some("Continue your Skincare Journey with the SkinDoc. Thank you.".into()),
        notice: Some(E_RECEIPT_NOTICE.into()),
    }
}
