//! Clients (patients) and their purchase history (Phase 3). Only what the clinic needs is
//! stored (brief §15, D16): no diagnoses or prescriptions.
//!
//! v0.3: typo-tolerant search, a duplicate check before a new client is created, and merging
//! of duplicate records (administrators; bills move to the surviving record, nothing is deleted).

use clinic_core::auth::Permission;
use clinic_core::time::Date;
use clinic_sqlite::Database;
use clinic_sqlite::repo::billing::{self as bills, BillItemRow, BillQuery, BillRow, ServiceItemRow, next_number};
use clinic_sqlite::repo::clients::{self as repo, ClientFields, ClientRow};
use clinic_sqlite::rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::{self, Actor};
use crate::auth::verify_actor;
use crate::error::invalid;
use crate::settings::clinic_today;
use crate::{ServiceError, Session};

pub const GENDERS: [&str; 4] = ["MALE", "FEMALE", "OTHER", "UNDISCLOSED"];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInput {
    pub id: Option<i64>,
    pub full_name: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    pub date_of_birth: Option<String>,
    #[serde(default = "undisclosed")]
    pub gender: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub emergency_contact: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "yes")]
    pub is_active: bool,
    /// The user saw the possible duplicates and chose to create a new client anyway.
    #[serde(default)]
    pub allow_duplicate: bool,
}

fn undisclosed() -> String {
    "UNDISCLOSED".to_string()
}

fn yes() -> bool {
    true
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

fn validate(input: &ClientInput, today: Date) -> Result<Option<String>, ServiceError> {
    let name = chars(input.full_name.trim());
    if name == 0 || name > 100 {
        return Err(invalid("fullName", "Client name is required (at most 100 characters)."));
    }
    let phone = input.phone.trim();
    if chars(phone) > 20 || !phone.chars().all(|c| c.is_ascii_digit() || matches!(c, ' ' | '+' | '-' | '(' | ')')) {
        return Err(invalid("phone", "Phone can contain only digits, spaces and + - ( ) (at most 20)."));
    }
    let email = input.email.trim();
    if !(email.is_empty() || (chars(email) <= 100 && email.contains('@') && !email.contains(char::is_whitespace))) {
        return Err(invalid("email", "Please enter a valid email address, or leave it empty."));
    }
    if !GENDERS.contains(&input.gender.as_str()) {
        return Err(invalid("gender", "Please choose an option."));
    }
    if chars(input.address.trim()) > 200 || chars(input.emergency_contact.trim()) > 100 {
        return Err(invalid("address", "Address must be at most 200 characters and emergency contact at most 100."));
    }
    if chars(input.notes.trim()) > 500 {
        return Err(invalid("notes", "Notes must be at most 500 characters (do not record diagnoses here)."));
    }
    match input.date_of_birth.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        None => Ok(None),
        Some(text) => match Date::parse(text) {
            Some(dob) if dob <= today => Ok(Some(dob.to_string())),
            _ => Err(invalid("dateOfBirth", "Date of birth must be a valid date, not in the future.")),
        },
    }
}

pub fn save(db: &mut Database, actor: &Session, input: ClientInput, now: i64) -> Result<ClientRow, ServiceError> {
    actor.require(Permission::ManageClients)?;
    db.write(|c| {
        verify_actor(c, actor)?;
        save_in_tx(c, actor, &input, now)
    })
}

/// A client typed on the New Bill screen (name and phone only), created in the bill's own
/// transaction at Finalize, so an abandoned bill never leaves a half-made client (DEC-034).
pub(crate) fn create_for_bill(c: &Connection, actor: &Session, full_name: &str, phone: &str, allow_duplicate: bool, now: i64) -> Result<i64, ServiceError> {
    actor.require(Permission::ManageClients)?;
    let input = ClientInput {
        id: None,
        full_name: full_name.to_string(),
        phone: phone.to_string(),
        email: String::new(),
        date_of_birth: None,
        gender: undisclosed(),
        address: String::new(),
        emergency_contact: String::new(),
        notes: String::new(),
        is_active: true,
        allow_duplicate,
    };
    Ok(save_in_tx(c, actor, &input, now)?.id)
}

