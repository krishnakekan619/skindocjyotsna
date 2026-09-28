//! Inventory → Import from CSV (administrators, DEC-041): many deliveries from one sheet.
//!
//! Columns (first row, any order, extra columns ignored): Vendor Name, Product Name, MRP, Clinic
//! Bought Price, Expiry Date (optional), Quantity. Every row becomes one delivery exactly as with
//! Add Inventory (same vendor/product matching, a made-up lot number, the audit entry).
//!
//! Nothing is imported until every row is correct, and then all rows go in together in ONE
//! transaction. A file already imported once is refused unless the admin confirms, and a repeated
//! request (double-click, retry) adds nothing.

use std::collections::HashSet;

use clinic_core::auth::Permission;
use clinic_core::time::Date;
use clinic_sqlite::Database;
use clinic_sqlite::repo::audit as audit_repo;
use clinic_sqlite::repo::inventory as repo;
use clinic_sqlite::rusqlite::Connection;
use serde::Serialize;
use serde_json::json;

use crate::audit::{self, Actor};
use crate::auth::verify_actor;
use crate::error::invalid;
use crate::inventory::{AddInventoryInput, add_delivery_in_tx};
use crate::settings::clinic_today;
use crate::{ServiceError, Session};

pub const MAX_FILE_BYTES: usize = 1_000_000;
pub const MAX_ROWS: usize = 2_000;
/// The empty sheet the admin fills in (opened in Excel by the app).
pub const TEMPLATE_CSV: &str = "Vendor Name,Product Name,MRP,Clinic Bought Price,Expiry Date,Quantity\r\n";
pub const TEMPLATE_FILE_NAME: &str = "Stock-import-sheet.csv";
const IMPORT_ACTION: &str = "STOCK_IMPORT";

/// One row of the file as understood, with anything wrong with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRow {
    /// Line number in the file (the column names are line 1).
    pub line: usize,
    pub vendor_name: String,
    pub product_name: String,
    pub mrp_paise: Option<i64>,
    pub purchase_price_paise: Option<i64>,
    /// `YYYY-MM-DD`; none = no expiry date.
    pub expiry_date: Option<String>,
    pub qty: Option<i64>,
    /// First row of a product not in the list yet: it will be added.
    pub new_product: bool,
    /// First row of a vendor not in the list yet: it will be added.
    pub new_vendor: bool,
    /// Must be fixed before anything is imported.
    pub errors: Vec<String>,
    /// Worth a look, but does not stop the import.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    /// Identifies the file's content, so importing the same file twice can be noticed.
    pub file_id: String,
    pub rows: Vec<ImportRow>,
    pub error_rows: usize,
    pub new_products: usize,
    pub new_vendors: usize,
    pub total_qty: i64,
    /// When this same file was imported before (Unix seconds), if it was.
    pub imported_before_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub rows: usize,
    pub new_products: usize,
    pub new_vendors: usize,
    pub total_qty: i64,
}

/// Checks the file and shows what would be imported. Changes nothing.
pub fn preview(db: &Database, actor: &Session, text: &str, now: i64) -> Result<ImportPreview, ServiceError> {
    actor.require(Permission::ManageInventory)?;
    let (file_id, mut rows) = parse_file(text)?;
    db.read(|c| {
        let (_, today) = clinic_today(c, now)?;
        annotate(c, &mut rows, today)?;
        let imported_before_at = audit_repo::last_occurrence(c, IMPORT_ACTION, &file_id)?;
        let error_rows = rows.iter().filter(|r| !r.errors.is_empty()).count();
        let new_products = rows.iter().filter(|r| r.new_product).count();
        let new_vendors = rows.iter().filter(|r| r.new_vendor).count();
        let total_qty = rows.iter().filter_map(|r| r.qty).sum();
        Ok(ImportPreview { file_id, rows, error_rows, new_products, new_vendors, total_qty, imported_before_at })
    })
}

