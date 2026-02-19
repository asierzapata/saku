use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_server")]
    pub server: ServerSection,
    pub auth: AuthSection,
    pub database: DatabaseSection,
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
    pub jwt_secret: String,
    #[serde(default = "default_access_token_mins")]
    pub access_token_mins: u64,
    #[serde(default = "default_refresh_token_days")]
    pub refresh_token_days: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSection {
    pub path: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageSection {
    pub bucket: String,
    #[serde(default = "default_region")]
    pub region: String,
    pub endpoint: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

fn default_server() -> ServerSection {
    ServerSection {
        host: default_host(),
        port: default_port(),
    }
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

/// Load config from a TOML file, with `SAKU__*` env var overrides.
///
/// Env var mapping: `SAKU__SECTION__KEY` → `section.key`
/// e.g. `SAKU__AUTH__JWT_SECRET` → `auth.jwt_secret`
pub fn load_config(path: &std::path::Path) -> anyhow::Result<ServerConfig> {
    let content = std::fs::read_to_string(path)?;
    let mut config: ServerConfig = toml::from_str(&content)?;

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

    Ok(config)
}
