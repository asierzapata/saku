# Document-Based Sync

## Overview

The Saku sync system now supports **document-based synchronization** in addition to traditional file-based sync. This enables platforms without filesystem access (like iOS apps) to sync JSON data structures directly, without needing to manage files.

## Key Concepts

### File-Based Sync (Traditional)
- Syncs entire JSON files (e.g., `store.json`)
- Tracks files by filesystem paths
- Uses file I/O for reading and writing
- Best for desktop applications and CLI tools

### Document-Based Sync (New)
- Syncs individual JSON documents by ID
- No filesystem required
- Direct JSON operations (get/put/list)
- Perfect for mobile apps (iOS, Android) and web applications

## API Overview

### DocumentStore

The `DocumentStore` provides a high-level API for working with JSON documents:

```rust
use saku_sync::document::DocumentStore;
use saku_sync::backend::local_fs::LocalFsSyncBackend;
use serde_json::json;

// Create a document store
let backend = LocalFsSyncBackend::new(std::path::Path::new("/tmp/sync"));
let mut store = DocumentStore::new("my-app", backend, b"passphrase".to_vec());

// Put a document
let user_data = json!({
    "id": "user-123",
    "name": "Alice",
    "email": "alice@example.com",
    "modified_at": {
        "wall_ms": 1708732800000,
        "lamport": 1,
        "device_id": "device-a"
    }
});
store.put_document("user-123", &user_data)?;

// Get a document
let doc = store.get_document("user-123")?.unwrap();
println!("User: {}", doc["name"]);

// List all documents
let doc_ids = store.list_documents()?;
for id in doc_ids {
    println!("Document: {}", id);
}

// Delete a document (logical delete with tombstone)
store.delete_document("user-123")?;
```

### Conflict Resolution

Documents use **Last-Writer-Wins (LWW)** conflict resolution based on the `modified_at` timestamp:

```rust
{
    "modified_at": {
        "wall_ms": 1708732800000,    // Wall clock milliseconds (primary)
        "lamport": 1,                 // Lamport counter (secondary, prevents clock rollback)
        "device_id": "device-a"       // Device ID (tertiary, deterministic tiebreaker)
    }
}
```

When two versions of a document conflict:
1. The version with the higher `wall_ms` wins
2. If `wall_ms` is equal, the higher `lamport` counter wins
3. If both are equal, the lexicographically higher `device_id` wins

## iOS Integration Example

Here's how an iOS app could use document-based sync:

```swift
// Swift pseudo-code showing conceptual usage
import SakuSync // hypothetical Swift bindings

class TaskManager {
    let documentStore: DocumentStore
    
    init(serverUrl: String, passphrase: String) {
        let backend = ServerSyncBackend(serverUrl: serverUrl)
        self.documentStore = DocumentStore(
            tool: "my-ios-app",
            backend: backend,
            passphrase: passphrase
        )
    }
    
    func saveTask(_ task: Task) throws {
        let json = task.toJSON() // includes modified_at with HybridTimestamp
        try documentStore.putDocument(task.id, json)
    }
    
    func loadTask(_ taskId: String) throws -> Task? {
        guard let json = try documentStore.getDocument(taskId) else {
            return nil
        }
        return Task.fromJSON(json)
    }
    
    func syncAllTasks() throws -> SyncResult {
        let localTasks = getAllLocalTasks()
        let localDocs = localTasks.map { ($0.id, $0.toJSON()) }
        return try documentStore.syncAllDocuments(localDocs)
    }
}
```

## Backend Support

### SyncBackend Trait Extensions

The `SyncBackend` trait now includes document-based methods:

```rust
pub trait SyncBackend {
    // File-based methods (existing)
    fn fetch(&self, tool: &str, path: &str) -> Result<Vec<u8>, SyncError>;
    fn push(&self, tool: &str, path: &str, data: &[u8]) -> Result<(), SyncError>;
    
    // Document-based methods (new)
    fn fetch_document(&self, tool: &str, document_id: &str) -> Result<Vec<u8>, SyncError>;
    fn push_document(&self, tool: &str, document_id: &str, data: &[u8]) -> Result<(), SyncError>;
    fn list_documents(&self, tool: &str) -> Result<Vec<String>, SyncError>;
}
```

### Server API Endpoints

The sync server now provides a document listing endpoint:

```
GET /api/v1/sync/:tool/list
Authorization: Bearer <access_token>

Response:
{
    "documents": ["doc-1", "doc-2", "doc-3"]
}
```

This endpoint lists all document IDs for a given tool, enabling clients to discover what documents exist remotely.

## Comparison: File-Based vs Document-Based

| Feature | File-Based Sync | Document-Based Sync |
|---------|----------------|---------------------|
| **Target Platform** | Desktop CLI, file-based apps | Mobile apps, web apps, platforms without FS |
| **Sync Unit** | Entire files (e.g., `store.json`) | Individual JSON documents by ID |
| **File System** | Required | Not required |
| **Conflict Resolution** | Entity-level LWW within files | Document-level LWW |
| **API** | `SyncEngine::sync()` | `DocumentStore::put/get/list()` |
| **Merkle Tree** | Per-file hashing | Per-document hashing (conceptual) |

## Migration Path

Existing file-based sync continues to work unchanged. Applications can:

1. **Use file-based sync only** - No changes needed
2. **Use document-based sync only** - Ideal for new mobile apps
3. **Use both** - Desktop CLI uses files, mobile app uses documents, same backend

## Implementation Details

### Encryption

Both file-based and document-based sync use the same encryption:
- Passphrase → Argon2id → Master Key (KEK)
- Per-document/file random Data Encryption Key (DEK)
- XChaCha20-Poly1305 authenticated encryption
- E2E encrypted - server never sees plaintext

### Storage

Documents are stored as encrypted blobs in the same backend storage:
- Local filesystem: `{root}/{tool}/{document_id}.enc`
- Server/S3: `{user_id}/{tool}/{document_id}.enc`

The `.enc` suffix is used for both files and documents, maintaining a unified storage format.

## Testing

Run document sync tests:

```bash
cargo test -p saku-sync --no-default-features document
```

## Future Enhancements

1. **Batch operations**: `put_documents()` and `get_documents()` for efficiency
2. **Delta sync**: Sync only changed fields within documents
3. **Partial document sync**: Sync specific JSON paths
4. **Real-time sync**: WebSocket-based push notifications for changes
5. **Offline-first**: Local document cache with background sync
