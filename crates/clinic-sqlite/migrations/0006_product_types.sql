-- v0.4: product types are the admin-managed list (the `category` table, shown as "Product
-- types"). Seed the usual types for a skin clinic; existing names are kept as they are.
INSERT OR IGNORE INTO category (name) VALUES
    ('Tablet'), ('Capsule'), ('Cream'), ('Ointment'), ('Gel'), ('Lotion'), ('Serum'),
    ('Sunscreen'), ('Face wash'), ('Shampoo'), ('Soap'), ('Injection'), ('Consumable'), ('Other');