/// Imports every row, all in one transaction. Refused if any row has a problem, or if this file
/// was imported before and `import_again` is not set. `request_key` is made once per dialog: the
/// same request sent again adds nothing.
pub fn import(db: &mut Database, actor: &Session, text: &str, request_key: &str, import_again: bool, now: i64) -> Result<ImportResult, ServiceError> {
    actor.require(Permission::ManageInventory)?;
    let key = request_key.trim();
    if key.is_empty() || key.len() > 40 || !key.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
        return Err(invalid("requestKey", "Invalid request. Please open Import again."));
    }
    let (file_id, mut rows) = parse_file(text)?;
    let total_qty: i64 = rows.iter().filter_map(|r| r.qty).sum();
    db.write(|c| {
        verify_actor(c, actor)?;
        // The same request again (double-click, retry): the first time already imported it all.
        if repo::find_batch_by_request_key(c, &format!("{key}-1"))?.is_some() {
            return Ok(ImportResult { rows: rows.len(), new_products: 0, new_vendors: 0, total_qty });
        }
        let (_, today) = clinic_today(c, now)?;
        annotate(c, &mut rows, today)?;
        let bad = rows.iter().filter(|r| !r.errors.is_empty()).count();
        if bad > 0 {
            return Err(ServiceError::NotAllowed(format!("{bad} row(s) have a problem. Nothing was imported: correct the file, save it and choose it again.")));
        }
        if !import_again && audit_repo::last_occurrence(c, IMPORT_ACTION, &file_id)?.is_some() {
            return Err(ServiceError::Conflict("This file was already imported. Tick \"Import again\" only if these are new deliveries.".into()));
        }
        let new_products = rows.iter().filter(|r| r.new_product).count();
        let new_vendors = rows.iter().filter(|r| r.new_vendor).count();
        for (index, row) in rows.iter().enumerate() {
            let input = AddInventoryInput {
                product_id: None,
                product_name: row.product_name.clone(),
                type_id: None,
                vendor_id: None,
                vendor_name: row.vendor_name.clone(),
                mrp_paise: row.mrp_paise.unwrap_or(0),
                purchase_price_paise: row.purchase_price_paise.unwrap_or(0),
                expiry_date: row.expiry_date.clone(),
                qty: row.qty.unwrap_or(0),
                request_key: None,
            };
            let expiry = row.expiry_date.as_deref().and_then(Date::parse);
            let batch_id = add_delivery_in_tx(c, actor, &input, expiry, today, now)?;
            repo::set_batch_request_key(c, batch_id, &format!("{key}-{}", index + 1))?;
        }
        audit::record(
            c,
            now,
            Actor::from(actor),
            IMPORT_ACTION,
            Some(("stock_import", file_id.clone())),
            Some(json!({ "rows": rows.len(), "newProducts": new_products, "newVendors": new_vendors, "totalQty": total_qty })),
        )?;
        Ok(ImportResult { rows: rows.len(), new_products, new_vendors, total_qty })
    })
}

// ---- checks that need the database -------------------------------------------------------------

/// Expired stock, archived products, and which products and vendors are new.
fn annotate(c: &Connection, rows: &mut [ImportRow], today: Date) -> Result<(), ServiceError> {
    let known_vendors: HashSet<String> = repo::list_suppliers(c)?.into_iter().map(|s| s.name.to_lowercase()).collect();
    let mut seen_vendors = HashSet::new();
    let mut seen_products = HashSet::new();
    let today_text = today.to_string();
    for row in rows.iter_mut() {
        if let Some(expiry) = row.expiry_date.as_deref().and_then(Date::parse) {
            if expiry < today {
                row.errors.push(format!("Expiry {} has already passed: expired stock cannot be added.", expiry.display()));
            }
        }
        if !row.vendor_name.is_empty() {
            let vendor = row.vendor_name.to_lowercase();
            if !known_vendors.contains(&vendor) && seen_vendors.insert(vendor) {
                row.new_vendor = true;
            }
        }
        if !row.product_name.is_empty() {
            match repo::find_product_id_by_name(c, &row.product_name)? {
                Some(id) => {
                    if let Some(product) = repo::find_product(c, id, &today_text)? {
                        if !product.is_active {
                            row.errors.push(format!("{} is archived. Restore it under Inventory → Update first, or remove this row.", product.name));
                        }
                    }
                }
                None => {
                    if seen_products.insert(row.product_name.to_lowercase()) {
                        row.new_product = true;
                    }
                }
            }
        }
    }
    Ok(())
}

