# saku-sync

Synchronization engine for Saku tools, supporting both file-based and document-based sync.

## Features

### File-Based Sync
- Traditional file synchronization for desktop applications
- Tracks entire JSON files (e.g., `store.json`)
- Entity-level Last-Writer-Wins (LWW) conflict resolution
- Merkle tree optimization for efficient change detection

### Document-Based Sync (New!)
- Direct JSON document synchronization without filesystem access
- Perfect for iOS/Android apps and web applications
- Simple get/put/list/delete operations
- Same encryption and conflict resolution as file-based sync

## Usage

### File-Based Sync (Traditional)

```rust
use saku_sync::{SyncEngine, SyncConfig, TrackedFile};
use saku_sync::backend::local_fs::LocalFsSyncBackend;

let backend = LocalFsSyncBackend::new(std::path::Path::new("/tmp/remote"));
let config = SyncConfig {
    db_path: std::path::PathBuf::from("/tmp/sync.db"),
    passphrase: b"my-passphrase".to_vec(),
    tracked_files: vec![TrackedFile {
        file_key: "tdo/store.json".to_string(),
        tool: "tdo".to_string(),
        relative_path: "store.json".to_string(),
        local_path: std::path::PathBuf::from("/home/user/.local/share/tdo/store.json"),
    }],
};

let mut engine = SyncEngine::new(config, backend)?;
let outcome = engine.sync()?;
```

### Document-Based Sync (New)

```rust
use saku_sync::document::DocumentStore;
use saku_sync::backend::local_fs::LocalFsSyncBackend;
use serde_json::json;

// Create a document store
let backend = LocalFsSyncBackend::new(std::path::Path::new("/tmp/sync"));
let mut store = DocumentStore::new("my-app", backend, b"passphrase".to_vec());

// Put a document
let doc = json!({
    "id": "user-123",
    "name": "Alice",
    "modified_at": {
        "wall_ms": 1708732800000,
        "lamport": 1,
        "device_id": "device-a"
    }
});
store.put_document("user-123", &doc)?;

// Get a document
let retrieved = store.get_document("user-123")?.unwrap();

// List all documents
let doc_ids = store.list_documents()?;

// Delete a document
store.delete_document("user-123")?;
```

## Architecture

### Encryption
- Passphrase → Argon2id → Master Key (KEK)
- Per-document/file random Data Encryption Key (DEK)  
- XChaCha20-Poly1305 authenticated encryption
- End-to-end encrypted - server never sees plaintext

### Conflict Resolution
Last-Writer-Wins (LWW) based on HybridTimestamp:
```rust
{
    "modified_at": {
        "wall_ms": 1708732800000,    // Primary: wall clock
        "lamport": 1,                 // Secondary: prevents clock rollback
        "device_id": "device-a"       // Tertiary: deterministic tiebreaker
    }
}
```

### Backends
- **LocalFsSyncBackend**: Local directory (for testing)
- **ServerSyncBackend**: Remote saku-server (production)

## Examples

See `examples/document_store_demo.rs` for a complete working example:

```bash
cargo run -p saku-sync --example document_store_demo --no-default-features
```

## Testing

Run all tests:
```bash
cargo test -p saku-sync --no-default-features
```

Run only document sync tests:
```bash
cargo test -p saku-sync --no-default-features document
```

## Documentation

See [documentation/document-sync.md](../../documentation/document-sync.md) for detailed documentation including iOS integration examples.

## License

AGPL-3.0-or-later
