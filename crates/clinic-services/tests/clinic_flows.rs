//! Inventory, billing, returns, cancellation, correction, clients and reports, end to end
//! against a real (in-memory) SQLite database.

use clinic_core::auth::Role;
use clinic_core::money::{BasisPoints, Paise};
use clinic_core::pricing::Discount;
use clinic_services::auth::{self, NewAccount, SetupInput};
use clinic_services::billing::{self, Approval, BillInput, BillLineInput, CorrectionInput, PaymentInput, ReturnInput, ReturnLineInput};
use clinic_services::clients::{self, ClientInput};
use clinic_services::inventory::{self, AdjustInput, ProductFilter, ProductInput, StockInInput};
use clinic_services::reports::{self, DateRange};
use clinic_services::settings::ClinicSettings;
use clinic_services::{ServiceError, Session, users};
use clinic_sqlite::Database;
use clinic_sqlite::repo::inventory::ledger_mismatches;

type TestResult = Result<(), ServiceError>;

/// 2026-09-25 10:00 in India (04:30 UTC).
const NOW: i64 = 1_790_310_600;
const DAY: i64 = 86_400;

struct Clinic {
    db: Database,
    owner: Session,
    reception: Session,
    paracetamol: i64,
    cream: i64,
}

fn account(username: &str, password: &str) -> NewAccount {
    NewAccount { username: username.into(), full_name: format!("{username} name"), password: password.into(), pin: None }
}

fn product(name: &str, unit: &str, gst_bp: i64, min_stock: i64) -> ProductInput {
    ProductInput {
        id: None,
        sku: String::new(),
        name: name.into(),
        generic_name: String::new(),
        category_id: None,
        product_type: "TABLET".into(),
        manufacturer: String::new(),
        unit: unit.into(),
        gst_rate_bp: gst_bp,
        default_selling_price_paise: 0,
        default_purchase_price_paise: 0,
        min_stock,
        requires_expiry: true,
        is_active: true,
        notes: String::new(),
    }
}

fn stock_in(product_id: i64, batch: &str, expiry: &str, price: i64, qty: i64) -> StockInInput {
    StockInInput {
        product_id,
        batch_no: batch.into(),
        expiry_date: Some(expiry.into()),
        supplier_id: None,
        purchase_price_paise: price / 2,
        selling_price_paise: price,
        qty,
        opening: false,
        note: String::new(),
    }
}

/// Owner + receptionist; Paracetamol (12% GST, ₹20) 100 in stock; cream (18%, ₹120) 10 in stock.
fn clinic() -> Result<Clinic, ServiceError> {
    let mut db = Database::open_in_memory()?;
    let owner = auth::complete_setup(
        &mut db,
        SetupInput { clinic: ClinicSettings { name: "Test Clinic".into(), ..ClinicSettings::default() }, admin: account("owner", "owner-pass-1"), second_admin: None },
        NOW,
    )?;
    users::create(&mut db, &owner, account("reception", "front-desk-1"), Role::Receptionist, NOW)?;
    let reception = auth::login(&mut db, "reception", "front-desk-1", NOW)?;
    let paracetamol = inventory::save_product(&mut db, &owner, product("Paracetamol 500mg", "strip", 1_200, 20), NOW)?.id;
    let cream = inventory::save_product(&mut db, &owner, ProductInput { product_type: "CREAM".into(), ..product("Pain Relief Cream", "tube", 1_800, 2) }, NOW)?.id;
    inventory::stock_in(&mut db, &owner, stock_in(paracetamol, "A1", "2027-06-30", 2_000, 100), NOW)?;
    inventory::stock_in(&mut db, &owner, stock_in(cream, "C1", "2027-12-31", 12_000, 10), NOW)?;
    Ok(Clinic { db, owner, reception, paracetamol, cream })
}

fn line(product_id: i64, qty: i64) -> BillLineInput {
    BillLineInput { product_id, qty, not_supplied_qty: 0 }
}

fn cash(amount: i64) -> Vec<PaymentInput> {
    vec![PaymentInput { method: "CASH".into(), amount_paise: amount, reference: String::new() }]
}

fn bill(key: &str, lines: Vec<BillLineInput>, discount: Discount, payments: Vec<PaymentInput>) -> BillInput {
    BillInput { idempotency_key: key.into(), client_id: None, lines, discount, payments, amount_received_paise: None, note: String::new(), approval: None }
}

