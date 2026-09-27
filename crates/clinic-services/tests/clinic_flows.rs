//! Inventory, billing, returns, cancellation, correction, clients and reports, end to end
//! against a real (in-memory) SQLite database.

use clinic_core::auth::Role;
use clinic_core::money::{BasisPoints, Paise};
use clinic_core::pricing::Discount;
use clinic_services::auth::{self, NewAccount, SetupInput};
use clinic_services::billing::{self, Approval, BillInput, BillLineInput, CorrectionInput, NewClientInput, PaymentInput, ReturnInput, ReturnLineInput, ServiceLineInput};
use clinic_services::catalog::{self, ServiceInput};
use clinic_services::share;
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
    BillInput {
        idempotency_key: key.into(),
        client_id: None,
        new_client: None,
        lines,
        services: Vec::new(),
        discount,
        payments,
        amount_received_paise: None,
        note: String::new(),
        approval: None,
    }
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
    let quote = billing::quote(&clinic.db, &clinic.reception, &input.lines, &[], input.discount, None, NOW)?;
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
    let result = billing::quote(&clinic.db, &clinic.reception, &[line(clinic.paracetamol, 1)], &[], Discount::None, None, expired_day);
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
    let quote = billing::quote(&clinic.db, &clinic.reception, &input.lines, &[], input.discount, None, NOW)?;
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
            allow_duplicate: false,
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
        allow_duplicate: false,
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

// ---- v0.3: consultations, procedures, duplicates, merge, WhatsApp ----------------------------

fn consultation_and_dressing(clinic: &Clinic) -> Result<(i64, i64), ServiceError> {
    let services = catalog::list(&clinic.db, &clinic.reception, false)?;
    let id = |name: &str| services.iter().find(|s| s.name == name).map(|s| s.id).ok_or(ServiceError::NotFound("service"));
    Ok((id("General Consultation")?, id("Dressing")?))
}

fn service_line(service_id: i64) -> ServiceLineInput {
    ServiceLineInput { service_id: Some(service_id), kind: None, name: None, qty: 1, unit_price_paise: None }
}

fn typed_service(kind: &str, name: &str, price: i64) -> ServiceLineInput {
    ServiceLineInput { service_id: None, kind: Some(kind.into()), name: Some(name.into()), qty: 1, unit_price_paise: Some(price) }
}

#[test]
fn consultation_and_procedures_are_billed_without_stock_or_discount() -> TestResult {
    let mut clinic = clinic()?;
    let (consultation, dressing) = consultation_and_dressing(&clinic)?;
    // ₹500 consultation + ₹300 dressing + medicines 2 x ₹20 + 1 x ₹120; ₹10 off the medicines.
    let mut input = bill("bill-key-0030", vec![line(clinic.paracetamol, 2), line(clinic.cream, 1)], Discount::Amount(Paise::new(1_000)), cash(95_000));
    input.services = vec![service_line(consultation), service_line(dressing)];
    let quote = billing::quote(&clinic.db, &clinic.reception, &input.lines, &input.services, input.discount, None, NOW)?;
    assert_eq!((quote.consultation_paise, quote.procedures_paise, quote.products_paise), (50_000, 30_000, 16_000));
    assert_eq!(quote.eligible_subtotal_paise, 16_000);
    assert!(quote.service_lines.iter().all(|l| l.discount_share_paise == 0), "no discount on consultation/procedures");
    assert_eq!(quote.total_paise, 95_000);

    let done = billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    assert_eq!(done.services.len(), 2);
    assert_eq!(done.bill.total_paise, 95_000);
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 98, "only products take stock");
    let receipt = billing::receipt(&clinic.db, &clinic.reception, done.bill.id)?;
    let sections: Vec<&str> = receipt.lines.iter().map(|l| l.section.as_str()).collect();
    assert_eq!(sections, vec!["Consultation", "Procedures", "Medicines & Products", "Medicines & Products"]);
    assert_eq!(receipt.breakdown.len(), 3);
    assert_eq!(receipt.discount_label, "Discount on medicines");
    assert!(receipt.notice.is_some_and(|n| n.contains("signature")));

    // Returning all medicines refunds only what they cost, never the consultation.
    let items: Vec<ReturnLineInput> = done.items.iter().map(|i| ReturnLineInput { bill_item_id: i.id, qty: i.qty, restock: true }).collect();
    let returned = billing::return_items(
        &mut clinic.db,
        &clinic.reception,
        ReturnInput { bill_id: done.bill.id, lines: items, reason: "Not needed".into(), refund_method: "CASH".into() },
        NOW + 60,
    )?;
    assert_eq!(returned.refund_paise, 15_000);
    assert_ledger_consistent(&clinic)
}

