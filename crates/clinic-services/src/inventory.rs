//! Products, categories, suppliers, batches and stock (Phase 2).

use clinic_core::auth::Permission;
use clinic_core::fefo::{self, BatchStock};
use clinic_core::money::Paise;
use clinic_core::time::Date;
use clinic_sqlite::Database;
use clinic_sqlite::repo::billing::next_number;
use clinic_sqlite::repo::inventory::{
    self as repo, BatchRow, Category, LedgerRow, Movement, NewBatch, ProductFields, ProductQuery, ProductRow, Supplier,
};
use clinic_sqlite::rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::{self, Actor};
use crate::auth::verify_actor;
use crate::error::invalid;
use crate::settings::clinic_today;
use crate::{ServiceError, Session};

pub const PRODUCT_TYPES: [&str; 9] = ["MEDICINE", "TABLET", "CAPSULE", "CREAM", "OINTMENT", "GEL", "MEDICAL_SUPPLY", "CONSUMABLE", "OTHER"];
/// Ways an administrator can correct stock (design §6.4).
pub const ADJUSTMENT_KINDS: [&str; 3] = ["ADJUSTMENT", "DAMAGE", "EXPIRY"];
/// Batches expiring within this many days are flagged when selling (D10).
pub const EXPIRY_WARNING_DAYS: i64 = 30;

fn chars(text: &str) -> usize {
    text.chars().count()
}

pub(crate) fn batch_stock(batch: &BatchRow) -> BatchStock {
    BatchStock {
        batch_id: batch.id,
        expiry: batch.expiry_date.as_deref().and_then(Date::parse),
        quantity: batch.quantity,
        price: Paise::new(batch.selling_price_paise),
    }
}

// ---- Categories and suppliers ----------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInput {
    pub id: Option<i64>,
    pub name: String,
    #[serde(default = "yes")]
    pub is_active: bool,
}

fn yes() -> bool {
    true
}

pub fn list_categories(db: &Database, actor: &Session) -> Result<Vec<Category>, ServiceError> {
    actor.require(Permission::ViewInventory)?;
    Ok(db.read(repo::list_categories)?)
}

