use serde::{Deserialize, Serialize};

// --- Auth types ---

#[derive(Serialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub device_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub device_name: String,
}

#[derive(Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in_secs: u64,
}

#[derive(Serialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Deserialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in_secs: u64,
}

// --- Sync types ---

#[derive(Deserialize)]
pub struct PresignedUrlResponse {
    pub url: String,
    pub expires_in_secs: u64,
}

#[derive(Serialize)]
pub struct UploadUrlRequest {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_length: Option<i64>,
}

#[derive(Deserialize)]
pub struct UploadUrlResponse {
    pub url: String,
    pub expires_in_secs: u64,
}

#[derive(Serialize)]
pub struct ConfirmUploadRequest {
    pub path: String,
    pub size_bytes: i64,
}

#[derive(Deserialize)]
pub struct MetadataResponse {
    pub size_bytes: u64,
    pub last_modified_ms: Option<i64>,
    pub etag: Option<String>,
}

#[derive(Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}
