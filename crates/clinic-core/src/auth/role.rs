use serde::{Deserialize, Serialize};

/// The two kinds of staff account (DEC-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    /// Clinic owner / doctor: everything, including users, settings, backups and the audit log.
    Admin,
    /// Front desk: day-to-day work only.
    Receptionist,
}

impl Role {
    /// The value stored in the database.
    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "ADMIN",
            Role::Receptionist => "RECEPTIONIST",
        }
    }

    pub fn parse(value: &str) -> Option<Role> {
        match value {
            "ADMIN" => Some(Role::Admin),
            "RECEPTIONIST" => Some(Role::Receptionist),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_values_round_trip() {
        for role in [Role::Admin, Role::Receptionist] {
            assert_eq!(Role::parse(role.as_str()), Some(role));
        }
        assert_eq!(Role::parse("admin"), None, "stored values are upper case");
    }
}
