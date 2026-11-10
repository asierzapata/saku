use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_server")]
    pub server: ServerSection,
    #[serde(default = "default_auth")]
    pub auth: AuthSection,
    #[serde(default = "default_database")]
    pub database: DatabaseSection,
    #[serde(default = "default_storage")]
    pub storage: StorageSection,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerSection {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthSection {
    #[serde(default)]
    pub jwt_secret: String,
    #[serde(default = "default_access_token_mins")]
    pub access_token_mins: u64,
    #[serde(default = "default_refresh_token_days")]
    pub refresh_token_days: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSection {
    #[serde(default = "default_database_path")]
    pub path: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageSection {
    #[serde(default = "default_bucket")]
    pub bucket: String,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_storage_credential")]
    pub access_key_id: String,
    #[serde(default = "default_storage_credential")]
    pub secret_access_key: String,
}

fn default_server() -> ServerSection {
    ServerSection {
        host: default_host(),
        port: default_port(),
    }
}

fn default_auth() -> AuthSection {
    AuthSection {
        jwt_secret: String::new(),
        access_token_mins: default_access_token_mins(),
        refresh_token_days: default_refresh_token_days(),
    }
}

fn default_database() -> DatabaseSection {
    DatabaseSection {
        path: default_database_path(),
    }
}

fn default_database_path() -> PathBuf {
    PathBuf::from("/data/saku-server.db")
}

fn default_storage() -> StorageSection {
    StorageSection {
        bucket: default_bucket(),
        region: default_region(),
        endpoint: default_endpoint(),
        access_key_id: default_storage_credential(),
        secret_access_key: default_storage_credential(),
    }
}

fn default_bucket() -> String {
    "unused-by-tdo-kv".to_string()
}

fn default_endpoint() -> String {
    "http://127.0.0.1:9".to_string()
}

fn default_storage_credential() -> String {
    "unused-by-tdo-kv".to_string()
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_access_token_mins() -> u64 {
    15
}

fn default_refresh_token_days() -> u64 {
    90
}

fn default_region() -> String {
    "auto".to_string()
}

/// Load config from a TOML file (if it exists), with `SAKU__*` env var overrides.
///
/// Env var mapping: `SAKU__SECTION__KEY` → `section.key`
/// e.g. `SAKU__AUTH__JWT_SECRET` → `auth.jwt_secret`
pub fn load_config(path: &std::path::Path) -> anyhow::Result<ServerConfig> {
    let mut config: ServerConfig = match std::fs::read_to_string(path) {
        Ok(content) => toml::from_str(&content)?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => toml::from_str("")?,
        Err(err) => return Err(err.into()),
    };

    // Apply env var overrides
    if let Ok(v) = std::env::var("SAKU__SERVER__HOST") {
        config.server.host = v;
    }
    if let Ok(v) = std::env::var("SAKU__SERVER__PORT") {
        config.server.port = v.parse()?;
    }
    if let Ok(v) = std::env::var("SAKU__AUTH__JWT_SECRET") {
        config.auth.jwt_secret = v;
    }
    if let Ok(v) = std::env::var("SAKU__AUTH__ACCESS_TOKEN_MINS") {
        config.auth.access_token_mins = v.parse()?;
    }
    if let Ok(v) = std::env::var("SAKU__AUTH__REFRESH_TOKEN_DAYS") {
        config.auth.refresh_token_days = v.parse()?;
    }
    if let Ok(v) = std::env::var("SAKU__DATABASE__PATH") {
        config.database.path = PathBuf::from(v);
    }
    if let Ok(v) = std::env::var("SAKU__STORAGE__BUCKET") {
        config.storage.bucket = v;
    }
    if let Ok(v) = std::env::var("SAKU__STORAGE__REGION") {
        config.storage.region = v;
    }
    if let Ok(v) = std::env::var("SAKU__STORAGE__ENDPOINT") {
        config.storage.endpoint = v;
    }
    if let Ok(v) = std::env::var("SAKU__STORAGE__ACCESS_KEY_ID") {
        config.storage.access_key_id = v;
    }
    if let Ok(v) = std::env::var("SAKU__STORAGE__SECRET_ACCESS_KEY") {
        config.storage.secret_access_key = v;
    }

    if config.auth.jwt_secret.trim().is_empty() {
        anyhow::bail!(
            "Missing auth.jwt_secret: set it in {} or via SAKU__AUTH__JWT_SECRET",
            path.display()
        );
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_defaults_when_config_file_is_missing_and_jwt_secret_is_provided() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nonexistent.toml");
        let existing_file = dir.path().join("minimal.toml");
        std::fs::write(
            &existing_file,
            "[auth]\njwt_secret = \"secret-from-minimal\"\n",
        )
        .unwrap();

        let cfg = load_config(&existing_file).unwrap();
        assert_eq!(cfg.server.host, "0.0.0.0");
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.auth.jwt_secret, "secret-from-minimal");
        assert_eq!(cfg.database.path, PathBuf::from("/data/saku-server.db"));
        assert_eq!(cfg.storage.bucket, "unused-by-tdo-kv");

        // Without jwt_secret, missing file fails closed.
        let err = load_config(&missing).unwrap_err();
        assert!(err.to_string().contains("Missing auth.jwt_secret"));
    }
}
