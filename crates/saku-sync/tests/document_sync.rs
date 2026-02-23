// Integration tests for document-based sync
use saku_sync::backend::local_fs::LocalFsSyncBackend;
use saku_sync::document::DocumentStore;
use serde_json::json;

#[test]
fn two_device_document_sync() {
    let remote_dir = tempfile::tempdir().unwrap();
    let backend_path = remote_dir.path();

    // Device A: Create and push a document
    let backend_a = LocalFsSyncBackend::new(backend_path);
    let mut store_a = DocumentStore::new("app", backend_a, b"shared-pass".to_vec());

    let doc_a = json!({
        "id": "doc-1",
        "content": "Created on device A",
        "modified_at": {
            "wall_ms": 1000_i64,
            "lamport": 1,
            "device_id": "device-a"
        }
    });

    store_a.put_document("doc-1", &doc_a).unwrap();

    // Device B: Fetch the same document
    let backend_b = LocalFsSyncBackend::new(backend_path);
    let store_b = DocumentStore::new("app", backend_b, b"shared-pass".to_vec());

    let fetched = store_b.get_document("doc-1").unwrap().unwrap();
    assert_eq!(fetched["content"], "Created on device A");
    assert_eq!(fetched["modified_at"]["device_id"], "device-a");
}

#[test]
fn document_conflict_resolution() {
    let remote_dir = tempfile::tempdir().unwrap();
    let backend_path = remote_dir.path();

    // Device A: Create a document
    let backend_a = LocalFsSyncBackend::new(backend_path);
    let mut store_a = DocumentStore::new("app", backend_a, b"shared-pass".to_vec());

    let doc_old = json!({
        "id": "task-1",
        "title": "Old version",
        "modified_at": {
            "wall_ms": 1000_i64,
            "lamport": 1,
            "device_id": "device-a"
        }
    });

    store_a.put_document("task-1", &doc_old).unwrap();

    // Device B: Update with a newer timestamp (should win)
    let backend_b = LocalFsSyncBackend::new(backend_path);
    let mut store_b = DocumentStore::new("app", backend_b, b"shared-pass".to_vec());

    let doc_new = json!({
        "id": "task-1",
        "title": "New version",
        "modified_at": {
            "wall_ms": 2000_i64,  // Newer
            "lamport": 1,
            "device_id": "device-b"
        }
    });

    store_b.put_document("task-1", &doc_new).unwrap();

    // Device A: Fetch again - should see newer version
    let backend_a2 = LocalFsSyncBackend::new(backend_path);
    let store_a2 = DocumentStore::new("app", backend_a2, b"shared-pass".to_vec());

    let final_doc = store_a2.get_document("task-1").unwrap().unwrap();
    assert_eq!(final_doc["title"], "New version");
    assert_eq!(final_doc["modified_at"]["device_id"], "device-b");
}

#[test]
fn list_multiple_documents() {
    let remote_dir = tempfile::tempdir().unwrap();
    let backend = LocalFsSyncBackend::new(remote_dir.path());
    let mut store = DocumentStore::new("notes-app", backend, b"pass".to_vec());

    // Create multiple documents
    for i in 1..=5 {
        let doc = json!({
            "id": format!("note-{}", i),
            "title": format!("Note {}", i),
            "content": format!("This is note number {}", i)
        });
        store
            .put_document(&format!("note-{}", i), &doc)
            .unwrap();
    }

    // List all documents
    let docs = store.list_documents().unwrap();
    assert_eq!(docs.len(), 5);

    // Verify we can fetch each one
    for i in 1..=5 {
        let doc_id = format!("note-{}", i);
        let doc = store.get_document(&doc_id).unwrap();
        assert!(doc.is_some(), "Document {} should exist", doc_id);
    }
}

#[test]
fn tombstone_deletion() {
    let remote_dir = tempfile::tempdir().unwrap();
    let backend = LocalFsSyncBackend::new(remote_dir.path());
    let mut store = DocumentStore::new("app", backend, b"pass".to_vec());

    // Create a document
    let doc = json!({
        "id": "doc-1",
        "data": "some data"
    });
    store.put_document("doc-1", &doc).unwrap();

    // Delete it
    store.delete_document("doc-1").unwrap();

    // Should still exist but have deletion marker
    let deleted = store.get_document("doc-1").unwrap().unwrap();
    assert_eq!(deleted.get("_deleted").unwrap(), true);
    assert!(deleted.get("_deleted_at").is_some());
}

#[test]
fn encryption_prevents_tampering() {
    let remote_dir = tempfile::tempdir().unwrap();
    let backend = LocalFsSyncBackend::new(remote_dir.path());
    let mut store = DocumentStore::new("secure-app", backend, b"correct-pass".to_vec());

    // Put a document with correct passphrase
    let doc = json!({"secret": "sensitive data"});
    store.put_document("secret-doc", &doc).unwrap();

    // Try to read with wrong passphrase (should fail)
    let backend2 = LocalFsSyncBackend::new(remote_dir.path());
    let store2 = DocumentStore::new("secure-app", backend2, b"wrong-pass".to_vec());

    let result = store2.get_document("secret-doc");
    assert!(
        result.is_err(),
        "Should fail to decrypt with wrong passphrase"
    );
}
