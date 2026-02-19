use bcrypt::{DEFAULT_COST, hash, verify};

/// Hash a password using bcrypt.
pub fn hash_password(password: &str) -> Result<String, bcrypt::BcryptError> {
    hash(password, DEFAULT_COST)
}

/// Verify a password against a bcrypt hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, bcrypt::BcryptError> {
    verify(password, hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify() {
        let hash = hash_password("my-secret-password").unwrap();
        assert!(verify_password("my-secret-password", &hash).unwrap());
        assert!(!verify_password("wrong-password", &hash).unwrap());
    }
}