// ---- reading the file ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Columns {
    vendor: usize,
    product: usize,
    mrp: usize,
    bought: usize,
    expiry: Option<usize>,
    qty: usize,
}

/// The file's id and its rows, each already checked on its own.
fn parse_file(text: &str) -> Result<(String, Vec<ImportRow>), ServiceError> {
    if text.len() > MAX_FILE_BYTES {
        return Err(invalid("file", "The file is larger than 1 MB. Split it into smaller files."));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records = split_records(text, delimiter_of(text))?.into_iter();
    let Some((_, header)) = records.next() else {
        return Err(invalid("file", "The file is empty."));
    };
    let columns = columns_of(&header)?;
    let mut rows = Vec::new();
    for (line, fields) in records {
        if rows.len() >= MAX_ROWS {
            return Err(invalid("file", "At most 2,000 rows can be imported at a time. Split the file into smaller files."));
        }
        let cell = |index: Option<usize>| index.and_then(|i| fields.get(i)).map(|s| s.trim().to_string()).unwrap_or_default();
        rows.push(check_row(
            line,
            cell(Some(columns.vendor)),
            cell(Some(columns.product)),
            &cell(Some(columns.mrp)),
            &cell(Some(columns.bought)),
            &cell(columns.expiry),
            &cell(Some(columns.qty)),
        ));
    }
    if rows.is_empty() {
        return Err(invalid("file", "The file has the column names but no rows."));
    }
    Ok((file_id(text), rows))
}

/// Excel saves CSV with commas, or semicolons in some regions; tab-separated text also works.
fn delimiter_of(text: &str) -> char {
    let first_line = text.lines().next().unwrap_or("");
    let count = |d: char| first_line.matches(d).count();
    if count(';') > count(',') && count(';') >= count('\t') {
        ';'
    } else if count('\t') > count(',') {
        '\t'
    } else {
        ','
    }
}

/// Splits CSV text into records of fields, with the line number each record starts on. Quoted
/// fields may contain the delimiter, line breaks and doubled quotes (""), as Excel writes them.
fn split_records(text: &str, delimiter: char) -> Result<Vec<(usize, Vec<String>)>, ServiceError> {
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut line = 1;
    let mut start_line = 1;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                if ch == '\n' {
                    line += 1;
                }
                field.push(ch);
            }
        } else if ch == '"' && field.trim().is_empty() {
            field.clear();
            in_quotes = true;
        } else if ch == delimiter {
            fields.push(std::mem::take(&mut field));
        } else if ch == '\n' {
            fields.push(std::mem::take(&mut field));
            records.push((start_line, std::mem::take(&mut fields)));
            line += 1;
            start_line = line;
        } else if ch != '\r' {
            field.push(ch);
        }
    }
    if in_quotes {
        return Err(invalid("file", &format!("Line {start_line}: a quote mark (\") is opened but never closed.")));
    }
    if !field.is_empty() || !fields.is_empty() {
        fields.push(field);
        records.push((start_line, fields));
    }
    Ok(records.into_iter().filter(|(_, f)| f.iter().any(|x| !x.trim().is_empty())).collect())
}

