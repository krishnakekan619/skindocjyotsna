//! Rules for usernames, names, passwords, PINs, lockout and idle lock. Messages are written
//! for clinic staff because they are shown in the app as-is.

/// A rule that input did not meet; `field` names the form field it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyViolation {
    pub field: &'static str,
    pub message: String,
}

fn violation(field: &'static str, message: &str) -> PolicyViolation {
    PolicyViolation { field, message: message.to_string() }
}

pub const PASSWORD_MIN_CHARS: usize = 8;
pub const PASSWORD_MAX_CHARS: usize = 128;
/// Wrong passwords in a row before the account is locked for a while.
pub const LOGIN_MAX_FAILURES: i64 = 5;
pub const LOGIN_LOCKOUT_SECONDS: i64 = 5 * 60;
/// Wrong PINs in a row before PIN unlock is disabled until the full password is used (DEC-023).
pub const PIN_MAX_FAILURES: i64 = 5;
/// Idle lock range and default in minutes (DEC-024).
pub const IDLE_LOCK_MIN_MINUTES: u32 = 5;
pub const IDLE_LOCK_MAX_MINUTES: u32 = 60;
pub const IDLE_LOCK_DEFAULT_MINUTES: u32 = 15;

/// Usernames are compared and stored in lower case, without surrounding spaces.
pub fn normalize_username(username: &str) -> String {
    username.trim().to_lowercase()
}

/// Expects a normalized username: 3-32 characters, starts with a letter, then letters,
/// digits, dot, dash or underscore.
pub fn validate_username(username: &str) -> Result<(), PolicyViolation> {
    let len = username.chars().count();
    if !(3..=32).contains(&len) {
        return Err(violation("username", "Username must be 3 to 32 characters."));
    }
    if !username.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(violation("username", "Username must start with a letter."));
    }
    if !username.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_')) {
        return Err(violation("username", "Username can only use letters, digits, dot, dash and underscore."));
    }
    Ok(())
}

pub fn validate_full_name(name: &str) -> Result<(), PolicyViolation> {
    let len = name.trim().chars().count();
    if len == 0 {
        return Err(violation("fullName", "Please enter the person's name."));
    }
    if len > 80 {
        return Err(violation("fullName", "Name must be at most 80 characters."));
    }
    if name.chars().any(char::is_control) {
        return Err(violation("fullName", "Name contains characters that are not allowed."));
    }
    Ok(())
}

pub fn validate_password(password: &str, username: &str) -> Result<(), PolicyViolation> {
    let len = password.chars().count();
    if len < PASSWORD_MIN_CHARS {
        return Err(violation("password", "Password must be at least 8 characters."));
    }
    if len > PASSWORD_MAX_CHARS {
        return Err(violation("password", "Password must be at most 128 characters."));
    }
    if password.chars().all(|c| c.is_ascii_digit()) {
        return Err(violation("password", "Password cannot be only numbers."));
    }
    if password.trim().is_empty() {
        return Err(violation("password", "Password cannot be only spaces."));
    }
    if password.eq_ignore_ascii_case(username) {
        return Err(violation("password", "Password cannot be the same as the username."));
    }
    Ok(())
}

pub fn validate_pin(pin: &str) -> Result<(), PolicyViolation> {
    let digits: Vec<u32> = pin.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() != pin.chars().count() || !(4..=6).contains(&digits.len()) || !pin.is_ascii() {
        return Err(violation("pin", "PIN must be 4 to 6 digits."));
    }
    let all_same = digits.windows(2).all(|w| w[0] == w[1]);
    let ascending = digits.windows(2).all(|w| w[1] == w[0] + 1);
    let descending = digits.windows(2).all(|w| w[0] == w[1] + 1);
    if all_same || ascending || descending {
        return Err(violation("pin", "PIN is too easy to guess (like 1111 or 1234). Choose another."));
    }
    Ok(())
}

pub fn validate_idle_lock_minutes(minutes: u32) -> Result<(), PolicyViolation> {
    if !(IDLE_LOCK_MIN_MINUTES..=IDLE_LOCK_MAX_MINUTES).contains(&minutes) {
        return Err(violation("idleLockMinutes", "Screen lock time must be between 5 and 60 minutes."));
    }
    Ok(())
}

/// After a failed login bringing the count to `failures`, returns when the lock ends (if any).
pub fn login_lockout(failures: i64, now: i64) -> Option<i64> {
    (failures >= LOGIN_MAX_FAILURES).then_some(now + LOGIN_LOCKOUT_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert_eq!(normalize_username("  Priya.S "), "priya.s");
        for ok in ["priya", "dr.jyotsna", "reception_1", "abc"] {
            assert!(validate_username(ok).is_ok(), "{ok} should be valid");
        }
        for bad in ["ab", "1priya", "priya s", "priya@clinic", "a".repeat(33).as_str(), ""] {
            assert!(validate_username(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn full_names() {
        assert!(validate_full_name("Dr. Jyotsna Kekan").is_ok());
        assert!(validate_full_name("   ").is_err());
        assert!(validate_full_name(&"x".repeat(81)).is_err());
        assert!(validate_full_name("tab\there").is_err());
    }

    #[test]
    fn passwords() {
        assert!(validate_password("clinic@2026", "priya").is_ok());
        assert!(validate_password("short1", "priya").is_err());
        assert!(validate_password("12345678", "priya").is_err());
        assert!(validate_password("Priyapriya", "priyapriya").is_err());
        assert!(validate_password("        ", "priya").is_err());
    }

    #[test]
    fn pins() {
        for ok in ["2580", "90817", "493721"] {
            assert!(validate_pin(ok).is_ok(), "{ok} should be valid");
        }
        for bad in ["123", "1234567", "12a4", "1111", "1234", "98765", "٣٤٥٦", ""] {
            assert!(validate_pin(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn idle_lock_range() {
        assert!(validate_idle_lock_minutes(15).is_ok());
        assert!(validate_idle_lock_minutes(4).is_err());
        assert!(validate_idle_lock_minutes(61).is_err());
    }

    #[test]
    fn lockout_after_five_failures() {
        assert_eq!(login_lockout(4, 1_000), None);
        assert_eq!(login_lockout(5, 1_000), Some(1_000 + LOGIN_LOCKOUT_SECONDS));
    }
}
