//! Bills (Phases 4, 5 and 7): quote, finalize, cancel, return, correct, receipt.
//!
//! Every write is ONE transaction (design §11): stock is taken from batches with a conditional
//! update that can never go below zero, the ledger records every change, and a repeated
//! Finalize with the same key returns the bill that was already made (D10).

use clinic_core::auth::{Permission, Role};
use clinic_core::fefo::{self, Allocation};
use clinic_core::money::{BasisPoints, Paise};
use clinic_core::pricing::{self, Discount, LineInput};
use clinic_core::time::{Date, format_local_datetime, local_date, local_day_start_utc};
use clinic_pdf::{E_RECEIPT_NOTICE, ReceiptData, ReceiptLine, ReceiptPayment, ReceiptTotal};
use clinic_sqlite::Database;
use clinic_sqlite::repo::billing::{
    self as repo, BillItemRow, BillQuery, BillRow, NewBill, NewBillItem, NewPayment, NewReturn, NewServiceItem, PaymentRow, ServiceItemRow,
    next_number,
};
use clinic_sqlite::repo::clients;
use clinic_sqlite::repo::inventory::{self as stock, Movement, ProductRow};
use clinic_sqlite::repo::services::{self as catalog, ServiceFields, ServiceRow};
use clinic_sqlite::repo::users;
use clinic_sqlite::rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::{self, Actor};
use crate::auth::{role_of, verify_actor};
use crate::catalog::{MAX_PRICE_PAISE, SERVICE_KINDS};
use crate::error::invalid;
use crate::inventory::{EXPIRY_WARNING_DAYS, batch_stock};
use crate::settings::{ClinicSettings, clinic_today};
use crate::{ServiceError, Session};

pub const PAYMENT_METHODS: [&str; 4] = ["CASH", "UPI", "CARD", "OTHER"];

fn chars(text: &str) -> usize {
    text.chars().count()
}

fn rupees(paise: i64) -> String {
    format!("₹{}", Paise::new(paise).to_indian_string())
}

// ---- Inputs ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillLineInput {
    pub product_id: i64,
    /// Quantity given to the client.
    #[serde(default)]
    pub qty: i64,
    /// Prescribed but not given because it is out of stock (DEC-002/DEC-007).
    #[serde(default)]
    pub not_supplied_qty: i64,
}

/// A consultation or procedure on the bill: one from the list (`service_id`), or a name typed at
/// the desk (`kind` + `name`), which joins the list at Finalize (DEC-034). `unit_price_paise`:
/// the amount charged; `None` = the usual price.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLineInput {
    #[serde(default)]
    pub service_id: Option<i64>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "one")]
    pub qty: i64,
    #[serde(default)]
    pub unit_price_paise: Option<i64>,
}

/// A client typed on the New Bill screen (not yet saved): created at Finalize.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewClientInput {
    pub full_name: String,
    #[serde(default)]
    pub phone: String,
    /// The receptionist saw a possible existing client and chose to create a new one anyway.
    #[serde(default)]
    pub allow_duplicate: bool,
}

fn one() -> i64 {
    1
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentInput {
    pub method: String,
    pub amount_paise: i64,
    #[serde(default)]
    pub reference: String,
}

/// An administrator's credentials, typed at the counter to approve a large discount (D7).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillInput {
    /// Created by the New Bill screen once per bill; repeated submits return the same bill.
    pub idempotency_key: String,
    pub client_id: Option<i64>,
    /// When `client_id` is empty: a new client typed on the bill, saved together with it.
    #[serde(default)]
    pub new_client: Option<NewClientInput>,
    /// Medicines and products.
    #[serde(default)]
    pub lines: Vec<BillLineInput>,
    /// Consultations and procedures (no stock).
    #[serde(default)]
    pub services: Vec<ServiceLineInput>,
    #[serde(default)]
    pub discount: Discount,
    #[serde(default)]
    pub payments: Vec<PaymentInput>,
    /// Cash handed over by the client, for the change calculation.
    pub amount_received_paise: Option<i64>,
    #[serde(default)]
    pub note: String,
    pub approval: Option<Approval>,
}

// ---- Planning (shared by quote and finalize) -------------------------------------------------

/// One bill item as it will be stored: a product at one price (a line whose stock comes from
/// batches with different prices becomes one item per price), or a not-supplied record.
#[derive(Debug, Clone)]
struct PlannedItem {
    product: ProductRow,
    qty: i64,
    not_supplied_qty: i64,
    unit_price: i64,
    allocations: Vec<Allocation>,
    expires_soon: bool,
}

/// `give_back`: (batch id, qty) pairs counted as available again: the stock of a bill that is
/// being corrected, which the correction returns before re-selling (quote only).
fn plan_items(c: &Connection, lines: &[BillLineInput], today: Date, give_back: &[(i64, i64)]) -> Result<Vec<PlannedItem>, ServiceError> {
    if lines.len() > 200 {
        return Err(invalid("lines", "A bill can have at most 200 lines."));
    }
    // The same product twice counts as one line, so its stock is not allocated twice.
    let mut merged: Vec<BillLineInput> = Vec::new();
    for line in lines {
        if line.qty < 0 || line.not_supplied_qty < 0 || line.qty + line.not_supplied_qty == 0 || line.qty > 100_000 || line.not_supplied_qty > 100_000 {
            return Err(invalid("lines", "Each line needs a quantity of at least 1."));
        }
        match merged.iter_mut().find(|m| m.product_id == line.product_id) {
            Some(m) => {
                m.qty += line.qty;
                m.not_supplied_qty += line.not_supplied_qty;
            }
            None => merged.push(line.clone()),
        }
    }
    let today_text = today.to_string();
    let mut items = Vec::new();
    for line in &merged {
        let product = stock::find_product(c, line.product_id, &today_text)?
            .filter(|p| p.is_active)
            .ok_or_else(|| ServiceError::NotAllowed("Unable to complete the operation. The selected product is no longer available.".into()))?;
        if line.qty > 0 {
            let mut batches: Vec<_> = stock::batches_for_product(c, product.id)?.iter().map(batch_stock).collect();
            for batch in &mut batches {
                batch.quantity += give_back.iter().filter(|(id, _)| *id == batch.batch_id).map(|(_, qty)| qty).sum::<i64>();
            }
            let allocations = fefo::allocate(&batches, today, line.qty)
                .map_err(|available| ServiceError::InsufficientStock { product: product.name.clone(), available })?;
            // Group consecutive allocations with the same price into one item.
            let mut groups: Vec<Vec<Allocation>> = Vec::new();
            for allocation in allocations {
                let same_price = groups.last().and_then(|g| g.first()).is_some_and(|g| g.price == allocation.price);
                if same_price {
                    if let Some(group) = groups.last_mut() {
                        group.push(allocation);
                    }
                } else {
                    groups.push(vec![allocation]);
                }
            }
            for group in groups {
                let expires_soon = group.iter().any(|a| {
                    batches.iter().find(|b| b.batch_id == a.batch_id).and_then(|b| b.expiry).is_some_and(|e| e <= today.add_days(EXPIRY_WARNING_DAYS))
                });
                items.push(PlannedItem {
                    product: product.clone(),
                    qty: group.iter().map(|a| a.qty).sum(),
                    not_supplied_qty: 0,
                    unit_price: group.first().map_or(0, |a| a.price.value()),
                    allocations: group,
                    expires_soon,
                });
            }
        }
        if line.not_supplied_qty > 0 {
            items.push(PlannedItem { product, qty: 0, not_supplied_qty: line.not_supplied_qty, unit_price: 0, allocations: Vec::new(), expires_soon: false });
        }
    }
    Ok(items)
}