#[test]
fn a_consultation_alone_is_a_valid_bill_and_any_fee_can_be_typed() -> TestResult {
    let mut clinic = clinic()?;
    let (consultation, _) = consultation_and_dressing(&clinic)?;
    let mut only = bill("bill-key-0031", Vec::new(), Discount::None, cash(50_000));
    only.services = vec![service_line(consultation)];
    assert_eq!(billing::finalize(&mut clinic.db, &clinic.reception, only, NOW)?.bill.total_paise, 50_000);

    // The amount typed at the desk is charged, even below the usual fee (DEC-034: no approval).
    let mut cheaper = bill("bill-key-0032", Vec::new(), Discount::None, cash(40_000));
    cheaper.services = vec![ServiceLineInput { unit_price_paise: Some(40_000), ..service_line(consultation) }];
    assert_eq!(billing::finalize(&mut clinic.db, &clinic.reception, cheaper, NOW)?.bill.total_paise, 40_000);

    let empty = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0033", Vec::new(), Discount::None, Vec::new()), NOW);
    assert!(matches!(empty, Err(ServiceError::Validation { field: "lines", .. })));
    Ok(())
}

#[test]
fn only_admins_edit_the_consultation_and_procedure_list() -> TestResult {
    let mut clinic = clinic()?;
    let input = ServiceInput {
        id: None,
        kind: "PROCEDURE".into(),
        name: "Chemical Peel".into(),
        default_price_paise: 150_000,
        gst_rate_bp: 0,
        discount_eligible: false,
        is_active: true,
        sort_order: 10,
    };
    assert!(matches!(catalog::save(&mut clinic.db, &clinic.reception, input.clone(), NOW), Err(ServiceError::PermissionDenied)));
    let saved = catalog::save(&mut clinic.db, &clinic.owner, input.clone(), NOW)?;
    assert_eq!(saved.name, "Chemical Peel");
    assert!(matches!(catalog::save(&mut clinic.db, &clinic.owner, input, NOW), Err(ServiceError::Conflict(_))), "names are unique per kind");
    Ok(())
}

fn client(name: &str, phone: &str) -> ClientInput {
    ClientInput { full_name: name.into(), phone: phone.into(), ..clientless() }
}

