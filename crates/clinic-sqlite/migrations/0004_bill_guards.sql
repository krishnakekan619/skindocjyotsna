-- Finalized bills are records: their amounts and lines never change. The only updates the app
-- makes are closing a bill (FINALIZED -> CANCELLED/CORRECTED), linking the correction, and
-- counting returns. Anything else is refused by the database itself, whatever the caller.

CREATE TRIGGER bill_details_fixed BEFORE UPDATE ON bill
WHEN NEW.bill_no IS NOT OLD.bill_no
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
  OR NEW.client_id IS NOT OLD.client_id
  OR NEW.subtotal_paise IS NOT OLD.subtotal_paise
  OR NEW.discount_paise IS NOT OLD.discount_paise
  OR NEW.tax_paise IS NOT OLD.tax_paise
  OR NEW.round_off_paise IS NOT OLD.round_off_paise
  OR NEW.total_paise IS NOT OLD.total_paise
  OR NEW.amount_received_paise IS NOT OLD.amount_received_paise
  OR NEW.change_paise IS NOT OLD.change_paise
  OR NEW.note IS NOT OLD.note
  OR NEW.created_by IS NOT OLD.created_by
  OR NEW.discount_approved_by IS NOT OLD.discount_approved_by
  OR NEW.finalized_at IS NOT OLD.finalized_at
  OR NEW.replaces_bill_id IS NOT OLD.replaces_bill_id
BEGIN SELECT RAISE(ABORT, 'bill amounts and details are never changed'); END;

CREATE TRIGGER bill_status_flow BEFORE UPDATE ON bill
WHEN NEW.status IS NOT OLD.status AND NOT (OLD.status = 'FINALIZED' AND NEW.status IN ('CANCELLED', 'CORRECTED'))
BEGIN SELECT RAISE(ABORT, 'a bill can only be closed once, from FINALIZED'); END;

CREATE TRIGGER bill_closed_once BEFORE UPDATE ON bill
WHEN (OLD.cancelled_at IS NOT NULL
      AND (NEW.cancelled_at IS NOT OLD.cancelled_at OR NEW.cancelled_by IS NOT OLD.cancelled_by OR NEW.cancel_reason IS NOT OLD.cancel_reason))
  OR (OLD.corrected_by_bill_id IS NOT NULL AND NEW.corrected_by_bill_id IS NOT OLD.corrected_by_bill_id)
BEGIN SELECT RAISE(ABORT, 'closing details are never changed'); END;

CREATE TRIGGER bill_refunds_bounded BEFORE UPDATE ON bill
WHEN NEW.returned_paise < OLD.returned_paise OR NEW.returned_paise > NEW.total_paise
BEGIN SELECT RAISE(ABORT, 'refunds only grow and never exceed the bill total'); END;

CREATE TRIGGER bill_item_fixed BEFORE UPDATE ON bill_item
WHEN NEW.bill_id IS NOT OLD.bill_id
  OR NEW.line_no IS NOT OLD.line_no
  OR NEW.product_id IS NOT OLD.product_id
  OR NEW.product_name IS NOT OLD.product_name
  OR NEW.sku IS NOT OLD.sku
  OR NEW.unit IS NOT OLD.unit
  OR NEW.qty IS NOT OLD.qty
  OR NEW.not_supplied_qty IS NOT OLD.not_supplied_qty
  OR NEW.unit_price_paise IS NOT OLD.unit_price_paise
  OR NEW.discount_share_paise IS NOT OLD.discount_share_paise
  OR NEW.gst_rate_bp IS NOT OLD.gst_rate_bp
  OR NEW.tax_paise IS NOT OLD.tax_paise
  OR NEW.line_total_paise IS NOT OLD.line_total_paise
  OR NEW.returned_qty < OLD.returned_qty
BEGIN SELECT RAISE(ABORT, 'bill lines are never changed (returns only add up)'); END;

CREATE TRIGGER bill_item_batch_fixed BEFORE UPDATE ON bill_item_batch
WHEN NEW.bill_item_id IS NOT OLD.bill_item_id
  OR NEW.batch_id IS NOT OLD.batch_id
  OR NEW.qty IS NOT OLD.qty
  OR NEW.returned_qty < OLD.returned_qty
BEGIN SELECT RAISE(ABORT, 'bill line batches are never changed (returns only add up)'); END;

CREATE TRIGGER sales_return_no_update BEFORE UPDATE ON sales_return
BEGIN SELECT RAISE(ABORT, 'returns are never changed'); END;
CREATE TRIGGER return_item_no_update BEFORE UPDATE ON return_item
BEGIN SELECT RAISE(ABORT, 'return items are never changed'); END;