fn save_in_tx(c: &Connection, actor: &Session, input: &ClientInput, now: i64) -> Result<ClientRow, ServiceError> {
    let (_, today) = clinic_today(c, now)?;
    let dob = validate(input, today)?;
    if input.id.is_none() && !input.allow_duplicate {
        // Checked here as well as in the screen, so no path creates an unconfirmed duplicate.
        if let Some(existing) = find_duplicates(c, &input.full_name, &input.phone, dob.as_deref(), None)?.into_iter().find(|m| m.strong) {
            return Err(ServiceError::PossibleDuplicate(format!(
                "{} ({}) has the same {}. Use the existing client, or confirm that this is a different person.",
                existing.client.full_name,
                existing.client.client_code,
                if existing.reason == "PHONE" { "phone number" } else { "name" }
            )));
        }
    }
    let fields = ClientFields {
        full_name: input.full_name.trim(),
        phone: input.phone.trim(),
        email: input.email.trim(),
        date_of_birth: dob.as_deref(),
        gender: &input.gender,
        address: input.address.trim(),
        emergency_contact: input.emergency_contact.trim(),
        notes: input.notes.trim(),
        is_active: input.is_active,
    };
    let (id, action) = match input.id {
        Some(id) if repo::update(c, id, &fields, now)? == 1 => (id, "CLIENT_UPDATE"),
        Some(_) => return Err(ServiceError::NotFound("client")),
        None => {
            let code = format!("CL-{:06}", next_number(c, "CLIENT", "ALL")?);
            (repo::insert(c, &code, &fields, now)?, "CLIENT_CREATE")
        }
    };
    let client = repo::find(c, id)?.ok_or(ServiceError::NotFound("client"))?;
    // Audit by client code only: no names or phone numbers in the audit log.
    audit::record(c, now, Actor::from(actor), action, Some(("client", id.to_string())), Some(json!({ "code": client.client_code })))?;
    Ok(client)
}

// ---- Search (typo tolerant) --------------------------------------------------------------------

/// Levenshtein distance between two short strings.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j] + cost).min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[b.len()]
}

/// Typing mistakes forgiven for a word of this length.
fn allowed_typos(len: usize) -> usize {
    match len {
        0..=3 => 0,
        4..=6 => 1,
        _ => 2,
    }
}

/// How far query word `q` is from name word `w`: 0 when `w` starts with `q` (typing is often
/// unfinished), otherwise the edit distance to `w` or to its start, if within the typo allowance.
fn word_distance(q: &str, w: &str) -> Option<usize> {
    if w.starts_with(q) {
        return Some(0);
    }
    let typos = allowed_typos(q.chars().count());
    if typos == 0 {
        return None;
    }
    let start: String = w.chars().take(q.chars().count()).collect();
    let distance = edit_distance(q, w).min(edit_distance(q, &start));
    (distance <= typos).then_some(distance)
}

/// Lower is closer; `None` if some query word matches no word of the name (in any order).
pub(crate) fn fuzzy_score(query_words: &[&str], name_key: &str) -> Option<usize> {
    let words: Vec<&str> = name_key.split(' ').collect();
    query_words.iter().try_fold(0, |score, q| words.iter().filter_map(|w| word_distance(q, w)).min().map(|d| score + d))
}

/// Active clients whose name is close to `key` (not the same), closest first.
fn similar_names(c: &Connection, key: &str, skip: &[i64], limit: usize) -> Result<Vec<ClientRow>, ServiceError> {
    let words: Vec<&str> = key.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let mut scored: Vec<(usize, i64)> = repo::active_name_keys(c)?
        .into_iter()
        .filter(|(id, _)| !skip.contains(id))
        .filter_map(|(id, candidate)| fuzzy_score(&words, &candidate).map(|score| (score, id)))
        .collect();
    scored.sort_unstable();
    let mut rows = Vec::new();
    for (_, id) in scored.into_iter().take(limit) {
        if let Some(row) = repo::find(c, id)? {
            rows.push(row);
        }
    }
    Ok(rows)
}