#[test]
fn duplicate_clients_are_caught_and_can_be_merged_keeping_every_bill() -> TestResult {
    let mut clinic = clinic()?;
    let rahul = clients::save(&mut clinic.db, &clinic.reception, client("Rahul Sharma", "98765 43210"), NOW)?;
    // Same phone, or the same name written differently: refused until confirmed.
    let same_phone = clients::save(&mut clinic.db, &clinic.reception, client("Rahul S.", "+91 98765-43210"), NOW);
    assert!(matches!(same_phone, Err(ServiceError::PossibleDuplicate(_))));
    let same_name = clients::save(&mut clinic.db, &clinic.reception, client("  rahul  SHARMA ", ""), NOW);
    assert!(matches!(same_name, Err(ServiceError::PossibleDuplicate(_))));
    let query = clients::DuplicateQuery { full_name: "Rahul Sarma".into(), phone: String::new(), date_of_birth: None, exclude_id: None };
    let similar = clients::possible_duplicates(&clinic.db, &clinic.reception, &query)?;
    assert!(similar.iter().any(|m| m.client.id == rahul.id && m.reason == "SIMILAR_NAME" && !m.strong));
    assert_eq!(clients::search(&clinic.db, &clinic.reception, "rahul sarma", false)?.first().map(|c| c.id), Some(rahul.id), "typo-tolerant search");

    let second = clients::save(&mut clinic.db, &clinic.reception, ClientInput { allow_duplicate: true, notes: "Prefers mornings".into(), ..client("Rahul S.", "9876543210") }, NOW)?;
    let mut input = bill("bill-key-0040", vec![line(clinic.paracetamol, 1)], Discount::None, cash(2_000));
    input.client_id = Some(second.id);
    let old_bill = billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    let groups = clients::duplicate_groups(&clinic.db, &clinic.reception)?;
    assert!(groups.iter().any(|g| g.reason == "PHONE" && g.clients.len() == 2));

    assert!(matches!(clients::merge(&mut clinic.db, &clinic.reception, rahul.id, second.id, NOW), Err(ServiceError::PermissionDenied)));
    let merged = clients::merge(&mut clinic.db, &clinic.owner, rahul.id, second.id, NOW + 60)?;
    assert_eq!(merged.moved_bills, 1);
    assert!(merged.client.notes.contains("Prefers mornings") && merged.client.notes.contains(&second.client_code));
    let moved = billing::get(&clinic.db, &clinic.owner, old_bill.bill.id)?;
    assert_eq!(moved.bill.client_id, Some(rahul.id));
    assert_eq!((moved.bill.total_paise, moved.items.len()), (old_bill.bill.total_paise, old_bill.items.len()), "the bill itself is unchanged");
    assert!(clients::search(&clinic.db, &clinic.reception, "Rahul", true)?.iter().all(|c| c.id != second.id), "merged record is hidden");
    assert_eq!(clients::profile(&clinic.db, &clinic.owner, rahul.id, None, None)?.bill_count, 1);
    assert!(clients::merge(&mut clinic.db, &clinic.owner, rahul.id, second.id, NOW + 120).is_err(), "cannot merge twice");
    Ok(())
}

#[test]
fn the_whatsapp_message_has_no_medicines_and_needs_a_phone() -> TestResult {
    let mut clinic = clinic()?;
    let rahul = clients::save(&mut clinic.db, &clinic.reception, client("Rahul Sharma", "98765 43210"), NOW)?;
    let mut input = bill("bill-key-0050", vec![line(clinic.paracetamol, 1)], Discount::None, cash(2_000));
    input.client_id = Some(rahul.id);
    let done = billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    let message = share::whatsapp_message(&clinic.db, &clinic.reception, done.bill.id)?;
    assert_eq!(message.phone, "919876543210");
    assert!(message.text.starts_with("Hello Rahul,"));
    assert!(message.text.contains(&done.bill.bill_no) && message.text.contains("signature"));
    assert!(!message.text.contains("Paracetamol"), "no medicines in the message");

    let walk_in = billing::finalize(&mut clinic.db, &clinic.reception, bill("bill-key-0051", vec![line(clinic.paracetamol, 1)], Discount::None, cash(2_000)), NOW)?;
    assert!(matches!(share::whatsapp_message(&clinic.db, &clinic.reception, walk_in.bill.id), Err(ServiceError::NoPhone(_))));
    Ok(())
}

// ---- v0.3.1: typed on the bill, standard discount, dashboard split ------------------------------