fn sellable(clinic: &Clinic, product_id: i64) -> Result<i64, ServiceError> {
    Ok(inventory::get_product(&clinic.db, &clinic.owner, product_id, NOW)?.product.sellable_qty)
}

fn assert_ledger_consistent(clinic: &Clinic) -> TestResult {
    let mismatches = clinic.db.read(ledger_mismatches)?;
    assert!(mismatches.is_empty(), "stock must always equal its ledger: {mismatches:?}");
    Ok(())
}

#[test]
fn the_example_bill_deducts_stock_and_prices_correctly() -> TestResult {
    let mut clinic = clinic()?;
    let input = bill("bill-key-0001", vec![line(clinic.paracetamol, 2), line(clinic.cream, 1)], Discount::Amount(Paise::new(1_000)), cash(15_000));
    let quote = billing::quote(&clinic.db, &clinic.reception, &input.lines, input.discount, None, NOW)?;
    assert_eq!((quote.subtotal_paise, quote.discount_paise, quote.total_paise), (16_000, 1_000, 15_000));

    let done = billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    assert_eq!(done.bill.bill_no, "INV/26-27/000001");
    assert_eq!(done.bill.total_paise, 15_000);
    assert_eq!(done.bill.tax_paise, 402 + 1_716);
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 98, "100 - 2");
    assert_eq!(sellable(&clinic, clinic.cream)?, 9);

    let receipt = billing::receipt(&clinic.db, &clinic.reception, done.bill.id)?;
    assert_eq!(receipt.total, Paise::new(15_000));
    assert!(receipt.lines[0].detail.as_deref().is_some_and(|d| d.contains("Batch A1")));
    assert_ledger_consistent(&clinic)
}

#[test]
fn insufficient_stock_is_refused_and_nothing_changes() -> TestResult {
    let mut clinic = clinic()?;
    let result = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0002", vec![line(clinic.paracetamol, 1), line(clinic.cream, 11)], Discount::None, cash(134_000)), NOW);
    match result {
        Err(ServiceError::InsufficientStock { available, .. }) => assert_eq!(available, 10),
        other => panic!("expected insufficient stock, got {other:?}"),
    }
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 100, "the whole bill is rolled back, including the valid line");
    assert!(billing::list(&clinic.db, &clinic.owner, Default::default())?.is_empty());
    Ok(())
}

#[test]
fn a_double_submitted_bill_is_created_once() -> TestResult {
    let mut clinic = clinic()?;
    let input = bill("bill-key-0003", vec![line(clinic.paracetamol, 3)], Discount::None, cash(6_000));
    let first = billing::finalize(&mut clinic.db, &clinic.reception, input.clone(), NOW)?;
    let second = billing::finalize(&mut clinic.db, &clinic.reception, input, NOW + 1)?;
    assert_eq!(first.bill.id, second.bill.id);
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 97, "stock taken once");
    assert_eq!(billing::list(&clinic.db, &clinic.owner, Default::default())?.len(), 1);
    Ok(())
}

#[test]
fn fefo_sells_the_soonest_expiring_batch_and_never_expired_stock() -> TestResult {
    let mut clinic = clinic()?;
    inventory::stock_in(&mut clinic.db, &clinic.owner, stock_in(clinic.paracetamol, "A0", "2026-12-31", 2_000, 5), NOW)?;
    let sold = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0004", vec![line(clinic.paracetamol, 7)], Discount::None, cash(14_000)), NOW)?;
    let batches: Vec<(String, i64)> = sold.items.iter().flat_map(|i| i.batches.iter().map(|b| (b.batch_no.clone(), b.qty))).collect();
    assert_eq!(batches, vec![("A0".to_string(), 5), ("A1".to_string(), 2)]);

    // Six months later batch A1 (expiry 2027-06-30) is still fine, but pretend a batch expired.
    let expired_day = NOW + 400 * DAY;
    let result = billing::quote(&clinic.db, &clinic.reception, &[line(clinic.paracetamol, 1)], Discount::None, None, expired_day);
    assert!(matches!(result, Err(ServiceError::InsufficientStock { available: 0, .. })), "expired stock is not sellable");
    Ok(())
}

