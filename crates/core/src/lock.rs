//! Friend-password hashing. Only the argon2 hash is ever stored.

use anyhow::{anyhow, Result};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

pub const MIN_PASSWORD_LEN: usize = 8;

pub fn hash_password(password: &str) -> Result<String> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(anyhow!("the friend password must be at least {MIN_PASSWORD_LEN} characters"));
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow!("hashing failed: {e}"))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_roundtrip() {
        let hash = hash_password("brother-ahmad-holds-this").unwrap();
        assert!(verify_password("brother-ahmad-holds-this", &hash));
        assert!(!verify_password("guess", &hash));
        assert!(!verify_password("x", "not a hash"));
        assert!(hash_password("short").is_err());
    }
}