#[test]
fn a_new_consultation_or_procedure_typed_on_the_bill_joins_the_list() -> TestResult {
    let mut clinic = clinic()?;
    let mut input = bill("bill-key-0060", Vec::new(), Discount::None, cash(150_000 + 150_000));
    input.services = vec![typed_service("PROCEDURE", "Chemical Peel", 150_000), typed_service("PROCEDURE", "chemical peel", 150_000)];
    let quote = billing::quote(&clinic.db, &clinic.reception, &input.lines, &input.services, input.discount, None, NOW)?;
    assert_eq!(quote.procedures_paise, 300_000);
    let done = billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    assert_eq!(done.services.len(), 2);
    assert_eq!(done.services[0].service_id, done.services[1].service_id, "the same new name is added to the list once");
    let listed = catalog::list(&clinic.db, &clinic.reception, false)?;
    assert!(listed.iter().any(|s| s.name == "Chemical Peel" && s.kind == "PROCEDURE" && s.default_price_paise == 150_000));

    // Next time the typed name matches the list entry (any case) instead of adding another.
    let mut again = bill("bill-key-0061", Vec::new(), Discount::None, cash(120_000));
    again.services = vec![typed_service("PROCEDURE", "CHEMICAL PEEL", 120_000)];
    let second = billing::finalize(&mut clinic.db, &clinic.reception, again, NOW)?;
    assert_eq!(second.services[0].service_id, done.services[0].service_id);
    assert_eq!(catalog::list(&clinic.db, &clinic.reception, false)?.len(), listed.len());
    Ok(())
}

#[test]
fn a_new_client_typed_on_the_bill_is_saved_with_it() -> TestResult {
    let mut clinic = clinic()?;
    let mut input = bill("bill-key-0070", vec![line(clinic.paracetamol, 1)], Discount::None, cash(2_000));
    input.new_client = Some(NewClientInput { full_name: "Anita Desai".into(), phone: "99887 76655".into(), allow_duplicate: false });
    let done = billing::finalize(&mut clinic.db, &clinic.reception, input.clone(), NOW)?;
    let client_id = done.bill.client_id.ok_or(ServiceError::NotFound("client"))?;
    assert_eq!(done.bill.client_name.as_deref(), Some("Anita Desai"));
    let profile = clients::profile(&clinic.db, &clinic.reception, client_id, None, None)?;
    assert_eq!((profile.client.phone.as_str(), profile.visit_count), ("99887 76655", 1));
    // A double-click on Finalize returns the same bill and does not create a second client.
    assert_eq!(billing::finalize(&mut clinic.db, &clinic.reception, input, NOW + 1)?.bill.id, done.bill.id);
    assert_eq!(clients::search(&clinic.db, &clinic.reception, "Anita", false)?.len(), 1);

    // Typing a phone that already belongs to a client is refused unless confirmed.
    let mut same_phone = bill("bill-key-0071", vec![line(clinic.paracetamol, 1)], Discount::None, cash(2_000));
    same_phone.new_client = Some(NewClientInput { full_name: "A. Desai".into(), phone: "9988776655".into(), allow_duplicate: false });
    assert!(matches!(billing::finalize(&mut clinic.db, &clinic.reception, same_phone, NOW), Err(ServiceError::PossibleDuplicate(_))));
    assert_eq!(sellable(&clinic, clinic.paracetamol)?, 99, "the refused bill took no stock");
    Ok(())
}

#[test]
fn the_standard_medicine_discount_stays_within_the_receptionist_limit() -> TestResult {
    let mut clinic = clinic()?;
    let settings = clinic_services::settings::get_clinic(&clinic.db)?;
    assert_eq!(settings.default_medicine_discount_percent, 10);
    let too_high = ClinicSettings { default_medicine_discount_percent: 15, ..settings.clone() };
    let refused = clinic_services::settings::update_clinic(&mut clinic.db, &clinic.owner, too_high, NOW);
    assert!(matches!(refused, Err(ServiceError::Validation { field: "defaultMedicineDiscountPercent", .. })));

    // The standard 10% on medicines needs no approval from a receptionist.
    let input = bill("bill-key-0080", vec![line(clinic.paracetamol, 5)], Discount::Percent(BasisPoints::new(1_000)), cash(9_000));
    assert!(!billing::quote(&clinic.db, &clinic.reception, &input.lines, &[], input.discount, None, NOW)?.needs_approval);
    assert_eq!(billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?.bill.total_paise, 9_000);
    Ok(())
}

