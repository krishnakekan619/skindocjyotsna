-- v0.3: consultations and procedures on bills, discount eligibility, client duplicate
-- detection and merging. Additive only: no existing row is rewritten except filling the new
-- client search keys (done by the app right after this migration).

-- Consultation types and procedures: charged on bills, never stock items (DEC-030).
CREATE TABLE service (
    id                   INTEGER PRIMARY KEY,
    kind                 TEXT    NOT NULL CHECK (kind IN ('CONSULTATION', 'PROCEDURE')),
    name                 TEXT    NOT NULL COLLATE NOCASE CHECK (length(name) BETWEEN 1 AND 80),
    default_price_paise  INTEGER NOT NULL CHECK (default_price_paise BETWEEN 0 AND 100000000),
    -- Healthcare services by a clinic are GST-exempt in India, so 0 unless the admin sets one.
    gst_rate_bp          INTEGER NOT NULL DEFAULT 0 CHECK (gst_rate_bp BETWEEN 0 AND 2800),
    -- Whether the bill discount may reduce this charge (off: discounts are for medicines).
    discount_eligible    INTEGER NOT NULL DEFAULT 0 CHECK (discount_eligible IN (0, 1)),
    is_active            INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    -- The first active consultation is the one-click "+ Consultation" button.
    sort_order           INTEGER NOT NULL DEFAULT 0 CHECK (sort_order BETWEEN 0 AND 9999),
    created_at           INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL,
    UNIQUE (kind, name)
) STRICT;

INSERT INTO service (kind, name, default_price_paise, sort_order, created_at, updated_at) VALUES
    ('CONSULTATION', 'General Consultation', 50000, 1, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)),
    ('CONSULTATION', 'Follow-up Consultation', 30000, 2, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)),
    ('PROCEDURE', 'Dressing', 30000, 1, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)),
    ('PROCEDURE', 'Injection', 20000, 2, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)),
    ('PROCEDURE', 'Nebulization', 30000, 3, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER));

-- Consultation and procedure lines of a bill (product lines stay in bill_item).
CREATE TABLE bill_service_item (
    id                    INTEGER PRIMARY KEY,
    bill_id               INTEGER NOT NULL REFERENCES bill (id),
    line_no               INTEGER NOT NULL,
    service_id            INTEGER NOT NULL REFERENCES service (id),
    kind                  TEXT    NOT NULL CHECK (kind IN ('CONSULTATION', 'PROCEDURE')),
    name                  TEXT    NOT NULL,     -- snapshot: receipts never change later
    qty                   INTEGER NOT NULL CHECK (qty > 0),
    unit_price_paise      INTEGER NOT NULL CHECK (unit_price_paise >= 0),
    -- The catalog price when billed: a lower price needed an administrator's approval.
    default_price_paise   INTEGER NOT NULL CHECK (default_price_paise >= 0),
    discount_eligible     INTEGER NOT NULL CHECK (discount_eligible IN (0, 1)),
    discount_share_paise  INTEGER NOT NULL CHECK (discount_share_paise >= 0),
    gst_rate_bp           INTEGER NOT NULL,
    tax_paise             INTEGER NOT NULL,
    line_total_paise      INTEGER NOT NULL CHECK (line_total_paise >= 0),
    UNIQUE (bill_id, line_no)
) STRICT;
CREATE INDEX idx_bill_service_item_bill ON bill_service_item (bill_id);

CREATE TRIGGER bill_service_item_no_update BEFORE UPDATE ON bill_service_item
BEGIN SELECT RAISE(ABORT, 'bill lines are never changed'); END;
CREATE TRIGGER bill_service_item_no_delete BEFORE DELETE ON bill_service_item
BEGIN SELECT RAISE(ABORT, 'bill lines are never deleted'); END;

-- Product lines record their discount eligibility explicitly too (all products: yes, for now).
ALTER TABLE bill_item ADD COLUMN discount_eligible INTEGER NOT NULL DEFAULT 1 CHECK (discount_eligible IN (0, 1));
CREATE TRIGGER bill_item_discount_flag_fixed BEFORE UPDATE OF discount_eligible ON bill_item
WHEN NEW.discount_eligible IS NOT OLD.discount_eligible
BEGIN SELECT RAISE(ABORT, 'bill lines are never changed (returns only add up)'); END;

-- Duplicate detection and merging. name_key: lower case, punctuation removed, single spaces;
-- phone_digits: the last 10 digits. Both are kept up to date by the app.
ALTER TABLE client ADD COLUMN name_key TEXT NOT NULL DEFAULT '';
ALTER TABLE client ADD COLUMN phone_digits TEXT NOT NULL DEFAULT '';
ALTER TABLE client ADD COLUMN merged_into_client_id INTEGER REFERENCES client (id);
ALTER TABLE client ADD COLUMN merged_at INTEGER;
CREATE INDEX idx_client_name_key ON client (name_key);
CREATE INDEX idx_client_phone_digits ON client (phone_digits);

-- A finalized bill's client may change only when that client was merged into another one:
-- the bill then moves to the surviving client. Everything else stays fixed (see 0004).
DROP TRIGGER bill_details_fixed;
CREATE TRIGGER bill_details_fixed BEFORE UPDATE ON bill
WHEN NEW.bill_no IS NOT OLD.bill_no
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
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

CREATE TRIGGER bill_client_moves_only_by_merge BEFORE UPDATE OF client_id ON bill
WHEN NEW.client_id IS NOT OLD.client_id
  AND NOT EXISTS (SELECT 1 FROM client WHERE id = OLD.client_id AND merged_into_client_id IS NEW.client_id)
BEGIN SELECT RAISE(ABORT, 'a bill moves to another client only when its client is merged'); END;
