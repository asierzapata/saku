mod common;

use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{json, Value};

use common::*;

async fn pull_cookie(app: &axum::Router, token: &str, tool: &str) -> String {
    let (status, body): (_, Value) = parse_json(get_kv(app, token, tool, None, None).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["has_more"], false);
    body["cookie"].as_str().unwrap().to_owned()
}

fn cookie_seq(cookie: &Value) -> i64 {
    cookie.as_str().unwrap().split_once(':').unwrap().1.parse().unwrap()
}

struct Snapshot(std::path::PathBuf);

impl Snapshot {
    fn capture(state: &saku_server::state::AppState) -> Self {
        let path = std::env::temp_dir().join(format!("saku-test-snapshot-{}.sqlite", uuid::Uuid::new_v4().simple()));
        state.inner.db.lock().unwrap().execute("VACUUM INTO ?1", [path.to_str().unwrap()]).unwrap();
        Self(path)
    }

    fn state(&self, template: &saku_server::state::AppState) -> saku_server::state::AppState {
        saku_server::state::AppState::new(
            rusqlite::Connection::open(&self.0).unwrap(),
            template.inner.storage.clone(),
            template.inner.config.clone(),
        )
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

#[tokio::test]
async fn restarted_and_restored_servers_invalidate_old_cursors_without_mutation() {
    let (app, state) = test_app();
    let user = create_test_user(&state, "epoch@test.com");
    let token = mint_token(&user, "device");
    let zero = pull_cookie(&app, &token, "tdo").await;
    let initial = json!({"base_cookie":zero,"entries":[{"key":"task/a","blob":BASE64.encode(b"old")}]});
    assert_eq!(put_kv_batch(&app, &token, "tdo", initial).await.status(), StatusCode::OK);
    let old = pull_cookie(&app, &token, "tdo").await;
    let snapshot = Snapshot::capture(&state);
    let reopened = snapshot.state(&state);
    assert_eq!(reopened.kv_generation(), reopened.clone().kv_generation());
    assert_ne!(state.kv_generation(), reopened.kv_generation());
    let restored_app = saku_server::build_router(reopened.clone());
    assert_eq!(get_kv(&restored_app, &token, "tdo", Some(&old), None).await.status(), StatusCode::GONE);
    let current = pull_cookie(&restored_app, &token, "tdo").await;
    assert_eq!(cookie_seq(&json!(current)), 1);
    let wrong_zero = format!("{}:0", state.kv_generation());
    assert_eq!(get_kv(&restored_app, &token, "tdo", Some(&wrong_zero), None).await.status(), StatusCode::GONE);
    let wrong_zero_put = json!({"base_cookie":wrong_zero,"entries":[{"key":"task/a","blob":BASE64.encode(b"stale")}]});
    assert_eq!(put_kv_batch(&restored_app, &token, "tdo", wrong_zero_put).await.status(), StatusCode::GONE);
    let mixed = json!({"base_cookie":old,"entries":[{"key":"task/new","blob":BASE64.encode(b"new")},{"key":"task/a","blob":BASE64.encode(b"stale")}]});
    assert_eq!(put_kv_batch(&restored_app, &token, "tdo", mixed).await.status(), StatusCode::GONE);
    assert_eq!(reopened.inner.db.lock().unwrap().query_row("SELECT next_seq FROM kv_seq_counters WHERE user_id = ?1 AND tool = 'tdo'", [&user], |row| row.get::<_, i64>(0)).unwrap(), 2);
    let (_, after): (_, Value) = parse_json(get_kv_snapshot(&restored_app, &token, "tdo", None).await).await;
    assert_eq!(after["entries"].as_array().unwrap().len(), 1);
    assert_eq!(after["entries"][0]["blob"], BASE64.encode(b"old"));
    assert_eq!(cookie_seq(&after["cookie"]), 1);
    let branch = json!({"base_cookie":current,"entries":[{"key":"task/a","blob":BASE64.encode(b"branch")}]});
    assert_eq!(put_kv_batch(&restored_app, &token, "tdo", branch).await.status(), StatusCode::OK);
    assert_eq!(get_kv(&restored_app, &token, "tdo", Some(&old), None).await.status(), StatusCode::GONE);
    let (_, after): (_, Value) = parse_json(get_kv_snapshot(&restored_app, &token, "tdo", None).await).await;
    assert_eq!(cookie_seq(&after["cookie"]), 2);
    let empty_snapshot = Snapshot::capture(&reopened);
    let empty_conn = rusqlite::Connection::open(&empty_snapshot.0).unwrap();
    empty_conn.execute_batch("DELETE FROM kv_entries; DELETE FROM kv_seq_counters;").unwrap();
    let empty = saku_server::state::AppState::new(empty_conn, state.inner.storage.clone(), state.inner.config.clone());
    let empty_app = saku_server::build_router(empty.clone());
    assert_eq!(get_kv(&empty_app, &token, "tdo", Some(&old), None).await.status(), StatusCode::GONE);
    let empty_cookie = pull_cookie(&empty_app, &token, "tdo").await;
    assert_eq!(empty_cookie, format!("{}:0", empty.kv_generation()));
    let (status, empty_body): (_, Value) = parse_json(get_kv(&empty_app, &token, "tdo", Some(""), None).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty_body["cookie"], empty_cookie);
}

#[tokio::test]
async fn stale_batch_cannot_replace_unseen_value() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "guard-regression@test.com");
    let a = mint_token(&user_id, "a");
    let b = mint_token(&user_id, "b");
    let zero = pull_cookie(&app, &a, "tdo").await;
    let initial = json!({"base_cookie":zero,"entries":[{"key":"task/a","blob":BASE64.encode(b"initial")}]});
    assert_eq!(put_kv_batch(&app, &a, "tdo", initial).await.status(), StatusCode::OK);
    let (_, pulled): (_, Value) = parse_json(get_kv(&app, &a, "tdo", None, None).await).await;
    assert_eq!(cookie_seq(&pulled["cookie"]), 1);
    let one = pulled["cookie"].as_str().unwrap().to_owned();
    let remote = json!({"base_cookie":one,"entries":[{"key":"task/a","blob":BASE64.encode(b"unseen")}]});
    assert_eq!(put_kv_batch(&app, &b, "tdo", remote).await.status(), StatusCode::OK);
    let stale = json!({"base_cookie":one,"entries":[{"key":"task/new","blob":BASE64.encode(b"new")},{"key":"task/a","blob":BASE64.encode(b"stale")}]});
    assert_eq!(put_kv_batch(&app, &a, "tdo", stale).await.status(), StatusCode::CONFLICT);
    let (_, snapshot): (_, Value) = parse_json(get_kv_snapshot(&app, &a, "tdo", None).await).await;
    assert_eq!(snapshot["entries"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["entries"][0]["blob"], BASE64.encode(b"unseen"));
    assert_eq!(cookie_seq(&snapshot["cookie"]), 2);
    let (_, pulled): (_, Value) = parse_json(get_kv(&app, &a, "tdo", Some(&one), None).await).await;
    assert_eq!(cookie_seq(&pulled["cookie"]), 2);
    let merged = json!({"base_cookie":pulled["cookie"],"entries":[{"key":"task/a","blob":BASE64.encode(b"merged")}]});
    let (status, result): (_, Value) = parse_json(put_kv_batch(&app, &a, "tdo", merged).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cookie_seq(&result["cookie"]), 3);
}

#[tokio::test]
async fn legacy_writes_are_retired_and_guard_requests_are_validated() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "validation@test.com");
    let token = mint_token(&user_id, "a");
    let zero = pull_cookie(&app, &token, "tdo").await;
    let body = json!({"base_cookie":zero,"entries":[{"key":"a","blob":BASE64.encode(b"a")}]});
    assert_eq!(put_kv_batch(&app, "invalid-token", "tdo", body.clone()).await.status(), StatusCode::UNAUTHORIZED);
    let req = Request::builder().method("PUT").uri("/api/v1/kv/tdo")
        .header("authorization", format!("Bearer {token}")).header("content-type", "application/json")
        .body(Body::from(body.to_string())).unwrap();
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::PRECONDITION_REQUIRED);
    let req = Request::builder().method("PUT").uri("/api/v1/kv/tdo").body(Body::empty()).unwrap();
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    for bad in [
        json!({"entries":[{"key":"a","blob":BASE64.encode(b"a")}]}),
        json!({"base_cookie":"","entries":[{"key":"a","blob":BASE64.encode(b"a")}]}),
        json!({"base_cookie":"bad","entries":[{"key":"a","blob":BASE64.encode(b"a")}]}),
        json!({"base_cookie":"-1","entries":[{"key":"a","blob":BASE64.encode(b"a")}]}),
        json!({"base_cookie":zero,"entries":[{"key":"a","blob":BASE64.encode(b"a")},{"key":"a","blob":BASE64.encode(b"a")}]}),
        json!({"base_cookie":zero,"entries":[]}),
    ] {
        assert_eq!(put_kv_batch(&app, &token, "tdo", bad).await.status(), StatusCode::BAD_REQUEST);
    }
    for bad in ["bad", "-1"] {
        assert_eq!(get_kv(&app, &token, "tdo", Some(bad), None).await.status(), StatusCode::BAD_REQUEST);
    }
    for numeric in ["0", "1"] {
        assert_eq!(get_kv(&app, &token, "tdo", Some(numeric), None).await.status(), StatusCode::GONE);
        let body = json!({"base_cookie":numeric,"entries":[{"key":"a","blob":BASE64.encode(b"a")}]});
        assert_eq!(put_kv_batch(&app, &token, "tdo", body).await.status(), StatusCode::GONE);
    }
    let ahead = format!("{}:1", state.kv_generation());
    assert_eq!(get_kv(&app, &token, "tdo", Some(&ahead), None).await.status(), StatusCode::GONE);
    let ahead_put = json!({"base_cookie":ahead,"entries":[{"key":"a","blob":BASE64.encode(b"a")}]});
    assert_eq!(put_kv_batch(&app, &token, "tdo", ahead_put).await.status(), StatusCode::GONE);
    assert_eq!(get_kv(&app, &token, "tdo", None, Some(i64::MAX)).await.status(), StatusCode::OK);
    assert_eq!(put_kv_batch(&app, &token, "tdo", body).await.status(), StatusCode::OK);
    let new = json!({"base_cookie":zero,"entries":[{"key":"b","blob":BASE64.encode(b"b")}]});
    assert_eq!(put_kv_batch(&app, &token, "tdo", new).await.status(), StatusCode::OK);
    let (status, entries): (_, Value) = parse_json(get_kv(&app, &token, "tdo", Some(""), None).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(entries["entries"].as_array().unwrap().len(), 2);
    let req = Request::builder().method("GET").uri("/api/v1/kv/tdo")
        .header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap();
    let (status, legacy): (_, Value) = parse_json(app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(legacy["entries"].as_array().unwrap().len(), 2);
    assert_eq!(legacy["cookie"], "2");
    let req = Request::builder().method("GET").uri("/api/v1/kv/tdo?cookie=1")
        .header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap();
    let (status, legacy): (_, Value) = parse_json(app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(legacy["entries"].as_array().unwrap().len(), 1);
    assert_eq!(legacy["cookie"], "2");
    let req = Request::builder().method("GET").uri("/api/v1/kv/tdo?cookie=")
        .header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap();
    let (status, legacy): (_, Value) = parse_json(app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(legacy["entries"].as_array().unwrap().len(), 2);
    let req = Request::builder().method("GET").uri("/api/v1/kv/tdo/snapshot")
        .header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap();
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn empty_store_pull() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "empty@test.com");
    let token = mint_token(&user_id, "dev-1");

    let resp = get_kv(&app, &token, "tdo", None, None).await;
    let (status, body): (_, Value) = parse_json(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"].as_array().unwrap().len(), 0);
    assert_eq!(cookie_seq(&body["cookie"]), 0);
    assert_eq!(body["cookie"], format!("{}:0", state.kv_generation()));
    assert_eq!(body["has_more"], false);
}

#[tokio::test]
async fn put_then_get_roundtrip() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "roundtrip@test.com");
    let token = mint_token(&user_id, "dev-1");
    let zero = pull_cookie(&app, &token, "tdo").await;

    let entries: Vec<Value> = (1..=3)
        .map(|i| {
            json!({
                "key": format!("key{i}"),
                "blob": BASE64.encode(format!("value{i}").as_bytes()),
            })
        })
        .collect();

    let put_resp = put_kv_batch(&app, &token, "tdo", json!({ "base_cookie":zero, "entries": entries })).await;
    let (put_status, put_body): (_, Value) = parse_json(put_resp).await;
    assert_eq!(put_status, StatusCode::OK);
    assert_eq!(put_body["results"].as_array().unwrap().len(), 3);

    let get_resp = get_kv(&app, &token, "tdo", None, None).await;
    let (get_status, get_body): (_, Value) = parse_json(get_resp).await;
    assert_eq!(get_status, StatusCode::OK);

    let returned_entries = get_body["entries"].as_array().unwrap();
    assert_eq!(returned_entries.len(), 3);

    // Verify blobs round-trip correctly
    for (i, entry) in returned_entries.iter().enumerate() {
        let blob = BASE64.decode(entry["blob"].as_str().unwrap()).unwrap();
        assert_eq!(blob, format!("value{}", i + 1).as_bytes());
    }
}

#[tokio::test]
async fn batch_put_multiple_entries() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "batch@test.com");
    let token = mint_token(&user_id, "dev-1");
    let zero = pull_cookie(&app, &token, "tdo").await;

    let entries: Vec<Value> = (1..=10)
        .map(|i| {
            json!({
                "key": format!("k{i}"),
                "blob": BASE64.encode(format!("v{i}").as_bytes()),
            })
        })
        .collect();

    let resp = put_kv_batch(&app, &token, "tdo", json!({ "base_cookie":zero, "entries": entries })).await;
    let (status, body): (_, Value) = parse_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 10);

    // Verify sequential seq numbers
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result["seq"], (i + 1) as i64);
    }
}