/// A consultation or procedure as it will be stored. A name typed at the desk that is not in the
/// list yet has `service.id == 0` until Finalize adds it.
#[derive(Debug, Clone)]
struct PlannedService {
    service: ServiceRow,
    qty: i64,
    unit_price: i64,
}

fn plan_services(c: &Connection, inputs: &[ServiceLineInput]) -> Result<Vec<PlannedService>, ServiceError> {
    if inputs.len() > 50 {
        return Err(invalid("services", "A bill can have at most 50 consultations and procedures."));
    }
    let mut planned = Vec::with_capacity(inputs.len());
    for input in inputs {
        let service = match input.service_id {
            // A deactivated entry (e.g. on a bill being corrected) is reactivated at Finalize.
            Some(id) => catalog::find(c, id)?
                .ok_or_else(|| ServiceError::NotAllowed("Unable to complete the operation. The selected consultation or procedure is no longer available.".into()))?,
            None => {
                let kind = input
                    .kind
                    .as_deref()
                    .filter(|k| SERVICE_KINDS.contains(k))
                    .ok_or_else(|| invalid("services", "Choose consultation or procedure."))?;
                let name = input
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|n| !n.is_empty() && n.chars().count() <= 80)
                    .ok_or_else(|| invalid("services", "Type the consultation or procedure name (at most 80 characters)."))?;
                // Typed but already in the list (any case): use that entry.
                catalog::find_by_name(c, kind, name)?.unwrap_or_else(|| ServiceRow {
                    id: 0,
                    kind: kind.to_string(),
                    name: name.to_string(),
                    default_price_paise: input.unit_price_paise.unwrap_or(0),
                    gst_rate_bp: 0,
                    discount_eligible: false,
                    is_active: true,
                    sort_order: 100,
                })
            }
        };
        if !(1..=100).contains(&input.qty) {
            return Err(invalid("services", "Each consultation or procedure needs a quantity from 1 to 100."));
        }
        let unit_price = input.unit_price_paise.unwrap_or(service.default_price_paise);
        if !(0..=MAX_PRICE_PAISE).contains(&unit_price) {
            return Err(invalid("services", "Price must be between ₹0 and ₹10,00,000."));
        }
        planned.push(PlannedService { service, qty: input.qty, unit_price });
    }
    Ok(planned)
}

struct Priced {
    items: Vec<PlannedItem>,
    /// Per item (not-supplied items get zeros).
    lines: Vec<pricing::PricedLine>,
    services: Vec<PlannedService>,
    service_lines: Vec<pricing::PricedLine>,
    totals: pricing::BillTotals,
}

impl Priced {
    /// Gross (before discount) of consultations, procedures and products.
    fn section_totals(&self) -> (i64, i64, i64) {
        let of_kind = |kind: &str| -> i64 {
            self.services.iter().zip(&self.service_lines).filter(|(s, _)| s.service.kind == kind).map(|(_, l)| l.gross.value()).sum()
        };
        (of_kind("CONSULTATION"), of_kind("PROCEDURE"), self.lines.iter().map(|l| l.gross.value()).sum())
    }
}