#[test]
fn the_dashboard_splits_sales_and_lists_top_sellers() -> TestResult {
    let mut clinic = clinic()?;
    let (consultation, dressing) = consultation_and_dressing(&clinic)?;
    let mut input = bill("bill-key-0090", vec![line(clinic.paracetamol, 2)], Discount::None, cash(50_000 + 30_000 + 4_000));
    input.services = vec![service_line(consultation), service_line(dressing)];
    billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    let dash = reports::dashboard(&clinic.db, &clinic.reception, NOW)?;
    let split = &dash.sales_split_today;
    assert_eq!((split.consultation_paise, split.procedures_paise, split.medicines_paise), (50_000, 30_000, 4_000));

    let range = DateRange { from: "2026-09-25".into(), to: "2026-09-25".into() };
    let top = reports::top_sellers(&clinic.db, &clinic.reception, &range)?;
    assert_eq!(top.consultations.first().map(|s| s.name.as_str()), Some("General Consultation"));
    assert_eq!(top.procedures.first().map(|s| s.name.as_str()), Some("Dressing"));
    assert_eq!(top.medicines.first().map(|m| m.product_name.as_str()), Some("Paracetamol 500mg"));
    Ok(())
}

#[test]
fn the_dashboard_split_adds_up_to_the_money_figures() -> TestResult {
    let mut clinic = clinic()?;
    let (consultation, _) = consultation_and_dressing(&clinic)?;
    // ₹500 consultation + ₹20.00 − ₹0.51 medicine = ₹519.49, rounded to ₹519.00 (round-off −49 paise).
    let mut input = bill("bill-key-0095", vec![line(clinic.paracetamol, 1)], Discount::Amount(Paise::new(51)), cash(51_900));
    input.services = vec![service_line(consultation)];
    let done = billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    assert_eq!((done.bill.total_paise, done.bill.round_off_paise), (51_900, -49));
    billing::return_items(
        &mut clinic.db,
        &clinic.reception,
        ReturnInput { bill_id: done.bill.id, lines: vec![ReturnLineInput { bill_item_id: done.items[0].id, qty: 1, restock: true }], reason: "Not needed".into(), refund_method: "CASH".into() },
        NOW + 60,
    )?;

    let dash = reports::dashboard(&clinic.db, &clinic.reception, NOW + 120)?;
    let split = &dash.sales_split_today;
    let parts = split.consultation_paise + split.procedures_paise + split.medicines_paise;
    assert_eq!(parts + split.round_off_paise, dash.sales_today.total_paise, "parts + round-off = total billed");
    assert_eq!(split.refunds_paise, dash.sales_today.returned_paise);
    assert!(split.refunds_paise > 0);
    assert_eq!(parts + split.round_off_paise - split.refunds_paise, dash.net_sales_today_paise, "− refunds = net");

    let range = DateRange { from: "2026-09-25".into(), to: "2026-09-25".into() };
    assert_eq!(&reports::top_sellers(&clinic.db, &clinic.reception, &range)?.split, split, "same figures for the period view");
    Ok(())
}

// ---- v0.4: simple Add Inventory, delete = archive ------------------------------------------------

fn add_stock(product_name: &str, vendor: &str, mrp: i64, bought: i64, qty: i64) -> inventory::AddInventoryInput {
    inventory::AddInventoryInput {
        product_id: None,
        product_name: product_name.into(),
        type_id: None,
        vendor_id: None,
        vendor_name: vendor.into(),
        mrp_paise: mrp,
        purchase_price_paise: bought,
        expiry_date: Some("2027-03-31".into()),
        qty,
        request_key: None,
    }
}

