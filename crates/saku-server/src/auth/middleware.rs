use axum::http::HeaderMap;

use crate::auth::jwt;
use crate::error::ServerError;
use crate::state::AppState;

/// Authenticated user info, extracted from Bearer JWT.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub device_id: String,
}

/// Extract and validate the Bearer JWT from request headers.
pub fn extract_auth(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthenticatedUser, ServerError> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ServerError::Unauthorized("Missing Authorization header".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ServerError::Unauthorized("Invalid Authorization format".to_string()))?;

    let claims =
        jwt::decode_access_token(token, &state.inner.config.auth.jwt_secret).map_err(|_| {
            ServerError::Unauthorized("Invalid or expired access token".to_string())
        })?;

    Ok(AuthenticatedUser {
        user_id: claims.sub,
        device_id: claims.device_id,
    })
}
