use axum::{Json, extract::{Path, State}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth::{jwt, password};
use crate::db::users;
use crate::error::ServerError;
use crate::state::AppState;

// --- Request/Response types ---

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub device_id: String,
    #[serde(default)]
    pub device_name: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in_secs: u64,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in_secs: u64,
}

// --- Handlers ---

/// POST /api/v1/auth/login
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ServerError> {
    let config = state.inner.config.clone();

    let (user, refresh_raw) = tokio::task::spawn_blocking(move || {
        let db = state.inner.db.lock().map_err(|_| {
            ServerError::Internal("DB lock poisoned".to_string())
        })?;

        // Find user
        let user = users::find_user_by_email(&db, &req.email)?
            .ok_or_else(|| ServerError::Unauthorized("Invalid credentials".to_string()))?;

        // Verify password
        let valid = password::verify_password(&req.password, &user.password_hash)
            .map_err(|_| ServerError::Internal("Password verification failed".to_string()))?;
        if !valid {
            return Err(ServerError::Unauthorized("Invalid credentials".to_string()));
        }

        // Upsert device
        users::upsert_device(&db, &req.device_id, &user.id, &req.device_name)?;

        // Generate refresh token
        let refresh_raw = uuid::Uuid::new_v4().to_string();
        let refresh_hash = sha256_hex(&refresh_raw);
        let expires_at = jiff::Timestamp::now()
            .checked_add(jiff::SignedDuration::from_hours(
                (config.auth.refresh_token_days * 24) as i64,
            ))
            .unwrap_or_else(|_| jiff::Timestamp::now());
        let expires_at_str = expires_at.strftime("%Y-%m-%d %H:%M:%S").to_string();

        users::store_refresh_token(&db, &refresh_hash, &req.device_id, &user.id, &expires_at_str)?;

        Ok::<_, ServerError>((user, refresh_raw))
    })
    .await
    .map_err(|e| ServerError::Internal(format!("Task join error: {e}")))??;

    let access_token = jwt::encode_access_token(
        &user.id,
        "", // device_id not critical in access token for login
        &config.auth.jwt_secret,
        config.auth.access_token_mins,
    )
    .map_err(|e| ServerError::Internal(format!("JWT encode error: {e}")))?;

    Ok(Json(LoginResponse {
        access_token,
        refresh_token: refresh_raw,
        expires_in_secs: config.auth.access_token_mins * 60,
    }))
}

/// POST /api/v1/auth/refresh
pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, ServerError> {
    let config = state.inner.config.clone();

    let (user_id, device_id, new_refresh_raw) = tokio::task::spawn_blocking(move || {
        let db = state.inner.db.lock().map_err(|_| {
            ServerError::Internal("DB lock poisoned".to_string())
        })?;

        let token_hash = sha256_hex(&req.refresh_token);
        let token = users::validate_refresh_token(&db, &token_hash)?
            .ok_or_else(|| ServerError::Unauthorized("Invalid or expired refresh token".to_string()))?;

        // Rotate: revoke old, issue new
        let new_refresh_raw = uuid::Uuid::new_v4().to_string();
        let new_refresh_hash = sha256_hex(&new_refresh_raw);
        let expires_at = jiff::Timestamp::now()
            .checked_add(jiff::SignedDuration::from_hours(
                (config.auth.refresh_token_days * 24) as i64,
            ))
            .unwrap_or_else(|_| jiff::Timestamp::now());
        let expires_at_str = expires_at.strftime("%Y-%m-%d %H:%M:%S").to_string();

        // Revoke old token
        db.execute(
            "UPDATE refresh_tokens SET revoked = 1 WHERE token_hash = ?1",
            rusqlite::params![token_hash],
        )?;

        users::store_refresh_token(
            &db,
            &new_refresh_hash,
            &token.device_id,
            &token.user_id,
            &expires_at_str,
        )?;

        // Update device last_seen
        users::upsert_device(&db, &token.device_id, &token.user_id, "")?;

        Ok::<_, ServerError>((token.user_id, token.device_id, new_refresh_raw))
    })
    .await
    .map_err(|e| ServerError::Internal(format!("Task join error: {e}")))??;

    let access_token = jwt::encode_access_token(
        &user_id,
        &device_id,
        &config.auth.jwt_secret,
        config.auth.access_token_mins,
    )
    .map_err(|e| ServerError::Internal(format!("JWT encode error: {e}")))?;

    Ok(Json(RefreshResponse {
        access_token,
        refresh_token: new_refresh_raw,
        expires_in_secs: config.auth.access_token_mins * 60,
    }))
}

/// DELETE /api/v1/auth/devices/:device_id
pub async fn delete_device(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let auth = crate::auth::middleware::extract_auth(&headers, &state)?;
    let user_id = auth.user_id;

    tokio::task::spawn_blocking(move || {
        let db = state.inner.db.lock().map_err(|_| {
            ServerError::Internal("DB lock poisoned".to_string())
        })?;
        users::revoke_device_tokens(&db, &device_id, &user_id)?;
        Ok::<_, ServerError>(())
    })
    .await
    .map_err(|e| ServerError::Internal(format!("Task join error: {e}")))??;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}
