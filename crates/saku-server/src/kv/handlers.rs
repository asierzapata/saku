use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use crate::auth::middleware;
use crate::db::kv;
use crate::error::ServerError;
use crate::state::AppState;

const DEFAULT_LIMIT: i64 = 100;

// --- Request/Response types ---

#[derive(Deserialize)]
pub struct GetEntriesQuery {
    pub cookie: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct GetEntriesResponse {
    pub entries: Vec<KvEntryResponse>,
    pub cookie: String,
    pub has_more: bool,
}

#[derive(Serialize)]
pub struct KvEntryResponse {
    pub key: String,
    pub blob: String, // base64-encoded
    pub seq: i64,
    pub deleted: bool,
}

#[derive(Deserialize)]
pub struct BatchPutRequest {
    pub entries: Vec<BatchPutEntry>,
}

#[derive(Deserialize)]
pub struct BatchPutEntry {
    pub key: String,
    pub blob: String, // base64-encoded
}

#[derive(Serialize)]
pub struct BatchPutResponse {
    pub results: Vec<BatchPutResult>,
    pub cookie: String,
}

#[derive(Serialize)]
pub struct BatchPutResult {
    pub key: String,
    pub seq: i64,
}

#[derive(Serialize)]
pub struct PutSingleResponse {
    pub seq: i64,
}

// --- Handlers ---

/// GET /api/v1/kv/:tool?cookie=&limit=
/// Incremental pull — returns entries with seq > cookie.
pub async fn get_entries(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tool): Path<String>,
    Query(query): Query<GetEntriesQuery>,
) -> Result<Json<GetEntriesResponse>, ServerError> {
    let auth = middleware::extract_auth(&headers, &state)?;
    let user_id = auth.user_id;
    let after_seq: i64 = query
        .cookie
        .as_deref()
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).max(1);

    let response = tokio::task::spawn_blocking(move || {
        let db = state
            .inner
            .db
            .lock()
            .map_err(|_| ServerError::Internal("DB lock poisoned".to_string()))?;

        // Fetch limit+1 to detect has_more
        let mut entries = kv::get_entries_since(&db, &user_id, &tool, after_seq, limit + 1)?;
        let has_more = entries.len() as i64 > limit;
        if has_more {
            entries.truncate(limit as usize);
        }

        let cookie = entries
            .last()
            .map(|e| e.seq.to_string())
            .unwrap_or_else(|| after_seq.to_string());

        let entries_response: Vec<KvEntryResponse> = entries
            .into_iter()
            .map(|e| KvEntryResponse {
                key: e.key,
                blob: BASE64.encode(&e.blob),
                seq: e.seq,
                deleted: e.deleted,
            })
            .collect();

        Ok::<_, ServerError>(GetEntriesResponse {
            entries: entries_response,
            cookie,
            has_more,
        })
    })
    .await
    .map_err(|e| ServerError::Internal(format!("Task join error: {e}")))??;

    Ok(Json(response))
}

/// PUT /api/v1/kv/:tool
/// Batch upsert with base64-encoded blobs.
pub async fn batch_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tool): Path<String>,
    Json(req): Json<BatchPutRequest>,
) -> Result<Json<BatchPutResponse>, ServerError> {
    let auth = middleware::extract_auth(&headers, &state)?;
    let user_id = auth.user_id;

    // Decode base64 blobs
    let entries: Vec<(String, Vec<u8>)> = req
        .entries
        .into_iter()
        .map(|e| {
            let blob = BASE64
                .decode(&e.blob)
                .map_err(|err| ServerError::BadRequest(format!("Invalid base64 for key '{}': {err}", e.key)))?;
            Ok((e.key, blob))
        })
        .collect::<Result<Vec<_>, ServerError>>()?;

    let response = tokio::task::spawn_blocking(move || {
        let db = state
            .inner
            .db
            .lock()
            .map_err(|_| ServerError::Internal("DB lock poisoned".to_string()))?;

        let (results, cookie) = kv::batch_upsert(&db, &user_id, &tool, &entries)?;

        let results_response: Vec<BatchPutResult> = results
            .into_iter()
            .map(|r| BatchPutResult {
                key: r.key,
                seq: r.seq,
            })
            .collect();

        Ok::<_, ServerError>(BatchPutResponse {
            results: results_response,
            cookie,
        })
    })
    .await
    .map_err(|e| ServerError::Internal(format!("Task join error: {e}")))??;

    Ok(Json(response))
}

/// PUT /api/v1/kv/:tool/:key
/// Single upsert with raw bytes body.
pub async fn put_single(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tool, key)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<PutSingleResponse>, ServerError> {
    let auth = middleware::extract_auth(&headers, &state)?;
    let user_id = auth.user_id;
    let blob = body.to_vec();

    let seq = tokio::task::spawn_blocking(move || {
        let db = state
            .inner
            .db
            .lock()
            .map_err(|_| ServerError::Internal("DB lock poisoned".to_string()))?;

        let seq = kv::upsert_single(&db, &user_id, &tool, &key, &blob)?;
        Ok::<_, ServerError>(seq)
    })
    .await
    .map_err(|e| ServerError::Internal(format!("Task join error: {e}")))??;

    Ok(Json(PutSingleResponse { seq }))
}

/// GET /api/v1/kv/:tool/snapshot
/// Full pull — same as get_entries with cookie=None.
pub async fn snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tool): Path<String>,
    Query(query): Query<GetEntriesQuery>,
) -> Result<Json<GetEntriesResponse>, ServerError> {
    // Force cookie to None (after_seq = 0) for full snapshot
    let query = GetEntriesQuery {
        cookie: None,
        limit: query.limit,
    };
    get_entries(State(state), headers, Path(tool), Query(query)).await
}
