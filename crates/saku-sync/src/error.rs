use saku_crypto::CryptoError;
use saku_storage::device::DeviceIdError;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("Backend error: {message}")]
    Backend { message: String },

    #[error("No passphrase provided")]
    NoPassphrase,

    #[error("Conflict detected for file: {file}")]
    Conflict { file: String },

    #[error("Device ID error: {0}")]
    DeviceId(#[from] DeviceIdError),

    #[error("KDF error: {0}")]
    Kdf(#[from] saku_crypto::KdfError),
}
