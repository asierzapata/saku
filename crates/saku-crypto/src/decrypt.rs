use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use zeroize::Zeroizing;

use crate::cipher::{DataKey, xchacha20_decrypt};
use crate::error::CryptoError;
use crate::format::read_file;
use crate::kdf::MasterKey;

/// Decrypt a saku-crypto encrypted blob using the master key.
///
/// Distinguishes between wrong-passphrase errors (`DekDecryptionFailed`)
/// and tampered-data errors (`ContentDecryptionFailed`).
pub fn decrypt(encrypted: &[u8], master_key: &MasterKey) -> Result<Vec<u8>, CryptoError> {
    let (header, ciphertext) = read_file(encrypted)?;

    // Decrypt the DEK using the master key (KEK)
    // Done inline to map to DekDecryptionFailed specifically
    let cipher = XChaCha20Poly1305::new(master_key.as_bytes().into());
    let dek_nonce = XNonce::from_slice(&header.dek_nonce);
    let dek_bytes = Zeroizing::new(
        cipher
            .decrypt(dek_nonce, header.enc_dek.as_ref())
            .map_err(|_| CryptoError::DekDecryptionFailed)?,
    );

    let mut dek_array = [0u8; 32];
    dek_array.copy_from_slice(&dek_bytes);
    let dek = DataKey::from_bytes(dek_array);
    // Zero out the temporary array
    dek_array.iter_mut().for_each(|b| *b = 0);

    // Decrypt the content using the DEK
    xchacha20_decrypt(dek.as_bytes(), &header.file_nonce, ciphertext)
}