fn price(items: Vec<PlannedItem>, services: Vec<PlannedService>, discount: Discount, clinic: &ClinicSettings) -> Result<Priced, ServiceError> {
    if items.is_empty() && services.is_empty() {
        return Err(invalid("lines", "Add a consultation, procedure or product to the bill."));
    }
    // Products first (all discount-eligible for now), then consultations and procedures.
    let product_inputs = items.iter().filter(|i| i.qty > 0).map(|i| LineInput {
        unit_price: Paise::new(i.unit_price),
        qty: u32::try_from(i.qty).unwrap_or(u32::MAX),
        gst_rate: BasisPoints::new(u32::try_from(i.product.gst_rate_bp).unwrap_or(0)),
        discount_eligible: true,
    });
    let service_inputs = services.iter().map(|s| LineInput {
        unit_price: Paise::new(s.unit_price),
        qty: u32::try_from(s.qty).unwrap_or(u32::MAX),
        gst_rate: BasisPoints::new(u32::try_from(s.service.gst_rate_bp).unwrap_or(0)),
        discount_eligible: s.service.discount_eligible,
    });
    let inputs: Vec<LineInput> = product_inputs.chain(service_inputs).collect();
    let totals = pricing::price_bill(&inputs, discount, clinic.round_to_rupee).map_err(|e| invalid("discount", &format!("{}.", capitalize(&e.to_string()))))?;
    let supplied_count = items.iter().filter(|i| i.qty > 0).count();
    let mut supplied = totals.lines.iter().take(supplied_count);
    let zero = pricing::PricedLine { gross: Paise::ZERO, discount_share: Paise::ZERO, net: Paise::ZERO, tax: Paise::ZERO };
    let lines = items.iter().map(|i| if i.qty > 0 { supplied.next().copied().unwrap_or(zero) } else { zero }).collect();
    let service_lines = totals.lines.iter().skip(supplied_count).copied().collect();
    Ok(Priced { items, lines, services, service_lines, totals })
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

// ---- Quote -----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteLine {
    pub product_id: i64,
    pub product_name: String,
    pub unit: String,
    pub qty: i64,
    pub not_supplied_qty: i64,
    pub unit_price_paise: i64,
    pub gst_rate_bp: i64,
    pub gross_paise: i64,
    pub discount_share_paise: i64,
    pub net_paise: i64,
    pub tax_paise: i64,
    pub batch_nos: Vec<String>,
    pub expires_soon: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteServiceLine {
    pub service_id: i64,
    pub kind: String,
    pub name: String,
    pub qty: i64,
    pub unit_price_paise: i64,
    pub default_price_paise: i64,
    pub gross_paise: i64,
    pub discount_share_paise: i64,
    pub net_paise: i64,
    pub tax_paise: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub lines: Vec<QuoteLine>,
    /// Consultations and procedures, in the order sent.
    pub service_lines: Vec<QuoteServiceLine>,
    /// Gross of each section, before the discount.
    pub consultation_paise: i64,
    pub procedures_paise: i64,
    pub products_paise: i64,
    /// What the discount applies to (the medicines/products, plus any eligible services).
    pub eligible_subtotal_paise: i64,
    pub subtotal_paise: i64,
    pub discount_paise: i64,
    pub tax_paise: i64,
    pub round_off_paise: i64,
    pub total_paise: i64,
    pub discount_rate_bp: u32,
    /// The discount is above the receptionist limit: an administrator must approve it.
    pub needs_approval: bool,
}

/// The discount rate checked against the receptionist cap. A percent discount is compared as
/// typed (10% is exactly 10%, even if rounding to paise makes the amount a little higher);
/// a rupee discount is converted to a rate of the amount it applies to.
fn discount_rate(discount: Discount, totals: &pricing::BillTotals) -> u32 {
    if totals.discount.value() == 0 {
        return 0; // e.g. a percent typed on a bill with nothing the discount applies to
    }
    match discount {
        Discount::Percent(bp) => bp.value(),
        Discount::None | Discount::Amount(_) => pricing::discount_rate_bp(totals.discount, totals.eligible_subtotal),
    }
}

fn needs_approval(actor: &Session, rate_bp: u32, clinic: &ClinicSettings) -> bool {
    actor.role != Role::Admin && rate_bp > clinic.receptionist_discount_cap_percent * 100
}

/// Prices a bill without saving anything (the New Bill screen calls this as lines change, so
/// all money arithmetic stays in Rust, never in JavaScript).
/// `correcting`: id of the bill being corrected; its stock counts as available (it goes back first).
pub fn quote(
    db: &Database,
    actor: &Session,
    lines: &[BillLineInput],
    services: &[ServiceLineInput],
    discount: Discount,
    correcting: Option<i64>,
    now: i64,
) -> Result<Quote, ServiceError> {
    actor.require(Permission::CreateBills)?;
    db.read(|c| {
        let (clinic, today) = clinic_today(c, now)?;
        let give_back: Vec<(i64, i64)> = match correcting {
            Some(bill_id) => {
                let original = load_detail(c, bill_id)?;
                if original.bill.status == "FINALIZED" {
                    original.items.iter().flat_map(|i| i.batches.iter().map(|b| (b.batch_id, b.qty - b.returned_qty))).collect()
                } else {
                    Vec::new()
                }
            }
            None => Vec::new(),
        };
        let priced = price(plan_items(c, lines, today, &give_back)?, plan_services(c, services)?, discount, &clinic)?;
        let mut quote_lines = Vec::with_capacity(priced.items.len());
        for (item, line) in priced.items.iter().zip(&priced.lines) {
            let mut batch_nos = Vec::new();
            for a in &item.allocations {
                if let Some(b) = stock::find_batch(c, a.batch_id)? {
                    batch_nos.push(b.batch_no);
                }
            }
            quote_lines.push(QuoteLine {
                product_id: item.product.id,
                product_name: item.product.name.clone(),
                unit: item.product.unit.clone(),
                qty: item.qty,
                not_supplied_qty: item.not_supplied_qty,
                unit_price_paise: item.unit_price,
                gst_rate_bp: item.product.gst_rate_bp,
                gross_paise: line.gross.value(),
                discount_share_paise: line.discount_share.value(),
                net_paise: line.net.value(),
                tax_paise: line.tax.value(),
                batch_nos,
                expires_soon: item.expires_soon,
            });
        }
        let service_lines = priced
            .services
            .iter()
            .zip(&priced.service_lines)
            .map(|(s, line)| QuoteServiceLine {
                service_id: s.service.id,
                kind: s.service.kind.clone(),
                name: s.service.name.clone(),
                qty: s.qty,
                unit_price_paise: s.unit_price,
                default_price_paise: s.service.default_price_paise,
                gross_paise: line.gross.value(),
                discount_share_paise: line.discount_share.value(),
                net_paise: line.net.value(),
                tax_paise: line.tax.value(),
            })
            .collect();
        let (consultation_paise, procedures_paise, products_paise) = priced.section_totals();
        let t = &priced.totals;
        let rate = discount_rate(discount, t);
        Ok(Quote {
            lines: quote_lines,
            service_lines,
            consultation_paise,
            procedures_paise,
            products_paise,
            eligible_subtotal_paise: t.eligible_subtotal.value(),
            subtotal_paise: t.subtotal.value(),
            discount_paise: t.discount.value(),
            tax_paise: t.tax.value(),
            round_off_paise: t.round_off.value(),
            total_paise: t.total.value(),
            discount_rate_bp: rate,
            needs_approval: needs_approval(actor, rate, &clinic),
        })
    })
}

// ---- Finalize --------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillDetail {
    pub bill: BillRow,
    /// Medicines and products.
    pub items: Vec<BillItemRow>,
    /// Consultations and procedures.
    pub services: Vec<ServiceItemRow>,
    pub payments: Vec<PaymentRow>,
}

