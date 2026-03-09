mod common;

use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{json, Value};

use common::*;

#[tokio::test]
async fn empty_store_pull() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "empty@test.com");
    let token = mint_token(&user_id, "dev-1");

    let resp = get_kv(&app, &token, "tdo", None, None).await;
    let (status, body): (_, Value) = parse_json(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"].as_array().unwrap().len(), 0);
    assert_eq!(body["cookie"], "0");
    assert_eq!(body["has_more"], false);
}

#[tokio::test]
async fn put_then_get_roundtrip() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "roundtrip@test.com");
    let token = mint_token(&user_id, "dev-1");

    let entries: Vec<Value> = (1..=3)
        .map(|i| {
            json!({
                "key": format!("key{i}"),
                "blob": BASE64.encode(format!("value{i}").as_bytes()),
            })
        })
        .collect();

    let put_resp = put_kv_batch(&app, &token, "tdo", json!({ "entries": entries })).await;
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

    let entries: Vec<Value> = (1..=10)
        .map(|i| {
            json!({
                "key": format!("k{i}"),
                "blob": BASE64.encode(format!("v{i}").as_bytes()),
            })
        })
        .collect();

    let resp = put_kv_batch(&app, &token, "tdo", json!({ "entries": entries })).await;
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

    // Insert 150 entries
    let entries: Vec<Value> = (1..=150)
        .map(|i| {
            json!({
                "key": format!("k{i:04}"),
                "blob": BASE64.encode(format!("v{i}").as_bytes()),
            })
        })
        .collect();

    let resp = put_kv_batch(&app, &token, "tdo", json!({ "entries": entries })).await;
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
async fn single_key_put_and_retrieval() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "single@test.com");
    let token = mint_token(&user_id, "dev-1");

    let raw_bytes = b"raw binary payload \x00\x01\x02";

    let put_resp = put_kv_single(&app, &token, "tdo", "mykey", raw_bytes).await;
    let (put_status, put_body): (_, Value) = parse_json(put_resp).await;
    assert_eq!(put_status, StatusCode::OK);
    assert_eq!(put_body["seq"], 1);

    // Verify it appears in GET
    let get_resp = get_kv(&app, &token, "tdo", None, None).await;
    let (_, get_body): (_, Value) = parse_json(get_resp).await;
    let entries = get_body["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["key"], "mykey");

    let blob = BASE64.decode(entries[0]["blob"].as_str().unwrap()).unwrap();
    assert_eq!(blob, raw_bytes);
}

#[tokio::test]
async fn snapshot_returns_all() {
    let (app, state) = test_app();
    let user_id = create_test_user(&state, "snapshot@test.com");
    let token = mint_token(&user_id, "dev-1");

    // Insert entries
    let entries: Vec<Value> = (1..=5)
        .map(|i| {
            json!({
                "key": format!("k{i}"),
                "blob": BASE64.encode(format!("v{i}").as_bytes()),
            })
        })
        .collect();
    put_kv_batch(&app, &token, "tdo", json!({ "entries": entries })).await;

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

    let user_b = create_test_user(&state, "bob@test.com");
    let token_b = mint_token(&user_b, "dev-b");

    // User A writes data
    let entries = vec![json!({
        "key": "secret",
        "blob": BASE64.encode(b"alice-data"),
    })];
    put_kv_batch(&app, &token_a, "tdo", json!({ "entries": entries })).await;

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

    // Write to tool "tdo"
    let entries = vec![json!({
        "key": "task1",
        "blob": BASE64.encode(b"tdo-data"),
    })];
    put_kv_batch(&app, &token, "tdo", json!({ "entries": entries })).await;

    // Tool "nte" should see nothing
    let resp = get_kv(&app, &token, "nte", None, None).await;
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

    // Write initial value
    let entries = vec![json!({
        "key": "mykey",
        "blob": BASE64.encode(b"old-value"),
    })];
    put_kv_batch(&app, &token, "tdo", json!({ "entries": entries })).await;

    // Update same key
    let entries = vec![json!({
        "key": "mykey",
        "blob": BASE64.encode(b"new-value"),
    })];
    let resp = put_kv_batch(&app, &token, "tdo", json!({ "entries": entries })).await;
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
