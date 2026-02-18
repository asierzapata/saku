# Saku Sync — Architecture Research

## Crate Structure

```
crates/
  saku-storage/     # unified storage traits (AppStorage + AppFiles)
  saku-crypto/      # E2E encryption layer
  saku-sync/        # sync engine (queue, Merkle, S3)
  saku-server/      # auth/coordination server
  tdo/              # existing
  nte/              # existing
```

---

## 1. Storage Traits (saku-storage)

The abstraction separates two concerns:

```rust
// AppStorage: versioned, structured JSON (tdo, hbt, cal...)
pub trait AppStorage: Serialize + DeserializeOwned {
    fn tool_name() -> &'static str;
    fn file_path() -> PathBuf;
}

// AppFiles: unstructured text files (nte markdown notes)
pub trait AppFiles {
    fn file_root() -> PathBuf;
    fn supported_extensions() -> &'static [&'static str]; // ["md"]
}
```

The sync crate operates on these two abstractions — it doesn't care about the internal structure of `Store` or individual tasks.

---

## 2. S3 Client — `opendal`

**Winner over `object_store`, `aws-sdk-s3`, and `rust-s3`.**

The decisive factor: `BlockingOperator` means no tokio runtime needed (fits the current sync CLI architecture). It has a dedicated `R2` service that handles path-style URLs and `region: "auto"` automatically. Also has a `LocalFs` backend for dev/offline testing with zero code changes.

```toml
opendal = { version = "0.50", default-features = false, features = [
    "services-s3",   # R2, MinIO, S3
    "services-fs",   # local filesystem for dev/testing
] }
```

### Alternatives considered

| Crate          | Verdict                                                            |
| -------------- | ------------------------------------------------------------------ |
| `opendal`      | **Recommended** — BlockingOperator, dedicated R2, LocalFs backend  |
| `object_store` | Runner-up — async-only, no sync API, established in data ecosystem |
| `aws-sdk-s3`   | Avoid — heavy deps, verbose API, overkill for this use case        |
| `s3` (rust-s3) | Avoid — erratic release history, community-only maintenance        |

---

## 3. E2E Encryption (saku-crypto)

**Stack: Argon2id → XChaCha20Poly1305, with KEK + per-file DEK pattern.**

```
user passphrase
  → Argon2id (m=64MB, t=3, p=4, salt=16B stored in bucket)
    → master_key (KEK, 32B, memory-only, zeroized on drop)
      → per-file random DEK (32B, OsRng)
        → XChaCha20Poly1305(DEK, nonce=24B random)
          → encrypted file content
      → encrypted_DEK stored in file header
```

### What gets encrypted

Everything that goes to S3 is encrypted client-side before upload:

- All app storage files (`store.json` per tool)
- All app files (`.md` notes)
- The `merkle.json` manifest itself — otherwise the bucket reveals file names and structure to anyone with access

Optionally, S3 object keys (the path strings like `tdo/store.json`) can be hashed so even the key names are opaque. This adds operational complexity but maximises privacy.

The **Argon2 salt** is the only thing stored in plaintext in the bucket. It is not secret — just unique per vault.

### How encryption interacts with sync

The sync engine works entirely in plaintext locally. Encryption only happens at the S3 boundary:

- **Before upload**: plaintext → encrypt → upload ciphertext
- **After download**: download ciphertext → decrypt → plaintext

Merkle hashes are always computed on **plaintext content**, not on the encrypted blob. Because every upload produces a different ciphertext (random DEK + nonce each time), hashing the encrypted blob would be useless for change detection. Both the local hash and the remote manifest store plaintext SHA-256s.

### File format (117 bytes overhead per file)

```
[SAKU 4B][v1 1B][kek_salt 16B][dek_nonce 24B][enc_dek 48B][file_nonce 24B][ciphertext + tag 16B]
```

### Why XChaCha20 over ChaCha20

The 192-bit nonce eliminates birthday-bound nonce collisions even with random generation across millions of files. Safe without hardware AES-NI — important for older ARM devices.

### Key design decisions

- Passphrase → Argon2id → KEK. The KEK never leaves the device.
- Random per-file DEK encrypted by KEK and stored in the file header.
- Rotating DEKs without re-deriving from the passphrase is free — just re-encrypt the DEK header.
- `zeroize` on all key material. KEK lives in memory only for the session duration.
- OS keychain (`keyring` crate) available as an opt-in convenience — user can decline for pure memory-only mode.
- The S3 bucket becomes a dumb blob store. The server never sees plaintext.

### Crates

```toml
argon2           = { version = "0.5", features = ["std"] }
chacha20poly1305 = "0.10"
rand             = "0.8"   # OsRng only — never thread_rng for crypto
zeroize          = { version = "1.7", features = ["derive"] }
keyring          = "2.3"   # optional OS keychain
```

### Argon2id parameters