/// The bill already saved under this idempotency key, if any. A repeat (double-click, retry
/// after a timeout) comes from the same user for the same client and the same correction; a key
/// reused for anything else is refused instead of silently returning an unrelated bill.
fn bill_for_key(c: &Connection, key: &str, actor: &Session, input: &BillInput, replaces: Option<i64>) -> Result<Option<i64>, ServiceError> {
    let Some(bill_id) = repo::find_bill_id_by_key(c, key)? else {
        return Ok(None);
    };
    let bill = repo::find_bill(c, bill_id)?.ok_or(ServiceError::NotFound("bill"))?;
    // A client typed on the bill was created by the first attempt, so the retry cannot know its id.
    let client_matches = bill.client_id == input.client_id || (input.client_id.is_none() && input.new_client.is_some());
    if bill.created_by != actor.user_id || !client_matches || bill.replaces_bill_id != replaces {
        return Err(ServiceError::NotAllowed("This bill was already saved with different details. Clear the bill and start again.".into()));
    }
    Ok(Some(bill_id))
}

fn load_detail(c: &Connection, bill_id: i64) -> Result<BillDetail, ServiceError> {
    Ok(BillDetail {
        bill: repo::find_bill(c, bill_id)?.ok_or(ServiceError::NotFound("bill"))?,
        items: repo::items(c, bill_id)?,
        services: repo::service_items(c, bill_id)?,
        payments: repo::payments(c, bill_id)?,
    })
}

fn validate_key(key: &str) -> Result<(), ServiceError> {
    if !(8..=64).contains(&key.len()) || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(invalid("idempotencyKey", "Invalid bill key. Please start a new bill."));
    }
    Ok(())
}

/// Checks the administrator's credentials BEFORE the bill's transaction, so a wrong password is
/// counted and audited even though the bill is not saved (`auth::verify_admin_approval`).
fn verified_approval(db: &mut Database, approval: Option<&Approval>, now: i64) -> Result<Option<i64>, ServiceError> {
    approval.map(|a| crate::auth::verify_admin_approval(db, &a.username, &a.password, now)).transpose()
}

/// The approving administrator (already verified); `missing` is the error when none was given.
fn approve(c: &Connection, approved_admin: Option<i64>, missing: ServiceError) -> Result<i64, ServiceError> {
    let Some(admin_id) = approved_admin else { return Err(missing) };
    match users::find_by_id(c, admin_id)? {
        Some(admin) if admin.is_active && role_of(&admin)? == Role::Admin => Ok(admin_id),
        _ => Err(invalid("approval", "Administrator username or password is not correct.")),
    }
}

