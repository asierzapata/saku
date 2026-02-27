use keyring::Entry;
use zeroize::Zeroizing;

use crate::error::KeychainError;

const SERVICE_NAME: &str = "saku-crypto";
const SYNC_CREDENTIALS_ACCOUNT: &str = "saku-sync-credentials";

const LEGACY_ACCESS_TOKEN_ACCOUNT: &str = "saku-sync-access-token";
const LEGACY_REFRESH_TOKEN_ACCOUNT: &str = "saku-sync-refresh-token";
const LEGACY_PASSPHRASE_ACCOUNT: &str = "saku-sync-passphrase";

/// OS keychain store for saku passphrases.
pub struct KeychainStore {
    entry: Entry,
}

impl KeychainStore {
    /// Create a new keychain store for the given account name.
    pub fn new(account: &str) -> Result<Self, KeychainError> {
        let entry = Entry::new(SERVICE_NAME, account).map_err(KeychainError::StoreFailed)?;
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

// ── Consolidated sync credentials ──────────────────────────────────────

/// All sync-related credentials stored as a single keychain entry.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct SyncCredentials {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub passphrase: Option<String>,
}

/// A keychain-backed store that keeps all sync credentials in one entry.
pub struct SyncCredentialStore {
    entry: Entry,
}

impl SyncCredentialStore {
    /// Create a new consolidated credential store.
    pub fn new() -> Result<Self, KeychainError> {
        let entry = Entry::new(SERVICE_NAME, SYNC_CREDENTIALS_ACCOUNT)
            .map_err(KeychainError::StoreFailed)?;
        Ok(Self { entry })
    }

    /// Serialize and store credentials in the keychain.
    pub fn store(&self, credentials: &SyncCredentials) -> Result<(), KeychainError> {
        let json =
            serde_json::to_string(credentials).map_err(KeychainError::SerializeFailed)?;
        self.entry
            .set_password(&json)
            .map_err(KeychainError::StoreFailed)
    }

    /// Load and deserialize credentials from the keychain.
    pub fn load(&self) -> Result<SyncCredentials, KeychainError> {
        let json = match self.entry.get_password() {
            Ok(pw) => pw,
            Err(keyring::Error::NoEntry) => return Err(KeychainError::NotFound),
            Err(e) => return Err(KeychainError::RetrieveFailed(e)),
        };
        serde_json::from_str(&json).map_err(KeychainError::DeserializeFailed)
    }

    /// Delete the consolidated credential entry.
    pub fn delete(&self) -> Result<(), KeychainError> {
        self.entry
            .delete_credential()
            .map_err(KeychainError::DeleteFailed)
    }

    /// Attempt to migrate legacy per-field keychain entries into a single
    /// consolidated entry. Returns `None` if no legacy entries exist.
    pub fn migrate_legacy(&self) -> Result<Option<SyncCredentials>, KeychainError> {
        let access_token = read_legacy(LEGACY_ACCESS_TOKEN_ACCOUNT);
        let refresh_token = read_legacy(LEGACY_REFRESH_TOKEN_ACCOUNT);
        let passphrase = read_legacy(LEGACY_PASSPHRASE_ACCOUNT);

        if access_token.is_none() && refresh_token.is_none() && passphrase.is_none() {
            return Ok(None);
        }

        let creds = SyncCredentials {
            access_token,
            refresh_token,
            passphrase,
        };

        self.store(&creds)?;

        // Best-effort cleanup of legacy entries
        let _ = delete_legacy(LEGACY_ACCESS_TOKEN_ACCOUNT);
        let _ = delete_legacy(LEGACY_REFRESH_TOKEN_ACCOUNT);
        let _ = delete_legacy(LEGACY_PASSPHRASE_ACCOUNT);

        Ok(Some(creds))
    }

    /// Try to load existing credentials; on `NotFound`, attempt legacy migration.
    pub fn load_or_migrate(&self) -> Result<SyncCredentials, KeychainError> {
        match self.load() {
            Ok(creds) => Ok(creds),
            Err(KeychainError::NotFound) => self
                .migrate_legacy()?
                .ok_or(KeychainError::NotFound),
            Err(e) => Err(e),
        }
    }
}

/// Read a single legacy keychain entry, returning `None` if missing or on error.
fn read_legacy(account: &str) -> Option<String> {
    KeychainStore::new(account)
        .and_then(|ks| ks.get_passphrase())
        .ok()
        .map(|z| z.to_string())
}

/// Delete a single legacy keychain entry (best-effort).
fn delete_legacy(account: &str) -> Result<(), KeychainError> {
    KeychainStore::new(account)?.delete()
}