/// By name, phone or client code; minor spelling mistakes are forgiven. Empty text: the most
/// recent visitors (the "Recent clients" list).
pub fn search(db: &Database, actor: &Session, text: &str, include_inactive: bool) -> Result<Vec<ClientRow>, ServiceError> {
    actor.require(Permission::ManageClients)?;
    db.read(|c| {
        let mut found = repo::search(c, text, include_inactive, 50)?;
        let key = repo::name_key(text);
        if found.len() < 8 && chars(&key) >= 3 {
            let skip: Vec<i64> = found.iter().map(|client| client.id).collect();
            let more = similar_names(c, &key, &skip, 8 - found.len())?;
            found.extend(more);
        }
        Ok(found)
    })
}

// ---- Duplicates ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateMatch {
    pub client: ClientRow,
    /// `PHONE` (same number), `NAME` (same name) or `SIMILAR_NAME`.
    pub reason: String,
    /// A likely duplicate: creating a new client needs an explicit confirmation.
    pub strong: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateQuery {
    pub full_name: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub date_of_birth: Option<String>,
    /// The client being edited (never reported as its own duplicate).
    #[serde(default)]
    pub exclude_id: Option<i64>,
}

fn push_unique(matches: &mut Vec<DuplicateMatch>, client: ClientRow, reason: &str, strong: bool) {
    if !matches.iter().any(|m| m.client.id == client.id) {
        matches.push(DuplicateMatch { client, reason: reason.to_string(), strong });
    }
}

/// Primary matches: the same phone number, or the same name (unless both dates of birth are
/// known and differ). Secondary: similar names. Never merges anything by itself.
fn find_duplicates(c: &Connection, full_name: &str, phone: &str, dob: Option<&str>, exclude: Option<i64>) -> Result<Vec<DuplicateMatch>, ServiceError> {
    let mut matches = Vec::new();
    let digits = repo::phone_digits(phone);
    if digits.len() >= 6 {
        for client in repo::with_phone_digits(c, &digits, exclude)? {
            push_unique(&mut matches, client, "PHONE", true);
        }
    }
    let key = repo::name_key(full_name);
    if !key.is_empty() {
        for client in repo::with_name_key(c, &key, exclude)? {
            let different_person = matches!((dob, client.date_of_birth.as_deref()), (Some(a), Some(b)) if a != b);
            push_unique(&mut matches, client, "NAME", !different_person);
        }
        let mut skip: Vec<i64> = matches.iter().map(|m| m.client.id).collect();
        skip.extend(exclude);
        for client in similar_names(c, &key, &skip, 5)? {
            push_unique(&mut matches, client, "SIMILAR_NAME", false);
        }
    }
    matches.truncate(10);
    Ok(matches)
}

