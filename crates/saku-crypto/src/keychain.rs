use keyring::Entry;
use zeroize::Zeroizing;

use crate::error::KeychainError;

const SERVICE_NAME: &str = "saku-crypto";

/// OS keychain store for saku passphrases.
pub struct KeychainStore {
    entry: Entry,
}

impl KeychainStore {
    /// Create a new keychain store for the given account name.
    pub fn new(account: &str) -> Result<Self, KeychainError> {
        let entry =
            Entry::new(SERVICE_NAME, account).map_err(KeychainError::StoreFailed)?;
        Ok(Self { entry })
    }

    /// Store a passphrase in the OS keychain.
    pub fn store_passphrase(&self, passphrase: &str) -> Result<(), KeychainError> {
        self.entry
            .set_password(passphrase)
            .map_err(KeychainError::StoreFailed)
    }

    /// Retrieve the stored passphrase from the OS keychain.
    pub fn get_passphrase(&self) -> Result<Zeroizing<String>, KeychainError> {
        match self.entry.get_password() {
            Ok(pw) => Ok(Zeroizing::new(pw)),
            Err(keyring::Error::NoEntry) => Err(KeychainError::NotFound),
            Err(e) => Err(KeychainError::RetrieveFailed(e)),
        }
    }

    /// Delete the stored passphrase from the OS keychain.
    pub fn delete(&self) -> Result<(), KeychainError> {
        self.entry
            .delete_credential()
            .map_err(KeychainError::DeleteFailed)
    }
}
