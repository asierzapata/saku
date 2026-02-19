use crate::cipher::{DataKey, generate_nonce, xchacha20_encrypt};
use crate::error::CryptoError;
use crate::format::{FileHeader, write_file};
use crate::kdf::MasterKey;

/// Encrypt plaintext using the master key (KEK) and salt.
///
/// Generates a fresh DEK and nonces per call. Returns a self-describing
/// binary blob (header + ciphertext).
pub fn encrypt(
    plaintext: &[u8],
    master_key: &MasterKey,
    kek_salt: &[u8; 16],
) -> Result<Vec<u8>, CryptoError> {
    // Generate a fresh DEK
    let dek = DataKey::generate();

    // Generate nonces
    let dek_nonce = generate_nonce();
    let file_nonce = generate_nonce();

    // Encrypt the DEK with the master key (KEK)
    let enc_dek_vec = xchacha20_encrypt(master_key.as_bytes(), &dek_nonce, dek.as_bytes());
    let mut enc_dek = [0u8; 48];
    enc_dek.copy_from_slice(&enc_dek_vec);

    // Encrypt the content with the DEK
    let ciphertext = xchacha20_encrypt(dek.as_bytes(), &file_nonce, plaintext);

    let header = FileHeader {
        kek_salt: *kek_salt,
        dek_nonce,
        enc_dek,
        file_nonce,
    };

    Ok(write_file(&header, &ciphertext))
}