/// Finds the columns by name, ignoring case, spaces, symbols and "(Rs)"/"(INR)".
fn columns_of(header: &[String]) -> Result<Columns, ServiceError> {
    const VENDOR: &[&str] = &["vendorname", "vendor", "pharmavendorname", "pharmavendor", "pharmaname", "pharma", "suppliername", "supplier"];
    const PRODUCT: &[&str] = &["productname", "product", "medicinename", "medicine", "itemname", "item", "name"];
    const MRP: &[&str] = &["mrp", "mrpprice"];
    const BOUGHT: &[&str] = &["clinicboughtprice", "boughtprice", "purchaseprice", "costprice", "clinicprice", "buyingprice"];
    const EXPIRY: &[&str] = &["expirydate", "expiry", "exp", "expdate", "expiresOn"];
    const QTY: &[&str] = &["quantity", "qty", "stock", "units"];
    let keys: Vec<String> = header.iter().map(String::as_str).map(header_key).collect();
    let find = |names: &[&str]| keys.iter().position(|k| names.iter().any(|n| n.eq_ignore_ascii_case(k)));
    let (vendor, product, mrp, bought, qty) = (find(VENDOR), find(PRODUCT), find(MRP), find(BOUGHT), find(QTY));
    let missing: Vec<&str> = [(vendor, "Vendor Name"), (product, "Product Name"), (mrp, "MRP"), (bought, "Clinic Bought Price"), (qty, "Quantity")]
        .iter()
        .filter(|(found, _)| found.is_none())
        .map(|(_, name)| *name)
        .collect();
    match (vendor, product, mrp, bought, qty) {
        (Some(vendor), Some(product), Some(mrp), Some(bought), Some(qty)) => Ok(Columns { vendor, product, mrp, bought, expiry: find(EXPIRY), qty }),
        _ => Err(invalid(
            "file",
            &format!(
                "The first row must be the column names: Vendor Name, Product Name, MRP, Clinic Bought Price, Expiry Date, Quantity. Missing: {}.",
                missing.join(", ")
            ),
        )),
    }
}

fn header_key(header: &str) -> String {
    let key: String = header.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_lowercase();
    ["inr", "rs"].iter().find_map(|unit| key.strip_suffix(unit).filter(|rest| !rest.is_empty())).map_or(key.clone(), str::to_string)
}

/// Checks one row on its own (the database checks come later, in `annotate`).
fn check_row(line: usize, vendor_name: String, product_name: String, mrp: &str, bought: &str, expiry: &str, qty: &str) -> ImportRow {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    if vendor_name.is_empty() {
        errors.push("Vendor name is missing.".to_string());
    } else if vendor_name.chars().count() > 100 {
        errors.push("Vendor name is longer than 100 characters.".to_string());
    }
    if product_name.is_empty() {
        errors.push("Product name is missing.".to_string());
    } else if product_name.chars().count() > 120 {
        errors.push("Product name is longer than 120 characters.".to_string());
    }
    let mrp_paise = parse_rupees(mrp);
    match mrp_paise {
        None if mrp.is_empty() => errors.push("MRP is missing.".to_string()),
        None => errors.push(format!("MRP \"{mrp}\" is not an amount.")),
        Some(p) if !(1..=100_000_000).contains(&p) => errors.push("MRP must be between ₹0.01 and ₹10,00,000.".to_string()),
        Some(_) => {}
    }
    let purchase_price_paise = parse_rupees(bought);
    match purchase_price_paise {
        None if bought.is_empty() => errors.push("Clinic bought price is missing (write 0 if it was free).".to_string()),
        None => errors.push(format!("Clinic bought price \"{bought}\" is not an amount.")),
        Some(p) if p > 100_000_000 => errors.push("Clinic bought price must be at most ₹10,00,000.".to_string()),
        Some(_) => {}
    }
    if let (Some(m), Some(b)) = (mrp_paise, purchase_price_paise) {
        if b > m {
            warnings.push("Bought price is higher than the MRP. Are the two columns swapped?".to_string());
        }
    }
    let expiry_date = match parse_expiry_cell(expiry) {
        Ok(date) => date.map(|d| d.to_string()),
        Err(message) => {
            errors.push(message);
            None
        }
    };
    let qty_value = parse_qty(qty);
    match qty_value {
        None if qty.is_empty() => errors.push("Quantity is missing.".to_string()),
        None => errors.push(format!("Quantity \"{qty}\" must be a whole number.")),
        Some(0) => errors.push("Quantity must be at least 1.".to_string()),
        Some(q) if q > 1_000_000 => errors.push("Quantity must be at most 10,00,000.".to_string()),
        Some(_) => {}
    }
    ImportRow {
        line,
        vendor_name,
        product_name,
        mrp_paise,
        purchase_price_paise,
        expiry_date,
        qty: qty_value,
        new_product: false,
        new_vendor: false,
        errors,
        warnings,
    }
}