#[test]
fn out_of_stock_items_are_recorded_as_not_supplied() -> TestResult {
    let mut clinic = clinic()?;
    let lines = vec![line(clinic.cream, 1), BillLineInput { product_id: clinic.paracetamol, qty: 0, not_supplied_qty: 3 }];
    let done = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0005", lines, Discount::None, cash(12_000)), NOW)?;
    let not_supplied = done.items.iter().find(|i| i.not_supplied_qty == 3).expect("not-supplied line");
    assert_eq!((not_supplied.qty, not_supplied.line_total_paise), (0, 0));
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 100, "nothing taken for a not-supplied line");
    Ok(())
}

#[test]
fn payments_must_match_and_change_is_calculated() -> TestResult {
    let mut clinic = clinic()?;
    let short = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0006", vec![line(clinic.paracetamol, 1)], Discount::None, cash(1_000)), NOW);
    assert!(matches!(short, Err(ServiceError::Validation { field: "payments", .. })));

    let mut split = bill("bill-key-0007", vec![line(clinic.cream, 1)], Discount::None, cash(5_000));
    split.payments.push(PaymentInput { method: "UPI".into(), amount_paise: 7_000, reference: "UPI123".into() });
    split.amount_received_paise = Some(10_000);
    let done = billing::finalize(&mut clinic.db, &clinic.reception, split, NOW)?;
    assert_eq!(done.bill.change_paise, Some(5_000));
    assert_eq!(done.payments.len(), 2);
    Ok(())
}

#[test]
fn large_discounts_by_receptionists_need_an_admin() -> TestResult {
    let mut clinic = clinic()?;
    let mut input = bill("bill-key-0008", vec![line(clinic.cream, 1)], Discount::Percent(BasisPoints::new(2_000)), cash(9_600));
    let quote = billing::quote(&clinic.db, &clinic.reception, &input.lines, input.discount, None, NOW)?;
    assert!(quote.needs_approval);
    assert!(matches!(billing::finalize(&mut clinic.db, &clinic.reception, input.clone(), NOW), Err(ServiceError::DiscountApprovalRequired { cap_percent: 10 })));
    input.approval = Some(Approval { username: "owner".into(), password: "wrong".into() });
    assert!(matches!(billing::finalize(&mut clinic.db, &clinic.reception, input.clone(), NOW), Err(ServiceError::Validation { field: "approval", .. })));
    input.approval = Some(Approval { username: "owner".into(), password: "owner-pass-1".into() });
    billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    Ok(())
}

#[test]
fn returns_restore_stock_and_refund_pro_rata() -> TestResult {
    let mut clinic = clinic()?;
    let done = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0009", vec![line(clinic.paracetamol, 5)], Discount::None, cash(10_000)), NOW)?;
    let item = done.items[0].id;
    let returned = billing::return_items(
        &mut clinic.db,
        &clinic.reception,
        ReturnInput { bill_id: done.bill.id, lines: vec![ReturnLineInput { bill_item_id: item, qty: 2, restock: true }], reason: "Not needed".into(), refund_method: "CASH".into() },
        NOW + 60,
    )?;
    assert_eq!(returned.return_no, "RET/26-27/000001");
    assert_eq!(returned.refund_paise, 4_000);
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 97, "95 + 2 returned");

    let too_many = billing::return_items(
        &mut clinic.db,
        &clinic.reception,
        ReturnInput { bill_id: done.bill.id, lines: vec![ReturnLineInput { bill_item_id: item, qty: 4, restock: true }], reason: "More".into(), refund_method: "CASH".into() },
        NOW + 120,
    );
    assert!(matches!(too_many, Err(ServiceError::Validation { .. })), "only 3 left to return");

    let damaged = billing::return_items(
        &mut clinic.db,
        &clinic.reception,
        ReturnInput { bill_id: done.bill.id, lines: vec![ReturnLineInput { bill_item_id: item, qty: 3, restock: false }], reason: "Packet opened".into(), refund_method: "UPI".into() },
        NOW + 180,
    )?;
    assert_eq!(damaged.refund_paise, 6_000, "the last units get the remaining amount exactly");
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 97, "not restocked: written off as damaged");
    assert_eq!(damaged.bill.bill.returned_paise, 10_000);

    let late = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0010", vec![line(clinic.cream, 1)], Discount::None, cash(12_000)), NOW)?;
    let too_late = billing::return_items(
        &mut clinic.db,
        &clinic.reception,
        ReturnInput { bill_id: late.bill.id, lines: vec![ReturnLineInput { bill_item_id: late.items[0].id, qty: 1, restock: true }], reason: "Late".into(), refund_method: "CASH".into() },
        NOW + 8 * DAY,
    );
    assert!(matches!(too_late, Err(ServiceError::NotAllowed(_))), "receptionists: 7-day window");
    assert_ledger_consistent(&clinic)
}