/// Creates the bill inside the caller's transaction and returns its id.
fn finalize_in_tx(c: &Connection, actor: &Session, input: &BillInput, now: i64, replaces: Option<i64>, approved_admin: Option<i64>) -> Result<i64, ServiceError> {
    let (clinic, today) = clinic_today(c, now)?;
    let client_id = match (input.client_id, &input.new_client) {
        (Some(client_id), _) => {
            if !clients::find(c, client_id)?.is_some_and(|client| client.is_active) {
                return Err(ServiceError::NotAllowed("Unable to complete the operation. The selected client is no longer available.".into()));
            }
            Some(client_id)
        }
        // Typed on the bill: saved now, in this same transaction (all or nothing with the bill).
        (None, Some(new)) => Some(crate::clients::create_for_bill(c, actor, &new.full_name, &new.phone, new.allow_duplicate, now)?),
        (None, None) => None,
    };
    let note = input.note.trim();
    if chars(note) > 200 {
        return Err(invalid("note", "Note must be at most 200 characters."));
    }
    let priced = price(plan_items(c, &input.lines, today, &[])?, plan_services(c, &input.services)?, input.discount, &clinic)?;
    let totals = &priced.totals;

    let rate = discount_rate(input.discount, totals);
    let approved_by = if needs_approval(actor, rate, &clinic) {
        Some(approve(c, approved_admin, ServiceError::DiscountApprovalRequired { cap_percent: clinic.receptionist_discount_cap_percent })?)
    } else {
        None
    };

    // Payments must add up to the total exactly (D13).
    for p in &input.payments {
        if !PAYMENT_METHODS.contains(&p.method.as_str()) || p.amount_paise <= 0 || chars(&p.reference) > 60 {
            return Err(invalid("payments", "Each payment needs a method and an amount above zero."));
        }
    }
    let paid: i64 = input.payments.iter().map(|p| p.amount_paise).sum();
    if paid != totals.total.value() {
        return Err(invalid("payments", &format!("Payments ({}) must add up to the bill total ({}).", rupees(paid), rupees(totals.total.value()))));
    }
    let cash: i64 = input.payments.iter().filter(|p| p.method == "CASH").map(|p| p.amount_paise).sum();
    let change = match input.amount_received_paise {
        Some(_) if cash == 0 => return Err(invalid("amountReceived", "Cash received can only be entered for a cash payment.")),
        Some(received) if received < cash => return Err(invalid("amountReceived", "Cash received is less than the cash payment.")),
        Some(received) => Some(received - cash),
        None => None,
    };

    let fy = today.financial_year();
    let bill_no = format!("{}/{}/{:06}", clinic.invoice_prefix, fy, next_number(c, "BILL", &fy)?);
    let bill_id = repo::insert_bill(
        c,
        &NewBill {
            bill_no: &bill_no,
            idempotency_key: &input.idempotency_key,
            client_id,
            subtotal_paise: totals.subtotal.value(),
            discount_paise: totals.discount.value(),
            tax_paise: totals.tax.value(),
            round_off_paise: totals.round_off.value(),
            total_paise: totals.total.value(),
            amount_received_paise: input.amount_received_paise,
            change_paise: change,
            note,
            created_by: actor.user_id,
            discount_approved_by: approved_by,
            replaces_bill_id: replaces,
            now,
        },
    )?;
    for (index, (item, line)) in priced.items.iter().zip(&priced.lines).enumerate() {
        let item_id = repo::insert_item(
            c,
            &NewBillItem {
                bill_id,
                line_no: index as i64 + 1,
                product_id: item.product.id,
                product_name: &item.product.name,
                sku: &item.product.sku,
                unit: &item.product.unit,
                qty: item.qty,
                not_supplied_qty: item.not_supplied_qty,
                unit_price_paise: item.unit_price,
                discount_share_paise: line.discount_share.value(),
                gst_rate_bp: item.product.gst_rate_bp,
                tax_paise: line.tax.value(),
                line_total_paise: line.net.value(),
            },
        )?;
        for allocation in &item.allocations {
            repo::insert_item_batch(c, item_id, allocation.batch_id, allocation.qty)?;
            let moved = stock::apply_movement(
                c,
                &Movement { batch_id: allocation.batch_id, kind: "SALE", qty_change: -allocation.qty, reason: &bill_no, bill_id: Some(bill_id), sales_return_id: None, user_id: actor.user_id, now },
            )?;
            if moved.is_none() {
                // Someone sold the same stock a moment ago: nothing of this bill is saved.
                return Err(ServiceError::InsufficientStock { product: item.product.name.clone(), available: 0 });
            }
        }
    }
    // Consultations and procedures: no stock involved. Names typed at the desk join the list.
    for (index, (planned, line)) in priced.services.iter().zip(&priced.service_lines).enumerate() {
        let service_id = match planned.service.id {
            0 => match catalog::find_by_name(c, &planned.service.kind, &planned.service.name)? {
                Some(existing) => existing.id, // the same new name twice on one bill
                None => {
                    let fields = ServiceFields {
                        kind: &planned.service.kind,
                        name: &planned.service.name,
                        default_price_paise: planned.unit_price,
                        gst_rate_bp: 0,
                        discount_eligible: false,
                        is_active: true,
                        sort_order: 100,
                    };
                    let id = catalog::insert(c, &fields, now)?;
                    audit::record(
                        c,
                        now,
                        Actor::from(actor),
                        "SERVICE_CREATE",
                        Some(("service", id.to_string())),
                        Some(json!({ "kind": planned.service.kind, "name": planned.service.name, "price": planned.unit_price, "addedOnBill": true })),
                    )?;
                    id
                }
            },
            id => {
                // A deactivated entry typed again by name comes back into the list (owner decision).
                if !planned.service.is_active {
                    catalog::set_active(c, id, true, now)?;
                    audit::record(c, now, Actor::from(actor), "SERVICE_REACTIVATE", Some(("service", id.to_string())), Some(json!({ "name": planned.service.name, "onBill": true })))?;
                }
                id
            }
        };
        repo::insert_service_item(
            c,
            &NewServiceItem {
                bill_id,
                line_no: index as i64 + 1,
                service_id,
                kind: &planned.service.kind,
                name: &planned.service.name,
                qty: planned.qty,
                unit_price_paise: planned.unit_price,
                default_price_paise: planned.service.default_price_paise,
                discount_eligible: planned.service.discount_eligible,
                discount_share_paise: line.discount_share.value(),
                gst_rate_bp: planned.service.gst_rate_bp,
                tax_paise: line.tax.value(),
                line_total_paise: line.net.value(),
            },
        )?;
    }
    for p in &input.payments {
        repo::insert_payment(c, &NewPayment { bill_id, method: &p.method, amount_paise: p.amount_paise, direction: "IN", reference: p.reference.trim(), sales_return_id: None, now })?;
    }
    if let Some(client_id) = client_id {
        clients::touch_visit(c, client_id, now)?;
    }
    audit::record(
        c,
        now,
        Actor::from(actor),
        "BILL_FINALIZE",
        Some(("bill", bill_id.to_string())),
        Some(json!({ "billNo": bill_no, "total": totals.total.value(), "items": priced.items.len(), "services": priced.services.len(), "approvedBy": approved_by })),
    )?;
    Ok(bill_id)
}

/// Saves the bill, takes the stock and records payments, all or nothing (design §11.1).
pub fn finalize(db: &mut Database, actor: &Session, input: BillInput, now: i64) -> Result<BillDetail, ServiceError> {
    actor.require(Permission::CreateBills)?;
    validate_key(&input.idempotency_key)?;
    let approved_admin = verified_approval(db, input.approval.as_ref(), now)?;
    db.write(|c| {
        verify_actor(c, actor)?;
        if let Some(existing) = bill_for_key(c, &input.idempotency_key, actor, &input, None)? {
            return load_detail(c, existing); // double-click, retry or restart: same bill
        }
        let bill_id = finalize_in_tx(c, actor, &input, now, None, approved_admin)?;
        load_detail(c, bill_id)
    })
}

// ---- Reading ---------------------------------------------------------------------------------

pub fn get(db: &Database, actor: &Session, bill_id: i64) -> Result<BillDetail, ServiceError> {
    actor.require(Permission::CreateBills)?;
    db.read(|c| load_detail(c, bill_id))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BillFilter {
    /// Local dates, inclusive (YYYY-MM-DD).
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub status: Option<String>,
    pub client_id: Option<i64>,
    pub text: String,
    /// How many rows ("Load more" asks for more); default 300, at most 5000.
    pub limit: Option<u32>,
}

pub fn list(db: &Database, actor: &Session, filter: BillFilter) -> Result<Vec<BillRow>, ServiceError> {
    actor.require(Permission::CreateBills)?;
    db.read(|c| {
        let (clinic, _) = clinic_today(c, 0)?;
        let bound = |text: &Option<String>, field: &'static str, extra: i64| -> Result<Option<i64>, ServiceError> {
            match text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                None => Ok(None),
                Some(t) => Date::parse(t).map(|d| Some(local_day_start_utc(d.add_days(extra), clinic.utc_offset_minutes))).ok_or_else(|| invalid(field, "Please enter a valid date.")),
            }
        };
        let status = filter.status.as_deref().filter(|s| !s.is_empty());
        Ok(repo::list_bills(
            c,
            &BillQuery { from: bound(&filter.from_date, "fromDate", 0)?, to: bound(&filter.to_date, "toDate", 1)?, status, client_id: filter.client_id, text: &filter.text, limit: filter.limit.unwrap_or(300).clamp(1, 5_000) },
        )?)
    })
}