#[test]
fn a_receptionist_adds_inventory_in_one_step_and_it_sells_at_mrp_minus_the_standard_discount() -> TestResult {
    let mut clinic = clinic()?;
    // New product and new vendor, typed; no batch number asked for.
    let lot = inventory::add_inventory(&mut clinic.db, &clinic.reception, add_stock("Sunscreen SPF 50", "Derma Pharma", 10_000, 7_000, 10), NOW)?;
    assert!(lot.batch_no.starts_with("LOT-"));
    assert_eq!((lot.quantity, lot.selling_price_paise, lot.purchase_price_paise), (10, 10_000, 7_000));
    assert_eq!(lot.supplier_name.as_deref(), Some("Derma Pharma"));
    // The same names again reuse the product and the vendor.
    let again = inventory::add_inventory(&mut clinic.db, &clinic.reception, add_stock("sunscreen spf 50", "derma pharma", 10_000, 7_000, 5), NOW)?;
    assert_eq!(again.product_id, lot.product_id);
    assert_eq!(inventory::list_suppliers(&clinic.db, &clinic.reception)?.len(), 1);
    assert_eq!(sellable(&clinic, lot.product_id)?, 15);
    let listed = inventory::list_products(&clinic.db, &clinic.reception, ProductFilter { text: "Derma".into(), ..Default::default() }, NOW)?;
    assert_eq!(listed.first().and_then(|p| p.last_vendor.clone()).as_deref(), Some("Derma Pharma"), "search and list by vendor");

    // ₹100 MRP with the standard 10% discount is billed at ₹90.
    let input = bill("bill-key-0100", vec![line(lot.product_id, 1)], Discount::Percent(BasisPoints::new(1_000)), cash(9_000));
    let done = billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    assert_eq!((done.items[0].unit_price_paise, done.items[0].discount_share_paise, done.bill.total_paise), (10_000, 1_000, 9_000));
    let receipt = billing::receipt(&clinic.db, &clinic.reception, done.bill.id)?;
    assert_eq!((receipt.lines[0].unit_price, receipt.lines[0].discount), (Paise::new(10_000), Paise::new(1_000)));

    let mut expired = add_stock("Old Cream", "Derma Pharma", 5_000, 3_000, 1);
    expired.expiry_date = Some("2026-01-01".into());
    assert!(matches!(inventory::add_inventory(&mut clinic.db, &clinic.reception, expired, NOW), Err(ServiceError::Validation { field: "expiryDate", .. })));
    Ok(())
}

#[test]
fn deleting_a_product_archives_it_when_it_has_history() -> TestResult {
    let mut clinic = clinic()?;
    assert!(matches!(inventory::delete_product(&mut clinic.db, &clinic.reception, clinic.cream, NOW), Err(ServiceError::PermissionDenied)));
    // In stock: refused, so stock never disappears silently.
    assert!(matches!(inventory::delete_product(&mut clinic.db, &clinic.owner, clinic.cream, NOW), Err(ServiceError::NotAllowed(_))));
    // Never stocked or sold: removed completely.
    let unused = inventory::save_product(&mut clinic.db, &clinic.owner, product("Never Used Gel", "tube", 0, 0), NOW)?;
    assert!(inventory::delete_product(&mut clinic.db, &clinic.owner, unused.id, NOW)?.deleted);
    // Has history, stock 0: archived (hidden, kept for old bills and the ledger).
    let batch = inventory::get_product(&clinic.db, &clinic.owner, clinic.cream, NOW)?.batches[0].id;
    inventory::adjust_stock(&mut clinic.db, &clinic.owner, AdjustInput { batch_id: batch, counted_qty: 0, kind: "ADJUSTMENT".into(), reason: "Stopped selling".into() }, NOW)?;
    let outcome = inventory::delete_product(&mut clinic.db, &clinic.owner, clinic.cream, NOW)?;
    assert!(outcome.archived && !outcome.deleted);
    assert!(inventory::list_products(&clinic.db, &clinic.owner, ProductFilter::default(), NOW)?.iter().all(|p| p.id != clinic.cream));
    assert_ledger_consistent(&clinic)
}