#[test]
fn cancelling_restores_stock_keeps_the_bill_and_is_admin_only() -> TestResult {
    let mut clinic = clinic()?;
    let done = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0011", vec![line(clinic.paracetamol, 3)], Discount::None, cash(6_000)), NOW)?;
    assert!(matches!(billing::cancel(&mut clinic.db, &clinic.reception, done.bill.id, "Mistake", NOW), Err(ServiceError::PermissionDenied)));
    let cancelled = billing::cancel(&mut clinic.db, &clinic.owner, done.bill.id, "Wrong client", NOW)?;
    assert_eq!(cancelled.bill.status, "CANCELLED");
    assert_eq!(cancelled.bill.cancel_reason.as_deref(), Some("Wrong client"));
    assert!(cancelled.payments.iter().any(|p| p.direction == "REFUND" && p.amount_paise == 6_000));
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 100);
    assert!(matches!(billing::cancel(&mut clinic.db, &clinic.owner, done.bill.id, "Again", NOW), Err(ServiceError::NotAllowed(_))));
    assert_ledger_consistent(&clinic)
}

#[test]
fn correcting_a_bill_reissues_it_with_a_new_number() -> TestResult {
    let mut clinic = clinic()?;
    let original = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0012", vec![line(clinic.paracetamol, 2), line(clinic.cream, 1)], Discount::None, cash(16_000)), NOW)?;
    let corrected = billing::correct(
        &mut clinic.db,
        &clinic.reception,
        CorrectionInput {
            original_bill_id: original.bill.id,
            reason: "Patient removed the cream".into(),
            bill: bill("bill-key-0013", vec![line(clinic.paracetamol, 2)], Discount::None, cash(4_000)),
        },
        NOW + 300,
    )?;
    assert_eq!(corrected.bill.bill_no, "INV/26-27/000002");
    assert_eq!(corrected.bill.replaces_bill_no.as_deref(), Some("INV/26-27/000001"));
    let old = billing::get(&clinic.db, &clinic.owner, original.bill.id)?;
    assert_eq!(old.bill.status, "CORRECTED");
    assert_eq!(old.bill.corrected_by_bill_no.as_deref(), Some("INV/26-27/000002"));
    assert_eq!(sellable(&clinic, clinic.cream)?, 10, "the cream went back");
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 98, "paracetamol taken once, by the new bill");

    let next_day = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0014", vec![line(clinic.cream, 1)], Discount::None, cash(12_000)), NOW)?;
    let refused = billing::correct(
        &mut clinic.db,
        &clinic.reception,
        CorrectionInput { original_bill_id: next_day.bill.id, reason: "Too late".into(), bill: bill("bill-key-0015", vec![line(clinic.cream, 1)], Discount::None, cash(12_000)) },
        NOW + DAY,
    );
    assert!(matches!(refused, Err(ServiceError::NotAllowed(_))), "receptionists: same day only");
    assert_ledger_consistent(&clinic)
}

#[test]
fn stock_adjustments_need_a_reason_and_never_go_negative() -> TestResult {
    let mut clinic = clinic()?;
    let batch = inventory::get_product(&clinic.db, &clinic.owner, clinic.cream, NOW)?.batches[0].id;
    let adjust = |kind: &str, counted: i64, reason: &str| AdjustInput { batch_id: batch, counted_qty: counted, kind: kind.into(), reason: reason.into() };
    assert!(matches!(inventory::adjust_stock(&mut clinic.db, &clinic.owner, adjust("DAMAGE", 8, ""), NOW), Err(ServiceError::Validation { field: "reason", .. })));
    assert!(matches!(inventory::adjust_stock(&mut clinic.db, &clinic.owner, adjust("DAMAGE", 12, "Found more"), NOW), Err(ServiceError::Validation { field: "kind", .. })));
    assert!(matches!(inventory::adjust_stock(&mut clinic.db, &clinic.owner, adjust("ADJUSTMENT", -1, "Count"), NOW), Err(ServiceError::Validation { .. })));
    assert!(matches!(inventory::adjust_stock(&mut clinic.db, &clinic.reception, adjust("DAMAGE", 8, "Broken tubes"), NOW), Err(ServiceError::PermissionDenied)));
    assert_eq!(inventory::adjust_stock(&mut clinic.db, &clinic.owner, adjust("DAMAGE", 8, "Broken tubes"), NOW)?.quantity, 8);
    let ledger = inventory::ledger(&clinic.db, &clinic.owner, Some(clinic.cream), 10)?;
    assert_eq!((ledger[0].kind.as_str(), ledger[0].previous_qty, ledger[0].new_qty), ("DAMAGE", 10, 8));
    assert_ledger_consistent(&clinic)
}