// ---- Cancel ----------------------------------------------------------------------------------

/// Puts back all stock of a bill that has not been returned, and refunds its payments.
fn reverse_in_tx(c: &Connection, detail: &BillDetail, actor: &Session, reason: &str, now: i64) -> Result<(), ServiceError> {
    for item in &detail.items {
        for share in &item.batches {
            let back = share.qty - share.returned_qty;
            if back > 0 {
                stock::apply_movement(
                    c,
                    &Movement { batch_id: share.batch_id, kind: "CANCELLATION", qty_change: back, reason, bill_id: Some(detail.bill.id), sales_return_id: None, user_id: actor.user_id, now },
                )?
                .ok_or_else(|| ServiceError::Corrupt("could not restore stock".into()))?;
            }
        }
    }
    for p in detail.payments.iter().filter(|p| p.direction == "IN") {
        repo::insert_payment(c, &NewPayment { bill_id: detail.bill.id, method: &p.method, amount_paise: p.amount_paise, direction: "REFUND", reference: reason, sales_return_id: None, now })?;
    }
    Ok(())
}

fn validate_reason(reason: &str) -> Result<&str, ServiceError> {
    let reason = reason.trim();
    if chars(reason) < 3 || chars(reason) > 200 {
        return Err(invalid("reason", "Please give a reason (3 to 200 characters)."));
    }
    Ok(reason)
}

fn closable(detail: &BillDetail) -> Result<(), ServiceError> {
    if detail.bill.status != "FINALIZED" {
        return Err(ServiceError::NotAllowed(format!("Bill {} is already {}.", detail.bill.bill_no, detail.bill.status.to_lowercase())));
    }
    if detail.bill.returned_paise > 0 || detail.items.iter().any(|i| i.returned_qty > 0) {
        return Err(ServiceError::NotAllowed("This bill has returns, so it cannot be cancelled or corrected. Return the remaining items instead.".into()));
    }
    Ok(())
}

/// Administrators only: marks the bill CANCELLED (never deleted), restores its stock to the
/// same batches and records refunds (D17).
pub fn cancel(db: &mut Database, actor: &Session, bill_id: i64, reason: &str, now: i64) -> Result<BillDetail, ServiceError> {
    actor.require(Permission::CancelBills)?;
    let reason = validate_reason(reason)?;
    db.write(|c| {
        verify_actor(c, actor)?;
        let detail = load_detail(c, bill_id)?;
        closable(&detail)?;
        reverse_in_tx(c, &detail, actor, reason, now)?;
        repo::close_bill(c, bill_id, "CANCELLED", actor.user_id, reason, now)?;
        audit::record(c, now, Actor::from(actor), "BILL_CANCEL", Some(("bill", bill_id.to_string())), Some(json!({ "billNo": detail.bill.bill_no, "total": detail.bill.total_paise })))?;
        load_detail(c, bill_id)
    })
}

// ---- Correct (cancel + reissue, DEC-001) -----------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionInput {
    pub original_bill_id: i64,
    pub reason: String,
    pub bill: BillInput,
}

/// Replaces a bill with a corrected one: the original becomes CORRECTED (kept for audit), its
/// stock and payments are reversed, and a new bill with a new number is created, all in one
/// transaction. Receptionists: same day only; administrators: any time.
pub fn correct(db: &mut Database, actor: &Session, input: CorrectionInput, now: i64) -> Result<BillDetail, ServiceError> {
    actor.require(Permission::CreateBills)?;
    validate_key(&input.bill.idempotency_key)?;
    let reason = validate_reason(&input.reason)?.to_string();
    let approved_admin = verified_approval(db, input.bill.approval.as_ref(), now)?;
    db.write(|c| {
        verify_actor(c, actor)?;
        if let Some(existing) = bill_for_key(c, &input.bill.idempotency_key, actor, &input.bill, Some(input.original_bill_id))? {
            return load_detail(c, existing);
        }
        let original = load_detail(c, input.original_bill_id)?;
        closable(&original)?;
        let (clinic, today) = clinic_today(c, now)?;
        let same_day = local_date(original.bill.finalized_at, clinic.utc_offset_minutes) == today;
        if !same_day && !actor.role.allows(Permission::CancelBills) {
            return Err(ServiceError::NotAllowed("Bills from earlier days can only be corrected by an administrator.".into()));
        }
        let note = format!("Correction of {}: {}", original.bill.bill_no, reason);
        reverse_in_tx(c, &original, actor, &note, now)?;
        repo::close_bill(c, original.bill.id, "CORRECTED", actor.user_id, &reason, now)?;
        let new_id = finalize_in_tx(c, actor, &input.bill, now, Some(original.bill.id), approved_admin)?;
        repo::set_corrected_by(c, original.bill.id, new_id)?;
        audit::record(c, now, Actor::from(actor), "BILL_CORRECT", Some(("bill", original.bill.id.to_string())), Some(json!({ "from": original.bill.bill_no, "toBillId": new_id })))?;
        load_detail(c, new_id)
    })
}