// ---- v0.4.1 hardening (review 2026-09-27) --------------------------------------------------------

#[test]
fn wrong_approval_passwords_lock_the_admin_and_are_audited() -> TestResult {
    let mut clinic = clinic()?;
    let mut input = bill("bill-key-0200", vec![line(clinic.cream, 1)], Discount::Percent(BasisPoints::new(2_000)), cash(9_600));
    input.approval = Some(Approval { username: "owner".into(), password: "guess".into() });
    for _ in 0..5 {
        assert!(matches!(billing::finalize(&mut clinic.db, &clinic.reception, input.clone(), NOW), Err(ServiceError::Validation { field: "approval", .. })));
    }
    // Now locked: even the right password is refused, at the desk and at sign-in.
    input.approval = Some(Approval { username: "owner".into(), password: "owner-pass-1".into() });
    assert!(matches!(billing::finalize(&mut clinic.db, &clinic.reception, input, NOW), Err(ServiceError::Validation { field: "approval", .. })));
    assert!(matches!(auth::login(&mut clinic.db, "owner", "owner-pass-1", NOW), Err(ServiceError::AccountLocked { .. })));
    let failures = clinic_services::audit::list(&clinic.db, &clinic.owner, 200, None)?.into_iter().filter(|e| e.action == "APPROVAL_FAILED").count();
    assert!(failures >= 5, "every wrong approval is in the audit log");
    Ok(())
}

#[test]
fn product_names_with_percent_signs_are_found_again() -> TestResult {
    let mut clinic = clinic()?;
    let first = inventory::add_inventory(&mut clinic.db, &clinic.reception, add_stock("Tretinoin 0.025% Cream", "Derma Pharma", 30_000, 20_000, 3), NOW)?;
    let second = inventory::add_inventory(&mut clinic.db, &clinic.reception, add_stock("tretinoin 0.025% cream", "Derma Pharma", 30_000, 20_000, 2), NOW)?;
    assert_eq!(second.product_id, first.product_id, "same product, not a copy");
    let found = inventory::list_products(&clinic.db, &clinic.reception, ProductFilter { text: "0.025%".into(), ..Default::default() }, NOW)?;
    assert_eq!(found.len(), 1);
    let underscore = inventory::list_products(&clinic.db, &clinic.reception, ProductFilter { text: "_".into(), ..Default::default() }, NOW)?;
    assert!(underscore.is_empty(), "_ matches itself, not any character");
    Ok(())
}

#[test]
fn add_inventory_is_sent_once_per_form() -> TestResult {
    let mut clinic = clinic()?;
    let input = inventory::AddInventoryInput { request_key: Some("form-key-0001".into()), ..add_stock("Sunscreen SPF 30", "Derma Pharma", 50_000, 35_000, 4) };
    let first = inventory::add_inventory(&mut clinic.db, &clinic.reception, input.clone(), NOW)?;
    let again = inventory::add_inventory(&mut clinic.db, &clinic.reception, input, NOW + 1)?;
    assert_eq!(again.id, first.id);
    assert_eq!(sellable(&clinic, first.product_id)?, 4, "a double-click does not double the stock");
    Ok(())
}

#[test]
fn a_hand_typed_lot_number_never_blocks_add_inventory() -> TestResult {
    let mut clinic = clinic()?;
    // The next automatic number would be LOT-000001: it is already taken by hand.
    inventory::stock_in(&mut clinic.db, &clinic.owner, stock_in(clinic.paracetamol, "lot-000001", "2027-06-30", 2_000, 5), NOW)?;
    let mut input = add_stock("", "Derma Pharma", 2_000, 1_000, 5);
    input.product_id = Some(clinic.paracetamol);
    let lot = inventory::add_inventory(&mut clinic.db, &clinic.reception, input, NOW)?;
    assert_eq!(lot.batch_no, "LOT-000002");
    Ok(())
}

