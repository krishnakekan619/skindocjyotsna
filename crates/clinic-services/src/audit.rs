use clinic_core::auth::Permission;
use clinic_sqlite::Database;
use clinic_sqlite::repo::audit::{self, NewAuditEntry};
use clinic_sqlite::rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::{ServiceError, Session};

/// Who performed an audited action: a signed-in user, or (for failed logins) just the name tried.
pub(crate) enum Actor<'a> {
    User { id: i64, username: &'a str },
    Anonymous { username_tried: &'a str },
    /// The app itself (e.g. automatic backups).
    System,
}

impl<'a> From<&'a Session> for Actor<'a> {
    fn from(session: &'a Session) -> Self {
        Actor::User { id: session.user_id, username: &session.username }
    }
}

/// Appends an audit entry inside the caller's transaction. `details` must never contain
/// passwords, PINs or patient data.
pub(crate) fn record(
    conn: &Connection,
    now: i64,
    actor: Actor<'_>,
    action: &str,
    entity: Option<(&str, String)>,
    details: Option<Value>,
) -> Result<(), ServiceError> {
    let (user_id, username) = match actor {
        Actor::User { id, username } => (Some(id), username),
        // Keep failed-login names short: they are typed by anyone at the login screen.
        Actor::Anonymous { username_tried } => (None, truncate(username_tried, 32)),
        Actor::System => (None, "system"),
    };
    let details_json = details.map(|d| d.to_string());
    audit::append(
        conn,
        &NewAuditEntry {
            occurred_at: now,
            user_id,
            username: Some(username),
            action,
            entity_type: entity.as_ref().map(|(kind, _)| *kind),
            entity_id: entity.as_ref().map(|(_, id)| id.as_str()),
            details_json: details_json.as_deref(),
        },
    )?;
    Ok(())
}

fn truncate(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((index, _)) => &text[..index],
        None => text,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: i64,
    pub occurred_at: i64,
    pub username: Option<String>,
    pub action: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub details: Option<Value>,
}

/// Admins only. Newest first; `before_id` pages further back.
pub fn list(db: &Database, actor: &Session, limit: u32, before_id: Option<i64>) -> Result<Vec<AuditEntry>, ServiceError> {
    actor.require(Permission::ViewAuditLog)?;
    let records = db.read(|c| audit::list(c, limit.clamp(1, 200), before_id))?;
    Ok(records
        .into_iter()
        .map(|r| AuditEntry {
            id: r.id,
            occurred_at: r.occurred_at,
            username: r.username,
            action: r.action,
            entity_type: r.entity_type,
            entity_id: r.entity_id,
            details: r.details_json.and_then(|json| serde_json::from_str(&json).ok()),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::truncate;

    #[test]
    fn truncates_on_character_boundaries() {
        assert_eq!(truncate("priya", 32), "priya");
        assert_eq!(truncate("ऋषिकेश", 3), "ऋषि");
    }
}