| Parameter   | Value             | Notes                                             |
| ----------- | ----------------- | ------------------------------------------------- |
| Algorithm   | Argon2id          | Best hybrid (side-channel + GPU resistance)       |
| Memory      | 65536 KiB (64 MB) | OWASP minimum; 256 MB better if affordable        |
| Iterations  | 3                 | OWASP recommendation                              |
| Parallelism | 4                 | Match available CPU threads                       |
| Output      | 32 bytes          | 256-bit key                                       |
| Salt        | 16 bytes          | Random, stored in bucket alongside encrypted data |

---

## 4. Sync Engine (saku-sync)

### Local state: `rusqlite` (bundled feature)

A single `~/.local/share/saku/sync.db` shared across all tools. The `bundled` feature compiles SQLite statically — no system dependency.

SQLite over plain JSON for sync state: the pending queue has genuinely different requirements from app data — it needs transactional integrity (if a sync crashes mid-write the queue must not corrupt), and the queue grows and shrinks in ways that make atomic JSON rewrites awkward. The rest of the system uses JSON for app data that the user owns and can read. SQLite is used here for internal sync machinery that benefits from proper transactions.

Two key tables:

- **`file_state`** — tracks `local_hash`, `remote_hash`, `sync_status` (`clean` / `dirty` / `uploading` / `conflict`), and timestamps per file
- **`pending_ops`** — the offline queue: each local write enqueues here, flushed opportunistically when online

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

### Change detection: full-file sync, no deltas

All files — JSON stores and markdown notes alike — are synced as complete files. When a file's SHA-256 hash differs from the remote, the whole file is uploaded or downloaded.

Delta/line-level sync is not worth the complexity here. Obsidian Sync works the same way: it syncs entire files, detecting changes by hash/mtime, and stores version history as full snapshots server-side. For the file sizes involved in a productivity tool (a task store is 10-50KB, a note is 1-10KB), the overhead of computing a diff, serialising a patch, uploading it, and applying it on another device likely exceeds just sending the whole file.

```toml
sha2 = "0.10"
```

### Conflict resolution: hybrid LWW

```rust
pub struct HybridTimestamp {
    pub wall_ms: i64,      // primary: wall clock
    pub lamport: u64,      // secondary: prevents clock rollback
    pub device_id: String, // tiebreaker: deterministic total order
}
```

LWW is applied **per entity** (per task UUID, per note file path) — never whole-file for structured data. This means `Task`, `Project`, and `Area` need a `modified_at: HybridTimestamp` field (schema migration).

For notes: on true conflict (both devices edited offline), write `note.md.conflict.<device_id>` and surface it to the user. Never silently discard either version.

### Merkle tree for efficient remote comparison

A Merkle tree is a tree where every node is a hash, and any change in a leaf bubbles up to change the root. In our two-level tree: leaves are file hashes, level-1 nodes are per-tool hashes, and the root is a hash of all tool hashes.

```
Root = SHA256(tdo_hash + nte_hash + hbt_hash)
            │                │              │
            ▼                ▼              ▼
  SHA256(store.json)  SHA256(note1.md   SHA256(hbt.json)
                           + note2.md)
```

If **any** file changes anywhere, its hash changes → its parent tool hash changes → the root changes. Checking "do I need to sync at all?" is one comparison of two 32-byte values.

The sync decision tree:

1. `GET saku/merkle.json` from S3 — **one API call**
2. Compare remote root vs local root
3. If equal → **done, zero more API calls**
4. If different → compare per-tool hashes to find which tool changed
5. For each changed tool → compare per-file hashes to find the exact file
6. Upload or download only that file

Best case (nothing changed): **1 GET**. Worst case (one file changed): **1 GET + 1 PUT**. Without the Merkle tree, you'd need a HEAD or GET for every file on every sync run.

The manifest is a small JSON document uploaded after every successful sync.

### Connectivity: try-and-fail

Call `try_flush_if_online()` at the end of every mutating command. If the server call fails with a network error, ops stay in the queue silently. No background threads, no polling.

- **When online**: ~50-200ms added latency (one presigned URL fetch + one S3 PUT)
- **When offline**: <1ms (failed connection attempt, queue intact)

HTTP client: **`ureq`** (synchronous, rustls, no tokio needed).

### The sync loop

```
1. DETECT LOCAL CHANGES
   Hash current file. If hash differs from stored local_hash → enqueue PendingOp.

2. COMPARE WITH REMOTE (Merkle shortcut)
   GET saku/merkle.json from S3 via presigned URL.
   If root hashes match → nothing to do, return.

3. FLUSH PENDING UPLOADS
   Request presigned PUT URL from saku-server.
   PUT encrypted blob directly to S3.
   On network error → break, retry next run.

4. PULL REMOTE CHANGES
   For each file where remote_hash ≠ local_hash → request presigned GET URL, download.
   Apply conflict resolution (HybridTimestamp LWW).
   Atomic write to disk.

5. UPDATE REMOTE MERKLE
   PUT updated merkle.json to S3.
```

---

## 5. Two-Tier Architecture