#[test]
fn a_deactivated_service_typed_again_comes_back() -> TestResult {
    let mut clinic = clinic()?;
    let services = catalog::list(&clinic.db, &clinic.owner, true)?;
    let neb = services.iter().find(|s| s.name == "Nebulization").ok_or(ServiceError::NotFound("service"))?.clone();
    catalog::save(
        &mut clinic.db,
        &clinic.owner,
        ServiceInput {
            id: Some(neb.id),
            kind: neb.kind.clone(),
            name: neb.name.clone(),
            default_price_paise: neb.default_price_paise,
            gst_rate_bp: 0,
            discount_eligible: false,
            is_active: false,
            sort_order: neb.sort_order,
        },
        NOW,
    )?;
    let mut input = bill("bill-key-0210", Vec::new(), Discount::None, cash(30_000));
    input.services = vec![typed_service("PROCEDURE", "nebulization", 30_000)];
    let done = billing::finalize(&mut clinic.db, &clinic.reception, input, NOW)?;
    assert_eq!(done.services[0].service_id, neb.id);
    assert!(catalog::list(&clinic.db, &clinic.reception, false)?.iter().any(|s| s.id == neb.id), "active again");
    Ok(())
}

#[test]
fn a_percent_on_a_bill_without_medicines_needs_no_approval() -> TestResult {
    let clinic = clinic()?;
    let (consultation, _) = consultation_and_dressing(&clinic)?;
    let quote = billing::quote(&clinic.db, &clinic.reception, &[], &[service_line(consultation)], Discount::Percent(BasisPoints::new(2_000)), None, NOW)?;
    assert_eq!((quote.discount_paise, quote.needs_approval), (0, false));
    Ok(())
}

#[test]
fn an_old_settings_record_never_gets_a_standard_discount_above_the_limit() -> TestResult {
    let mut clinic = clinic()?;
    // Saved before v0.3.1: a 5% limit and no standard discount field.
    let old = r#"{"name":"Test Clinic","receptionistDiscountCapPercent":5}"#;
    clinic.db.write(|c| clinic_sqlite::repo::settings::set(c, "clinic.profile", old))?;
    let settings = clinic_services::settings::get_clinic(&clinic.db)?;
    assert_eq!(settings.default_medicine_discount_percent, 5);
    clinic_services::settings::update_clinic(&mut clinic.db, &clinic.owner, ClinicSettings { phone: "020-1234".into(), ..settings }, NOW)?;
    Ok(())
}

#[test]
fn a_deactivated_user_loses_the_session_and_bills_keep_their_client() -> TestResult {
    let mut clinic = clinic()?;
    assert!(clinic_services::auth::session_still_valid(&clinic.db, &clinic.reception)?);
    let reception = users::list(&clinic.db, &clinic.owner, NOW)?.into_iter().find(|u| u.username == "reception").ok_or(ServiceError::NotFound("user"))?;
    users::update(&mut clinic.db, &clinic.owner, reception.id, users::UserUpdate { full_name: reception.full_name.clone(), role: Role::Receptionist, is_active: false }, NOW)?;
    assert!(!clinic_services::auth::session_still_valid(&clinic.db, &clinic.reception)?);

    let client = clients::save(&mut clinic.db, &clinic.owner, client("Asha Rao", "91234 56789"), NOW)?;
    let mut input = bill("bill-key-0220", vec![line(clinic.paracetamol, 1)], Discount::None, cash(2_000));
    input.client_id = Some(client.id);
    let done = billing::finalize(&mut clinic.db, &clinic.owner, input, NOW)?;
    let cleared = clinic.db.write(|c| c.execute("UPDATE bill SET client_id = NULL WHERE id = ?1", [done.bill.id]));
    assert!(cleared.is_err(), "a bill can never be detached from its client");
    Ok(())
}
