-- v0.4.1 hardening (review 2026-09-27). Additive: a fixed trigger, indexes, one column.

-- A finalized bill's client may change only to the client it was merged into; it can never be
-- cleared (the 0005 version let `client_id` be set to NULL).
DROP TRIGGER bill_client_moves_only_by_merge;
CREATE TRIGGER bill_client_moves_only_by_merge BEFORE UPDATE OF client_id ON bill
WHEN NEW.client_id IS NOT OLD.client_id
  AND (NEW.client_id IS NULL
       OR NOT EXISTS (SELECT 1 FROM client WHERE id = OLD.client_id AND merged_into_client_id = NEW.client_id))
BEGIN SELECT RAISE(ABORT, 'a bill moves to another client only when its client is merged'); END;

-- Reports and refunds look these up by date or by line.
CREATE INDEX IF NOT EXISTS idx_sales_return_created ON sales_return (created_at);
CREATE INDEX IF NOT EXISTS idx_payment_created ON payment (created_at);
CREATE INDEX IF NOT EXISTS idx_return_item_bill_item ON return_item (bill_item_id);
CREATE INDEX IF NOT EXISTS idx_bill_item_batch_item ON bill_item_batch (bill_item_id);
CREATE INDEX IF NOT EXISTS idx_batch_supplier ON inventory_batch (supplier_id);

-- Add Inventory is sent once per form: a double-click or retry finds the stock already added.
ALTER TABLE inventory_batch ADD COLUMN request_key TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_batch_request_key ON inventory_batch (request_key) WHERE request_key IS NOT NULL;
