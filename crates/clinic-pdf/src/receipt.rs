use clinic_core::money::{BasisPoints, Paise};
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptLine {
    pub name: String,
    /// Small second line, e.g. "Batch A23 · Exp 12/2026".
    pub detail: Option<String>,
    pub qty: u32,
    pub unit_price: Paise,
    pub amount: Paise,
    /// Prescribed but not given because it was out of stock (qty and amount are then 0).
    pub not_supplied_qty: u32,
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
    let tax = Paise::new(3_750).included_tax(BasisPoints::new(1_200)).value()
        + Paise::new(11_250).included_tax(BasisPoints::new(1_800)).value();
    ReceiptData {
        clinic_name: "SkinDocJyotsna Clinic".into(),
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
                detail: Some("Batch A23 · Exp 12/2026".into()),
                qty: 2,
                unit_price: Paise::new(2_000),
                amount: Paise::new(4_000),
                not_supplied_qty: 0,
            },
            ReceiptLine {
                name: "Pain Relief Cream".into(),
                detail: Some("Batch C19 · Exp 08/2027".into()),
                qty: 1,
                unit_price: Paise::new(12_000),
                amount: Paise::new(12_000),
                not_supplied_qty: 0,
            },
            ReceiptLine {
                name: "Sunscreen SPF 50".into(),
                detail: None,
                qty: 0,
                unit_price: Paise::ZERO,
                amount: Paise::ZERO,
                not_supplied_qty: 1,
            },
        ],
        subtotal: Paise::new(16_000),
        discount: Paise::new(1_000),
        tax_label: "GST included".into(),
        tax: Paise::new(tax),
        round_off: Paise::ZERO,
        total: Paise::new(15_000),
        payments: vec![ReceiptPayment { method: "Cash".into(), amount: Paise::new(15_000) }],
        amount_received: Some(Paise::new(20_000)),
        change_due: Some(Paise::new(5_000)),
        billed_by: Some("Priya".into()),
        footer: Some("Thank you. Get well soon!".into()),
    }
}
