use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::KdfError;

/// A 256-bit master key (KEK) derived from a passphrase via Argon2id.
///
/// Zeroized on drop. Cannot be cloned.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey {
    bytes: [u8; 32],
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(***)")
    }
}

impl MasterKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

/// Derive a master key from a passphrase and salt using Argon2id.
///
/// Parameters: m=64 MiB, t=3 iterations, p=4 lanes.
pub fn derive_master_key(passphrase: &[u8], salt: &[u8; 16]) -> Result<MasterKey, KdfError> {
    let params = Params::new(64 * 1024, 3, 4, Some(32)).map_err(KdfError::InvalidParams)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key_bytes = [0u8; 32];
    argon2
        .hash_password_into(passphrase, salt, &mut key_bytes)
        .map_err(KdfError::DerivationFailed)?;

    Ok(MasterKey { bytes: key_bytes })
}

/// Generate a random 16-byte salt for KDF.
pub fn generate_kek_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Derive a deterministic 16-byte salt for entry-level encryption.
///
/// Every device with the same passphrase produces the same salt, so they
/// derive the same KEK without out-of-band salt exchange.
///
/// `SHA-256(b"saku-sync-kek-salt-v1:" || passphrase)[0..16]`
pub fn derive_deterministic_salt(passphrase: &[u8]) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(b"saku-sync-kek-salt-v1:");
    hasher.update(passphrase);
    let hash = hasher.finalize();
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&hash[..16]);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_derivation() {
        let passphrase = b"test passphrase";
        let salt = [1u8; 16];

        let k1 = derive_master_key(passphrase, &salt).unwrap();
        let k2 = derive_master_key(passphrase, &salt).unwrap();

        assert_eq!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn different_salt_produces_different_key() {
        let passphrase = b"test passphrase";
        let salt1 = [1u8; 16];
        let salt2 = [2u8; 16];

        let k1 = derive_master_key(passphrase, &salt1).unwrap();
        let k2 = derive_master_key(passphrase, &salt2).unwrap();

        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn different_passphrase_produces_different_key() {
        let salt = [1u8; 16];

        let k1 = derive_master_key(b"passphrase one", &salt).unwrap();
        let k2 = derive_master_key(b"passphrase two", &salt).unwrap();

        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn generate_salt_is_random() {
        let s1 = generate_kek_salt();
        let s2 = generate_kek_salt();
        assert_ne!(s1, s2);
    }

    #[test]
    fn deterministic_salt_is_deterministic() {
        let s1 = derive_deterministic_salt(b"my passphrase");
        let s2 = derive_deterministic_salt(b"my passphrase");
        assert_eq!(s1, s2);
    }

    #[test]
    fn different_passphrase_different_salt() {
        let s1 = derive_deterministic_salt(b"passphrase one");
        let s2 = derive_deterministic_salt(b"passphrase two");
        assert_ne!(s1, s2);
    }
}