/// "₹1,250.50", "Rs 120", "120" -> paise. Up to 2 decimals (more only if they are zeros).
fn parse_rupees(text: &str) -> Option<i64> {
    let lower = text.trim().to_lowercase();
    let unit_free = lower.trim_start_matches('₹').trim_start_matches("rs.").trim_start_matches("rs").trim_start_matches("inr");
    let clean: String = unit_free.chars().filter(|c| *c != ',' && !c.is_whitespace()).collect();
    let (whole, frac) = clean.split_once('.').unwrap_or((clean.as_str(), ""));
    let frac = if frac.len() > 2 && frac.get(2..).is_some_and(|rest| rest.chars().all(|c| c == '0')) { frac.get(..2).unwrap_or(frac) } else { frac };
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if (whole.is_empty() && frac.is_empty()) || !digits(whole) || !digits(frac) || frac.len() > 2 || whole.len() > 9 {
        return None;
    }
    let rupees: i64 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
    let paise: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        _ => frac.parse().ok()?,
    };
    Some(rupees * 100 + paise)
}

/// "12", "1,200", "12.0" -> a whole number.
fn parse_qty(text: &str) -> Option<i64> {
    let clean: String = text.trim().chars().filter(|c| *c != ',').collect();
    let (whole, frac) = clean.split_once('.').unwrap_or((clean.as_str(), ""));
    if whole.is_empty() || whole.len() > 7 || !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c == '0') {
        return None;
    }
    whole.parse().ok()
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

fn month_of(text: &str) -> Option<u32> {
    if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()) {
        return text.parse().ok().filter(|m| (1..=12).contains(m));
    }
    let lower = text.to_ascii_lowercase();
    let index = MONTHS.iter().position(|m| lower.len() >= 3 && lower.starts_with(m))?;
    u32::try_from(index + 1).ok()
}

fn year_of(text: &str) -> Option<i32> {
    if !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    match text.len() {
        4 => text.parse().ok(),
        2 => text.parse::<i32>().ok().map(|y| 2000 + y),
        _ => None,
    }
}

fn day_month_year(day: &str, month: &str, year: &str) -> Option<Date> {
    Date::new(year_of(year)?, month_of(month)?, day.parse().ok()?)
}

/// Medicine packs often print only month and year: the last day of that month.
fn end_of_month(month: &str, year: &str) -> Option<Date> {
    let (year, month) = (year_of(year)?, month_of(month)?);
    (28..=31).rev().find_map(|day| Date::new(year, month, day))
}

/// Day first, as written in India: 31-12-2027, 31/12/27, 31-Dec-2027, 2027-12-31, 12/2027, Dec-2027.
/// An empty cell means no expiry date.
fn parse_expiry_cell(text: &str) -> Result<Option<Date>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let parts: Vec<&str> = text.split(['-', '/', '.', ' ']).filter(|p| !p.is_empty()).collect();
    let date = match parts.as_slice() {
        [year, month, day] if year.len() == 4 => day_month_year(day, month, year),
        [day, month, year] => day_month_year(day, month, year),
        [month, year] => end_of_month(month, year),
        _ => None,
    };
    date.map(Some)
        .ok_or_else(|| format!("Expiry \"{text}\" is not a date. Write it day first, like 31-12-2027, or month and year, like 12-2027."))
}

