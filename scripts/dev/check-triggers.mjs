#!/usr/bin/env node
// Development check: applies every migration to an empty SQLite database (Node's built-in
// SQLite) and proves the protective triggers work: the app's own updates pass, tampering fails.
// Runs without compiling Rust.
//
//   node --no-warnings scripts/dev/check-triggers.mjs
import { DatabaseSync } from 'node:sqlite';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const migrations = join(root, 'crates', 'clinic-sqlite', 'migrations');
const db = new DatabaseSync(':memory:');
db.exec('PRAGMA foreign_keys = ON');
for (const file of readdirSync(migrations).sort()) db.exec(readFileSync(join(migrations, file), 'utf8'));

db.exec(`
INSERT INTO app_user (id, username, full_name, role, password_hash, created_at, updated_at) VALUES (1, 'owner', 'Owner', 'ADMIN', 'x', 0, 0);
INSERT INTO product (id, sku, name, product_type, unit, created_at, updated_at) VALUES (1, 'P-1', 'Para', 'OTHER', 'pcs', 0, 0);
INSERT INTO inventory_batch (id, product_id, batch_no, purchase_price_paise, selling_price_paise, quantity, created_at, updated_at) VALUES (1, 1, 'A1', 1000, 2000, 10, 0, 0);
INSERT INTO client (id, client_code, full_name, created_at, updated_at, name_key, phone_digits) VALUES
  (1, 'CL-000001', 'Rahul S.', 0, 0, 'rahul s', '9876543210'),
  (2, 'CL-000002', 'Rahul Sharma', 0, 0, 'rahul sharma', '9876543210'),
  (3, 'CL-000003', 'Priya', 0, 0, 'priya', '');
INSERT INTO bill (id, bill_no, idempotency_key, client_id, status, subtotal_paise, discount_paise, tax_paise, round_off_paise, total_paise, created_by, finalized_at)
  VALUES (1, 'INV/1', 'k1', NULL, 'FINALIZED', 2000, 51, 0, -49, 1900, 1, 0),
         (2, 'INV/2', 'k2', NULL, 'FINALIZED', 2000, 0, 0, 0, 2000, 1, 0),
         (3, 'INV/3', 'k3', NULL, 'FINALIZED', 2000, 0, 0, 0, 2000, 1, 0),
         (4, 'INV/4', 'k4', 1, 'FINALIZED', 50000, 0, 0, 0, 50000, 1, 0),
         (5, 'INV/5', 'k5', 3, 'FINALIZED', 1000, 0, 0, 0, 1000, 1, 0);
INSERT INTO bill_item (id, bill_id, line_no, product_id, product_name, sku, unit, qty, unit_price_paise, discount_share_paise, gst_rate_bp, tax_paise, line_total_paise)
  VALUES (1, 1, 1, 1, 'Para', 'P-1', 'pcs', 2, 1000, 51, 0, 0, 1949);
INSERT INTO bill_item_batch (id, bill_item_id, batch_id, qty) VALUES (1, 1, 1, 2);
INSERT INTO sales_return (id, return_no, bill_id, reason, refund_paise, refund_method, user_id, created_at) VALUES (1, 'RET/1', 1, 'x', 100, 'CASH', 1, 0);
INSERT INTO bill_service_item (bill_id, line_no, service_id, kind, name, qty, unit_price_paise, default_price_paise, discount_eligible, discount_share_paise, gst_rate_bp, tax_paise, line_total_paise)
  VALUES (4, 1, 1, 'CONSULTATION', 'General Consultation', 1, 50000, 50000, 0, 0, 0, 0, 50000);
`);

const results = [];
const expect = (label, sql, shouldPass) => {
  let ok;
  let message = '';
  try {
    db.exec(sql);
    ok = true;
  } catch (e) {
    ok = false;
    message = e.message;
  }
  results.push({ label, pass: ok === shouldPass, detail: ok ? 'allowed' : `refused: ${message}` });
};

