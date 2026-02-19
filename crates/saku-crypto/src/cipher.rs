use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use rand::RngCore;
use rand::rngs::OsRng;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::CryptoError;

/// A 256-bit data encryption key (DEK). Zeroized on drop, cannot be cloned.
#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct DataKey {
    bytes: [u8; 32],
}

impl DataKey {
    /// Generate a random DEK.
    pub(crate) fn generate() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Self { bytes }
    }

    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

/// Generate a random 24-byte nonce for XChaCha20Poly1305.
pub(crate) fn generate_nonce() -> [u8; 24] {
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

/// Encrypt plaintext with XChaCha20Poly1305.
pub(crate) fn xchacha20_encrypt(key: &[u8; 32], nonce: &[u8; 24], plaintext: &[u8]) -> Vec<u8> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let xnonce = XNonce::from_slice(nonce);
    cipher
        .encrypt(xnonce, plaintext)
        .expect("XChaCha20Poly1305 encryption should not fail")
}

/// Decrypt ciphertext with XChaCha20Poly1305.
pub(crate) fn xchacha20_decrypt(
    key: &[u8; 32],
    nonce: &[u8; 24],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let xnonce = XNonce::from_slice(nonce);
    cipher
        .decrypt(xnonce, ciphertext)
        .map_err(|_| CryptoError::ContentDecryptionFailed)
}