/// Possible existing clients for the details being typed (shown before a new client is saved).
pub fn possible_duplicates(db: &Database, actor: &Session, query: &DuplicateQuery) -> Result<Vec<DuplicateMatch>, ServiceError> {
    actor.require(Permission::ManageClients)?;
    let dob = query.date_of_birth.as_deref().map(str::trim).filter(|d| !d.is_empty());
    db.read(|c| find_duplicates(c, &query.full_name, &query.phone, dob, query.exclude_id))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupedClient {
    #[serde(flatten)]
    pub client: ClientRow,
    pub bill_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    /// `PHONE` or `NAME`.
    pub reason: String,
    pub clients: Vec<GroupedClient>,
}

/// Groups of active clients sharing a phone number or a name ("Find duplicates").
pub fn duplicate_groups(db: &Database, actor: &Session) -> Result<Vec<DuplicateGroup>, ServiceError> {
    actor.require(Permission::ManageClients)?;
    db.read(|c| {
        let with_counts = |rows: Vec<ClientRow>| -> Result<Vec<GroupedClient>, ServiceError> {
            rows.into_iter()
                .map(|client| -> Result<GroupedClient, ServiceError> { Ok(GroupedClient { bill_count: repo::bill_count(c, client.id)?, client }) })
                .collect()
        };
        let mut groups: Vec<DuplicateGroup> = Vec::new();
        for digits in repo::shared_phone_digits(c, 50)? {
            groups.push(DuplicateGroup { reason: "PHONE".into(), clients: with_counts(repo::with_phone_digits(c, &digits, None)?)? });
        }
        for key in repo::shared_name_keys(c, 50)? {
            let clients = with_counts(repo::with_name_key(c, &key, None)?)?;
            let mut ids: Vec<i64> = clients.iter().map(|g| g.client.id).collect();
            ids.sort_unstable();
            // Skip a name group that is exactly a phone group already listed.
            let listed = groups.iter().any(|g| {
                let mut other: Vec<i64> = g.clients.iter().map(|x| x.client.id).collect();
                other.sort_unstable();
                other == ids
            });
            if !listed {
                groups.push(DuplicateGroup { reason: "NAME".into(), clients });
            }
        }
        Ok(groups.into_iter().filter(|g| g.clients.len() > 1).collect())
    })
}

// ---- Merge ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub client: ClientRow,
    pub moved_bills: usize,
}

/// Keeps the first non-empty value.
fn first_filled(primary: &str, secondary: &str) -> String {
    if primary.trim().is_empty() { secondary.trim().to_string() } else { primary.trim().to_string() }
}

/// Administrators: merges `secondary` into `primary` in ONE transaction. The primary keeps its
/// details (empty ones are filled from the secondary) and receives all the secondary's bills,
/// with their items and payments unchanged; the secondary is kept, marked merged and inactive.
pub fn merge(db: &mut Database, actor: &Session, primary_id: i64, secondary_id: i64, now: i64) -> Result<MergeResult, ServiceError> {
    actor.require(Permission::MergeClients)?;
    if primary_id == secondary_id {
        return Err(invalid("secondaryId", "Choose two different clients."));
    }
    db.write(|c| {
        verify_actor(c, actor)?;
        let load = |id: i64| -> Result<ClientRow, ServiceError> {
            repo::find(c, id)?.filter(|client| client.merged_into_client_id.is_none()).ok_or(ServiceError::NotFound("client"))
        };
        let primary = load(primary_id)?;
        let secondary = load(secondary_id)?;

        let mut notes = primary.notes.trim().to_string();
        let merged_line = format!("Merged from {} ({}).", secondary.client_code, secondary.full_name);
        for part in [merged_line.as_str(), secondary.notes.trim()] {
            if !part.is_empty() {
                if !notes.is_empty() {
                    notes.push('\n');
                }
                notes.push_str(part);
            }
        }
        let notes: String = notes.chars().take(500).collect();
        let phone = first_filled(&primary.phone, &secondary.phone);
        let email = first_filled(&primary.email, &secondary.email);
        let address = first_filled(&primary.address, &secondary.address);
        let emergency = first_filled(&primary.emergency_contact, &secondary.emergency_contact);
        let dob = primary.date_of_birth.clone().or_else(|| secondary.date_of_birth.clone());
        let gender = if primary.gender == "UNDISCLOSED" { secondary.gender.clone() } else { primary.gender.clone() };
        repo::update(
            c,
            primary.id,
            &ClientFields {
                full_name: &primary.full_name,
                phone: &phone,
                email: &email,
                date_of_birth: dob.as_deref(),
                gender: &gender,
                address: &address,
                emergency_contact: &emergency,
                notes: &notes,
                is_active: true,
            },
            now,
        )?;
        // Order matters: the database allows moving bills only from a merged client.
        if repo::mark_merged(c, secondary.id, primary.id, now)? != 1 {
            return Err(ServiceError::Conflict("This client was just merged by someone else. Refresh and try again.".into()));
        }
        let moved = repo::move_bills(c, secondary.id, primary.id)?;
        repo::keep_latest_visit(c, primary.id, secondary.id)?;
        audit::record(
            c,
            now,
            Actor::from(actor),
            "CLIENT_MERGE",
            Some(("client", primary.id.to_string())),
            Some(json!({ "primary": primary.client_code, "secondary": secondary.client_code, "movedBillIds": moved })),
        )?;
        Ok(MergeResult { client: load(primary.id)?, moved_bills: moved.len() })
    })
}