Direct S3 access from the client is dropped. Storing long-lived S3 credentials (`access_key_id` + `secret_access_key`) on a device is a meaningful security risk — if the device is compromised, an attacker has persistent read/write/delete access to all sync data with no expiry and no simple revocation. Short-lived presigned URLs issued by a server are strictly safer.

"Bring your own S3" moves to a **server-side config option** rather than a client concern. The client always talks to a `saku-server` instance — either self-hosted or managed. This simplifies the client significantly: one `SyncBackend` implementation, one config shape.

```
Tier 0: Local only (current behavior)
  No config. Just ~/.local/share/tdo/store.json.
  Sync is not configured, not running.

Tier 1: Self-hosted saku-server
  User runs their own server (single binary + docker-compose).
  Server holds S3 credentials, issues short-lived presigned URLs.
  Client only ever has a short-lived JWT — scoped, revocable.
  User controls everything: their own server, their own S3 bucket.

Tier 2: Managed saku-server
  You run it. Users pay. Same server code, different deployment.
  Self-hosters who don't want to pay just run Tier 1.
```

```rust
pub trait SyncBackend {
    fn fetch(&self, tool: &str) -> Result<Vec<u8>, SyncError>;
    fn push(&self, tool: &str, data: &[u8]) -> Result<(), SyncError>;
}

// One implementation: always talks to a saku-server URL
pub struct ServerSyncBackend {
    server_url: String,
    device_token: String, // JWT, stored in OS keychain
    http_client: ureq::Agent,
}
```

Config file (`~/.config/saku/sync.toml`):

```toml
[sync]
server_url = "https://sync.example.com"  # self-hosted or managed
# device_token stored in OS keychain, written on first login
```

---

## 6. Server (saku-server)

### Framework: axum 0.7

axum is the clear choice for new Rust web servers in 2025/2026 — Tokio team, tower middleware ecosystem, type-safe extractors, no macro magic.

### Key design decisions

| Decision       | Choice                                           | Rationale                                   |
| -------------- | ------------------------------------------------ | ------------------------------------------- |
| Web framework  | axum 0.7                                         | Tokio team, tower middleware, type-safe     |
| Database       | SQLite default, Postgres optional                | Self-hosters get zero-dep single binary     |
| Auth           | JWT (15 min) + rotating refresh tokens (90 days) | Stateless server, long-lived devices        |
| File access    | Pre-signed S3 URLs only                          | Server never proxies data bytes             |
| S3 credentials | Server-side only, in config                      | Never stored on client devices              |
| Config         | `config.toml` + `SAKU__*` env vars               | Docker secrets injection without file edits |
| Binary         | Single static binary, embedded migrations        | ~20MB distroless Docker image               |

### API surface

```
POST   /api/v1/auth/register
POST   /api/v1/auth/login
POST   /api/v1/auth/refresh
DELETE /api/v1/auth/devices/{device_id}

GET    /api/v1/sync/{tool}/download-url    → presigned GET URL
POST   /api/v1/sync/{tool}/upload-url      → presigned PUT URL + headers
POST   /api/v1/sync/{tool}/confirm-upload  → update quota record
GET    /api/v1/sync/{tool}/metadata        → size, etag, last_modified
```

### Pre-signed URL flow

```
CLI                    saku-server              S3 / R2
 │                         │                        │
 │── GET /sync/tdo/url ──► │                        │
 │                         │── presign GetObject ──►│
 │                         │◄── presigned URL ──────│
 │◄── { url, expires } ────│                        │
 │                                                  │
 │────────── GET presigned URL ───────────────────►│
 │◄─────────────── file bytes ────────────────────│
```

The server is an auth and metadata broker, not a data proxy. A self-hoster with a $5/month VPS + Cloudflare R2 free tier can run the entire stack with negligible egress cost.

### Self-hosting

```yaml
# docker-compose.yml
services:
  saku-server:
    image: ghcr.io/asierzapata/saku-server:latest
    ports:
      - "8080:8080"
    volumes:
      - ./data:/data
      - ./config.toml:/etc/saku-server/config.toml:ro
    environment:
      SAKU__AUTH__JWT_SECRET: "${JWT_SECRET}"
      SAKU__STORAGE__SECRET_ACCESS_KEY: "${S3_SECRET_KEY}"
    restart: unless-stopped
```

---

## Implementation Roadmap

1. **Phase 1** — Extract `saku-storage` crate. Add `modified_at: HybridTimestamp` to all entities (schema migration). Generate stable `device_id` on first run.
2. **Phase 2** — Build `saku-crypto`. Argon2id key derivation, XChaCha20Poly1305 encrypt/decrypt, file format, zeroize. Write tests with known vectors.
3. **Phase 3** — Build `saku-sync`. rusqlite schema, pending queue, Merkle tree, sync loop (full-file, no deltas).
4. **Phase 4** — Build `saku-server`. axum auth server, presigned URLs, SQLite-backed, Docker image.