#[tokio::test]
async fn pagination_with_has_more() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "pagination@test.com");
    let token = mint_token(&user_id, "dev-1");
    let zero = pull_cookie(&app, &token, "tdo").await;

    // Insert 150 entries
    let entries: Vec<Value> = (1..=150)
        .map(|i| {
            json!({
                "key": format!("k{i:04}"),
                "blob": BASE64.encode(format!("v{i}").as_bytes()),
            })
        })
        .collect();

    let resp = put_kv_batch(&app, &token, "tdo", json!({ "base_cookie":zero, "entries": entries })).await;
    let (status, _): (_, Value) = parse_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // Page 1: limit=50, no cookie
    let resp = get_kv(&app, &token, "tdo", None, Some(50)).await;
    let (_, page1): (_, Value) = parse_json(resp).await;
    assert_eq!(page1["entries"].as_array().unwrap().len(), 50);
    assert_eq!(page1["has_more"], true);
    let cookie1 = page1["cookie"].as_str().unwrap();

    // Page 2: limit=50, cookie from page 1
    let resp = get_kv(&app, &token, "tdo", Some(cookie1), Some(50)).await;
    let (_, page2): (_, Value) = parse_json(resp).await;
    assert_eq!(page2["entries"].as_array().unwrap().len(), 50);
    assert_eq!(page2["has_more"], true);
    let cookie2 = page2["cookie"].as_str().unwrap();

    // Page 3: limit=50, cookie from page 2 — should be last page
    let resp = get_kv(&app, &token, "tdo", Some(cookie2), Some(50)).await;
    let (_, page3): (_, Value) = parse_json(resp).await;
    assert_eq!(page3["entries"].as_array().unwrap().len(), 50);
    assert_eq!(page3["has_more"], false);
}

