pub mod decrypt;
pub mod encrypt;
pub mod entry;
pub mod error;
pub mod kdf;

mod cipher;
mod format;

#[cfg(feature = "keychain")]
pub mod keychain;

// Re-exports for convenience
pub use decrypt::decrypt;
pub use encrypt::encrypt;
pub use entry::{decrypt_entry, encrypt_entry};
pub use error::{CryptoError, KdfError};
pub use kdf::{MasterKey, derive_deterministic_salt};

#[cfg(test)]
mod tests {
    use super::*;

    fn test_master_key() -> (MasterKey, [u8; 16]) {
        let salt = kdf::generate_kek_salt();
        let mk = kdf::derive_master_key(b"test passphrase", &salt).unwrap();
        (mk, salt)
    }

    #[test]
    fn encrypt_decrypt_empty() {
        let (mk, salt) = test_master_key();
        let plaintext = b"";
        let encrypted = encrypt(plaintext, &mk, &salt).unwrap();
        let decrypted = decrypt(&encrypted, &mk).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_decrypt_small() {
        let (mk, salt) = test_master_key();
        let plaintext = b"hello world";
        let encrypted = encrypt(plaintext, &mk, &salt).unwrap();
        let decrypted = decrypt(&encrypted, &mk).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_decrypt_json() {
        let (mk, salt) = test_master_key();
        let plaintext = br#"{"tasks":[{"id":"abc","title":"test"}]}"#;
        let encrypted = encrypt(plaintext, &mk, &salt).unwrap();
        let decrypted = decrypt(&encrypted, &mk).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn two_encryptions_differ() {
        let (mk, salt) = test_master_key();
        let plaintext = b"same data";
        let enc1 = encrypt(plaintext, &mk, &salt).unwrap();
        let enc2 = encrypt(plaintext, &mk, &salt).unwrap();
        // Different random DEKs and nonces each time
        assert_ne!(enc1, enc2);
    }

    #[test]
    fn wrong_key_gives_dek_decryption_failed() {
        let (mk, salt) = test_master_key();
        let encrypted = encrypt(b"secret", &mk, &salt).unwrap();

        let wrong_salt = kdf::generate_kek_salt();
        let wrong_mk = kdf::derive_master_key(b"wrong passphrase", &wrong_salt).unwrap();

        let err = decrypt(&encrypted, &wrong_mk).unwrap_err();
        assert!(matches!(err, CryptoError::DekDecryptionFailed));
    }

    #[test]
    fn tampered_ciphertext_gives_content_decryption_failed() {
        let (mk, salt) = test_master_key();
        let mut encrypted = encrypt(b"secret data", &mk, &salt).unwrap();

        // Tamper with the ciphertext (after the 117-byte header)
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;

        let err = decrypt(&encrypted, &mk).unwrap_err();
        assert!(matches!(err, CryptoError::ContentDecryptionFailed));
    }

    #[test]
    fn output_length_is_correct() {
        let (mk, salt) = test_master_key();
        let plaintext = b"twelve chars";
        let encrypted = encrypt(plaintext, &mk, &salt).unwrap();
        // header (117) + plaintext (12) + AEAD tag (16)
        assert_eq!(encrypted.len(), 117 + plaintext.len() + 16);
    }
}