// What the app does (repo/billing.rs and repo/clients.rs statements, same shape).
expect('return: item returned qty', 'UPDATE bill_item SET returned_qty = returned_qty + 1 WHERE id = 1', true);
expect('return: batch returned qty', 'UPDATE bill_item_batch SET returned_qty = returned_qty + 1 WHERE id = 1', true);
expect('return: bill refunds', 'UPDATE bill SET returned_paise = returned_paise + 900 WHERE id = 1', true);
expect('return: up to the total', 'UPDATE bill SET returned_paise = returned_paise + 1000 WHERE id = 1', true);
expect('cancel: close_bill', "UPDATE bill SET status = 'CANCELLED', cancelled_at = 5, cancelled_by = 1, cancel_reason = 'Wrong' WHERE id = 2 AND status = 'FINALIZED'", true);
expect('correct: close_bill', "UPDATE bill SET status = 'CORRECTED', cancelled_at = 5, cancelled_by = 1, cancel_reason = 'Fix' WHERE id = 3 AND status = 'FINALIZED'", true);
expect('correct: set_corrected_by', 'UPDATE bill SET corrected_by_bill_id = 2 WHERE id = 3', true);
expect('merge: mark merged', 'UPDATE client SET merged_into_client_id = 2, merged_at = 1, is_active = 0 WHERE id = 1 AND merged_into_client_id IS NULL', true);
expect('merge: move bills', 'UPDATE bill SET client_id = 2 WHERE client_id = 1', true);

// Tampering.
expect('refund above total', 'UPDATE bill SET returned_paise = 1901 WHERE id = 1', false);
expect('refund decreasing', 'UPDATE bill SET returned_paise = 0 WHERE id = 1', false);
expect('change bill total', 'UPDATE bill SET total_paise = 1 WHERE id = 1', false);
expect('change bill number', "UPDATE bill SET bill_no = 'X' WHERE id = 1", false);
expect('reopen a cancelled bill', "UPDATE bill SET status = 'FINALIZED' WHERE id = 2", false);
expect('cancelled -> corrected', "UPDATE bill SET status = 'CORRECTED' WHERE id = 2", false);
expect('rewrite cancel reason', "UPDATE bill SET cancel_reason = 'other' WHERE id = 2", false);
expect('relink correction', 'UPDATE bill SET corrected_by_bill_id = 1 WHERE id = 3', false);
expect('change line price', 'UPDATE bill_item SET unit_price_paise = 1 WHERE id = 1', false);
expect('returned qty decreasing', 'UPDATE bill_item SET returned_qty = 0 WHERE id = 1', false);
expect('change line batch', 'UPDATE bill_item_batch SET batch_id = 2 WHERE id = 1', false);
expect('edit a return', 'UPDATE sales_return SET refund_paise = 0 WHERE id = 1', false);
expect('move bill without merge', 'UPDATE bill SET client_id = 2 WHERE id = 5', false);
expect('clear a bill client', 'UPDATE bill SET client_id = NULL WHERE id = 5', false);
expect('give a walk-in bill a client', 'UPDATE bill SET client_id = 3 WHERE id = 2', false);
expect('edit a service line', 'UPDATE bill_service_item SET unit_price_paise = 0', false);
expect('delete a service line', 'DELETE FROM bill_service_item', false);
expect('delete a bill', 'DELETE FROM bill WHERE id = 5', false);
expect('edit the stock ledger', "INSERT INTO inventory_transaction (occurred_at, product_id, batch_id, kind, qty_change, previous_qty, new_qty, user_id) VALUES (0, 1, 1, 'SALE', -1, 10, 8, 1)", false);
expect('duplicate service name', "INSERT INTO service (kind, name, default_price_paise, created_at, updated_at) VALUES ('PROCEDURE', 'dressing', 1, 0, 0)", false);
expect('duplicate Add Inventory key', "UPDATE inventory_batch SET request_key = 'k' WHERE id = 1; INSERT INTO inventory_batch (product_id, batch_no, purchase_price_paise, selling_price_paise, created_at, updated_at, request_key) VALUES (1, 'A2', 1, 1, 0, 0, 'k')", false);

for (const r of results) console.log(`${r.pass ? 'PASS' : 'FAIL'}  ${r.label.padEnd(30)} ${r.detail}`);
const failed = results.filter((r) => !r.pass).length;
console.log(failed ? `${failed} of ${results.length} FAILED` : `all ${results.length} trigger checks passed`);
process.exit(failed ? 1 : 0);
