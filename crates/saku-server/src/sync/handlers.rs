use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::auth::middleware;
use crate::db::quota;
use crate::error::ServerError;
use crate::state::AppState;
use crate::sync::storage::object_key;

// --- Request/Response types ---

#[derive(Deserialize)]
pub struct DownloadQuery {
    pub path: String,
}

#[derive(Serialize)]
pub struct PresignedUrlResponse {
    pub url: String,
    pub expires_in_secs: u64,
}

#[derive(Deserialize)]
pub struct UploadUrlRequest {
    pub path: String,
    pub content_length: Option<i64>,
}

#[derive(Serialize)]
pub struct UploadUrlResponse {
    pub url: String,
    pub expires_in_secs: u64,
}

#[derive(Deserialize)]
pub struct ConfirmUploadRequest {
    pub path: String,
    pub size_bytes: i64,
}

#[derive(Serialize)]
pub struct MetadataResponse {
    pub size_bytes: u64,
    pub last_modified_ms: Option<i64>,
    pub etag: Option<String>,
}

// --- Handlers ---

const PRESIGN_TTL_SECS: u64 = 3600; // 1 hour

/// GET /api/v1/sync/:tool/download-url?path=...
pub async fn download_url(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tool): Path<String>,
    Query(query): Query<DownloadQuery>,
) -> Result<Json<PresignedUrlResponse>, ServerError> {
    let auth = middleware::extract_auth(&headers, &state)?;
    let key = object_key(&auth.user_id, &tool, &query.path);
    let url = state
        .inner
        .storage
        .presign_read(&key, Duration::from_secs(PRESIGN_TTL_SECS))
        .await
        .map_err(|e| ServerError::Storage(format!("Failed to presign download: {e}")))?
        .uri()
        .to_string();

    Ok(Json(PresignedUrlResponse {
        url,
        expires_in_secs: PRESIGN_TTL_SECS,
    }))
}

/// POST /api/v1/sync/:tool/upload-url
pub async fn upload_url(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tool): Path<String>,
    Json(req): Json<UploadUrlRequest>,
) -> Result<Json<UploadUrlResponse>, ServerError> {
    let auth = middleware::extract_auth(&headers, &state)?;
    let key = object_key(&auth.user_id, &tool, &req.path);
    let url = state
        .inner
        .storage
        .presign_write(&key, Duration::from_secs(PRESIGN_TTL_SECS))
        .await
        .map_err(|e| ServerError::Storage(format!("Failed to presign upload: {e}")))?
        .uri()
        .to_string();

    Ok(Json(UploadUrlResponse {
        url,
        expires_in_secs: PRESIGN_TTL_SECS,
    }))
}

/// POST /api/v1/sync/:tool/confirm-upload
pub async fn confirm_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(_tool): Path<String>,
    Json(req): Json<ConfirmUploadRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let auth = middleware::extract_auth(&headers, &state)?;
    let user_id = auth.user_id;
    let size_bytes = req.size_bytes;

    tokio::task::spawn_blocking(move || {
        let db = state.inner.db.lock().map_err(|_| {
            ServerError::Internal("DB lock poisoned".to_string())
        })?;
        quota::update_usage(&db, &user_id, size_bytes)?;
        Ok::<_, ServerError>(())
    })
    .await
    .map_err(|e| ServerError::Internal(format!("Task join error: {e}")))??;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

/// GET /api/v1/sync/:tool/metadata?path=...
pub async fn metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tool): Path<String>,
    Query(query): Query<DownloadQuery>,
) -> Result<Json<MetadataResponse>, ServerError> {
    let auth = middleware::extract_auth(&headers, &state)?;
    let key = object_key(&auth.user_id, &tool, &query.path);
    let stat = state
        .inner
        .storage
        .stat(&key)
        .await
        .map_err(|e| ServerError::Storage(format!("Failed to stat object: {e}")))?;

    Ok(Json(MetadataResponse {
        size_bytes: stat.content_length(),
        last_modified_ms: stat
            .last_modified()
            .map(|t| t.timestamp_millis()),
        etag: stat.etag().map(|s| s.to_string()),
    }))
}