#[tokio::test]
async fn legacy_single_put_is_rejected_without_mutation() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "single@test.com");
    let token = mint_token(&user_id, "dev-1");

    let raw_bytes = b"raw binary payload \x00\x01\x02";

    let put_resp = put_kv_single(&app, &token, "tdo", "mykey", raw_bytes).await;
    let (put_status, put_body): (_, Value) = parse_json(put_resp).await;
    assert_eq!(put_status, StatusCode::PRECONDITION_REQUIRED);
    assert!(put_body["error"].as_str().unwrap().contains("guarded sync v2"));
    assert_eq!(
        put_kv_single(&app, &token, "tdo", "snapshot", raw_bytes).await.status(),
        StatusCode::PRECONDITION_REQUIRED
    );

    // Rejected writes must leave the store empty.
    let get_resp = get_kv(&app, &token, "tdo", None, None).await;
    let (_, get_body): (_, Value) = parse_json(get_resp).await;
    let entries = get_body["entries"].as_array().unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn snapshot_returns_all() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "snapshot@test.com");
    let token = mint_token(&user_id, "dev-1");
    let zero = pull_cookie(&app, &token, "tdo").await;

    // Insert entries
    let entries: Vec<Value> = (1..=5)
        .map(|i| {
            json!({
                "key": format!("k{i}"),
                "blob": BASE64.encode(format!("v{i}").as_bytes()),
            })
        })
        .collect();
    put_kv_batch(&app, &token, "tdo", json!({ "base_cookie":zero, "entries": entries })).await;

    // Do a partial pull (get first 3)
    let resp = get_kv(&app, &token, "tdo", None, Some(3)).await;
    let (_, partial): (_, Value) = parse_json(resp).await;
    assert_eq!(partial["entries"].as_array().unwrap().len(), 3);
    assert_eq!(partial["has_more"], true);

    // Snapshot should still return everything
    let resp = get_kv_snapshot(&app, &token, "tdo", None).await;
    let (status, snap): (_, Value) = parse_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snap["entries"].as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn user_isolation() {
    let (app, state) = test_app();

    let user_a = create_test_user(&state, "alice@test.com");
    let token_a = mint_token(&user_a, "dev-a");
    let zero = pull_cookie(&app, &token_a, "tdo").await;

    let user_b = create_test_user(&state, "bob@test.com");
    let token_b = mint_token(&user_b, "dev-b");

    // User A writes data
    let entries = vec![json!({
        "key": "secret",
        "blob": BASE64.encode(b"alice-data"),
    })];
    put_kv_batch(&app, &token_a, "tdo", json!({ "base_cookie":zero, "entries": entries })).await;

    // User B should see nothing
    let resp = get_kv(&app, &token_b, "tdo", None, None).await;
    let (_, body): (_, Value) = parse_json(resp).await;
    assert_eq!(body["entries"].as_array().unwrap().len(), 0);

    // User A should see their data
    let resp = get_kv(&app, &token_a, "tdo", None, None).await;
    let (_, body): (_, Value) = parse_json(resp).await;
    assert_eq!(body["entries"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn tool_isolation() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "tools@test.com");
    let token = mint_token(&user_id, "dev-1");
    let zero = pull_cookie(&app, &token, "tdo").await;

    // Write to tool "tdo"
    let entries = vec![json!({
        "key": "task1",
        "blob": BASE64.encode(b"tdo-data"),
    })];
    put_kv_batch(&app, &token, "tdo", json!({ "base_cookie":zero, "entries": entries })).await;

    // Tool "other-tool" should see nothing
    let resp = get_kv(&app, &token, "other-tool", None, None).await;
    let (_, body): (_, Value) = parse_json(resp).await;
    assert_eq!(body["entries"].as_array().unwrap().len(), 0);

    // Tool "tdo" should see the entry
    let resp = get_kv(&app, &token, "tdo", None, None).await;
    let (_, body): (_, Value) = parse_json(resp).await;
    assert_eq!(body["entries"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn update_existing_key() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "update@test.com");
    let token = mint_token(&user_id, "dev-1");
    let zero = pull_cookie(&app, &token, "tdo").await;

    // Write initial value
    let entries = vec![json!({
        "key": "mykey",
        "blob": BASE64.encode(b"old-value"),
    })];
    put_kv_batch(&app, &token, "tdo", json!({ "base_cookie":zero, "entries": entries })).await;

    // Update same key
    let entries = vec![json!({
        "key": "mykey",
        "blob": BASE64.encode(b"new-value"),
    })];
    let (_, pulled): (_, Value) = parse_json(get_kv(&app, &token, "tdo", None, None).await).await;
    let resp = put_kv_batch(&app, &token, "tdo", json!({ "base_cookie":pulled["cookie"], "entries": entries })).await;
    let (_, put_body): (_, Value) = parse_json(resp).await;
    assert_eq!(put_body["results"][0]["seq"], 2); // New seq assigned

    // GET should return only 1 entry with new value
    let resp = get_kv(&app, &token, "tdo", None, None).await;
    let (_, body): (_, Value) = parse_json(resp).await;
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["key"], "mykey");

    let blob = BASE64.decode(entries[0]["blob"].as_str().unwrap()).unwrap();
    assert_eq!(blob, b"new-value");
    assert_eq!(entries[0]["seq"], 2);
}

#[tokio::test]
async fn auth_rejection() {
    let (app, _state) = test_app();

    // No auth header → 401
    let resp = get_kv_no_auth(&app, "tdo").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Bad token → 401
    let resp = get_kv_bad_auth(&app, "tdo").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

struct RouterTransport {
    runtime: tokio::runtime::Runtime,
    app: axum::Router,
    token: String,
    before_push: Option<(String, Value)>,
    pushes: usize,
    conflicts: usize,
    invalidations: usize,
    pull_cookies: Vec<Option<String>>,
    restart_before_push: Option<axum::Router>,
}

impl saku_sync::replica::Transport for RouterTransport {
    fn pull(&mut self, cookie: Option<&str>) -> Result<saku_sync::replica::PullPage, saku_sync::SyncError> {
        self.pull_cookies.push(cookie.map(str::to_owned));
        let (status, body): (_, Value) = self.runtime.block_on(async {
            parse_json(get_kv(&self.app, &self.token, "tdo", cookie, None).await).await
        });
        if status == StatusCode::GONE {
            self.invalidations += 1;
            return Err(saku_sync::SyncError::CursorInvalidated);
        }
        if status != StatusCode::OK {
            return Err(saku_sync::SyncError::Backend { message: format!("pull status {status}") });
        }
        Ok(saku_sync::replica::PullPage {
            entries: body["entries"].as_array().unwrap().iter().map(|entry| saku_sync::replica::RemoteEntry {
                key: entry["key"].as_str().unwrap().into(),
                blob: BASE64.decode(entry["blob"].as_str().unwrap()).unwrap(),
                deleted: entry["deleted"].as_bool().unwrap(),
            }).collect(),
            cookie: body["cookie"].as_str().unwrap().into(),
            has_more: body["has_more"].as_bool().unwrap(),
        })
    }

    fn push(&mut self, entries: &[(String, Vec<u8>)], base_cookie: &str) -> Result<Vec<String>, saku_sync::SyncError> {
        if let Some(app) = self.restart_before_push.take() {
            self.app = app;
        }
        if let Some((token, body)) = self.before_push.take() {
            let response = self.runtime.block_on(put_kv_batch(&self.app, &token, "tdo", body));
            assert_eq!(response.status(), StatusCode::OK);
        }
        self.pushes += 1;
        let body = json!({"base_cookie":base_cookie,"entries":entries.iter().map(|(key,blob)| json!({"key":key,"blob":BASE64.encode(blob)})).collect::<Vec<_>>()});
        let response = self.runtime.block_on(put_kv_batch(&self.app, &self.token, "tdo", body));
        if response.status() == StatusCode::GONE {
            self.invalidations += 1;
            return Err(saku_sync::SyncError::CursorInvalidated);
        }
        if response.status() == StatusCode::CONFLICT {
            self.conflicts += 1;
            return Err(saku_sync::SyncError::StaleWrite);
        }
        let (status, body): (_, Value) = self.runtime.block_on(parse_json(response));
        if status != StatusCode::OK {
            return Err(saku_sync::SyncError::Backend { message: format!("push status {status}: {body}") });
        }
        Ok(body["results"].as_array().unwrap().iter().map(|entry| entry["key"].as_str().unwrap().to_owned()).collect())
    }
}

#[test]
fn real_router_and_sqlite_resolve_conflicting_interleavings_without_losing_history() {
    use saku_sync::replica::sync_replica;
    use saku_tdo::{models::task::Task, storage::{StorageError, sqlite::SqliteStorage}};

    let key = saku_crypto::kdf::derive_master_key(b"router integration", &[7; 16]).unwrap();
    for (local_ms, expected) in [(300, "local edit"), (150, "remote edit")] {
        let (app, state) = test_app();
        let user_id = create_test_user(&state, "replica@test.com");
        let a = mint_token(&user_id, "a");
        let b = mint_token(&user_id, "b");
        let mut transport = RouterTransport {
            runtime: tokio::runtime::Runtime::new().unwrap(),
            app,
            token: a,
            before_push: None,
            pushes: 0,
            conflicts: 0,
            invalidations: 0,
            pull_cookies: vec![],
            restart_before_push: None,
        };
        let mut storage = SqliteStorage::open(":memory:").unwrap();
        let mut initial = Task { storage_key_suffix: "a".into(), task_number: 1, title: "initial".into(), ..Task::default() };
        initial.modified_at.wall_ms = 100;
        let initial = serde_json::to_value(initial).unwrap();
        storage.import_json(&json!({"version":9,"entries":{"task/a":initial}}).to_string()).unwrap();
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        storage.transaction::<_, StorageError>(|store, _| {
            let task = store.tasks.get_mut("task/a").unwrap();
            task.title = "local edit".into();
            task.modified_at.wall_ms = local_ms;
            Ok(())
        }).unwrap();
        let local: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        let mut remote = initial.clone();
        remote["title"] = json!("remote edit");
        remote["modified_at"]["wall_ms"] = json!(200);
        let blob = saku_crypto::encrypt_entry(&serde_json::to_vec(&remote).unwrap(), &key);
        let base = saku_sync::replica::Replica::prepare_sync(&mut storage, "server").unwrap().unwrap();
        transport.before_push = Some((b, json!({"base_cookie":base,"entries":[{"key":"task/a","blob":BASE64.encode(blob)}]})));
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        assert_eq!(transport.conflicts, 1);
        assert_eq!(storage.load().unwrap().tasks["task/a"].title, expected);
        assert!(storage.pending_changes().unwrap().is_empty());
        let history = storage.history_entries(Some("task/a")).unwrap();
        assert_eq!(history.len(), 2);
        assert!(history.iter().any(|entry| entry.source == "local" && entry.value == local["entries"]["task/a"]));
        assert!(history.iter().any(|entry| entry.source == "remote" && entry.value == remote));
        let (_, snapshot): (_, Value) = transport.runtime.block_on(async {
            parse_json(get_kv_snapshot(&transport.app, &transport.token, "tdo", None).await).await
        });
        let blob = BASE64.decode(snapshot["entries"][0]["blob"].as_str().unwrap()).unwrap();
        let server: Value = serde_json::from_slice(&saku_crypto::decrypt_entry(&blob, &key).unwrap()).unwrap();
        assert_eq!(server["title"], expected);
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        assert_eq!(storage.load().unwrap().tasks["task/a"].title, expected);
        assert_eq!(storage.history_entries(Some("task/a")).unwrap(), history);
        assert!(storage.pending_changes().unwrap().is_empty());
    }
}

#[test]
fn sqlite_and_router_recover_restored_server_and_ordinary_restarts() {
    use saku_sync::replica::{Replica, sync_replica};
    use saku_tdo::{models::task::Task, storage::{StorageError, sqlite::SqliteStorage}};
    let key = saku_crypto::kdf::derive_master_key(b"restore integration", &[4; 16]).unwrap();
    for (local_ms, expected) in [(300, "local edit"), (150, "remote edit")] {
        let (app, state) = test_app();
        let user = create_test_user(&state, "restore@test.com");
        let token = mint_token(&user, "a");
        let backup = Snapshot::capture(&state);
        let mut transport = RouterTransport {
            runtime: tokio::runtime::Runtime::new().unwrap(),
            app, token: token.clone(), before_push: None, pushes: 0, conflicts: 0,
            invalidations: 0, pull_cookies: vec![], restart_before_push: None,
        };
        let make = |suffix: &str, number: u64, title: &str| {
            let mut task = Task { storage_key_suffix: suffix.into(), task_number: number, title: title.into(), ..Task::default() };
            task.modified_at.wall_ms = 100;
            serde_json::to_value(task).unwrap()
        };
        let initial = make("a", 1, "initial");
        let missing = make("missing", 2, "clean missing");
        let mut tombstone = make("tomb", 3, "deleted");
        tombstone["deleted_at"] = json!("2026-01-01T00:00:00Z");
        let mut storage = SqliteStorage::open(":memory:").unwrap();
        storage.import_json(&json!({"version":9,"entries":{"task/a":initial,"task/missing":missing,"task/tomb":tombstone}}).to_string()).unwrap();
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        let old_cookie = storage.prepare_sync("server").unwrap().unwrap();
        assert_eq!(old_cookie.split_once(':').unwrap().1, "3");
        storage.transaction::<_, StorageError>(|store, _| {
            let task = store.tasks.get_mut("task/a").unwrap();
            task.title = "local edit".into();
            task.modified_at.wall_ms = local_ms;
            Ok(())
        }).unwrap();
        let local: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        let mut remote = initial.clone();
        remote["title"] = json!("remote edit");
        remote["modified_at"]["wall_ms"] = json!(200);
        let restored = backup.state(&state);
        let restored_app = saku_server::build_router(restored.clone());
        let zero = transport.runtime.block_on(pull_cookie(&restored_app, &token, "tdo"));
        let branch = [
            ("task/x", make("x", 4, "x")),
            ("task/y", make("y", 5, "y")),
            ("task/a", remote.clone()),
        ];
        let body = json!({"base_cookie":zero,"entries":branch.iter().map(|(name,value)| json!({
            "key":name,"blob":BASE64.encode(saku_crypto::encrypt_entry(&serde_json::to_vec(value).unwrap(), &key))
        })).collect::<Vec<_>>()});
        assert_eq!(transport.runtime.block_on(put_kv_batch(&restored_app, &token, "tdo", body)).status(), StatusCode::OK);
        let restored_cookie = transport.runtime.block_on(pull_cookie(&restored_app, &token, "tdo"));
        assert_eq!(restored_cookie.split_once(':').unwrap().1, "3");
        transport.app = restored_app;
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        assert_eq!(transport.invalidations, 1);
        assert_eq!(storage.prepare_sync("server").unwrap().as_deref(), Some(restored_cookie.as_str()));
        assert_eq!(storage.load().unwrap().tasks["task/a"].title, expected);
        assert!(storage.pending_changes().unwrap().is_empty());
        let history = storage.history_entries(Some("task/a")).unwrap();
        assert_eq!(history.len(), 2);
        assert!(history.iter().any(|entry| entry.source == "local" && entry.value == local["entries"]["task/a"]));
        assert!(history.iter().any(|entry| entry.source == "remote" && entry.value == remote));
        let (_, snapshot): (_, Value) = transport.runtime.block_on(async {
            parse_json(get_kv_snapshot(&transport.app, &token, "tdo", None).await).await
        });
        let values: std::collections::HashMap<_, Value> = snapshot["entries"].as_array().unwrap().iter().map(|entry| {
            let ciphertext = BASE64.decode(entry["blob"].as_str().unwrap()).unwrap();
            let value = serde_json::from_slice(&saku_crypto::decrypt_entry(&ciphertext, &key).unwrap()).unwrap();
            (entry["key"].as_str().unwrap().to_owned(), value)
        }).collect();
        assert_eq!(values["task/a"]["title"], expected);
        assert_eq!(values["task/missing"], missing);
        assert_eq!(values["task/tomb"], tombstone);
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        assert_eq!(storage.history_entries(Some("task/a")).unwrap(), history);
        assert_eq!(transport.invalidations, 1);
        let restart_snapshot = Snapshot::capture(&restored);
        let restarted = restart_snapshot.state(&restored);
        transport.app = saku_server::build_router(restarted.clone());
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        assert_eq!(transport.invalidations, 2);
        assert_eq!(storage.history_entries(Some("task/a")).unwrap(), history);
        assert!(storage.pending_changes().unwrap().is_empty());
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        assert_eq!(transport.invalidations, 2);
        storage.transaction::<_, StorageError>(|store, _| {
            let task = store.tasks.get_mut("task/a").unwrap();
            task.title = "after restart".into();
            task.modified_at.wall_ms = 500;
            Ok(())
        }).unwrap();
        let restart_snapshot2 = Snapshot::capture(&restarted);
        let restarted2 = restart_snapshot2.state(&restarted);
        transport.restart_before_push = Some(saku_server::build_router(restarted2));
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        assert_eq!(transport.invalidations, 3);
        assert_eq!(storage.load().unwrap().tasks["task/a"].title, "after restart");
        assert!(storage.pending_changes().unwrap().is_empty());
        drop(transport);
    }
}

#[test]
fn empty_server_restart_republishes_clean_local_entries() {
    use saku_sync::replica::{Replica, sync_replica};
    use saku_tdo::{models::task::Task, storage::sqlite::SqliteStorage};
    for finish_pull in [false, true] {
        let (app, state) = test_app();
        let user = create_test_user(&state, "empty-restore@test.com");
        let token = mint_token(&user, "a");
        let mut transport = RouterTransport {
            runtime: tokio::runtime::Runtime::new().unwrap(),
            app, token, before_push: None, pushes: 0, conflicts: 0, invalidations: 0,
            pull_cookies: vec![], restart_before_push: None,
        };
        let key = saku_crypto::kdf::derive_master_key(b"empty restore", &[3; 16]).unwrap();
        let task = Task { storage_key_suffix: "a".into(), task_number: 1, title: "clean".into(), ..Task::default() };
        let mut storage = SqliteStorage::open(":memory:").unwrap();
        storage.import_json(&json!({"version":9,"entries":{"task/a":task}}).to_string()).unwrap();
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        if finish_pull {
            sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        }
        let before_cookie = storage.prepare_sync("server").unwrap().unwrap();
        assert_eq!(before_cookie.split_once(':').unwrap().1, if finish_pull { "1" } else { "0" });
        assert!(storage.pending_changes().unwrap().is_empty());
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        saku_server::db::migrations::run_migrations(&conn).unwrap();
        let empty = saku_server::state::AppState::new(conn, state.inner.storage.clone(), state.inner.config.clone());
        transport.app = saku_server::build_router(empty);
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        assert_eq!(transport.invalidations, 1);
        assert_eq!(storage.prepare_sync("server").unwrap().unwrap().split_once(':').unwrap().1, "0");
        assert!(storage.pending_changes().unwrap().is_empty());
        let (_, snapshot): (_, Value) = transport.runtime.block_on(async {
            parse_json(get_kv_snapshot(&transport.app, &transport.token, "tdo", None).await).await
        });
        assert_eq!(snapshot["entries"].as_array().unwrap().len(), 1);
        let blob = BASE64.decode(snapshot["entries"][0]["blob"].as_str().unwrap()).unwrap();
        let value: Value = serde_json::from_slice(&saku_crypto::decrypt_entry(&blob, &key).unwrap()).unwrap();
        assert_eq!(value["title"], "clean");
        sync_replica(&mut storage, &mut transport, &key, "server").unwrap();
        assert_eq!(transport.invalidations, 1);
        assert!(storage.history_entries(None).unwrap().is_empty());
    }
}
