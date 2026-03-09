use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use tower::ServiceExt;

use saku_server::auth::jwt::encode_access_token;
use saku_server::config::{AuthSection, DatabaseSection, ServerConfig, ServerSection, StorageSection};
use saku_server::db::{migrations::run_migrations, users::create_user};
use saku_server::state::AppState;

const TEST_JWT_SECRET: &str = "test-secret-for-integration-tests";

/// Build a test app with in-memory SQLite and memory storage operator.
pub fn test_app() -> (Router, AppState) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    let storage = opendal::Operator::new(opendal::services::Memory::default())
        .unwrap()
        .finish();

    let config = ServerConfig {
        server: ServerSection {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        auth: AuthSection {
            jwt_secret: TEST_JWT_SECRET.to_string(),
            access_token_mins: 60,
            refresh_token_days: 90,
        },
        database: DatabaseSection {
            path: ":memory:".into(),
        },
        storage: StorageSection {
            bucket: "test".to_string(),
            region: "auto".to_string(),
            endpoint: "http://localhost".to_string(),
            access_key_id: "test".to_string(),
            secret_access_key: "test".to_string(),
        },
    };

    let state = AppState::new(conn, storage, config);
    let router = saku_server::build_router(state.clone());
    (router, state)
}

/// Mint a JWT token for testing (bypasses bcrypt login flow).
pub fn mint_token(user_id: &str, device_id: &str) -> String {
    encode_access_token(user_id, device_id, TEST_JWT_SECRET, 60).unwrap()
}

/// Create a test user directly in the DB and return their user_id.
pub fn create_test_user(state: &AppState, email: &str) -> String {
    let db = state.inner.db.lock().unwrap();
    create_user(&db, email, "$2b$12$dummy_hash_not_used_in_tests").unwrap()
}

/// Send a GET /api/v1/kv/:tool request.
pub async fn get_kv(app: &Router, token: &str, tool: &str, cookie: Option<&str>, limit: Option<i64>) -> axum::response::Response {
    let mut uri = format!("/api/v1/kv/{tool}");
    let mut params = vec![];
    if let Some(c) = cookie {
        params.push(format!("cookie={c}"));
    }
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if !params.is_empty() {
        uri.push('?');
        uri.push_str(&params.join("&"));
    }

    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    app.clone().oneshot(req).await.unwrap()
}

/// Send a PUT /api/v1/kv/:tool batch request.
pub async fn put_kv_batch(app: &Router, token: &str, tool: &str, body: serde_json::Value) -> axum::response::Response {
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/kv/{tool}"))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    app.clone().oneshot(req).await.unwrap()
}

/// Send a PUT /api/v1/kv/:tool/:key single-entry request with raw bytes.
pub async fn put_kv_single(app: &Router, token: &str, tool: &str, key: &str, body: &[u8]) -> axum::response::Response {
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/kv/{tool}/{key}"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_vec()))
        .unwrap();

    app.clone().oneshot(req).await.unwrap()
}

/// Send a GET /api/v1/kv/:tool/snapshot request.
pub async fn get_kv_snapshot(app: &Router, token: &str, tool: &str, limit: Option<i64>) -> axum::response::Response {
    let mut uri = format!("/api/v1/kv/{tool}/snapshot");
    if let Some(l) = limit {
        uri.push_str(&format!("?limit={l}"));
    }

    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    app.clone().oneshot(req).await.unwrap()
}

/// Parse a response body as JSON and return (status, parsed_body).
pub async fn parse_json<T: DeserializeOwned>(response: axum::response::Response) -> (StatusCode, T) {
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let parsed: T = serde_json::from_slice(&body).unwrap();
    (status, parsed)
}

/// Send a request with no auth header.
pub async fn get_kv_no_auth(app: &Router, tool: &str) -> axum::response::Response {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/kv/{tool}"))
        .body(Body::empty())
        .unwrap();

    app.clone().oneshot(req).await.unwrap()
}

/// Send a request with an invalid auth token.
pub async fn get_kv_bad_auth(app: &Router, tool: &str) -> axum::response::Response {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/kv/{tool}"))
        .header("authorization", "Bearer invalid-token-garbage")
        .body(Body::empty())
        .unwrap();

    app.clone().oneshot(req).await.unwrap()
}