/// A short fingerprint of the file's content (FNV-1a), to notice the same file imported twice.
fn file_id(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.replace("\r\n", "\n").trim().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_are_read_with_or_without_rupee_signs_and_commas() {
        assert_eq!(parse_rupees("120"), Some(12_000));
        assert_eq!(parse_rupees("₹1,250.50"), Some(125_050));
        assert_eq!(parse_rupees("Rs. 99.5"), Some(9_950));
        assert_eq!(parse_rupees("0"), Some(0));
        assert_eq!(parse_rupees("12.5000"), Some(1_250), "Excel may add zeros");
        assert_eq!(parse_rupees("12.345"), None);
        assert_eq!(parse_rupees("abc"), None);
        assert_eq!(parse_rupees(""), None);
        assert_eq!(parse_qty("1,200"), Some(1_200));
        assert_eq!(parse_qty("12.0"), Some(12));
        assert_eq!(parse_qty("1.5"), None);
        assert_eq!(parse_qty("-3"), None);
    }

    #[test]
    fn expiry_dates_are_read_day_first_or_as_month_and_year() {
        let d = |y, m, day| Ok(Date::new(y, m, day));
        assert_eq!(parse_expiry_cell("31-12-2027"), d(2027, 12, 31));
        assert_eq!(parse_expiry_cell("31/12/27"), d(2027, 12, 31));
        assert_eq!(parse_expiry_cell("05.03.2028"), d(2028, 3, 5));
        assert_eq!(parse_expiry_cell("31-Dec-2027"), d(2027, 12, 31));
        assert_eq!(parse_expiry_cell("2027-12-31"), d(2027, 12, 31));
        assert_eq!(parse_expiry_cell("02/2028"), d(2028, 2, 29), "month and year: the last day, leap year too");
        assert_eq!(parse_expiry_cell("Nov 2027"), d(2027, 11, 30));
        assert_eq!(parse_expiry_cell(""), Ok(None));
        assert!(parse_expiry_cell("12/31/2027").is_err(), "month first is not accepted");
        assert!(parse_expiry_cell("soon").is_err());
    }

    #[test]
    fn csv_quotes_semicolons_and_blank_lines_are_handled() -> Result<(), ServiceError> {
        let text = "Vendor Name;Product Name;MRP\r\n\"Derma; Pharma\";\"Cream \"\"Gold\"\"\";120\r\n\r\n;;\r\nX;Y;1";
        let records = split_records(text, delimiter_of(text))?;
        assert_eq!(records.len(), 3, "the blank lines are skipped");
        assert_eq!(records[1], (2, vec!["Derma; Pharma".to_string(), "Cream \"Gold\"".to_string(), "120".to_string()]));
        assert_eq!(records[2].0, 5, "line numbers count every line in the file");
        assert!(split_records("a,\"b\n", ',').is_err(), "an unclosed quote is reported");
        Ok(())
    }

    #[test]
    fn columns_are_found_by_name_in_any_order() -> Result<(), ServiceError> {
        let header: Vec<String> = ["Qty", "MRP (Rs)", "Product", "Expiry", "Pharma / Vendor Name", "Purchase Price", "Notes"].iter().map(|s| s.to_string()).collect();
        let c = columns_of(&header)?;
        assert_eq!((c.qty, c.mrp, c.product, c.expiry, c.vendor, c.bought), (0, 1, 2, Some(3), 4, 5));
        let missing = columns_of(&["Product".to_string()]);
        assert!(matches!(missing, Err(ServiceError::Validation { message, .. }) if message.contains("Vendor Name, MRP, Clinic Bought Price, Quantity")));
        Ok(())
    }

    #[test]
    fn each_row_lists_what_is_wrong_and_expiry_is_optional() -> Result<(), ServiceError> {
        let text = "Vendor Name,Product Name,MRP,Clinic Bought Price,Expiry Date,Quantity\n\
                    Derma Pharma,Sunscreen SPF 50,650,480,31-12-2027,10\n\
                    Derma Pharma,Soap,120,80,,5\n\
                    ,Cream,abc,,31-31-2027,0\n\
                    Derma Pharma,Gel,100,150,,1\n";
        let (_, rows) = parse_file(text)?;
        assert_eq!(rows.len(), 4);
        assert!(rows[0].errors.is_empty() && rows[0].expiry_date.as_deref() == Some("2027-12-31"));
        assert!(rows[1].errors.is_empty() && rows[1].expiry_date.is_none(), "no expiry date is fine");
        assert_eq!(rows[2].line, 4);
        assert_eq!(rows[2].errors.len(), 5, "vendor, MRP, bought price, expiry, quantity: {:?}", rows[2].errors);
        assert!(rows[3].errors.is_empty() && rows[3].warnings.len() == 1, "bought above MRP is only a warning");
        Ok(())
    }

    #[test]
    fn the_same_content_gives_the_same_file_id() {
        assert_eq!(file_id("a,b\r\n1,2\r\n"), file_id("a,b\n1,2\n"));
        assert_ne!(file_id("a,b\n1,2\n"), file_id("a,b\n1,3\n"));
    }
}