pub fn save_category(db: &mut Database, actor: &Session, input: CategoryInput, now: i64) -> Result<Vec<Category>, ServiceError> {
    actor.require(Permission::ManageInventory)?;
    let name = input.name.trim().to_string();
    if name.is_empty() || chars(&name) > 60 {
        return Err(invalid("name", "Category name is required (at most 60 characters)."));
    }
    db.write(|c| {
        verify_actor(c, actor)?;
        if repo::category_name_taken(c, &name, input.id)? {
            return Err(ServiceError::Conflict("A category with this name already exists.".into()));
        }
        let id = match input.id {
            Some(id) if repo::update_category(c, id, &name, input.is_active)? == 1 => id,
            Some(_) => return Err(ServiceError::NotFound("category")),
            None => repo::insert_category(c, &name, input.is_active)?,
        };
        audit::record(c, now, Actor::from(actor), "CATEGORY_SAVE", Some(("category", id.to_string())), Some(json!({ "name": name })))?;
        Ok(repo::list_categories(c)?)
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierInput {
    pub id: Option<i64>,
    pub name: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub gstin: String,
    #[serde(default = "yes")]
    pub is_active: bool,
}

pub fn list_suppliers(db: &Database, actor: &Session) -> Result<Vec<Supplier>, ServiceError> {
    actor.require(Permission::ViewInventory)?;
    Ok(db.read(repo::list_suppliers)?)
}

pub fn save_supplier(db: &mut Database, actor: &Session, input: SupplierInput, now: i64) -> Result<Vec<Supplier>, ServiceError> {
    actor.require(Permission::ManageInventory)?;
    let name = input.name.trim().to_string();
    let phone = input.phone.trim().to_string();
    let gstin = input.gstin.trim().to_uppercase();
    if name.is_empty() || chars(&name) > 100 {
        return Err(invalid("name", "Supplier name is required (at most 100 characters)."));
    }
    if chars(&phone) > 20 {
        return Err(invalid("phone", "Phone must be at most 20 characters."));
    }
    if !(gstin.is_empty() || (gstin.len() == 15 && gstin.chars().all(|c| c.is_ascii_alphanumeric()))) {
        return Err(invalid("gstin", "GSTIN must be 15 letters and digits, or left empty."));
    }
    db.write(|c| {
        verify_actor(c, actor)?;
        if repo::supplier_name_taken(c, &name, input.id)? {
            return Err(ServiceError::Conflict("A supplier with this name already exists.".into()));
        }
        let id = match input.id {
            Some(id) if repo::update_supplier(c, id, &name, &phone, &gstin, input.is_active)? == 1 => id,
            Some(_) => return Err(ServiceError::NotFound("supplier")),
            None => repo::insert_supplier(c, &name, &phone, &gstin, input.is_active)?,
        };
        audit::record(c, now, Actor::from(actor), "SUPPLIER_SAVE", Some(("supplier", id.to_string())), Some(json!({ "name": name })))?;
        Ok(repo::list_suppliers(c)?)
    })
}

// ---- Products --------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProductFilter {
    pub text: String,
    pub category_id: Option<i64>,
    pub include_inactive: bool,
    /// "ALL" (default), "LOW" or "OUT".
    pub stock: String,
    /// How many rows ("Load more" asks for more); default 200, at most 5000.
    pub limit: Option<u32>,
}

pub fn list_products(db: &Database, actor: &Session, filter: ProductFilter, now: i64) -> Result<Vec<ProductRow>, ServiceError> {
    actor.require(Permission::ViewInventory)?;
    let stock = match filter.stock.as_str() {
        "" | "ALL" => "ALL",
        "LOW" => "LOW",
        "OUT" => "OUT",
        _ => return Err(invalid("stock", "Unknown stock filter.")),
    };
    db.read(|c| {
        let (_, today) = clinic_today(c, now)?;
        let today = today.to_string();
        Ok(repo::query_products(
            c,
            &ProductQuery {
                text: &filter.text,
                category_id: filter.category_id,
                active_only: !filter.include_inactive,
                stock,
                product_id: None,
                today: &today,
                limit: filter.limit.unwrap_or(200).clamp(1, 5_000),
            },
        )?)
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductDetail {
    pub product: ProductRow,
    pub batches: Vec<BatchRow>,
}

pub fn get_product(db: &Database, actor: &Session, product_id: i64, now: i64) -> Result<ProductDetail, ServiceError> {
    actor.require(Permission::ViewInventory)?;
    db.read(|c| {
        let (_, today) = clinic_today(c, now)?;
        let product = repo::find_product(c, product_id, &today.to_string())?.ok_or(ServiceError::NotFound("product"))?;
        Ok(ProductDetail { batches: repo::batches_for_product(c, product_id)?, product })
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductInput {
    pub id: Option<i64>,
    /// Empty: a SKU like P-000012 is generated.
    #[serde(default)]
    pub sku: String,
    pub name: String,
    #[serde(default)]
    pub generic_name: String,
    pub category_id: Option<i64>,
    pub product_type: String,
    #[serde(default)]
    pub manufacturer: String,
    pub unit: String,
    #[serde(default)]
    pub gst_rate_bp: i64,
    #[serde(default)]
    pub default_selling_price_paise: i64,
    #[serde(default)]
    pub default_purchase_price_paise: i64,
    #[serde(default)]
    pub min_stock: i64,
    #[serde(default = "yes")]
    pub requires_expiry: bool,
    #[serde(default = "yes")]
    pub is_active: bool,
    #[serde(default)]
    pub notes: String,
}

fn validate_product(p: &ProductInput) -> Result<(), ServiceError> {
    let name_len = chars(p.name.trim());
    if name_len == 0 || name_len > 120 {
        return Err(invalid("name", "Product name is required (at most 120 characters)."));
    }
    if chars(p.generic_name.trim()) > 120 || chars(p.manufacturer.trim()) > 100 {
        return Err(invalid("genericName", "Generic name and manufacturer must be at most 120 characters."));
    }
    if !PRODUCT_TYPES.contains(&p.product_type.as_str()) {
        return Err(invalid("productType", "Please choose a product type."));
    }
    let unit_len = chars(p.unit.trim());
    if unit_len == 0 || unit_len > 20 {
        return Err(invalid("unit", "Unit is required, like strip, tube or bottle (at most 20 characters)."));
    }
    if !(0..=2800).contains(&p.gst_rate_bp) {
        return Err(invalid("gstRateBp", "GST rate must be between 0% and 28%."));
    }
    if p.default_selling_price_paise < 0 || p.default_purchase_price_paise < 0 || p.default_selling_price_paise > 100_000_000 {
        return Err(invalid("defaultSellingPricePaise", "Prices cannot be negative."));
    }
    if !(0..=1_000_000).contains(&p.min_stock) {
        return Err(invalid("minStock", "Minimum stock must be 0 or more."));
    }
    let sku = p.sku.trim();
    if chars(sku) > 40 || !sku.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.')) {
        return Err(invalid("sku", "SKU can use letters, digits and - _ / . (at most 40)."));
    }
    if chars(p.notes.trim()) > 500 {
        return Err(invalid("notes", "Notes must be at most 500 characters."));
    }
    Ok(())
}

/// The next automatic SKU, skipping numbers already taken by a hand-typed SKU (or every later
/// save would fail).
fn new_sku(c: &Connection) -> Result<String, ServiceError> {
    loop {
        let candidate = format!("P-{:06}", next_number(c, "PRODUCT", "ALL")?);
        if !repo::sku_taken(c, &candidate, None)? {
            return Ok(candidate);
        }
    }
}

pub fn save_product(db: &mut Database, actor: &Session, input: ProductInput, now: i64) -> Result<ProductRow, ServiceError> {
    actor.require(Permission::ManageInventory)?;
    validate_product(&input)?;
    db.write(|c| {
        verify_actor(c, actor)?;
        if let Some(category_id) = input.category_id {
            if !repo::list_categories(c)?.iter().any(|cat| cat.id == category_id) {
                return Err(ServiceError::NotAllowed("The selected category no longer exists.".into()));
            }
        }
        let sku = match input.sku.trim() {
            "" => match input.id {
                Some(id) => repo::find_product(c, id, "0000-00-00")?.ok_or(ServiceError::NotFound("product"))?.sku,
                None => new_sku(c)?,
            },
            given => given.to_uppercase(),
        };
        if repo::sku_taken(c, &sku, input.id)? {
            return Err(ServiceError::Conflict(format!("Another product already uses SKU {sku}.")));
        }
        let fields = ProductFields {
            sku: &sku,
            name: input.name.trim(),
            generic_name: input.generic_name.trim(),
            category_id: input.category_id,
            product_type: &input.product_type,
            manufacturer: input.manufacturer.trim(),
            unit: input.unit.trim(),
            gst_rate_bp: input.gst_rate_bp,
            default_selling_price_paise: input.default_selling_price_paise,
            default_purchase_price_paise: input.default_purchase_price_paise,
            min_stock: input.min_stock,
            requires_expiry: input.requires_expiry,
            is_active: input.is_active,
            notes: input.notes.trim(),
        };
        let (id, action) = match input.id {
            Some(id) if repo::update_product(c, id, &fields, now)? == 1 => (id, "PRODUCT_UPDATE"),
            Some(_) => return Err(ServiceError::NotFound("product")),
            None => (repo::insert_product(c, &fields, now)?, "PRODUCT_CREATE"),
        };
        audit::record(c, now, Actor::from(actor), action, Some(("product", id.to_string())), Some(json!({ "sku": sku, "active": input.is_active })))?;
        let (_, today) = clinic_today(c, now)?;
        repo::find_product(c, id, &today.to_string())?.ok_or(ServiceError::NotFound("product"))
    })
}

// ---- Stock in and adjustments ----------------------------------------------------------------

fn parse_expiry(text: Option<&str>) -> Result<Option<Date>, ServiceError> {
    match text.map(str::trim).filter(|t| !t.is_empty()) {
        None => Ok(None),
        Some(t) => Date::parse(t).map(Some).ok_or_else(|| invalid("expiryDate", "Expiry date must be a valid date.")),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockInInput {
    pub product_id: i64,
    pub batch_no: String,
    pub expiry_date: Option<String>,
    pub supplier_id: Option<i64>,
    pub purchase_price_paise: i64,
    pub selling_price_paise: i64,
    pub qty: i64,
    /// Opening stock when the clinic starts using the app (INITIAL_STOCK instead of PURCHASE).
    #[serde(default)]
    pub opening: bool,
    /// e.g. supplier invoice number.
    #[serde(default)]
    pub note: String,
}

/// The one-screen Add Inventory form (DEC-036): vendor, product, type, MRP, clinic bought price,
/// expiry and quantity. A new vendor or product is created on the way; the lot number is made
/// up automatically (never asked for). The MRP is the price billed before the standard discount.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddInventoryInput {
    /// An existing product, or `None` and `product_name` for a new one.
    #[serde(default)]
    pub product_id: Option<i64>,
    #[serde(default)]
    pub product_name: String,
    /// Product type (the admin-managed list), for a new product.
    #[serde(default)]
    pub type_id: Option<i64>,
    /// An existing vendor, or `None` and `vendor_name` for a new one.
    #[serde(default)]
    pub vendor_id: Option<i64>,
    #[serde(default)]
    pub vendor_name: String,
    pub mrp_paise: i64,
    pub purchase_price_paise: i64,
    pub expiry_date: Option<String>,
    pub qty: i64,
    /// Made once per form by the screen: a double-click or retry adds the stock only once.
    #[serde(default)]
    pub request_key: Option<String>,
}

pub fn add_inventory(db: &mut Database, actor: &Session, input: AddInventoryInput, now: i64) -> Result<BatchRow, ServiceError> {
    actor.require(Permission::AddStock)?;
    if !(1..=1_000_000).contains(&input.qty) {
        return Err(invalid("qty", "Quantity must be at least 1."));
    }
    if !(1..=100_000_000).contains(&input.mrp_paise) {
        return Err(invalid("mrpPaise", "MRP must be between ₹0.01 and ₹10,00,000."));
    }
    if !(0..=100_000_000).contains(&input.purchase_price_paise) {
        return Err(invalid("purchasePricePaise", "Bought price must be between ₹0 and ₹10,00,000."));
    }
    let expiry = parse_expiry(input.expiry_date.as_deref())?.ok_or_else(|| invalid("expiryDate", "Expiry date is required."))?;
    let request_key = input.request_key.as_deref().map(str::trim).filter(|k| !k.is_empty());
    if request_key.is_some_and(|k| k.len() > 64 || !k.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')) {
        return Err(invalid("requestKey", "Invalid request. Please open Add Inventory again."));
    }
    db.write(|c| {
        verify_actor(c, actor)?;
        if let Some(key) = request_key {
            if let Some(batch_id) = repo::find_batch_by_request_key(c, key)? {
                return repo::find_batch(c, batch_id)?.ok_or(ServiceError::NotFound("batch"));
            }
        }
        let (_, today) = clinic_today(c, now)?;
        if expiry < today {
            return Err(invalid("expiryDate", "This stock has already expired; it cannot be added."));
        }
        // Vendor: chosen from the list, or typed (found by name, or added).
        let vendor_id = match input.vendor_id {
            Some(id) => repo::list_suppliers(c)?.into_iter().find(|s| s.id == id).map(|s| s.id).ok_or(ServiceError::NotFound("vendor"))?,
            None => {
                let name = input.vendor_name.trim();
                if name.is_empty() || chars(name) > 100 {
                    return Err(invalid("vendorName", "Vendor name is required (at most 100 characters)."));
                }
                match repo::list_suppliers(c)?.into_iter().find(|s| s.name.to_lowercase() == name.to_lowercase()) {
                    Some(existing) => existing.id,
                    None => {
                        let id = repo::insert_supplier(c, name, "", "", true)?;
                        audit::record(c, now, Actor::from(actor), "SUPPLIER_CREATE", Some(("supplier", id.to_string())), Some(json!({ "name": name, "addedWithStock": true })))?;
                        id
                    }
                }
            }
        };
        // Product: chosen from the list, or typed (found by name, or added with its type).
        let today_text = today.to_string();
        let (product, is_new) = match input.product_id {
            Some(id) => (repo::find_product(c, id, &today_text)?.ok_or(ServiceError::NotFound("product"))?, false),
            None => {
                let name = input.product_name.trim();
                if name.is_empty() || chars(name) > 120 {
                    return Err(invalid("productName", "Product name is required (at most 120 characters)."));
                }
                // Exact name (any case), active first: "Tretinoin 0.025%" is found as typed.
                let same_name = match repo::find_product_id_by_name(c, name)? {
                    Some(id) => repo::find_product(c, id, &today_text)?,
                    None => None,
                };
                match same_name {
                    Some(existing) => (existing, false),
                    None => {
                        if let Some(type_id) = input.type_id {
                            if !repo::list_categories(c)?.iter().any(|t| t.id == type_id && t.is_active) {
                                return Err(ServiceError::NotAllowed("The selected product type no longer exists.".into()));
                            }
                        }
                        let sku = new_sku(c)?;
                        let fields = ProductFields {
                            sku: &sku,
                            name,
                            generic_name: "",
                            category_id: input.type_id,
                            product_type: "OTHER",
                            manufacturer: "",
                            unit: "pcs",
                            gst_rate_bp: 0,
                            default_selling_price_paise: input.mrp_paise,
                            default_purchase_price_paise: input.purchase_price_paise,
                            min_stock: 0,
                            requires_expiry: true,
                            is_active: true,
                            notes: "",
                        };
                        let id = repo::insert_product(c, &fields, now)?;
                        audit::record(c, now, Actor::from(actor), "PRODUCT_CREATE", Some(("product", id.to_string())), Some(json!({ "sku": sku, "addedWithStock": true })))?;
                        (repo::find_product(c, id, &today_text)?.ok_or(ServiceError::NotFound("product"))?, true)
                    }
                }
            }
        };
        if !product.is_active {
            return Err(ServiceError::NotAllowed(format!("{} is archived. An administrator can restore it under Inventory → Update.", product.name)));
        }
        // Prices shown in lists follow the latest delivery, set by an administrator; a
        // receptionist's delivery keeps its own prices on the lot.
        if !is_new && actor.role.allows(Permission::ManageInventory) {
            repo::update_product_prices(c, product.id, input.mrp_paise, input.purchase_price_paise, now)?;
        }
        // Skip numbers already used by a hand-typed batch number, or this product would be stuck.
        let lot_no = loop {
            let candidate = format!("LOT-{:06}", next_number(c, "LOT", "ALL")?);
            if repo::find_batch_by_no(c, product.id, &candidate)?.is_none() {
                break candidate;
            }
        };
        let expiry_text = expiry.to_string();
        let batch_id = repo::insert_batch(
            c,
            &NewBatch {
                product_id: product.id,
                batch_no: &lot_no,
                expiry_date: Some(&expiry_text),
                supplier_id: Some(vendor_id),
                purchase_price_paise: input.purchase_price_paise,
                selling_price_paise: input.mrp_paise,
                now,
            },
        )?;
        repo::apply_movement(c, &Movement { batch_id, kind: "PURCHASE", qty_change: input.qty, reason: "Stock received", bill_id: None, sales_return_id: None, user_id: actor.user_id, now })?
            .ok_or_else(|| ServiceError::Corrupt("add inventory failed".into()))?;
        if let Some(key) = request_key {
            repo::set_batch_request_key(c, batch_id, key)?;
        }
        audit::record(c, now, Actor::from(actor), "STOCK_IN", Some(("batch", batch_id.to_string())), Some(json!({ "product": product.sku, "lot": lot_no, "qty": input.qty, "mrp": input.mrp_paise })))?;
        repo::find_batch(c, batch_id)?.ok_or(ServiceError::NotFound("batch"))
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteOutcome {
    /// Removed completely (it was never stocked or sold).
    pub deleted: bool,
    /// Kept for its history but hidden from billing and lists (soft delete).
    pub archived: bool,
}

/// Inventory → Delete (administrators): a product with any stock history or bills is archived,
/// never removed, so old bills and the stock ledger stay complete. An unused one is removed.
pub fn delete_product(db: &mut Database, actor: &Session, product_id: i64, now: i64) -> Result<DeleteOutcome, ServiceError> {
    actor.require(Permission::ManageInventory)?;
    db.write(|c| {
        verify_actor(c, actor)?;
        let product = repo::find_product(c, product_id, "0000-00-00")?.ok_or(ServiceError::NotFound("product"))?;
        let details = Some(json!({ "sku": product.sku, "name": product.name }));
        if repo::product_in_use(c, product_id)? {
            if product.total_qty > 0 {
                return Err(ServiceError::NotAllowed(format!(
                    "{} still has {} {} in stock. Adjust the stock to 0 first, or keep the product.",
                    product.name, product.total_qty, product.unit
                )));
            }
            repo::set_product_active(c, product_id, false, now)?;
            audit::record(c, now, Actor::from(actor), "PRODUCT_ARCHIVE", Some(("product", product_id.to_string())), details)?;
            Ok(DeleteOutcome { deleted: false, archived: true })
        } else {
            repo::delete_unused_product(c, product_id)?;
            audit::record(c, now, Actor::from(actor), "PRODUCT_DELETE", Some(("product", product_id.to_string())), details)?;
            Ok(DeleteOutcome { deleted: true, archived: false })
        }
    })
}

/// Receives stock into a batch (new or existing) and records it in the ledger.
pub fn stock_in(db: &mut Database, actor: &Session, input: StockInInput, now: i64) -> Result<BatchRow, ServiceError> {
    actor.require(Permission::ManageInventory)?;
    let batch_no = input.batch_no.trim().to_uppercase();
    if batch_no.is_empty() || chars(&batch_no) > 40 {
        return Err(invalid("batchNo", "Batch number is required (at most 40 characters)."));
    }
    if !(1..=1_000_000).contains(&input.qty) {
        return Err(invalid("qty", "Quantity must be at least 1."));
    }
    if input.purchase_price_paise < 0 || input.selling_price_paise < 0 || input.purchase_price_paise > 100_000_000 || input.selling_price_paise > 100_000_000 {
        return Err(invalid("sellingPricePaise", "Prices must be between ₹0 and ₹10,00,000."));
    }
    if chars(input.note.trim()) > 200 {
        return Err(invalid("note", "Note must be at most 200 characters."));
    }
    let expiry = parse_expiry(input.expiry_date.as_deref())?;
    db.write(|c| {
        verify_actor(c, actor)?;
        let (_, today) = clinic_today(c, now)?;
        let product = repo::find_product(c, input.product_id, &today.to_string())?.ok_or(ServiceError::NotFound("product"))?;
        if !product.is_active {
            return Err(ServiceError::NotAllowed("This product is inactive. Activate it before adding stock.".into()));
        }
        if product.requires_expiry && expiry.is_none() {
            return Err(invalid("expiryDate", "This product needs an expiry date."));
        }
        if expiry.is_some_and(|e| e < today) {
            return Err(invalid("expiryDate", "This batch has already expired; it cannot be added as stock."));
        }
        if let Some(supplier_id) = input.supplier_id {
            if !repo::list_suppliers(c)?.iter().any(|s| s.id == supplier_id) {
                return Err(ServiceError::NotAllowed("The selected supplier no longer exists.".into()));
            }
        }
        let expiry_text = expiry.map(|e| e.to_string());
        let batch_id = match repo::find_batch_by_no(c, product.id, &batch_no)? {
            Some(existing) => {
                if existing.expiry_date != expiry_text {
                    return Err(invalid(
                        "expiryDate",
                        &format!("Batch {batch_no} already exists with expiry {}.", existing.expiry_date.as_deref().unwrap_or("none")),
                    ));
                }
                // Adding to a batch must not silently reprice the units already on the shelf.
                if existing.selling_price_paise != input.selling_price_paise || existing.purchase_price_paise != input.purchase_price_paise {
                    return Err(invalid(
                        "sellingPricePaise",
                        &format!("Batch {batch_no} is already in stock at different prices. Use the same prices, or a different batch number."),
                    ));
                }
                repo::update_batch_details(c, existing.id, expiry_text.as_deref(), input.supplier_id.or(existing.supplier_id), input.purchase_price_paise, input.selling_price_paise, now)?;
                existing.id
            }
            None => repo::insert_batch(
                c,
                &NewBatch {
                    product_id: product.id,
                    batch_no: &batch_no,
                    expiry_date: expiry_text.as_deref(),
                    supplier_id: input.supplier_id,
                    purchase_price_paise: input.purchase_price_paise,
                    selling_price_paise: input.selling_price_paise,
                    now,
                },
            )?,
        };
        let kind = if input.opening { "INITIAL_STOCK" } else { "PURCHASE" };
        let reason = if input.note.trim().is_empty() { "Stock received" } else { input.note.trim() };
        repo::apply_movement(c, &Movement { batch_id, kind, qty_change: input.qty, reason, bill_id: None, sales_return_id: None, user_id: actor.user_id, now })?
            .ok_or_else(|| ServiceError::Corrupt("stock in failed".into()))?;
        audit::record(c, now, Actor::from(actor), "STOCK_IN", Some(("batch", batch_id.to_string())), Some(json!({ "product": product.sku, "batch": batch_no, "qty": input.qty, "kind": kind })))?;
        repo::find_batch(c, batch_id)?.ok_or(ServiceError::NotFound("batch"))
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustInput {
    pub batch_id: i64,
    /// The quantity actually on the shelf.
    pub counted_qty: i64,
    /// ADJUSTMENT, DAMAGE or EXPIRY.
    pub kind: String,
    pub reason: String,
}

/// Sets a batch to the counted quantity (never below zero), with a mandatory reason.
pub fn adjust_stock(db: &mut Database, actor: &Session, input: AdjustInput, now: i64) -> Result<BatchRow, ServiceError> {
    actor.require(Permission::ManageInventory)?;
    if !ADJUSTMENT_KINDS.contains(&input.kind.as_str()) {
        return Err(invalid("kind", "Choose adjustment, damage or expiry."));
    }
    let reason = input.reason.trim();
    if chars(reason) < 3 || chars(reason) > 200 {
        return Err(invalid("reason", "Please give a reason (3 to 200 characters)."));
    }
    if !(0..=1_000_000).contains(&input.counted_qty) {
        return Err(invalid("countedQty", "Counted quantity must be 0 or more."));
    }
    db.write(|c| {
        verify_actor(c, actor)?;
        let batch = repo::find_batch(c, input.batch_id)?.ok_or(ServiceError::NotFound("batch"))?;
        let change = input.counted_qty - batch.quantity;
        if change == 0 {
            return Err(ServiceError::NotAllowed("The counted quantity is the same as the stock; nothing to change.".into()));
        }
        if input.kind != "ADJUSTMENT" && change > 0 {
            return Err(invalid("kind", "Damage and expiry can only reduce stock. Use 'Adjustment' to increase it."));
        }
        repo::apply_movement(c, &Movement { batch_id: batch.id, kind: &input.kind, qty_change: change, reason, bill_id: None, sales_return_id: None, user_id: actor.user_id, now })?
            .ok_or_else(|| ServiceError::Corrupt("adjustment failed".into()))?;
        audit::record(c, now, Actor::from(actor), "STOCK_ADJUST", Some(("batch", batch.id.to_string())), Some(json!({ "from": batch.quantity, "to": input.counted_qty, "kind": input.kind })))?;
        repo::find_batch(c, batch.id)?.ok_or(ServiceError::NotFound("batch"))
    })
}

pub fn ledger(db: &Database, actor: &Session, product_id: Option<i64>, limit: u32) -> Result<Vec<LedgerRow>, ServiceError> {
    actor.require(Permission::ViewStockLedger)?;
    Ok(db.read(|c| repo::ledger(c, product_id, limit.clamp(1, 1000)))?)
}

// ---- Expiry --------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpiringBatch {
    #[serde(flatten)]
    pub batch: BatchRow,
    /// Negative: already expired that many days ago.
    pub days_left: i64,
}

/// Batches with stock that are expired or expire within `within_days` (30/60/90 in the UI).
pub fn expiring(db: &Database, actor: &Session, within_days: i64, now: i64) -> Result<Vec<ExpiringBatch>, ServiceError> {
    actor.require(Permission::ViewInventory)?;
    db.read(|c| {
        let (_, today) = clinic_today(c, now)?;
        let until = today.add_days(within_days.clamp(0, 3650)).to_string();
        Ok(repo::expiring_batches(c, &until)?
            .into_iter()
            .map(|batch| {
                let days_left = batch.expiry_date.as_deref().and_then(Date::parse).map_or(0, |e| e.days_since_epoch() - today.days_since_epoch());
                ExpiringBatch { batch, days_left }
            })
            .collect())
    })
}

// ---- Product search for the billing screen -------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaleProduct {
    pub product_id: i64,
    pub name: String,
    pub generic_name: String,
    pub sku: String,
    pub unit: String,
    pub gst_rate_bp: i64,
    /// Price of the batch that would be sold first (FEFO).
    pub price_paise: i64,
    /// Sellable today (expired stock excluded).
    pub available_qty: i64,
    pub next_expiry: Option<String>,
    /// The next batch expires within 30 days (D10 warning).
    pub expires_soon: bool,
}

pub(crate) fn sale_product(c: &Connection, product: &ProductRow, today: Date) -> Result<SaleProduct, ServiceError> {
    let batches: Vec<BatchStock> = repo::batches_for_product(c, product.id)?.iter().map(batch_stock).collect();
    let first = fefo::allocate(&batches, today, 1).ok().and_then(|picks| picks.first().copied());
    let first_batch = first.and_then(|a| batches.iter().find(|b| b.batch_id == a.batch_id));
    Ok(SaleProduct {
        product_id: product.id,
        name: product.name.clone(),
        generic_name: product.generic_name.clone(),
        sku: product.sku.clone(),
        unit: product.unit.clone(),
        gst_rate_bp: product.gst_rate_bp,
        price_paise: first.map_or(product.default_selling_price_paise, |a| a.price.value()),
        available_qty: fefo::available(&batches, today),
        next_expiry: first_batch.and_then(|b| b.expiry).map(|e| e.to_string()),
        expires_soon: first_batch.and_then(|b| b.expiry).is_some_and(|e| e <= today.add_days(EXPIRY_WARNING_DAYS)),
    })
}

/// Active products matching the text, with what can be sold today. Out-of-stock products are
/// included (available 0) so they can be recorded as "not supplied" (DEC-002).
/// Recently sold products, for one-click adding on the New Bill screen.
pub fn recent_for_sale(db: &Database, actor: &Session, now: i64) -> Result<Vec<SaleProduct>, ServiceError> {
    actor.require(Permission::CreateBills)?;
    db.read(|c| {
        let (_, today) = clinic_today(c, now)?;
        let today_text = today.to_string();
        let mut products = Vec::new();
        for id in clinic_sqlite::repo::billing::recent_product_ids(c, 8)? {
            let rows = repo::query_products(
                c,
                &ProductQuery { text: "", category_id: None, active_only: true, stock: "ALL", product_id: Some(id), today: &today_text, limit: 1 },
            )?;
            for row in &rows {
                products.push(sale_product(c, row, today)?);
            }
        }
        Ok(products)
    })
}

pub fn search_for_sale(db: &Database, actor: &Session, text: &str, now: i64) -> Result<Vec<SaleProduct>, ServiceError> {
    actor.require(Permission::CreateBills)?;
    db.read(|c| {
        let (_, today) = clinic_today(c, now)?;
        let today_text = today.to_string();
        let products = repo::query_products(
            c,
            &ProductQuery { text, category_id: None, active_only: true, stock: "ALL", product_id: None, today: &today_text, limit: 20 },
        )?;
        products.iter().map(|p| sale_product(c, p, today)).collect()
    })
}