// ---- Profile -------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Visit {
    pub bill: BillRow,
    pub items: Vec<BillItemRow>,
    pub services: Vec<ServiceItemRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientProfile {
    pub client: ClientRow,
    /// Newest first (at most 200); cancelled and corrected bills are included for the record.
    pub visits: Vec<Visit>,
    /// Finalized bills in the period (all of them, not only the ones listed).
    pub visit_count: i64,
    /// All bills in the period, including cancelled and corrected ones.
    pub bill_count: i64,
    /// Kept by the client: finalized bills minus refunds.
    pub total_spent_paise: i64,
}

/// Profile and history, optionally limited to local dates `from`..=`to` (YYYY-MM-DD).
pub fn profile(db: &Database, actor: &Session, client_id: i64, from: Option<&str>, to: Option<&str>) -> Result<ClientProfile, ServiceError> {
    actor.require(Permission::ManageClients)?;
    db.read(|c| {
        let client = repo::find(c, client_id)?.ok_or(ServiceError::NotFound("client"))?;
        let (clinic, _) = clinic_today(c, 0)?;
        let bound = |text: Option<&str>, field: &'static str, extra_days: i64| -> Result<Option<i64>, ServiceError> {
            match text.map(str::trim).filter(|t| !t.is_empty()) {
                None => Ok(None),
                Some(t) => Date::parse(t)
                    .map(|d| Some(clinic_core::time::local_day_start_utc(d.add_days(extra_days), clinic.utc_offset_minutes)))
                    .ok_or_else(|| invalid(field, "Please enter a valid date.")),
            }
        };
        let (from_ts, to_ts) = (bound(from, "from", 0)?, bound(to, "to", 1)?);
        let rows = bills::list_bills(c, &BillQuery { from: from_ts, to: to_ts, status: None, client_id: Some(client_id), text: "", limit: 200 })?;
        let bill_count = if from_ts.is_none() && to_ts.is_none() { repo::bill_count(c, client_id)? } else { rows.len() as i64 };
        let mut visits = Vec::with_capacity(rows.len());
        for bill in rows {
            visits.push(Visit { items: bills::items(c, bill.id)?, services: bills::service_items(c, bill.id)?, bill });
        }
        let (visit_count, total_spent_paise) = repo::finalized_totals(c, client_id, from_ts, to_ts)?;
        Ok(ClientProfile { client, visits, visit_count, bill_count, total_spent_paise })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matching_forgives_small_mistakes_in_any_order() {
        assert_eq!(fuzzy_score(&["sudrshan"], "sudarshan kulkarni"), Some(1));
        assert_eq!(fuzzy_score(&["rahul", "sarma"], "rahul sharma"), Some(1));
        assert_eq!(fuzzy_score(&["sharma", "rahul"], "rahul sharma"), Some(0));
        assert_eq!(fuzzy_score(&["joh"], "john doe"), Some(0));
        assert_eq!(fuzzy_score(&["sudr"], "sudarshan"), Some(1), "unfinished word with a typo");
        assert_eq!(fuzzy_score(&["xyz"], "rahul sharma"), None);
        assert_eq!(fuzzy_score(&["priya", "shah"], "priya patel"), None);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }
}
