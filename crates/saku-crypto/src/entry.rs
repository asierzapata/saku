use crate::cipher::{generate_nonce, xchacha20_decrypt, xchacha20_encrypt};
use crate::error::CryptoError;
use crate::kdf::MasterKey;

const NONCE_LEN: usize = 24;

/// Encrypt a single KV entry value directly with the master key.
///
/// Returns: `[nonce: 24B][ciphertext + AEAD tag: len(plaintext) + 16B]`
///
/// Total overhead: 40 bytes (24 nonce + 16 AEAD tag).
pub fn encrypt_entry(plaintext: &[u8], master_key: &MasterKey) -> Vec<u8> {
    let nonce = generate_nonce();
    let ciphertext = xchacha20_encrypt(master_key.as_bytes(), &nonce, plaintext);

    let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    blob
}

/// Decrypt a single KV entry value.
///
/// Expects the format produced by [`encrypt_entry`]:
/// `[nonce: 24B][ciphertext + AEAD tag]`
pub fn decrypt_entry(blob: &[u8], master_key: &MasterKey) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < NONCE_LEN {
        return Err(CryptoError::EntryBlobTooShort);
    }

    let nonce: [u8; NONCE_LEN] = blob[..NONCE_LEN]
        .try_into()
        .expect("slice is exactly NONCE_LEN bytes");
    let ciphertext = &blob[NONCE_LEN..];

    xchacha20_decrypt(master_key.as_bytes(), &nonce, ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kdf::{derive_deterministic_salt, derive_master_key, generate_kek_salt};

    fn test_master_key() -> MasterKey {
        let salt = generate_kek_salt();
        derive_master_key(b"test passphrase", &salt).unwrap()
    }

    #[test]
    fn round_trip_empty() {
        let mk = test_master_key();
        let plaintext = b"";
        let blob = encrypt_entry(plaintext, &mk);
        let decrypted = decrypt_entry(&blob, &mk).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn round_trip_small() {
        let mk = test_master_key();
        let plaintext = b"hello world";
        let blob = encrypt_entry(plaintext, &mk);
        let decrypted = decrypt_entry(&blob, &mk).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn round_trip_json() {
        let mk = test_master_key();
        let plaintext = br#"{"key":"task/abc","value":{"title":"test"}}"#;
        let blob = encrypt_entry(plaintext, &mk);
        let decrypted = decrypt_entry(&blob, &mk).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn two_encryptions_differ() {
        let mk = test_master_key();
        let plaintext = b"same data";
        let enc1 = encrypt_entry(plaintext, &mk);
        let enc2 = encrypt_entry(plaintext, &mk);
        assert_ne!(enc1, enc2);
    }

    #[test]
    fn output_length() {
        let mk = test_master_key();
        let plaintext = b"twelve chars";
        let blob = encrypt_entry(plaintext, &mk);
        // nonce (24) + plaintext (12) + AEAD tag (16) = 52
        assert_eq!(blob.len(), 24 + plaintext.len() + 16);
    }

    #[test]
    fn wrong_key_fails() {
        let mk = test_master_key();
        let blob = encrypt_entry(b"secret", &mk);

        let wrong_mk = test_master_key(); // different random salt → different key
        let err = decrypt_entry(&blob, &wrong_mk).unwrap_err();
        assert!(matches!(err, CryptoError::ContentDecryptionFailed));
    }

    #[test]
    fn tampered_blob_fails() {
        let mk = test_master_key();
        let mut blob = encrypt_entry(b"secret data", &mk);
        let last = blob.len() - 1;
        blob[last] ^= 0xFF;

        let err = decrypt_entry(&blob, &mk).unwrap_err();
        assert!(matches!(err, CryptoError::ContentDecryptionFailed));
    }

    #[test]
    fn too_short_blob_fails() {
        let mk = test_master_key();
        let err = decrypt_entry(&[0u8; 23], &mk).unwrap_err();
        assert!(matches!(err, CryptoError::EntryBlobTooShort));

        let err = decrypt_entry(&[], &mk).unwrap_err();
        assert!(matches!(err, CryptoError::EntryBlobTooShort));
    }

    #[test]
    fn integration_deterministic_salt_full_chain() {
        let passphrase = b"my sync passphrase";
        let salt = derive_deterministic_salt(passphrase);
        let mk = derive_master_key(passphrase, &salt).unwrap();

        let plaintext = br#"{"key":"task/abc123","title":"Ship feature"}"#;
        let blob = encrypt_entry(plaintext, &mk);
        let decrypted = decrypt_entry(&blob, &mk).unwrap();
        assert_eq!(decrypted, plaintext);

        // Same passphrase on another "device" produces same key and can decrypt
        let salt2 = derive_deterministic_salt(passphrase);
        let mk2 = derive_master_key(passphrase, &salt2).unwrap();
        let decrypted2 = decrypt_entry(&blob, &mk2).unwrap();
        assert_eq!(decrypted2, plaintext);
    }
}
