//! Argon2id password hashing.
//!
//! Uses the `argon2` crate's default parameters (m=19456 KiB, t=2, p=1)
//! which match the 2023 OWASP guidance. Tests override via
//! `Hasher::fast_for_tests` to keep CI fast.

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("hash: {0}")]
    Hash(String),
    #[error("invalid password format")]
    Format,
}

#[derive(Clone)]
pub struct Hasher {
    argon: Argon2<'static>,
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            argon: Argon2::default(),
        }
    }

    /// Minimum legal Argon2id params for fast tests. ~5 ms on CI.
    /// Not suitable for production.
    pub fn fast_for_tests() -> Self {
        use argon2::{Algorithm, Params, Version};
        let params = Params::new(8, 1, 1, None).expect("params");
        Self {
            argon: Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
        }
    }

    pub fn hash(&self, plain: &str) -> Result<String, PasswordError> {
        let salt = SaltString::generate(&mut OsRng);
        self.argon
            .hash_password(plain.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| PasswordError::Hash(e.to_string()))
    }

    pub fn verify(&self, hashed: &str, plain: &str) -> Result<bool, PasswordError> {
        let parsed = PasswordHash::new(hashed).map_err(|_| PasswordError::Format)?;
        Ok(self
            .argon
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok())
    }
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_roundtrip() {
        let h = Hasher::fast_for_tests();
        let hashed = h.hash("hunter2").expect("hash");
        assert!(h.verify(&hashed, "hunter2").expect("verify"));
    }

    #[test]
    fn verify_rejects_wrong_password() {
        let h = Hasher::fast_for_tests();
        let hashed = h.hash("hunter2").expect("hash");
        assert!(!h.verify(&hashed, "hunter3").expect("verify"));
    }

    #[test]
    fn verify_rejects_corrupt_hash() {
        let h = Hasher::fast_for_tests();
        let err = h.verify("not-a-real-hash", "x").unwrap_err();
        assert!(matches!(err, PasswordError::Format));
    }

    #[test]
    fn hashes_are_unique_per_call() {
        let h = Hasher::fast_for_tests();
        let a = h.hash("same").unwrap();
        let b = h.hash("same").unwrap();
        assert_ne!(a, b, "salt should make every hash unique");
    }
}
