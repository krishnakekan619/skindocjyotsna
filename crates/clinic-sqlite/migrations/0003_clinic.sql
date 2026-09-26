-- 0003: inventory, clients, billing, returns (Phases 2-7).
-- Money: INTEGER paise. Rates: INTEGER basis points (18% = 1800).
-- Timestamps: INTEGER Unix seconds UTC. Calendar dates (expiry, birth): TEXT 'YYYY-MM-DD'.

CREATE TABLE category (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL UNIQUE COLLATE NOCASE CHECK (length(name) BETWEEN 1 AND 60),
    is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1))
) STRICT;

CREATE TABLE supplier (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL UNIQUE COLLATE NOCASE CHECK (length(name) BETWEEN 1 AND 100),
    phone      TEXT    NOT NULL DEFAULT '',
    gstin      TEXT    NOT NULL DEFAULT '',
    is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1))
) STRICT;

CREATE TABLE product (
    id                            INTEGER PRIMARY KEY,
    sku                           TEXT    NOT NULL UNIQUE COLLATE NOCASE CHECK (length(sku) BETWEEN 1 AND 40),
    name                          TEXT    NOT NULL COLLATE NOCASE CHECK (length(name) BETWEEN 1 AND 120),
    generic_name                  TEXT    NOT NULL DEFAULT '',
    category_id                   INTEGER REFERENCES category (id) ON DELETE RESTRICT,
    product_type                  TEXT    NOT NULL CHECK (product_type IN ('MEDICINE', 'TABLET', 'CAPSULE', 'CREAM', 'OINTMENT', 'GEL', 'MEDICAL_SUPPLY', 'CONSUMABLE', 'OTHER')),
    manufacturer                  TEXT    NOT NULL DEFAULT '',
    unit                          TEXT    NOT NULL CHECK (length(unit) BETWEEN 1 AND 20),
    gst_rate_bp                   INTEGER NOT NULL DEFAULT 0 CHECK (gst_rate_bp BETWEEN 0 AND 2800),
    default_selling_price_paise   INTEGER NOT NULL DEFAULT 0 CHECK (default_selling_price_paise >= 0),
    default_purchase_price_paise  INTEGER NOT NULL DEFAULT 0 CHECK (default_purchase_price_paise >= 0),
    min_stock                     INTEGER NOT NULL DEFAULT 0 CHECK (min_stock >= 0),
    requires_expiry               INTEGER NOT NULL DEFAULT 1 CHECK (requires_expiry IN (0, 1)),
    is_active                     INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    notes                         TEXT    NOT NULL DEFAULT '',
    created_at                    INTEGER NOT NULL,
    updated_at                    INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_product_name ON product (name);
CREATE INDEX idx_product_generic ON product (generic_name);

CREATE TABLE inventory_batch (
    id                    INTEGER PRIMARY KEY,
    product_id            INTEGER NOT NULL REFERENCES product (id) ON DELETE RESTRICT,
    batch_no              TEXT    NOT NULL COLLATE NOCASE CHECK (length(batch_no) BETWEEN 1 AND 40),
    expiry_date           TEXT CHECK (expiry_date IS NULL OR expiry_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    supplier_id           INTEGER REFERENCES supplier (id) ON DELETE RESTRICT,
    purchase_price_paise  INTEGER NOT NULL CHECK (purchase_price_paise >= 0),
    selling_price_paise   INTEGER NOT NULL CHECK (selling_price_paise >= 0),
    -- Stock can never go negative (design D7).
    quantity              INTEGER NOT NULL DEFAULT 0 CHECK (quantity >= 0),
    created_at            INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL,
    UNIQUE (product_id, batch_no)
) STRICT;
CREATE INDEX idx_batch_fefo ON inventory_batch (product_id, expiry_date);
CREATE INDEX idx_batch_expiry ON inventory_batch (expiry_date);

CREATE TABLE client (
    id                 INTEGER PRIMARY KEY,
    client_code        TEXT    NOT NULL UNIQUE,
    full_name          TEXT    NOT NULL COLLATE NOCASE CHECK (length(full_name) BETWEEN 1 AND 100),
    phone              TEXT    NOT NULL DEFAULT '',
    email              TEXT    NOT NULL DEFAULT '',
    date_of_birth      TEXT CHECK (date_of_birth IS NULL OR date_of_birth GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    gender             TEXT    NOT NULL DEFAULT 'UNDISCLOSED' CHECK (gender IN ('MALE', 'FEMALE', 'OTHER', 'UNDISCLOSED')),
    address            TEXT    NOT NULL DEFAULT '',
    emergency_contact  TEXT    NOT NULL DEFAULT '',
    notes              TEXT    NOT NULL DEFAULT '',
    last_visit_at      INTEGER,
    is_active          INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_client_name ON client (full_name);
CREATE INDEX idx_client_phone ON client (phone);
CREATE INDEX idx_client_last_visit ON client (last_visit_at);

CREATE TABLE bill (
    id                      INTEGER PRIMARY KEY,
    bill_no                 TEXT    NOT NULL UNIQUE,
    -- One key per bill on screen: a repeated Finalize returns the same bill (design D10).
    idempotency_key         TEXT    NOT NULL UNIQUE,
    client_id               INTEGER REFERENCES client (id) ON DELETE RESTRICT,
    status                  TEXT    NOT NULL CHECK (status IN ('FINALIZED', 'CANCELLED', 'CORRECTED')),
    subtotal_paise          INTEGER NOT NULL CHECK (subtotal_paise >= 0),
    discount_paise          INTEGER NOT NULL CHECK (discount_paise >= 0),
    tax_paise               INTEGER NOT NULL CHECK (tax_paise >= 0),
    round_off_paise         INTEGER NOT NULL,
    total_paise             INTEGER NOT NULL CHECK (total_paise >= 0),
    amount_received_paise   INTEGER,
    change_paise            INTEGER,
    returned_paise          INTEGER NOT NULL DEFAULT 0 CHECK (returned_paise >= 0),
    note                    TEXT    NOT NULL DEFAULT '',
    created_by              INTEGER NOT NULL REFERENCES app_user (id),
    discount_approved_by    INTEGER REFERENCES app_user (id),
    finalized_at            INTEGER NOT NULL,
    cancelled_at            INTEGER,
    cancelled_by            INTEGER REFERENCES app_user (id),
    cancel_reason           TEXT,
    replaces_bill_id        INTEGER REFERENCES bill (id),
    corrected_by_bill_id    INTEGER REFERENCES bill (id)
) STRICT;
CREATE INDEX idx_bill_finalized ON bill (finalized_at);
CREATE INDEX idx_bill_client ON bill (client_id, finalized_at);

CREATE TABLE bill_item (
    id                         INTEGER PRIMARY KEY,
    bill_id                    INTEGER NOT NULL REFERENCES bill (id),
    line_no                    INTEGER NOT NULL,
    product_id                 INTEGER NOT NULL REFERENCES product (id),
    product_name               TEXT    NOT NULL,     -- snapshot: receipts never change later
    sku                        TEXT    NOT NULL,
    unit                       TEXT    NOT NULL,
    qty                        INTEGER NOT NULL CHECK (qty >= 0),
    -- Prescribed but not given because it was out of stock (DEC-002): qty 0, amounts 0.
    not_supplied_qty           INTEGER NOT NULL DEFAULT 0 CHECK (not_supplied_qty >= 0),
    unit_price_paise           INTEGER NOT NULL CHECK (unit_price_paise >= 0),
    discount_share_paise       INTEGER NOT NULL CHECK (discount_share_paise >= 0),
    gst_rate_bp                INTEGER NOT NULL,
    tax_paise                  INTEGER NOT NULL,
    line_total_paise           INTEGER NOT NULL CHECK (line_total_paise >= 0),
    returned_qty               INTEGER NOT NULL DEFAULT 0,
    CHECK (qty > 0 OR not_supplied_qty > 0),
    CHECK (returned_qty BETWEEN 0 AND qty),
    UNIQUE (bill_id, line_no)
) STRICT;
CREATE INDEX idx_bill_item_product ON bill_item (product_id);

CREATE TABLE bill_item_batch (
    id            INTEGER PRIMARY KEY,
    bill_item_id  INTEGER NOT NULL REFERENCES bill_item (id),
    batch_id      INTEGER NOT NULL REFERENCES inventory_batch (id),
    qty           INTEGER NOT NULL CHECK (qty > 0),
    returned_qty  INTEGER NOT NULL DEFAULT 0,
    CHECK (returned_qty BETWEEN 0 AND qty)
) STRICT;

CREATE TABLE sales_return (
    id             INTEGER PRIMARY KEY,
    return_no      TEXT    NOT NULL UNIQUE,
    bill_id        INTEGER NOT NULL REFERENCES bill (id),
    reason         TEXT    NOT NULL CHECK (length(reason) BETWEEN 1 AND 200),
    refund_paise   INTEGER NOT NULL CHECK (refund_paise >= 0),
    refund_method  TEXT    NOT NULL CHECK (refund_method IN ('CASH', 'UPI', 'CARD', 'OTHER')),
    user_id        INTEGER NOT NULL REFERENCES app_user (id),
    created_at     INTEGER NOT NULL
) STRICT;

CREATE TABLE return_item (
    id               INTEGER PRIMARY KEY,
    sales_return_id  INTEGER NOT NULL REFERENCES sales_return (id),
    bill_item_id     INTEGER NOT NULL REFERENCES bill_item (id),
    batch_id         INTEGER NOT NULL REFERENCES inventory_batch (id),
    qty              INTEGER NOT NULL CHECK (qty > 0),
    refund_paise     INTEGER NOT NULL CHECK (refund_paise >= 0),
    restocked        INTEGER NOT NULL CHECK (restocked IN (0, 1))
) STRICT;

CREATE TABLE payment (
    id               INTEGER PRIMARY KEY,
    bill_id          INTEGER NOT NULL REFERENCES bill (id),
    method           TEXT    NOT NULL CHECK (method IN ('CASH', 'UPI', 'CARD', 'OTHER')),
    amount_paise     INTEGER NOT NULL CHECK (amount_paise > 0),
    direction        TEXT    NOT NULL DEFAULT 'IN' CHECK (direction IN ('IN', 'REFUND')),
    reference        TEXT    NOT NULL DEFAULT '',
    sales_return_id  INTEGER REFERENCES sales_return (id),
    created_at       INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_payment_bill ON payment (bill_id);

-- Every stock change, with before/after quantities (brief §9). Append-only.
CREATE TABLE inventory_transaction (
    id               INTEGER PRIMARY KEY,
    occurred_at      INTEGER NOT NULL,
    product_id       INTEGER NOT NULL REFERENCES product (id),
    batch_id         INTEGER NOT NULL REFERENCES inventory_batch (id),
    kind             TEXT    NOT NULL CHECK (kind IN ('INITIAL_STOCK', 'PURCHASE', 'SALE', 'RETURN', 'ADJUSTMENT', 'DAMAGE', 'EXPIRY', 'CANCELLATION')),
    qty_change       INTEGER NOT NULL CHECK (qty_change <> 0),
    previous_qty     INTEGER NOT NULL,
    new_qty          INTEGER NOT NULL,
    reason           TEXT    NOT NULL DEFAULT '',
    bill_id          INTEGER REFERENCES bill (id),
    sales_return_id  INTEGER REFERENCES sales_return (id),
    user_id          INTEGER NOT NULL REFERENCES app_user (id),
    CHECK (new_qty >= 0 AND new_qty = previous_qty + qty_change)
) STRICT;
CREATE INDEX idx_inventory_tx_batch ON inventory_transaction (batch_id, id);
CREATE INDEX idx_inventory_tx_product ON inventory_transaction (product_id, occurred_at);
CREATE INDEX idx_inventory_tx_bill ON inventory_transaction (bill_id);

-- Gapless numbers per series and period (bill numbers restart every financial year, D14).
CREATE TABLE number_sequence (
    series      TEXT    NOT NULL,
    period      TEXT    NOT NULL,
    next_value  INTEGER NOT NULL CHECK (next_value >= 1),
    PRIMARY KEY (series, period)
) STRICT;

-- Financial and stock history is never deleted; the stock ledger is never edited either.
CREATE TRIGGER inventory_transaction_no_update BEFORE UPDATE ON inventory_transaction
BEGIN SELECT RAISE(ABORT, 'inventory_transaction is append-only'); END;
CREATE TRIGGER inventory_transaction_no_delete BEFORE DELETE ON inventory_transaction
BEGIN SELECT RAISE(ABORT, 'inventory_transaction is append-only'); END;
CREATE TRIGGER bill_no_delete BEFORE DELETE ON bill
BEGIN SELECT RAISE(ABORT, 'bills are never deleted'); END;
CREATE TRIGGER bill_item_no_delete BEFORE DELETE ON bill_item
BEGIN SELECT RAISE(ABORT, 'bill items are never deleted'); END;
CREATE TRIGGER bill_item_batch_no_delete BEFORE DELETE ON bill_item_batch
BEGIN SELECT RAISE(ABORT, 'bill item batches are never deleted'); END;
CREATE TRIGGER payment_no_delete BEFORE DELETE ON payment
BEGIN SELECT RAISE(ABORT, 'payments are never deleted'); END;
CREATE TRIGGER payment_no_update BEFORE UPDATE ON payment
BEGIN SELECT RAISE(ABORT, 'payments are never changed'); END;
CREATE TRIGGER sales_return_no_delete BEFORE DELETE ON sales_return
BEGIN SELECT RAISE(ABORT, 'returns are never deleted'); END;
CREATE TRIGGER return_item_no_delete BEFORE DELETE ON return_item
BEGIN SELECT RAISE(ABORT, 'return items are never deleted'); END;
