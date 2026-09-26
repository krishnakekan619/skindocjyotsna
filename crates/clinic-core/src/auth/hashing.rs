//! Argon2id hashing for passwords and PINs (OWASP-recommended defaults: 19 MiB, 2 passes).
//! Only the resulting PHC string (`$argon2id$v=19$...`, includes a random salt) is stored.

use argon2::Argon2;
use argon2::password_hash::{Error as HashLibError, PasswordHasher, PasswordVerifier};

#[derive(Debug, thiserror::Error)]
#[error("password hashing failed: {0}")]
pub struct HashError(String);

pub fn hash_secret(secret: &str) -> Result<String, HashError> {
    Argon2::default()
        .hash_password(secret.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|error| HashError(error.to_string()))
}

/// `Ok(false)` for a wrong secret; `Err` only if the stored hash itself is unusable.
pub fn verify_secret(secret: &str, stored_hash: &str) -> Result<bool, HashError> {
    match Argon2::default().verify_password(secret.as_bytes(), stored_hash) {
        Ok(()) => Ok(true),
        Err(HashLibError::PasswordInvalid) => Ok(false),
        Err(error) => Err(HashError(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_salted_argon2id_and_verify() -> Result<(), HashError> {
        let first = hash_secret("clinic@2026")?;
        let second = hash_secret("clinic@2026")?;
        assert!(first.starts_with("$argon2id$"), "unexpected format: {first}");
        assert_ne!(first, second, "each hash must use its own random salt");
        assert!(verify_secret("clinic@2026", &first)?);
        assert!(!verify_secret("clinic@2025", &first)?);
        Ok(())
    }

    #[test]
    fn a_damaged_hash_is_an_error_not_a_match() {
        assert!(verify_secret("x", "not-a-hash").is_err());
    }
}