#[test]
fn low_stock_and_expiry_lists() -> TestResult {
    let mut clinic = clinic()?;
    let low = inventory::list_products(&clinic.db, &clinic.owner, ProductFilter { stock: "LOW".into(), ..Default::default() }, NOW)?;
    assert!(low.is_empty(), "100 strips is above the minimum of 20");
    billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0016", vec![line(clinic.paracetamol, 85)], Discount::None, cash(170_000)), NOW)?;
    let low = inventory::list_products(&clinic.db, &clinic.owner, ProductFilter { stock: "LOW".into(), ..Default::default() }, NOW)?;
    assert_eq!(low.iter().map(|p| p.id).collect::<Vec<_>>(), vec![clinic.paracetamol]);

    let within_year = inventory::expiring(&clinic.db, &clinic.owner, 365, NOW)?;
    assert_eq!(within_year.len(), 1, "only A1 (2027-06-30) expires within a year");
    assert!(within_year[0].days_left > 0);
    Ok(())
}

#[test]
fn clients_have_a_purchase_history() -> TestResult {
    let mut clinic = clinic()?;
    let john = clients::save(
        &mut clinic.db,
        &clinic.reception,
        ClientInput {
            id: None,
            full_name: "John Doe".into(),
            phone: "98765 43210".into(),
            email: String::new(),
            date_of_birth: Some("1985-03-12".into()),
            gender: "MALE".into(),
            address: String::new(),
            emergency_contact: String::new(),
            notes: String::new(),
            is_active: true,
        },
        NOW,
    )?;
    assert_eq!(john.client_code, "CL-000001");
    assert_eq!(clients::search(&clinic.db, &clinic.reception, "98765", false)?.len(), 1);
    assert_eq!(clients::search(&clinic.db, &clinic.reception, "doe", false)?.len(), 1);

    let mut input = bill("bill-key-0017", vec![line(clinic.paracetamol, 2)], Discount::None, cash(4_000));
    input.client_id = Some(john.id);
    billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    let profile = clients::profile(&clinic.db, &clinic.reception, john.id, None, None)?;
    assert_eq!((profile.visit_count, profile.total_spent_paise), (1, 4_000));
    assert_eq!(profile.visits[0].items[0].product_name, "Paracetamol 500mg");
    assert!(profile.client.last_visit_at.is_some());

    let future_dob = clients::save(&mut clinic.db, &clinic.reception, ClientInput { date_of_birth: Some("2099-01-01".into()), id: Some(john.id), ..clientless() }, NOW);
    assert!(matches!(future_dob, Err(ServiceError::Validation { field: "dateOfBirth", .. })));
    Ok(())
}

fn clientless() -> ClientInput {
    ClientInput {
        id: None,
        full_name: "Someone".into(),
        phone: String::new(),
        email: String::new(),
        date_of_birth: None,
        gender: "UNDISCLOSED".into(),
        address: String::new(),
        emergency_contact: String::new(),
        notes: String::new(),
        is_active: true,
    }
}

