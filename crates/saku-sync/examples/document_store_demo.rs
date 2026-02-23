// Example demonstrating document-based sync for a simple task manager
// that doesn't need file system access.
//
// Run with:
//   cargo run -p saku-sync --example document_store_demo --no-default-features

use saku_sync::backend::local_fs::LocalFsSyncBackend;
use saku_sync::document::DocumentStore;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Document-Based Sync Example ===\n");

    // Set up a temporary backend directory
    let temp_dir = tempfile::tempdir()?;
    let backend_path = temp_dir.path();
    println!("Using backend at: {}\n", backend_path.display());

    // Create a document store for our "task-app"
    let backend = LocalFsSyncBackend::new(backend_path);
    let mut store = DocumentStore::new("task-app", backend, b"example-passphrase".to_vec());

    // Create some task documents with timestamps
    println!("Creating task documents...");
    
    let task1 = json!({
        "id": "task-001",
        "title": "Buy groceries",
        "status": "pending",
        "modified_at": {
            "wall_ms": 1708732800000_i64,
            "lamport": 1,
            "device_id": "device-a"
        }
    });

    let task2 = json!({
        "id": "task-002", 
        "title": "Write documentation",
        "status": "in_progress",
        "modified_at": {
            "wall_ms": 1708732900000_i64,
            "lamport": 1,
            "device_id": "device-a"
        }
    });

    let task3 = json!({
        "id": "task-003",
        "title": "Review pull request",
        "status": "pending",
        "modified_at": {
            "wall_ms": 1708733000000_i64,
            "lamport": 1,
            "device_id": "device-a"
        }
    });

    // Put documents to the store
    store.put_document("task-001", &task1)?;
    store.put_document("task-002", &task2)?;
    store.put_document("task-003", &task3)?;
    println!("✓ Created 3 task documents\n");

    // List all documents
    println!("Listing all documents:");
    let doc_ids = store.list_documents()?;
    for id in &doc_ids {
        println!("  - {}", id);
    }
    println!();

    // Retrieve a specific document
    println!("Fetching task-001:");
    if let Some(task) = store.get_document("task-001")? {
        println!("  Title: {}", task["title"]);
        println!("  Status: {}", task["status"]);
    }
    println!();

    // Simulate a document from another device with a newer timestamp
    println!("Simulating conflict with newer version from device-b...");
    let task1_updated = json!({
        "id": "task-001",
        "title": "Buy groceries and prepare dinner",
        "status": "completed",
        "modified_at": {
            "wall_ms": 1708733100000_i64,  // Newer timestamp
            "lamport": 2,
            "device_id": "device-b"
        }
    });

    store.put_document("task-001", &task1_updated)?;
    println!("✓ Updated task-001 with newer version\n");

    // Verify the update
    if let Some(task) = store.get_document("task-001")? {
        println!("After conflict resolution:");
        println!("  Title: {}", task["title"]);
        println!("  Status: {}", task["status"]);
        println!("  Device: {}", task["modified_at"]["device_id"]);
    }
    println!();

    // Delete a document (logical delete with tombstone)
    println!("Deleting task-003...");
    store.delete_document("task-003")?;
    println!("✓ Document marked as deleted\n");

    // Verify tombstone
    if let Some(doc) = store.get_document("task-003")? {
        if doc.get("_deleted").and_then(|v| v.as_bool()).unwrap_or(false) {
            println!("  Document has deletion tombstone: {:?}", doc);
        }
    }

    println!("\n=== Example complete! ===");
    println!("This example demonstrates:");
    println!("  • Creating documents without filesystem access");
    println!("  • Listing documents by ID");
    println!("  • Retrieving specific documents");
    println!("  • Conflict resolution using timestamps");
    println!("  • Logical deletion with tombstones");
    println!("\nPerfect for iOS apps that don't need file system operations!");

    Ok(())
}
