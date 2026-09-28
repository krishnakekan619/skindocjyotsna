-- v0.4.3 consultation types (owner brief 2026-09-28). Data only; nothing is removed.
-- General Consultation is now ₹400 (only if it still has the original ₹500: a price the clinic
-- set itself is kept). Follow-up Consultation (₹300) was added in 0005. Discounted Consultation
-- has no set price: the receptionist types the amount on the bill.
-- Bills already made keep the name and price they were billed with (bill_service_item snapshot).
UPDATE service
SET default_price_paise = 40000, updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE kind = 'CONSULTATION' AND name = 'General Consultation' AND default_price_paise = 50000;

INSERT OR IGNORE INTO service (kind, name, default_price_paise, sort_order, created_at, updated_at) VALUES
    ('CONSULTATION', 'Follow-up Consultation', 30000, 2, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)),
    ('CONSULTATION', 'Discounted Consultation', 0, 3, CAST(strftime('%s', 'now') AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER));
