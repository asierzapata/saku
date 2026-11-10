use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
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
    pub base_cookie: String,
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

fn parse_cookie(cookie: &str) -> Result<i64, ServerError> {
    cookie.parse::<i64>().ok().filter(|value| *value >= 0)
        .ok_or_else(|| ServerError::BadRequest("invalid sync cookie".into()))
}

fn parse_v2_cookie(cookie: &str, generation: &str) -> Result<i64, ServerError> {
    if !cookie.is_empty() && cookie.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ServerError::CursorInvalidated);
    }
    let (epoch, sequence) = cookie.split_once(':').ok_or_else(|| ServerError::BadRequest("invalid sync cookie".into()))?;
    if epoch.len() != 32 || !epoch.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || sequence.is_empty() || !sequence.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ServerError::BadRequest("invalid sync cookie".into()));
    }
    let sequence = parse_cookie(sequence)?;
    if epoch != generation { return Err(ServerError::CursorInvalidated); }
    Ok(sequence)
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
    get_entries_inner(state, headers, tool, query, false).await
}

pub async fn get_entries_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tool): Path<String>,
    Query(query): Query<GetEntriesQuery>,
) -> Result<Json<GetEntriesResponse>, ServerError> {
    get_entries_inner(state, headers, tool, query, true).await
}

async fn get_entries_inner(
    state: AppState,
    headers: HeaderMap,
    tool: String,
    query: GetEntriesQuery,
    v2: bool,
) -> Result<Json<GetEntriesResponse>, ServerError> {
    let auth = middleware::extract_auth(&headers, &state)?;
    let user_id = auth.user_id;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 5000);

    let response = tokio::task::spawn_blocking(move || {
        let db = state
            .inner
            .db
            .lock()
            .map_err(|_| ServerError::Internal("DB lock poisoned".to_string()))?;

        let after_seq = query.cookie.as_deref().filter(|cookie| !cookie.is_empty())
            .map(|cookie| if v2 { parse_v2_cookie(cookie, state.kv_generation()) } else { parse_cookie(cookie) })
            .transpose()?.unwrap_or(0);
        if after_seq > kv::get_max_seq(&db, &user_id, &tool)? {
            if v2 { return Err(ServerError::CursorInvalidated); }
            return Err(ServerError::BadRequest("sync cookie is ahead of server state; recovery required".into()));
        }
        // Fetch limit+1 to detect has_more
        let mut entries = kv::get_entries_since(&db, &user_id, &tool, after_seq, limit + 1)?;
        let has_more = entries.len() as i64 > limit;
        if has_more {
            entries.truncate(limit as usize);
        }

        let last_seq = entries.last().map(|e| e.seq).unwrap_or(after_seq);
        let cookie = if v2 { format!("{}:{last_seq}", state.kv_generation()) } else { last_seq.to_string() };

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

/// PUT /api/v2/kv/:tool
/// Batch upsert with base64-encoded blobs.
pub async fn batch_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tool): Path<String>,
    req: Result<Json<BatchPutRequest>, JsonRejection>,
) -> Result<Json<BatchPutResponse>, ServerError> {
    let auth = middleware::extract_auth(&headers, &state)?;
    let user_id = auth.user_id;
    let Json(req) = req.map_err(|error| ServerError::BadRequest(error.to_string()))?;
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

        let base_seq = parse_v2_cookie(&req.base_cookie, state.kv_generation())?;
        if base_seq > kv::get_max_seq(&db, &user_id, &tool)? {
            return Err(ServerError::CursorInvalidated);
        }
        let (results, cookie) = kv::batch_upsert(&db, &user_id, &tool, base_seq, &entries)
            .map_err(|error| match error {
                kv::KvWriteError::Db(error) => ServerError::Db(error),
                kv::KvWriteError::InvalidRequest(message) => ServerError::BadRequest(message),
                kv::KvWriteError::StaleWrite => ServerError::Conflict("entries changed since the last pull".into()),
                kv::KvWriteError::SequenceExhausted => ServerError::Internal("server sequence exhausted".into()),
            })?;

        let results_response: Vec<BatchPutResult> = results
            .into_iter()
            .map(|r| BatchPutResult {
                key: r.key,
                seq: r.seq,
            })
            .collect();

        Ok::<_, ServerError>(BatchPutResponse {
            results: results_response,
            cookie: format!("{}:{cookie}", state.kv_generation()),
        })
    })
    .await
    .map_err(|e| ServerError::Internal(format!("Task join error: {e}")))??;

    Ok(Json(response))
}

pub async fn reject_legacy_write(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(), ServerError> {
    middleware::extract_auth(&headers, &state)?;
    Err(ServerError::PreconditionRequired("KV writes require guarded sync v2; upgrade the client".into()))
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

pub async fn snapshot_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tool): Path<String>,
    Query(query): Query<GetEntriesQuery>,
) -> Result<Json<GetEntriesResponse>, ServerError> {
    let query = GetEntriesQuery { cookie: None, limit: query.limit };
    get_entries_v2(State(state), headers, Path(tool), Query(query)).await
}