// ---- Returns ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnLineInput {
    pub bill_item_id: i64,
    pub qty: i64,
    /// Put back on the shelf (ignored for expired batches, which are written off).
    #[serde(default = "yes")]
    pub restock: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnInput {
    pub bill_id: i64,
    pub lines: Vec<ReturnLineInput>,
    pub reason: String,
    pub refund_method: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnResult {
    pub return_no: String,
    pub refund_paise: i64,
    pub bill: BillDetail,
}

struct ReturnPortion {
    item_id: i64,
    item_batch_id: i64,
    batch_id: i64,
    qty: i64,
    refund: i64,
    restock: bool,
}

/// Returns items against the original bill (D15): stock back to the same batch (or written off
/// as damaged), refund pro-rata to what was paid for the line, recorded against the bill.
pub fn return_items(db: &mut Database, actor: &Session, input: ReturnInput, now: i64) -> Result<ReturnResult, ServiceError> {
    actor.require(Permission::ProcessReturns)?;
    let reason = validate_reason(&input.reason)?;
    if !PAYMENT_METHODS.contains(&input.refund_method.as_str()) {
        return Err(invalid("refundMethod", "Choose how the money is refunded."));
    }
    if input.lines.is_empty() || input.lines.iter().any(|l| l.qty <= 0) {
        return Err(invalid("lines", "Choose at least one item and a quantity to return."));
    }
    // One line per bill item: the per-item limits below are checked against the saved bill.
    let mut seen = std::collections::HashSet::new();
    if !input.lines.iter().all(|l| seen.insert(l.bill_item_id)) {
        return Err(invalid("lines", "Each item can appear only once in a return."));
    }
    db.write(|c| {
        verify_actor(c, actor)?;
        let detail = load_detail(c, input.bill_id)?;
        if detail.bill.status != "FINALIZED" {
            return Err(ServiceError::NotAllowed(format!("Bill {} is {}; items cannot be returned.", detail.bill.bill_no, detail.bill.status.to_lowercase())));
        }
        let (clinic, today) = clinic_today(c, now)?;
        // Calendar days in clinic time: a bill from the 1st with a 7-day window can be returned
        // until the end of the 8th.
        let last_day = local_date(detail.bill.finalized_at, clinic.utc_offset_minutes).add_days(i64::from(clinic.return_window_days));
        if last_day < today && !actor.role.allows(Permission::CancelBills) {
            return Err(ServiceError::NotAllowed(format!("Returns more than {} days after the bill need an administrator.", clinic.return_window_days)));
        }
        let mut portions = Vec::new();
        for line in &input.lines {
            let item = detail.items.iter().find(|i| i.id == line.bill_item_id).ok_or(ServiceError::NotFound("bill item"))?;
            let returnable = item.qty - item.returned_qty;
            if line.qty > returnable {
                return Err(invalid("lines", &format!("Only {returnable} of {} can still be returned.", item.product_name)));
            }
            let refund = if line.qty == returnable {
                item.line_total_paise - repo::refunded_for_item(c, item.id)?
            } else {
                i64::try_from(i128::from(item.line_total_paise) * i128::from(line.qty) / i128::from(item.qty)).unwrap_or(0)
            };
            // Take the returned units from the item's batches in order, then split the refund.
            let mut remaining = line.qty;
            let mut takes = Vec::new();
            for share in &item.batches {
                let take = remaining.min(share.qty - share.returned_qty);
                if take > 0 {
                    let expired = share.expiry_date.as_deref().and_then(Date::parse).is_some_and(|e| e < today);
                    takes.push((share, take, line.restock && !expired));
                    remaining -= take;
                }
            }
            let refunds = pricing::allocate_proportionally(refund, &takes.iter().map(|(_, take, _)| *take).collect::<Vec<_>>());
            for ((share, take, restock), part) in takes.into_iter().zip(refunds) {
                portions.push(ReturnPortion { item_id: item.id, item_batch_id: share.id, batch_id: share.batch_id, qty: take, refund: part, restock });
            }
        }
        // Line totals are before the bill's round-off. When this return completes the bill, refund
        // exactly what is left of the amount collected, so refunds never exceed the bill total.
        let completes_bill = detail.items.iter().all(|item| {
            let now_returning: i64 = input.lines.iter().filter(|l| l.bill_item_id == item.id).map(|l| l.qty).sum();
            item.returned_qty + now_returning == item.qty
        });
        // Consultations and procedures are not returned (correct or cancel the bill instead), so
        // their share of the total is never refunded here.
        let services_total: i64 = detail.services.iter().map(|s| s.line_total_paise).sum();
        if completes_bill {
            let left_to_refund = (detail.bill.total_paise - services_total - detail.bill.returned_paise).max(0);
            let difference = left_to_refund - portions.iter().map(|p| p.refund).sum::<i64>();
            if let Some(largest) = portions.iter_mut().max_by_key(|p| p.refund) {
                largest.refund = (largest.refund + difference).max(0);
            }
        }
        // Partial returns too: all refunds together never exceed what the client paid.
        let mut excess = portions.iter().map(|p| p.refund).sum::<i64>() - (detail.bill.total_paise - services_total - detail.bill.returned_paise).max(0);
        while excess > 0 {
            let Some(largest) = portions.iter_mut().filter(|p| p.refund > 0).max_by_key(|p| p.refund) else { break };
            let cut = excess.min(largest.refund);
            largest.refund -= cut;
            excess -= cut;
        }
        let refund_total: i64 = portions.iter().map(|p| p.refund).sum();
        let fy = today.financial_year();
        let return_no = format!("RET/{}/{:06}", fy, next_number(c, "RETURN", &fy)?);
        let return_id = repo::insert_return(
            c,
            &NewReturn { return_no: &return_no, bill_id: detail.bill.id, reason, refund_paise: refund_total, refund_method: &input.refund_method, user_id: actor.user_id, now },
        )?;
        let damaged_reason = format!("Returned, not restocked: {reason}");
        for p in &portions {
            let movement = |kind: &'static str, qty_change: i64, why: &str| -> Result<(), ServiceError> {
                stock::apply_movement(
                    c,
                    &Movement { batch_id: p.batch_id, kind, qty_change, reason: why, bill_id: Some(detail.bill.id), sales_return_id: Some(return_id), user_id: actor.user_id, now },
                )?
                .ok_or_else(|| ServiceError::Corrupt("return stock movement failed".into()))?;
                Ok(())
            };
            movement("RETURN", p.qty, &return_no)?;
            if !p.restock {
                movement("DAMAGE", -p.qty, &damaged_reason)?;
            }
            repo::insert_return_item(c, return_id, p.item_id, p.batch_id, p.qty, p.refund, p.restock)?;
            repo::record_returned(c, detail.bill.id, p.item_id, p.item_batch_id, p.qty, p.refund)?;
        }
        if refund_total > 0 {
            repo::insert_payment(c, &NewPayment { bill_id: detail.bill.id, method: &input.refund_method, amount_paise: refund_total, direction: "REFUND", reference: &return_no, sales_return_id: Some(return_id), now })?;
        }
        audit::record(c, now, Actor::from(actor), "RETURN_CREATE", Some(("bill", detail.bill.id.to_string())), Some(json!({ "returnNo": return_no, "refund": refund_total })))?;
        Ok(ReturnResult { return_no, refund_paise: refund_total, bill: load_detail(c, detail.bill.id)? })
    })
}

