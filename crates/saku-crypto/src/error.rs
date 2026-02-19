#[derive(Debug, thiserror::Error)]
pub enum KdfError {
    #[error("Invalid KDF parameters: {0}")]
    InvalidParams(argon2::Error),

    #[error("Key derivation failed: {0}")]
    DerivationFailed(argon2::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("Input too short to contain a valid header")]
    HeaderTooShort,

    #[error("Invalid magic bytes — not a saku-crypto file")]
    InvalidMagic,

    #[error("Unsupported file format version: {0}")]
    UnsupportedVersion(u8),

    #[error("DEK decryption failed — wrong passphrase or corrupted header")]
    DekDecryptionFailed,

    #[error("Content decryption failed — data may be tampered")]
    ContentDecryptionFailed,

    #[error("Key derivation error: {0}")]
    Kdf(#[from] KdfError),
}

#[cfg(feature = "keychain")]
#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    #[error("Failed to store passphrase in keychain: {0}")]
    StoreFailed(keyring::Error),

    #[error("Failed to retrieve passphrase from keychain: {0}")]
    RetrieveFailed(keyring::Error),

    #[error("Failed to delete passphrase from keychain: {0}")]
    DeleteFailed(keyring::Error),

    #[error("No passphrase found in keychain")]
    NotFound,
}