#[test]
fn dashboard_and_reports() -> TestResult {
    let mut clinic = clinic()?;
    billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0018", vec![line(clinic.paracetamol, 2)], Discount::None, cash(4_000)), NOW)?;
    billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0019", vec![line(clinic.cream, 1)], Discount::None, cash(12_000)), NOW)?;

    let dash = reports::dashboard(&clinic.db, &clinic.reception, NOW)?;
    assert_eq!((dash.sales_today.bill_count, dash.net_sales_today_paise), (2, 16_000));
    assert_eq!(dash.active_products, 2);
    assert_eq!(dash.recent_bills.len(), 2);

    let range = DateRange { from: "2026-09-25".into(), to: "2026-09-25".into() };
    assert!(matches!(reports::sales(&clinic.db, &clinic.reception, &range), Err(ServiceError::PermissionDenied)));
    let sales = reports::sales(&clinic.db, &clinic.owner, &range)?;
    assert_eq!(sales.totals.total_paise, 16_000);
    assert_eq!(sales.by_method.iter().map(|m| m.received_paise).sum::<i64>(), 16_000);
    let by_product = reports::product_sales(&clinic.db, &clinic.owner, &range)?;
    assert_eq!(by_product[0].product_name, "Pain Relief Cream", "sorted by revenue");
    let stock = reports::stock(&clinic.db, &clinic.owner, NOW)?;
    assert_eq!(stock.batches.len(), 2);
    Ok(())
}

#[test]
fn a_full_return_refunds_exactly_what_was_collected_after_round_off() -> TestResult {
    let mut clinic = clinic()?;
    // ₹20.00 − ₹0.51 = ₹19.49, rounded to ₹19.00: the line total is 49 paise more than was paid.
    let done = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0020", vec![line(clinic.paracetamol, 1)], Discount::Amount(Paise::new(51)), cash(1_900)), NOW)?;
    assert_eq!(done.bill.total_paise, 1_900);
    let returned = billing::return_items(
        &mut clinic.db,
        &clinic.reception,
        ReturnInput { bill_id: done.bill.id, lines: vec![ReturnLineInput { bill_item_id: done.items[0].id, qty: 1, restock: true }], reason: "Not needed".into(), refund_method: "CASH".into() },
        NOW + 60,
    )?;
    assert_eq!(returned.refund_paise, 1_900, "never more than the bill total");
    assert_eq!(returned.bill.bill.returned_paise, returned.bill.bill.total_paise);
    Ok(())
}

#[test]
fn a_bill_key_cannot_be_reused_for_a_different_bill() -> TestResult {
    let mut clinic = clinic()?;
    let input = bill("bill-key-0021", vec![line(clinic.paracetamol, 1)], Discount::None, cash(2_000));
    billing::finalize(&mut clinic.db, &clinic.reception, input.clone(), NOW)?;
    let by_someone_else = billing::finalize(&mut clinic.db, &clinic.owner, input, NOW + 1);
    assert!(matches!(by_someone_else, Err(ServiceError::NotAllowed(_))));
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 99, "stock taken once");
    Ok(())
}

#[test]
fn returns_reject_duplicate_lines_and_cash_received_needs_cash() -> TestResult {
    let mut clinic = clinic()?;
    let done = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0022", vec![line(clinic.paracetamol, 4)], Discount::None, cash(8_000)), NOW)?;
    let item = done.items[0].id;
    let duplicate = billing::return_items(
        &mut clinic.db,
        &clinic.reception,
        ReturnInput {
            bill_id: done.bill.id,
            lines: vec![ReturnLineInput { bill_item_id: item, qty: 1, restock: true }, ReturnLineInput { bill_item_id: item, qty: 1, restock: true }],
            reason: "Twice".into(),
            refund_method: "CASH".into(),
        },
        NOW + 60,
    );
    assert!(matches!(duplicate, Err(ServiceError::Validation { field: "lines", .. })));

    let mut upi = bill("bill-key-0023", vec![line(clinic.paracetamol, 1)], Discount::None, vec![PaymentInput { method: "UPI".into(), amount_paise: 2_000, reference: String::new() }]);
    upi.amount_received_paise = Some(5_000);
    assert!(matches!(billing::finalize(&mut clinic.db, &clinic.reception, upi, NOW), Err(ServiceError::Validation { field: "amountReceived", .. })));
    Ok(())
}

#[test]
fn discounts_arrive_from_the_ui_in_the_documented_json_shape() {
    // The New Bill screen sends exactly these; "NONE" has no "value" key.
    let parsed: Vec<Discount> = ["{\"kind\":\"NONE\"}", "{\"kind\":\"PERCENT\",\"value\":1000}", "{\"kind\":\"AMOUNT\",\"value\":1500}"]
        .iter()
        .filter_map(|json| serde_json::from_str(json).ok())
        .collect();
    assert_eq!(parsed, vec![Discount::None, Discount::Percent(BasisPoints::new(1_000)), Discount::Amount(Paise::new(1_500))]);
}