// ---- Receipt ---------------------------------------------------------------------------------

fn method_label(method: &str) -> String {
    match method {
        "CASH" => "Cash",
        "UPI" => "UPI",
        "CARD" => "Card",
        _ => "Other",
    }
    .to_string()
}

/// `2026-12-31` -> `12/2026` (receipts show month and year of expiry).
fn short_expiry(date: &str) -> String {
    Date::parse(date).map_or_else(|| date.to_string(), |d| format!("{:02}/{}", d.month, d.year))
}

/// Everything printed on the receipt, for the PDF and the on-screen/print preview.
pub fn receipt(db: &Database, actor: &Session, bill_id: i64) -> Result<ReceiptData, ServiceError> {
    actor.require(Permission::CreateBills)?;
    db.read(|c| {
        let detail = load_detail(c, bill_id)?;
        let (clinic, _) = clinic_today(c, 0)?;
        let bill = &detail.bill;
        // Headings and section subtotals only when the bill has consultations or procedures;
        // a medicines-only bill prints exactly as before.
        let sectioned = !detail.services.is_empty();
        let service_line = |s: &ServiceItemRow, section: &str| ReceiptLine {
            name: s.name.clone(),
            detail: None,
            qty: u32::try_from(s.qty).unwrap_or(0),
            unit_price: Paise::new(s.unit_price_paise),
            discount: Paise::new(s.discount_share_paise),
            amount: Paise::new(s.line_total_paise),
            not_supplied_qty: 0,
            section: section.to_string(),
        };
        let consultations: Vec<&ServiceItemRow> = detail.services.iter().filter(|s| s.kind == "CONSULTATION").collect();
        let procedures: Vec<&ServiceItemRow> = detail.services.iter().filter(|s| s.kind == "PROCEDURE").collect();
        let product_section = if sectioned { "Medicines & Products" } else { "" };
        let mut lines: Vec<ReceiptLine> = consultations.iter().map(|&s| service_line(s, "Consultation")).collect();
        lines.extend(procedures.iter().map(|&s| service_line(s, "Procedures")));
        let product_lines: Vec<ReceiptLine> = detail
            .items
            .iter()
            .map(|i| ReceiptLine {
                name: i.product_name.clone(),
                detail: (!i.batches.is_empty()).then(|| {
                    i.batches
                        .iter()
                        // Lot numbers made up by Add Inventory mean nothing to patients: expiry only.
                        .map(|b| match (&b.expiry_date, b.batch_no.starts_with("LOT-")) {
                            (Some(e), true) => format!("Exp {}", short_expiry(e)),
                            (Some(e), false) => format!("Batch {} · Exp {}", b.batch_no, short_expiry(e)),
                            (None, _) => format!("Batch {}", b.batch_no),
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                }),
                qty: u32::try_from(i.qty).unwrap_or(0),
                unit_price: Paise::new(i.unit_price_paise),
                discount: Paise::new(i.discount_share_paise),
                amount: Paise::new(i.line_total_paise),
                not_supplied_qty: u32::try_from(i.not_supplied_qty).unwrap_or(0),
                section: product_section.to_string(),
            })
            .collect();
        lines.extend(product_lines);
        let gross_of = |rows: &[&ServiceItemRow]| -> i64 { rows.iter().map(|s| s.line_total_paise + s.discount_share_paise).sum() };
        let products_gross: i64 = detail.items.iter().map(|i| i.line_total_paise + i.discount_share_paise).sum();
        let mut breakdown = Vec::new();
        if sectioned {
            if !consultations.is_empty() {
                breakdown.push(ReceiptTotal { label: "Consultation".into(), amount: Paise::new(gross_of(consultations.as_slice())) });
            }
            if !procedures.is_empty() {
                breakdown.push(ReceiptTotal { label: "Procedures".into(), amount: Paise::new(gross_of(procedures.as_slice())) });
            }
            if !detail.items.is_empty() {
                breakdown.push(ReceiptTotal { label: "Medicines & products".into(), amount: Paise::new(products_gross) });
            }
        }
        let discount_label = if sectioned && detail.services.iter().all(|s| !s.discount_eligible) { "Discount on medicines" } else { "Discount" };
        let status_banner = match bill.status.as_str() {
            "CANCELLED" => Some("CANCELLED".to_string()),
            "CORRECTED" => Some(format!("CORRECTED - see {}", bill.corrected_by_bill_no.as_deref().unwrap_or("new bill"))),
            _ if bill.returned_paise > 0 => Some(format!("PARTLY RETURNED - {} refunded", rupees(bill.returned_paise))),
            _ => None,
        };
        let non_empty = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
        Ok(ReceiptData {
            clinic_name: clinic.name.clone(),
            clinic_address_lines: clinic.address_lines.clone(),
            clinic_phone: non_empty(&clinic.phone),
            clinic_gstin: non_empty(&clinic.gstin),
            status_banner,
            bill_no: bill.bill_no.clone(),
            date_time: format_local_datetime(bill.finalized_at, clinic.utc_offset_minutes),
            client_label: bill.client_name.as_ref().map(|name| match &bill.client_code {
                Some(code) => format!("{name} ({code})"),
                None => name.clone(),
            }),
            lines,
            subtotal: Paise::new(bill.subtotal_paise),
            breakdown,
            discount_label: discount_label.to_string(),
            discount: Paise::new(bill.discount_paise),
            tax_label: "GST included".to_string(),
            tax: Paise::new(bill.tax_paise),
            round_off: Paise::new(bill.round_off_paise),
            total: Paise::new(bill.total_paise),
            payments: detail
                .payments
                .iter()
                .filter(|p| p.direction == "IN")
                .map(|p| ReceiptPayment { method: method_label(&p.method), amount: Paise::new(p.amount_paise) })
                .collect(),
            amount_received: bill.amount_received_paise.map(Paise::new),
            change_due: bill.change_paise.map(Paise::new),
            billed_by: Some(bill.created_by_name.clone()),
            footer: non_empty(&clinic.receipt_footer),
            notice: Some(E_RECEIPT_NOTICE.to_string()),
        })
    })
}
