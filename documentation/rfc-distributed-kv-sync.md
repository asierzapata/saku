# RFC: Distributed Encrypted KV Sync

**Author:** @asierzapata
**Status:** Drafting

## Summary

We want every client — CLI, iOS app, web app — to share the same task data. A task added on iPhone should appear in `tdo view today` on the terminal and in the browser, and vice versa. Our current sync architecture syncs **whole encrypted files** — the entire `store.json` per tool. This works between two CLI instances, but it breaks down for non-CLI clients: the whole-file sync is not atomic (two devices can race on the Merkle tree update), background sync on iOS has tight time constraints (~30 seconds), and web apps need to work with browser storage constraints. We need to move to **per-entry sync over an HTTP-native protocol** and extend the server to support entry-level operations. The HTTP protocol *is* the SDK — any client that can make HTTP requests, implement LWW merge, and handle the crypto can participate. Each platform gets a native implementation: Rust for CLI, Swift for iOS, TypeScript for web.

## Context and Problem Statement

### What we have today

The sync stack is four crates working together:

```
saku-storage   KvStore + Entity trait + LWW merge + HybridTimestamp
saku-crypto    Argon2id KDF → XChaCha20Poly1305 encryption (per-file DEK)
saku-sync      SyncEngine + Merkle tree + SyncBackend trait + StateDb
saku-server    axum auth server + presigned S3 URLs (never sees plaintext)
```

**Storage** is a flat KV map on disk (`crates/saku-storage/src/kv_store.rs`):

```rust
pub struct KvStore {
    pub version: u32,
    pub entries: HashMap<String, Value>,  // "task/k7m2a3x9" → { ...fields... }
}
```

Each entry carries a `modified_at: HybridTimestamp` with `wall_ms`, `lamport`, and `device_id`. The merge function `lww_merge_kv` compares these timestamps per-key — the entry with the higher timestamp wins. Renames produce tombstones with `renamed_to` fields, and `repair_references` follows those chains to fix dangling foreign keys.

**Sync** operates on whole files (`crates/saku-sync/src/sync_engine.rs`). The engine:

1. Hashes each local file, compares to `StateDb`
2. Fetches the remote Merkle tree — if roots match, done (1 API call)
3. Downloads changed files, decrypts, LWW-merges the JSON
4. Re-encrypts merged result, uploads
5. Pushes updated Merkle tree

This is clean for CLI-to-CLI sync. The entire `store.json` is ~10-50KB, a single S3 object per tool per user. Merkle comparison is fast. The server is a dumb presigned-URL broker.

**Crypto** encrypts at the S3 boundary — plaintext locally, ciphertext remotely. Each file gets a random DEK encrypted by the KEK (derived from user passphrase via Argon2id). The binary format is 117 bytes of header plus ciphertext.

### Where this breaks for non-CLI clients

**1. Atomicity.** The current approach of downloading, merging, and re-uploading the entire store is not atomic — two devices syncing simultaneously can race on the Merkle tree update. While the per-entry merge inside `merge_store_json` handles concurrent edits correctly, the window for races is wider on mobile because background sync is unreliable and the user may switch between CLI and phone frequently.

**2. Background sync constraints.** iOS gives background tasks ~30 seconds of execution time. A full sync cycle (fetch Merkle + download file + decrypt + merge + encrypt + upload + update Merkle) may exceed that on a slow connection, especially as stores grow. Per-entry sync fits naturally into this window — each individual push/pull is tiny.

**3. No path to iOS or web.** The merge logic, crypto, and sync engine are all in Rust. The whole-file sync protocol is tightly coupled to the Rust implementation (Merkle trees, `StateDb`, presigned S3 URLs). An iOS or web client would need to reimplement this entire stack — or we move to a protocol simple enough that any language can implement it independently.

**4. Bandwidth (minor).** Syncing the entire `store.json` every time one task changes transfers 50-100KB on cellular for a one-field change. This is tolerable on modern LTE (sub-100ms), but unnecessarily wasteful at scale and adds up with frequent syncs.

**5. Real-time feel.** Users expect mobile and web apps to sync in seconds, not minutes. The current "sync on every mutating command" model works for CLI, but an iOS app or web dashboard needs to feel snappier — ideally showing changes from other devices within seconds of opening the app.

**6. Protocol too complex for multi-language implementation.** The current sync protocol (Merkle tree comparison, presigned S3 URLs, multi-step file upload/download) is complex enough that reimplementing it correctly in Swift or TypeScript would be error-prone. A simpler HTTP protocol (push/pull individual entries with a cookie-based cursor) is easy to implement in any language with an HTTP client and a crypto library.

---

## Design Principles

### HTTP-Native Sync Protocol

The sync protocol is plain HTTP. Any client that speaks HTTP can participate in sync — the CLI, an iOS app, a web app, a future Android app, a cron job, even `curl`. There is no custom binary protocol, no WebSocket requirement, no platform-specific SDK needed to sync.

This is a deliberate architectural choice:

- **Openness.** saku is AGPL, multi-platform by design. An HTTP protocol means third-party tools can integrate without importing any saku library.
- **Simplicity.** HTTP is the most widely understood and debuggable protocol. Developers can inspect sync traffic with standard tools (`curl`, browser devtools, `httpie`).
- **Broad reach.** Any platform that has an HTTP client can sync. No need for platform-specific transport layers.

The "smart" parts of sync — merge logic, crypto, conflict resolution — live entirely in the client. The server is a thin, dumb HTTP KV store that accepts encrypted blobs and hands them back. This is the same pattern used by [Replicache](https://doc.replicache.dev/concepts/how-it-works) — the protocol is HTTP push/pull, the intelligence is client-side.

### E2E Encryption Constrains the Architecture

The server never sees plaintext. This is a hard constraint that rules out server-authoritative sync models (like Replicache's mutation-replay pattern, where the server re-executes mutations against canonical state). Since the server can't read the data, it can't re-execute mutations, can't validate entries, and can't compute diffs based on content. The server's role is limited to: store opaque blobs, track when they arrived, hand them back when asked. All merge intelligence lives in the client.

### Client-Authoritative Model: Trade-offs and Applicability

In a server-authoritative system like Replicache, the server is the single source of truth. Mutations are *proposals* — the server can reject, transform, or reorder them. A client's optimistic local state is speculative until the server confirms it. This enables business rule enforcement: "you can't book an already-booked meeting room," "you can't withdraw more than your balance," "only admins can delete records."

saku uses the opposite model: **every client is authoritative.** If a client performs a mutation (complete a task, rename a project, delete an entry), that mutation is final. It propagates to all other devices via sync. No server-side gatekeeper validates or rejects it. The server is a blind relay for encrypted blobs — it *cannot* validate even if it wanted to, because it can't read the data.

This has concrete implications:

**When client-authoritative works well (saku's case):**

- **Single-user, multi-device.** There is no "other user" whose state you need to protect. All devices belong to the same person. If the user completes a task on their phone, every device should see it — there's no reason to reject it.
- **Personal data with no invariants.** A task manager has no hard business rules that require server enforcement. You can't "overspend" tasks. There's no shared resource to protect. The worst case of a conflicting edit is that one version of a task title wins over another — annoying but not data-corrupting.
- **Offline-first.** Mutations always succeed locally, instantly. There's no "pending confirmation" state. The app feels fast because there's nothing to wait for.
- **Privacy by design.** Client authority is a natural consequence of E2E encryption. If the server can't read the data, it can't enforce rules on it. The two properties reinforce each other.

**When client-authoritative breaks down:**

- **Multi-user collaboration with access control.** If Alice and Bob share a project and only Alice should be able to delete tasks, a client-authoritative model can't enforce this — Bob's client could push a deletion and it would propagate. You need a server that understands permissions.
- **Shared resource constraints.** Inventory management, booking systems, financial transactions — anything where two clients competing for the same limited resource must have a tiebreaker *before* the mutation is committed. LWW would let both clients "succeed" locally and then one silently loses.
- **Auditability and compliance.** If regulations require that every mutation is validated against business rules before persistence, a blind relay server doesn't satisfy this.
- **Untrusted clients.** In a multi-user system, a malicious or compromised client could push arbitrary data. With E2E encryption, the server can't even detect this. In a single-user system, a compromised client means the user's own device is compromised — at that point, server-side validation wouldn't help either.

**saku's position:** saku is a personal productivity tool — one user, their own devices, their own data. Client authority is not just acceptable, it's the right model. It enables E2E encryption, instant offline mutations, and a simple server. The trade-off (no server-side validation) is irrelevant for the use case. If saku ever added multi-user features (shared projects, team task boards), this decision would need to be revisited — but that's a fundamentally different product.

---

## Prior Art

The RFC designs a sync protocol for an encrypted KV store. Several existing sync frameworks inform the design, though none fit saku's constraints directly (E2E encryption + multi-platform CLI + open protocol).

### Replicache / Zero (server-authoritative, mutation replay)

[Replicache](https://doc.replicache.dev/concepts/how-it-works) is the most architecturally relevant comparison. Its model:

- **Push** sends mutations (function name + arguments) to the server. The server re-executes them against canonical state — the server is authoritative.
- **Pull** returns a diff (patch) from the server. The client rebases pending mutations on top, like `git rebase`.
- **Cookie** is an opaque version token the client passes back on pull. The server uses it to compute minimal diffs — no clock assumptions.
- **Poke** is a contentless hint (via WebSocket/SSE) telling the client to pull. Enables near-real-time sync without polling.

Replicache is now in maintenance mode; its successor [Zero](https://zero.rocicorp.dev/) extends the model with partial sync and SQL-like queries.

**What we borrow:** The cookie-based diffing mechanism (opaque cursor instead of `?since=timestamp`) and the HTTP push/pull separation. These are directly applicable and strictly better than timestamp-based approaches — no clock drift, no missed entries.

**What doesn't fit:** Mutation replay requires the server to understand the data model and re-execute application logic. This is incompatible with E2E encryption — our server can't read entries, let alone replay mutations against them. saku uses client-side LWW merge instead.

| Aspect | Replicache | saku (this RFC) |
|--------|-----------|-----------------|
| What gets pushed | Mutations (name + args) | Encrypted entry blobs |
| Server role | Re-executes mutations, authoritative | Stores opaque blobs, dumb |
| Conflict resolution | Server-side replay | Client-side LWW |
| Diff mechanism | CVR diffing (cookie) | Cookie-based cursor |
| Encryption | None (server sees data) | E2E (server sees nothing) |

### ElectricSQL (CRDT-based, Postgres sync)

[ElectricSQL](https://electric-sql.com/) syncs Postgres to local SQLite using CRDTs. Their recent [Durable Streams](https://electric-sql.com/blog/2025/12/09/announcing-durable-streams) work is relevant — it's an HTTP protocol for resumable, real-time streaming using monotonic offsets (conceptually similar to cookies/cursors).

**Relevance:** Low. ElectricSQL assumes a server-side Postgres database it can replicate from. saku's server stores encrypted blobs. CRDTs are overkill when the data model is simple KV entries with LWW semantics.

### PowerSync (local SQLite sync, Rust core)

[PowerSync](https://www.powersync.com/) syncs a backend database to client-side SQLite. They recently [rewrote their sync client in Rust](https://www.powersync.com/blog/speeding-up-powersync-with-a-sqlite-extension-written-in-rust) for cross-platform performance — the same Rust core compiles to iOS, Android, and web via their SDK.

**Relevance:** PowerSync chose one codebase (Rust) shared across platforms via FFI. saku takes a different approach: the HTTP protocol is simple enough that each platform gets a native implementation (Rust, Swift, TypeScript). Both are valid — PowerSync's data model is more complex and benefits from a single implementation. saku's contract surface (LWW merge + standard crypto + HTTP push/pull) is small enough for independent implementations. Their protocol also assumes a readable server-side database, which doesn't apply to saku's E2E encryption model.

### CRDTs (Automerge, Yjs, cr-sqlite)

CRDT-based solutions let peers sync without a central authority. No server needed in theory.

**Why not for saku:**
- CRDTs add significant complexity to the data model — every field needs CRDT metadata, documents can grow unbounded with history.
- saku's data is simple — tasks, projects, areas with clear ownership. LWW per entry is sufficient.
- CRDTs don't solve the E2E encryption problem — you still need a relay server.
- The operational complexity is much higher than what the data model warrants.

LWW is the right choice for saku's data model. The one improvement worth considering in the future is per-field LWW (each field carries its own `HybridTimestamp`) instead of per-entry — this would prevent silent data loss when two devices edit different fields of the same task. But that's independent of the sync protocol.

### Summary: what we borrow

| Framework | Borrowed | Not applicable |
|-----------|----------|---------------|
| Replicache | Cookie-based diffing, HTTP push/pull separation | Mutation replay (incompatible with E2E encryption) |
| ElectricSQL | Monotonic offset / durable stream concept | CRDT complexity, Postgres coupling |
| PowerSync | Cross-platform sync architecture patterns | Server-readable data assumption, single-codebase FFI approach |
| CRDTs | — | Everything (overkill for the data model) |

---

## Design Options

We see three viable approaches, each representing a different trade-off between sync granularity, server complexity, and migration effort.

### Option A: Per-Entry Server-Side KV Store (Recommended)

**The idea.** The server becomes a per-entry encrypted KV store instead of a dumb blob store. Each KV entry is individually addressable. Clients push and pull individual entries, not whole files. The HTTP protocol is the contract — each platform implements the client natively (Rust for CLI, Swift for iOS, TypeScript for web).

**Server API (HTTP-native):**

```
PUT  /api/v1/kv/{tool}              → batch upsert encrypted entries
PUT  /api/v1/kv/{tool}/{key}        → upsert single encrypted entry
GET  /api/v1/kv/{tool}?cookie={c}   → get entries changed since cookie
GET  /api/v1/kv/{tool}/snapshot     → full encrypted snapshot (initial sync)
```

There is no DELETE endpoint. Deletes are performed by the client pushing a tombstone entry via the normal batch PUT — the client sets the entry's internal `deleted` / `renamed_to` fields and pushes it as an encrypted blob. The server never interprets entry contents and cannot distinguish a live entry from a tombstone.

**How cookie-based diffing works:** The server assigns a monotonically increasing sequence number to each write. The `cookie` is an opaque cursor encoding the client's last-seen sequence number. On pull, the server returns all entries with sequence numbers greater than the cookie, plus a `next_cookie` for the next pull. The client does not know or care how the cookie is structured — it just passes it back.

This approach is borrowed from [Replicache's CVR pattern](https://doc.replicache.dev/strategies/row-version). It is strictly better than a `?since=timestamp` approach: no server clock drift, no missed entries from clock skew, no ambiguity about which timestamp to save after a multi-step sync.

**Client sync flow:**

```
1. GET /kv/tdo?cookie={last_cookie}    → changed entries + next_cookie
2. For each changed entry:
   - Decrypt
   - LWW merge with local version
   - Write to local store
3. Load full store into memory
4. Run reconcile_renames + repair_references (may dirty more entries)
5. For each locally dirty entry:
   - Encrypt
   - Batch PUT /kv/tdo (all dirty entries in one request)
6. Save next_cookie from step 1 as checkpoint
```

**Checkpoint semantics:** The `next_cookie` saved in step 6 comes from the pull response in step 1. On the next sync, entries pushed by other devices *during* this sync (between steps 1 and 5) will be fetched again — but LWW merge is idempotent, so this is correct. The client's own pushed entries may also be re-fetched, which is a no-op. This is simpler and safer than trying to compute a "perfect" checkpoint.

**What stays the same:** The local `KvStore` format, `lww_merge_kv`, `reconcile_renames`, `repair_references`, `HybridTimestamp` — all unchanged. The `saku-crypto` layer is unchanged — we just encrypt/decrypt individual entries instead of whole files.

**What changes:**

| Component | Change |
|-----------|--------|
| `saku-server` | New `/kv` endpoints, per-entry storage table, monotonic sequence tracking |
| `saku-sync` | New `SyncBackend` impl for per-entry ops. `SyncEngine` refactored from file-based to entry-based |
| `saku-storage` | Add dirty-tracking per entry (persisted, crash-safe) |
| S3 / R2 | Still used, but for per-entry blobs or a server-managed bucket |
| Merkle tree | Replaced by cookie-based cursor — simpler, no tree maintenance |

**Encryption at entry level:** Each entry is individually encrypted. The header overhead is 117 bytes per entry (the current file format). For a store with 200 tasks + 10 projects + 5 areas = 215 entries, that's ~25KB of overhead. The entries themselves are small (200-500 bytes each), so total encrypted storage is ~100-130KB — comparable to the current whole-file approach.

Alternatively, we could use a lighter encryption format for entries — skip the per-entry DEK and just use the KEK directly with a random nonce. This drops the header to 29 bytes (magic + version + nonce + tag). The per-file DEK pattern was designed for large files where you might want to rotate the DEK without re-deriving from passphrase. For small entries, this is unnecessary.

**Pros:**
- Minimal bandwidth per sync — only changed entries transfer
- Fast background sync — a single batch push/pull fits in iOS's 30-second window easily
- Natural conflict resolution — LWW per entry, same logic as today
- No Merkle tree maintenance — cookie-based cursor is simpler and clock-independent
- Server remains E2E encrypted — stores opaque blobs
- Batch PUT minimizes HTTP round-trips

**Cons:**
- Server stores per-entry metadata — more storage, more database rows
- Server API surface grows — more endpoints, more complexity
- CLI must migrate from whole-file to per-entry sync
- `reconcile_renames` and `repair_references` need all entries in memory — can't run incrementally per-entry (but this is fine for the expected data sizes)
- Loses the "1 GET and done" Merkle fast-path for the no-changes case (though `?cookie` with an empty response is equally fast)

---

### Option B: Keep Whole-File Sync, Bridge Rust to iOS

**The idea.** Don't change the sync protocol at all. The iOS app uses the exact same Rust code (via UniFFI) — same `SyncEngine`, same `SyncBackend`, same whole-file approach. The iOS app is a thin SwiftUI shell over the Rust core.

**Architecture:**

```
┌─────────────────────┐
│   SwiftUI Views     │  ← Pure Swift, observes Rust state
├─────────────────────┤
│  Swift Bridge Layer │  ← UniFFI-generated, thin
├─────────────────────┤
│  saku-storage       │
│  saku-crypto        │  ← Same Rust code as CLI
│  saku-sync          │
└─────────────────────┘
```

**UniFFI boundary:** Expose a high-level API from Rust:

```rust
#[uniffi::export]
fn load_store(data_dir: String) -> Result<StoreHandle, StorageError>;

#[uniffi::export]
fn add_task(store: &StoreHandle, title: String, project: Option<String>) -> Result<TaskView, StorageError>;

#[uniffi::export]
fn complete_task(store: &StoreHandle, task_number: u64) -> Result<(), StorageError>;

#[uniffi::export]
fn sync(store: &StoreHandle, passphrase: String) -> Result<SyncOutcome, SyncError>;

#[uniffi::export]
fn get_today_tasks(store: &StoreHandle) -> Vec<TaskView>;
```

The `serde_json::Value` boundary is handled by passing JSON strings across FFI:

```rust
#[uniffi::export]
fn get_store_json(store: &StoreHandle) -> String;  // full store as JSON

#[uniffi::export]
fn merge_from_json(store: &StoreHandle, remote_json: String) -> Result<(), StorageError>;
```

**What stays the same:** Everything. The sync protocol, server API, Merkle tree, crypto format — all unchanged. The iOS app is just another `SyncBackend::Server` client.

**What changes:**

| Component | Change |
|-----------|--------|
| Rust crates | Add `uniffi` annotations, expose public API, compile as static lib for `aarch64-apple-ios` |
| Build system | Cross-compilation toolchain, XCFramework packaging |
| iOS app | SwiftUI views, call Rust via UniFFI, iOS keychain for passphrase, BGTaskScheduler for background sync |
| Server | Nothing |

**Pros:**
- Zero server changes — ship faster
- Guaranteed compatibility — same code, same merge logic, same crypto
- Eliminates the crypto parameter-matching problem entirely
- Minimal risk of subtle sync bugs
- Android app later follows the same pattern (UniFFI generates Kotlin too)

**Cons:**
- Whole-file sync on cellular — 50-100KB per sync for a one-field change
- Binary size — Rust static library adds ~2-5MB to the iOS app
- FFI complexity — UniFFI handles most of it, but edge cases exist (e.g., `zeroize` semantics don't cross FFI)
- Background sync may be slow on poor connections (full download + upload)
- `serde_json::Value` doesn't cross FFI natively — need JSON string serialization at the boundary
- Debugging across the FFI boundary is harder

---

### Option C: Hybrid — Per-Entry Sync with Rust Core via UniFFI

**The idea.** Combine the best of A and B. Use UniFFI to bridge the Rust core to iOS (same merge logic, same crypto). Simultaneously evolve the sync protocol to per-entry granularity. The Rust `SyncEngine` gains a new per-entry mode that both CLI and iOS use.

**Architecture:**

```
┌─────────────────────┐
│   SwiftUI Views     │
├─────────────────────┤
│  Swift Bridge (FFI) │
├─────────────────────┤        ┌──────────────────┐
│  saku-storage       │        │   saku-server     │
│  saku-crypto        │◄──────►│  /kv endpoints    │
│  saku-sync (v2)     │        │  per-entry store  │
└─────────────────────┘        └──────────────────┘
```

**Phased approach:**

**Phase 1 — UniFFI bridge + existing whole-file sync.** Ship the iOS app using Option B. This gets a working iOS app out quickly with zero server changes. The whole-file sync works — it's just not optimal on cellular.

**Phase 2 — Per-entry dirty tracking.** Add a dirty set to `saku-storage` that tracks which entries changed since last sync. The dirty set is **persisted** (not runtime-only) to survive app crashes and iOS process kills:

```rust
pub struct KvStore {
    pub version: u32,
    pub entries: HashMap<String, Value>,
}

// Persisted separately in a sidecar file (dirty.json) or SQLite table
pub struct DirtyTracker {
    pub dirty_keys: HashSet<String>,
    pub last_cookie: Option<String>,
}
```

**Phase 3 — Server-side per-entry KV.** Add the `/kv` endpoints from Option A. The `SyncEngine` gets a new `PerEntrySyncBackend` that pushes/pulls individual entries instead of whole files.

**Phase 4 — Drop whole-file sync.** Once all clients support per-entry sync, deprecate the whole-file Merkle approach. The server can garbage-collect the old S3 blobs.

**Migration path:** During the transition (Phase 3), the server supports both protocols. A client that supports per-entry sync uses `/kv` endpoints. Legacy CLI versions still use the old presigned-URL flow. The server reconciles by treating the whole-file upload as a batch of per-entry upserts internally.

**How `reconcile_renames` works in per-entry mode:** This is the hardest part. `reconcile_renames` and `repair_references` need the full entry set in memory to follow rename chains and fix dangling references. In per-entry sync:

1. After pulling changed entries, load the full local store into memory
2. Run `reconcile_renames` and `repair_references` on the full in-memory store (same as today)
3. Any entries modified by the repair become dirty and get pushed in the next cycle

This means per-entry sync doesn't eliminate the need to load the full store — it just eliminates the need to *transfer* the full store. The merge is still done in-memory on the full dataset. This is fine for the expected data sizes (hundreds of entries, not millions).

**Pros:**
- Ships an iOS app quickly (Phase 1 is Option B with a clear upgrade path)
- Evolves toward optimal sync granularity without a big-bang migration
- Rust core guarantees compatibility across all platforms
- Each phase is independently shippable and testable
- Android gets the same benefits when it arrives (UniFFI generates Kotlin)

**Cons:**
- More total engineering work than either A or B alone
- Server must support two sync protocols during transition
- Phase 3-4 are significant changes to both server and sync engine
- The phased approach means the first iOS release has the whole-file sync drawback

---

## Comparison

| Dimension | A: Per-Entry Server KV (Recommended) | B: Whole-File + UniFFI | C: Hybrid |
|-----------|----------------------|----------------------|-------------------------|
| **Time to first iOS app** | Moderate — server changes needed first | Fast — zero server changes | Fast — Phase 1 is B |
| **Sync efficiency** | Optimal — entry-level | Poor — whole file on cellular | Optimal after Phase 3 |
| **Code sharing** | Native SDK per platform (Rust, Swift, TS) | Full sharing via UniFFI | Full sharing via UniFFI + WASM |
| **Server complexity** | High — new KV store, new API | None | High — but deferred to Phase 3 |
| **Migration risk** | High — big-bang protocol change | None | Low — phased migration |
| **Crypto compatibility** | Ensured by spec + cross-platform integration tests | Guaranteed (same Rust code) | Guaranteed (same Rust code) |
| **Background sync on iOS** | Fast (small payloads) | Slow (full file) | Fast after Phase 3 |
| **Web app path** | Direct — native TS implementation, same HTTP protocol | Possible but awkward (whole-file over HTTP) | Direct after Phase 3 |
| **Android path** | Native Kotlin implementation, same HTTP protocol | UniFFI generates Kotlin too | UniFFI generates Kotlin too |
| **Protocol openness** | HTTP-native, any client can sync | HTTP via presigned URLs | HTTP-native after Phase 3 |

---

## Recommendation: Option A (Per-Entry Server KV + Native SDKs)

We go directly to per-entry sync. No interim whole-file phase, no migration complexity. Build it right from the start. The HTTP protocol is the contract — each platform gets a native SDK implementation: Rust for CLI, Swift for iOS, TypeScript for web.

The reasoning:

1. **Atomicity.** The whole-file approach has a known race condition — two devices syncing simultaneously can conflict on the Merkle tree update. Per-entry sync with cookie-based cursors eliminates this. Each entry is individually addressable, and the cookie mechanism ensures no entries are missed regardless of concurrent activity.

2. **Background task fit.** iOS gives background tasks ~30 seconds. A batch PUT of a few dirty entries and a single GET with cookie completes in milliseconds on any connection. The whole-file approach risks timing out on slow cellular — download 50KB, merge, encrypt, upload 50KB, update Merkle. Per-entry sync is inherently bounded per operation.

3. **HTTP-native openness.** The sync protocol is plain HTTP. A `curl` call can push an entry. A cron job can pull changes. A web app, an Android app, a third-party integration — they all just speak HTTP. The protocol *is* the SDK. This aligns with saku's AGPL, multi-platform design.

4. **Native SDKs, no FFI complexity.** Each platform uses idiomatic tools: Swift with CryptoKit on iOS, TypeScript with libsodium.js in the browser, Rust on the CLI. No UniFFI cross-compilation, no WASM bundle, no FFI bridge debugging. Each SDK is a self-contained library in its platform's native language.

5. **The contract surface is small.** The risk of multi-implementation divergence is proportional to the contract surface. saku's contract is: (a) HTTP endpoints with JSON payloads, (b) LWW merge on three fields (`wall_ms`, `lamport`, `device_id`), (c) Argon2id KDF with fixed parameters → XChaCha20Poly1305 encryption. All three are standardized, well-documented, and have mature libraries in every language. Cross-platform integration tests (encrypt in Rust, decrypt in Swift and TS) catch parameter mismatches in CI.

6. **The server change is bounded.** We add a `kv_entries` table and a few endpoints. The existing auth system is untouched. The old whole-file sync endpoints can remain for backward compatibility with existing CLI installations, or be migrated at the same time.

7. **Simpler architecture overall.** Per-entry sync with a cookie-based cursor is conceptually simpler than the Merkle tree approach. One less data structure to maintain, one less thing to get wrong. No clock-drift concerns (unlike a `?since=timestamp` approach).

8. **Any language can join.** Because the protocol is HTTP and the contract is documented, a fourth client (Go, Python, Kotlin) can be written without touching any existing codebase. The barrier to entry is: "can you make HTTP requests and implement Argon2id + XChaCha20Poly1305?" — which is yes for essentially every modern language.

---

## Client Architectures

The sync protocol is platform-agnostic. Each client implements the same sync flow (pull → merge → push) natively in its platform's language. The contract is small enough that independent implementations are practical and preferable to cross-language bridging (UniFFI, WASM).

Three native SDKs:

| Platform | Language | Crypto library | Local storage | HTTP client |
|----------|----------|---------------|---------------|-------------|
| CLI | Rust | `saku-crypto` (existing) | JSON file | `reqwest` |
| iOS | Swift | CryptoKit + `swift-sodium` (or CryptoKit-only) | SQLite (GRDB) | `URLSession` |
| Web | TypeScript | `libsodium.js` or `@noble/ciphers` | IndexedDB | `fetch` |

Each SDK implements the same contract: HTTP push/pull, LWW merge on `HybridTimestamp`, Argon2id KDF → XChaCha20Poly1305 encryption. Cross-platform integration tests (encrypt in one language, decrypt in another) run in CI to catch parameter drift.

### iOS App Architecture

The iOS app is a pure Swift application. A native Swift sync SDK handles storage, crypto, and sync logic using platform-idiomatic libraries. No Rust bridging, no UniFFI, no cross-compilation.

```
┌─────────────────────┐
│   SwiftUI Views     │  ← Pure Swift, observes SQLite via GRDB
├─────────────────────┤
│  SakuSync (Swift)   │  ← Native Swift SDK: KV store, LWW merge, crypto
├─────────────────────┤        ┌──────────────────┐
│  GRDB (SQLite)      │        │   saku-server     │
│  CryptoKit / sodium │◄──────►│  /kv endpoints    │
│  URLSession         │        │  per-entry store  │
└─────────────────────┘        └──────────────────┘
```

### Local Storage

**GRDB.swift (SQLite)** for the local KV store on iOS. A single table maps directly to the `KvStore` model:

```sql
CREATE TABLE entries (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,           -- JSON blob
    modified_at_wall_ms INTEGER,   -- extracted for indexing
    modified_at_lamport INTEGER,
    modified_at_device_id TEXT,
    dirty INTEGER DEFAULT 0        -- 1 if modified since last sync
);
```

Why SQLite over a plain JSON file:
- Partial reads — query individual entries without loading everything
- Transactional writes — atomic merges with rollback
- Observable via GRDB's `ValueObservation` — drives SwiftUI updates reactively
- Concurrent access safe (WAL mode) — app extensions (widgets) can read while the app writes
- Dirty tracking survives crashes — the `dirty` column persists through process kills

The Swift SDK writes to this SQLite database directly. No separate `store.json` — the SQLite *is* the local KV store on iOS. The CLI keeps its JSON file. Both produce the same entries; the on-device format is a platform concern, not a sync concern. The sync protocol deals in individual encrypted entries, not files.

### Keychain

Use iOS Keychain (Security.framework) with `kSecAttrAccessibleAfterFirstUnlock`:

- **Passphrase** — stored in keychain, optionally protected by Face ID/Touch ID
- **Access token + refresh token** — stored in keychain, accessible in background
- `kSecAttrAccessibleAfterFirstUnlock` is critical — it allows background sync to access credentials after the user unlocks the device once post-boot

iOS and macOS keychain namespaces are isolated — they don't share items (the CLI uses the `keyring` crate on macOS, which accesses a different keychain namespace). For v1, the user enters their passphrase manually on iOS during setup. A QR-code-based pairing flow (`tdo sync pair` on CLI) is a good v2 improvement.

### Sync Triggers

A layered approach:

1. **Foreground sync** (primary) — sync when the app enters foreground. 100% reliable, no time limits.
2. **BGAppRefreshTask** (secondary) — iOS periodically wakes the app for ~30 seconds. Per-entry sync fits comfortably in this window — a batch PUT/GET is tiny.
3. **Push notifications** (deferred) — add silent pushes later for near-real-time sync. Requires APNs infrastructure on the server. Not needed for v1.

### Crypto

The Swift SDK implements the same crypto using native Swift libraries:

- **Argon2id KDF** — via `swift-sodium` (libsodium wrapper) or a pure-Swift Argon2 implementation. Parameters must match exactly: 64MB memory, 3 iterations, parallelism 4, 32-byte output, deterministic salt.
- **XChaCha20Poly1305** — via `swift-sodium` or CryptoKit (CryptoKit supports ChaCha20Poly1305; for the XChaCha20 variant, `swift-sodium` is needed).

The entry encryption format is specified (see "Entry encryption format" below) — `[nonce 24B][ciphertext + tag 16B]`. Cross-platform integration tests verify that entries encrypted by the Rust CLI can be decrypted by the Swift SDK and vice versa.

### Web App Architecture

The web app is a pure TypeScript application. A native TypeScript sync SDK handles storage, crypto, and sync logic using browser APIs and JS crypto libraries. No WASM, no Rust compilation, no bridge layer.

```
┌─────────────────────┐
│   Web UI (JS/TS)    │  ← Any framework (React, Svelte, vanilla)
├─────────────────────┤
│  SakuSync (TS)      │  ← Native TS SDK: KV store, LWW merge, crypto
├─────────────────────┤        ┌──────────────────┐
│  IndexedDB          │        │   saku-server     │
│  libsodium.js       │◄──────►│  /kv endpoints    │
│  fetch              │        │  per-entry store  │
└─────────────────────┘        └──────────────────┘
```

#### Local Storage

**IndexedDB** for the local KV store in the browser. A single object store maps to the `KvStore` model:

```javascript
// IndexedDB object store: "entries"
{
    key: "task/k7m2a3x9",        // primary key
    value: "{ ... }",            // JSON blob
    modified_at_wall_ms: 1709...,
    modified_at_lamport: 42,
    modified_at_device_id: "web-abc123",
    dirty: 1                     // 1 if modified since last sync
}
// Index on "dirty" for efficient dirty-entry queries
```

Why IndexedDB over other browser storage:
- **Capacity** — no practical size limit (unlike localStorage's 5-10MB). Quota is generous (typically hundreds of MB, up to GB with user permission).
- **Structured data** — supports indexes, range queries, and cursors. Dirty-entry lookups are efficient.
- **Transactional** — atomic reads and writes. A merge can update multiple entries in one transaction.
- **Persistent** — survives page reloads and browser restarts. Data persists until explicitly cleared.
- **Async** — non-blocking API. Won't freeze the UI during sync.
- **Web Worker compatible** — the sync engine can run in a Web Worker, keeping the main thread responsive.

Alternative: **OPFS (Origin Private File System)** with SQLite compiled to WASM (via `sql.js` or `wa-sqlite`). This gives true SQLite semantics in the browser, matching iOS more closely. The trade-off is a larger bundle (~1MB for SQLite WASM) and more complex setup. For v1, IndexedDB is simpler and sufficient. OPFS+SQLite is a good optimization if query patterns become complex.

#### Credentials and Key Management

Web apps don't have a system keychain. Options for v1:

- **Passphrase prompt per session** — the user enters their encryption passphrase when they open the app. The derived key lives in JS memory for the session duration. On tab close, it's gone. This is the safest approach — no secrets persisted in browser storage.
- **Session token in memory** — the auth token (JWT) is stored in a JS variable or `sessionStorage`. It doesn't survive tab close, which is acceptable for a web app — the user simply re-authenticates.

What we explicitly **don't** do:
- No passphrase in `localStorage` — browser extensions and XSS could read it.
- No long-lived refresh tokens in browser storage — the threat model is different from native apps.

The TypeScript SDK implements the same crypto using `libsodium.js` (or `@noble/ciphers` + `@noble/hashes` for a lighter-weight alternative). Same algorithms, same parameters, same entry format — cross-platform integration tests verify compatibility.

#### Sync Triggers

A simpler model than iOS, matching browser capabilities:

1. **On load** — sync when the app opens (tab becomes visible). Primary mechanism.
2. **Periodic polling** — `setInterval` while the tab is active. Pull every 30-60 seconds for near-real-time updates.
3. **Visibility change** — sync when the tab regains focus (`visibilitychange` event). Catches changes made on other devices while the user was in another tab.
4. **Server-Sent Events (deferred)** — SSE is a natural fit for the browser (persistent HTTP connection, automatic reconnect, no WebSocket complexity). The server sends a contentless "poke" when entries change, and the client pulls. This gives true real-time sync. Deferred to v2.

Note: **Service Workers** could enable background sync (the browser retries failed sync requests when connectivity returns), but this adds complexity and is not essential for v1. The foreground sync model is sufficient for a task manager.

#### Web-Specific Constraints

- **Argon2id is CPU-intensive.** Key derivation may block the main thread for ~1 second. Solution: run it in a Web Worker. `libsodium.js` supports this natively.
- **No `zeroize` guarantee.** JavaScript is garbage-collected — you can overwrite a `Uint8Array` with zeros, but the GC may have copied the data elsewhere. This is an inherent limitation of the web platform. The same limitation applies to any web app handling secrets.
- **No filesystem access.** All persistence goes through IndexedDB. The TypeScript SDK treats IndexedDB as its storage backend, the same way the CLI uses JSON files and iOS uses SQLite.
- **Bundle size.** `libsodium.js` is ~200KB (gzipped). The rest of the TypeScript SDK is small. Total bundle is much lighter than a WASM approach would be.

---

## Detailed Design: Client SDK Contract

> **Normative section.** This section is the protocol specification. Any conforming client implementation MUST match the behaviors described here. The key words "MUST", "SHOULD", "MAY", "MUST NOT", and "SHOULD NOT" in this section are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

The sync protocol defines a contract that any client SDK MUST implement. Each platform implements this contract natively. The contract has three parts: HTTP sync protocol, crypto specification, and merge algorithm.

### Sync Protocol Contract

Every SDK must implement the same sync flow:

```
1. Load local dirty set (entries modified since last sync)
2. GET /kv/{tool}?cookie={last_cookie}
   → receive changed_entries + next_cookie
3. For each remote entry in changed_entries:
   a. Decrypt with KEK
   b. LWW merge with local entry (HybridTimestamp comparison)
   c. Write merged result to local store
   d. If the remote entry won the merge, clear its dirty flag
4. Load full store into memory
5. Run reconcile_renames + repair_references (may mark more entries dirty)
6. Collect all dirty entries
7. For each dirty entry: encrypt with KEK
8. Batch PUT /kv/{tool} with all encrypted dirty entries
9. Clear dirty flags for successfully pushed entries
10. Save next_cookie (from step 2) as last_cookie checkpoint
```

This flow is identical across all platforms. The platform-specific parts are: where dirty flags are stored (SQLite column, JSON sidecar, IndexedDB field), how HTTP requests are made (`reqwest`, `URLSession`, `fetch`), and how crypto is invoked.

### Crypto Specification

All SDKs must implement identical crypto. The parameters are fixed and versioned:

**Key derivation (Argon2id):**
- Version: 0x13 (19)
- Memory: 64 MB (`m = 65536`)
- Iterations: 3 (`t = 3`)
- Parallelism: 4 (`p = 4`)
- Output: 32 bytes
- Salt: 16 bytes, deterministic: `SHA-256(b"saku-sync-kek-salt-v1:" || passphrase_bytes)[0..16]`

**Why deterministic salt:** In per-entry sync, every device must independently derive the same KEK from the same passphrase — there is no single encrypted file header where a random salt could live. A random salt would require either transmitting the salt out-of-band or storing it per-device (defeating the purpose). The deterministic derivation ensures any device with the correct passphrase produces the same KEK without coordination. See `crates/saku-sync/src/sync_engine.rs:16-23` for the reference implementation.

**Entry encryption (XChaCha20Poly1305):**
- Nonce: 24 bytes (random per entry)
- Key: 32 bytes (from Argon2id)
- Tag: 16 bytes (appended to ciphertext)
- Wire format: `[nonce 24B][ciphertext + tag 16B]`

**Platform libraries:**

| Platform | Argon2id | XChaCha20Poly1305 |
|----------|----------|-------------------|
| Rust (CLI) | `argon2` crate | `chacha20poly1305` crate |
| Swift (iOS) | `swift-sodium` (libsodium wrapper) | `swift-sodium` |
| TypeScript (web) | `libsodium.js` or `argon2-browser` | `libsodium.js` or `@noble/ciphers` |

These are all wrappers around the same underlying algorithms. Cross-platform integration tests encrypt an entry in each language and verify that every other language can decrypt it.

### Merge Algorithm (LWW)

Every SDK must implement Last-Writer-Wins merge using `HybridTimestamp`:

```
struct HybridTimestamp {
    wall_ms: i64,      // wall clock milliseconds (signed 64-bit integer)
    lamport: u64,      // logical clock, monotonically increasing
    device_id: String,  // unique device identifier
}
```

**Comparison order:** `wall_ms` (higher wins) → `lamport` (higher wins) → `device_id` (lexicographic tiebreaker). This is a total order — every pair of timestamps has a deterministic winner.

**Entry-level merge:** For each key, compare the local and remote `HybridTimestamp`. The entry with the higher timestamp wins entirely (all fields). The losing entry is discarded.

**`reconcile_renames` and `repair_references`:** After merging, load all entries and follow rename chains (tombstones with `renamed_to` fields) to fix dangling foreign keys. This is the same logic across all platforms — the algorithms are documented below and deterministic.

**`reconcile_renames` algorithm** (source: `kv_store.rs:73-183`):

1. **Collect rename tombstones.** Scan all entries for those with a `renamed_to` field. Build a map of `old_key → new_key`.
2. **Resolve concurrent renames.** If two devices rename the same entity simultaneously (e.g., device A renames `project/foo` → `project/bar`, device B renames `project/foo` → `project/baz`), the rename with the higher `HybridTimestamp` on the tombstone wins. The losing rename's `new_key` entry is itself tombstoned with a `renamed_to` pointing to the winner's `new_key`.
3. **Shortcut chains.** Follow `renamed_to` chains (A → B → C becomes A → C) up to `MAX_CHAIN_DEPTH = 10`. Chains longer than 10 are truncated — this prevents infinite loops from circular renames.

**`repair_references` algorithm** (source: `kv_store.rs:185-272`):

1. **Build rename map.** Collect all `old_key → final_key` mappings from reconciled tombstones.
2. **Resolve chains.** Follow each mapping through rename chains (max depth 10) to find the final destination key.
3. **Scan entries by entity schema.** For each live (non-tombstone) entry, check its foreign key fields based on entity type:
   - `task` → `project_key` (project), `area_key` (area), `parent_task_key` (task), `depends_on` (array of task keys)
   - `project` → `area_key` (area)
4. **Update references.** For single-value FK fields, replace `old_key` with `final_key`. For array FK fields (`depends_on`), replace each element that appears in the rename map. Mark any modified entry as dirty.

**Entity schemas** (source: `conflict.rs:12-28`):

| Entity type | FK fields | Target type | Cardinality |
|-------------|-----------|-------------|-------------|
| `task` | `project_key` | project | single |
| `task` | `area_key` | area | single |
| `task` | `parent_task_key` | task | single |
| `task` | `depends_on` | task | array |
| `project` | `area_key` | area | single |

**`fix_duplicate_task_numbers_kv`:** Runs after merge to resolve task number collisions that can occur when two devices independently assign the same number. The oldest task by `created_at` keeps its number; duplicates are reassigned to `max_existing_number + 1`.

### Cross-Platform Integration Tests

To prevent parameter drift between implementations, CI runs a test matrix:

```
For each pair (language_A, language_B):
    1. language_A encrypts a known plaintext with a known passphrase
    2. language_B decrypts and verifies the plaintext matches
    3. language_A creates entries with known timestamps
    4. language_B merges and verifies the same winner is chosen
```

This catches: wrong Argon2id parameters, wrong nonce size, wrong tag handling, wrong timestamp comparison order, and encoding differences (e.g., UTF-8 normalization of keys).

**Required test vectors:**

Each SDK MUST pass the following test vectors (reference values computed from the Rust implementation):

1. **KDF vector:** Given a known passphrase and the deterministic salt derivation, produce the expected KEK. All implementations MUST produce identical 32-byte KEK output for the same passphrase input.

2. **Encryption round-trip:** Given a known KEK, a known nonce (24 bytes), and a known plaintext, produce the expected ciphertext. Verify decryption recovers the original plaintext. This tests the `[nonce 24B][ciphertext + tag 16B]` wire format.

3. **LWW merge vectors** (5 cases):
   - Different `wall_ms` → higher `wall_ms` wins
   - Same `wall_ms`, different `lamport` → higher `lamport` wins
   - Same `wall_ms` and `lamport`, different `device_id` → lexicographically greater `device_id` wins
   - Tombstone with higher timestamp → tombstone wins (entry is deleted)
   - Tombstone with lower timestamp → live entry wins (tombstone is discarded)

4. **`reconcile_renames` fixture:** A set of entries containing concurrent renames (two devices rename the same project) → expected output after reconciliation (winner determined by `HybridTimestamp`, loser's target gets a `renamed_to` pointing to winner's target, chains are shortcut).

### Alternative: Shared Rust Core via UniFFI / WASM

Instead of native SDKs per platform, the Rust core could be shared directly:

- **iOS:** Compile Rust to a static library, bridge to Swift via [UniFFI](https://mozilla.github.io/uniffi-rs/). Handle-based API where the store and crypto keys live in Rust memory.
- **Web:** Compile Rust to WebAssembly via [wasm-pack](https://rustwasm.github.io/wasm-pack/). Bridge to JS via `wasm-bindgen`.

**Pros of shared Rust core:** Guaranteed byte-for-byte crypto compatibility. Single implementation of merge logic. No risk of parameter drift.

**Cons of shared Rust core:** Cross-compilation complexity (iOS targets, WASM targets). FFI debugging is harder than native code. Larger binaries (~2-5MB for iOS, ~500KB-1MB WASM for web). Edge cases at the boundary (`zeroize` semantics don't cross FFI, `serde_json::Value` requires JSON string serialization). Tighter coupling to Rust toolchain.

**Why we chose native SDKs:** The contract surface is small enough (LWW on 3 fields, standard crypto, HTTP push/pull) that independent implementations are practical. Native SDKs are idiomatic, easier to debug, and have no FFI overhead. The cross-platform integration test suite provides the same correctness guarantee as a shared codebase, with less operational complexity. If the contract surface grows significantly in the future (e.g., per-field CRDT merge), revisiting the shared-core approach would make sense.

**Sync flow example (TypeScript):**

```typescript
async function sync(store: SakuStore, key: CryptoKey) {
    // 1. Pull from server
    const lastCookie = await idb.get('sync_meta', 'last_cookie');
    const resp = await fetch(`/api/v1/kv/tdo?cookie=${lastCookie || ''}`);
    const { entries, next_cookie } = await resp.json();

    // 2. Decrypt + merge
    for (const entry of entries) {
        const plaintext = decrypt(entry.blob, key);
        store.mergeRemoteEntry(entry.key, JSON.parse(plaintext));
    }

    // 3. Get dirty entries, encrypt, push
    const dirty = store.getDirtyEntries();
    if (dirty.length > 0) {
        const encrypted = dirty.map(e => ({
            key: e.key,
            blob: encrypt(JSON.stringify(e.value), key)
        }));
        await fetch('/api/v1/kv/tdo', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ entries: encrypted })
        });
    }

    // 4. Persist + save cookie
    await store.persistToIndexedDB();
    await idb.put('sync_meta', next_cookie, 'last_cookie');
}
```

---

## Detailed Design: Per-Entry Server KV

### Server schema

```sql
CREATE TABLE kv_entries (
    user_id     TEXT NOT NULL,
    tool        TEXT NOT NULL,
    key         TEXT NOT NULL,
    blob        BLOB NOT NULL,          -- encrypted entry
    seq         INTEGER NOT NULL,       -- monotonic sequence number (per user+tool)
    deleted     BOOLEAN DEFAULT FALSE,
    written_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),  -- server-side unix timestamp for GC
    PRIMARY KEY (user_id, tool, key)
);

CREATE INDEX idx_kv_seq ON kv_entries (user_id, tool, seq);

-- Monotonic counter per user+tool pair
CREATE TABLE kv_seq_counters (
    user_id     TEXT NOT NULL,
    tool        TEXT NOT NULL,
    next_seq    INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (user_id, tool)
);
```

The `seq` column replaces the previous `received_at` timestamp. It is a monotonically increasing integer per `(user_id, tool)` pair, incremented on every write. This eliminates clock-drift concerns entirely.

### Server endpoints

```
GET /api/v1/kv/:tool?cookie=<opaque>&limit=<n>
→ { entries: [{ key, blob, seq }], next_cookie: "<opaque>", has_more: bool }

PUT /api/v1/kv/:tool
Body: { entries: [{ key, blob }, ...] }
→ { results: [{ key, seq }], next_cookie: "<opaque>" }

PUT /api/v1/kv/:tool/:key
Body: encrypted blob
→ { seq: <n> }

GET /api/v1/kv/:tool/snapshot
→ { entries: [{ key, blob, seq }], next_cookie: "<opaque>", has_more: bool }
```

**Cookie format (opaque to client):** For v1, the cookie is simply the `seq` value encoded as a string (e.g., `"42"`). The client treats it as an opaque token. This allows us to change the internal representation later (e.g., to a composite cursor for sharding) without breaking clients.

**Pagination:** The `limit` query parameter controls how many entries are returned per request (default: 1000, max: 5000). When `has_more` is `true`, the client MUST make another GET request using the returned `next_cookie` to fetch the remaining entries. This prevents unbounded responses on initial sync or after long offline periods. The `next_cookie` serves as the pagination cursor — each subsequent request resumes from where the previous one left off.

**First sync / snapshot equivalence:** When the cookie is empty or absent, the GET endpoint returns all entries — this is functionally equivalent to the `/snapshot` endpoint. The `/snapshot` endpoint exists as a semantic alias for clarity but behaves identically to a GET with no cookie.

**Batch PUT atomicity:** A batch PUT is executed as a single SQLite transaction. All entries in the batch are written and assigned sequential `seq` values atomically — either all entries are persisted or none are. This guarantees that a client observing any entry from a batch will eventually observe all entries from that batch.

**Sequence counter atomicity:** The `next_seq` read, assignment to entries, and increment all happen within the same SQLite transaction. This guarantees monotonicity — no two entries for the same `(user_id, tool)` pair can receive the same `seq` value, even under concurrent requests (SQLite's write serialization ensures this).

**Batch PUT** is the primary write endpoint. Pushing N dirty entries in one HTTP request is critical for iOS background tasks where time is limited. The single-entry PUT exists for simple cases but the batch endpoint is preferred.

The `snapshot` endpoint returns all entries for initial sync or recovery. Both return encrypted blobs — the server never sees plaintext.

**Transport compression:** Clients and server SHOULD support `Content-Encoding: gzip`. Encrypted blobs compress poorly, but the JSON envelope (keys, metadata) benefits modestly from compression.

### Client sync flow (detailed)

```
1. Load local dirty set (entries with dirty=1 in SQLite, or dirty_keys from sidecar)
2. GET /kv/tdo?cookie={last_cookie}
   → receive changed_entries + next_cookie
3. For each remote entry in changed_entries:
   a. Decrypt with KEK
   b. LWW merge with local entry (HybridTimestamp comparison)
   c. Write merged result to local store
   d. If the remote entry won the merge, clear its dirty flag
4. Load full store into memory
5. Run reconcile_renames + repair_references (may mark more entries dirty)
6. Collect all dirty entries
7. For each dirty entry: encrypt with KEK
8. Batch PUT /kv/tdo with all encrypted dirty entries
9. Clear dirty flags for successfully pushed entries
10. Save next_cookie (from step 2) as last_cookie checkpoint
```

**Why `next_cookie` from step 2, not step 8:** The cookie from the pull response captures the server state at the time of the pull. Any entries pushed by other devices *after* step 2 will have higher sequence numbers and will be fetched on the next sync. The client's own entries pushed in step 8 may be re-fetched next time — LWW merge is idempotent, so this is correct but harmless. This avoids a complex "what if the server state changed during our sync?" problem.

**Why step 4 loads the full store:** `reconcile_renames` and `repair_references` need all entries in memory to follow rename chains and fix dangling foreign keys. Per-entry sync doesn't eliminate the need to load the full store locally — it eliminates the need to *transfer* the full store over the network. The merge runs in-memory on the full local dataset, same as today. This is fine for the expected data sizes (hundreds of entries, not millions).

### Dirty tracking (crash-safe)

Dirty tracking must survive app crashes and process kills. Three approaches depending on platform:

- **iOS (SQLite):** The `dirty` column in the `entries` table (see Local Storage schema). Setting `dirty=1` happens in the same transaction as the entry write — if the app crashes mid-write, either both happen or neither does.
- **CLI (JSON file):** A sidecar file `dirty.json` next to `store.json`, containing `{ "dirty_keys": ["task/abc", ...], "last_cookie": "42" }`. Written after every mutation. If the CLI crashes between writing `store.json` and `dirty.json`, the worst case is that some entries are re-pushed on next sync — LWW merge makes this a no-op on the server.
- **Web (IndexedDB):** The `dirty` field in the IndexedDB object store (see Web App Local Storage schema). Setting `dirty=1` happens in the same IndexedDB transaction as the entry write — atomic by design. Browser crashes or tab kills are handled the same way: the transaction either committed or it didn't.

### Crash safety analysis

LWW idempotency makes every crash scenario recoverable. The worst case is redundant work on the next sync — never data loss or corruption.

| Crash point | State after restart | Recovery action |
|-------------|-------------------|----------------|
| During step 2 (GET pull) | No local state changed. Cookie unchanged. | Next sync re-fetches the same entries. No-op via LWW. |
| During step 3 (decrypt + merge) | Some remote entries merged, some not. Cookie unchanged. | Next sync re-fetches all entries since the old cookie. Already-merged entries are no-ops via LWW. Unmerged entries get merged. |
| During step 5 (reconcile_renames / repair_references) | Partial reference repairs. Some entries dirty, some not. | Next sync re-pulls (same cookie), re-runs full reconciliation. Repairs are idempotent. |
| During step 8 (batch PUT) | Some dirty entries pushed, some not. Cookie unchanged. | Next sync re-pulls (same cookie), re-pushes all dirty entries. Server receives duplicates — LWW makes these no-ops. |
| During step 10 (save cookie) | All entries pushed successfully. Cookie NOT saved. | Next sync re-pulls from old cookie. Re-fetches entries already merged (including own pushed entries). All merges are no-ops via LWW. Redundant work, but correct. |
| Between step 8 and 10 (push succeeded, cookie not saved) | Same as above — redundant re-pull on next sync. | Same as above. |

**Key insight:** Because LWW merge is idempotent (merging the same entry twice produces the same result), and because the cookie is saved *last*, any crash results in at most redundant network traffic on the next sync — never in data loss, duplication, or inconsistency.

### Tombstone garbage collection

Deleted entries become tombstones (`deleted=true` on the server). Without GC, the server table grows forever.

**Policy:**
- Tombstones where `written_at < now() - 90 days` are eligible for GC. The `written_at` column is a server-side timestamp (set when the entry is received, not client-controlled). The server cannot use the encrypted `modified_at` field for GC because it cannot read entry contents.
- The server runs GC periodically (daily cron or on-demand).
- A client that hasn't synced in 90+ days must do a **full snapshot sync** instead of incremental `?cookie=` sync. The server detects this by checking if the client's cookie references a `seq` that has been GC'd, and returns a `410 Gone` status, prompting the client to call `/snapshot`.
- The 90-day window is generous for a personal task manager — if a device hasn't synced in 3 months, a full snapshot is appropriate.

### Entry encryption format

For individual entries, we can use a simpler format than the full file format:

```
[nonce 24B][ciphertext + tag 16B]
```

The KEK encrypts each entry directly (no per-entry DEK). This drops the overhead from 117 bytes to 40 bytes per entry. The KEK is derived once per session from the passphrase via Argon2id (same as today). Since entries are small and we're not worried about individual entry key rotation, the simpler format is sufficient.

### Migration from whole-file encryption

Existing CLI installations use whole-file encryption (random salt stored in the 117-byte file header, per-file DEK encrypted by KEK). The per-entry format uses a different KEK (deterministic salt) and no per-entry DEK. Migration is a one-time operation:

1. **Decrypt with old format.** Read the encrypted `store.json`, parse the 117-byte header (magic, version, random salt, encrypted DEK, nonce), derive the old KEK from passphrase + random salt, decrypt the DEK, decrypt the file body.
2. **Parse entries.** Deserialize the JSON into individual KV entries.
3. **Derive new KEK.** Compute the deterministic salt: `SHA-256(b"saku-sync-kek-salt-v1:" || passphrase_bytes)[0..16]`. Derive the new KEK from passphrase + deterministic salt via Argon2id.
4. **Re-encrypt each entry.** For each entry, encrypt with the new KEK using the entry format: `[nonce 24B][ciphertext + tag 16B]`.
5. **Batch PUT to server.** Push all re-encrypted entries via the batch PUT endpoint.
6. **Save cookie.** Store the `next_cookie` from the server response as the initial checkpoint.
7. **Delete old files.** Remove the old `store.json` and Merkle tree state. Write the new local format (per-entry storage).

**Note:** The old KEK (derived from random salt) and the new KEK (derived from deterministic salt) are different keys — this is expected. Migration re-encrypts everything under the new key. The passphrase itself does not change.

---

## Resolved Design Decisions

### 1. SQLite on the server for per-entry storage

**Decision:** SQLite on the server, single-process deployment. The entries are small (200-500 bytes encrypted), and for the foreseeable user count, SQLite handles this without issue. The `kv_entries` table with `(user_id, tool, key, blob, seq)` is simple and directly queryable for the `?cookie=` pattern.

**Deployment constraint:** SQLite does not handle concurrent writes from multiple server processes. The server must run as a single process (no horizontal scaling behind a load balancer). This is fine for a personal productivity tool with a handful of users. If scale becomes a concern (thousands of concurrent users), migrate to Postgres — the schema and queries are trivially portable.

### 2. Conflict UI on iOS — deferred

**Decision:** Not relevant for the tdo iOS app. LWW handles all structured data conflicts automatically — the user never sees a conflict. Conflict copies (`.conflict.{device_id}`) only apply to unstructured files like notes (nte). We'll design the conflict UI when we add nte to iOS.

### 3. Task numbers are not shown on iOS or web

**Decision:** The iOS and web apps use storage keys internally and do not display task numbers. Task numbers exist as a CLI convenience — typing `tdo done 43` is faster than typing a hash. On iOS and web, you tap/click a task in the UI — there is no need for a short numeric identifier. The `task_number` field still exists in the data model for CLI compatibility, and `fix_duplicate_task_numbers_kv` still runs during merge, but the non-CLI UIs simply don't surface it.

### 4. Manual passphrase entry for v1

**Decision:** The user types their encryption passphrase on iOS during initial setup. This is a one-time operation. The passphrase is stored in the iOS Keychain with `kSecAttrAccessibleAfterFirstUnlock` so subsequent syncs don't require re-entry.

A QR-code-based pairing flow (`tdo sync pair` displays a QR, iOS app scans it) is a natural v2 improvement — better UX, same security model. Deferred to keep v1 scope focused.

### 5. App Group shared container for widgets

**Decision:** The GRDB SQLite database lives in the App Group shared container. Both the main app and widgets read from the same database. Widgets are read-only — they display "Today" tasks but don't modify anything. GRDB's WAL mode handles concurrent reads safely.

Since the Swift SDK writes to SQLite directly, and the SQLite file is in the App Group container, widgets get access without any extra coordination.

### 6. Foreground sync + BGAppRefreshTask for v1

**Decision:** Two sync triggers for v1:

- **Foreground sync** — sync when the app enters foreground. Primary mechanism, 100% reliable.
- **BGAppRefreshTask** — periodic background refresh (~30 seconds). Per-entry sync fits comfortably in this window.

`BGProcessingTask` (longer overnight tasks) and push notifications are deferred. We'll add them if we identify a concrete need — e.g., if tombstone GC or large initial syncs exceed the 30-second background window.

### 7. Auth token lifecycle per platform

**Decision:** Each platform manages auth tokens using its native credential storage:

- **CLI (Rust):** Tokens stored via `keyring` crate (macOS Keychain, Linux secret service). Refresh handled in the Rust sync engine.
- **iOS (Swift):** Tokens stored in iOS Keychain (`kSecAttrAccessibleAfterFirstUnlock`). If a sync request returns 401, the Swift SDK reads the refresh token from Keychain, calls `/auth/refresh`, stores the new access token, and retries.
- **Web (TypeScript):** Access token in `sessionStorage` or JS memory. Refresh token not persisted (see decision #8). User re-authenticates on session expiry.

The sync flow treats auth as a platform concern — the sync algorithm itself is auth-agnostic. Each SDK passes an `Authorization: Bearer <token>` header on HTTP requests and handles 401 responses according to platform conventions.

### 8. Web app passphrase handling

**Decision:** The web app prompts for the encryption passphrase on each session (tab open). The derived key lives in JS memory for the session duration. No passphrase or key material is persisted in browser storage.

This is deliberately more conservative than iOS (where the passphrase is stored in Keychain). The browser threat model is different — browser extensions, XSS, and shared computers make persistent secret storage risky. The trade-off is that the user must enter their passphrase each time they open the web app. This is acceptable for a supplementary interface — users who want persistent, seamless sync use the CLI or iOS app.

### 9. Web app as a progressive enhancement

**Decision:** The web app is a fully functional client, not a read-only dashboard. It can create, edit, complete, and delete tasks — same as CLI and iOS. The sync protocol makes no distinction between client types. However, the web app is expected to be a *supplementary* interface — quick access from any browser, not the primary daily-driver. This justifies the per-session passphrase trade-off (decision #8).

### 10. Per-field LWW — deferred, decision locked

**Decision:** Per-entry LWW is sufficient for v1. We do not implement per-field LWW (where each field carries its own `HybridTimestamp`).

**Rationale:** saku is a single-user personal task manager. The scenario where two devices edit *different fields* of the same task simultaneously (e.g., phone edits title while CLI edits project) is rare. When it does happen, one edit silently wins — annoying but not data-corrupting. Per-field LWW would eliminate this edge case but adds significant complexity: every entry's JSON schema must carry per-field timestamps, the merge algorithm becomes field-aware, and every SDK must implement field-level comparison.

**Why this is a "now or never" decision point:** With multiple platform implementations (Rust, Swift, TypeScript), adding per-field LWW later requires coordinated updates to all SDKs, a wire format migration, and backward compatibility handling. If we determine per-field LWW is needed, the least painful time to add it is before the first non-Rust SDK ships. Current position: per-entry LWW is sufficient. Revisit if user reports indicate silent field-level data loss is a real problem.

### 11. Conflict logging

**Decision:** Each client SHOULD maintain a local-only log of merge conflicts — entries where a remote update overwrote a local change during LWW merge.

**Format:** Append-only JSONL file (e.g., `~/.local/share/saku/sync_conflicts.jsonl`). Each line records: timestamp, entry key, winning device_id, losing device_id, winning `HybridTimestamp`, losing `HybridTimestamp`.

**Purpose:** Debugging "my edit disappeared" reports. The CLI can expose this via `tdo sync log` to show recent conflicts. iOS and web apps MAY surface this in a debug/settings screen.

**Retention:** 1000 entries or 30 days, whichever comes first. Older entries are pruned on write.

---

## Rejected Alternatives

### CloudKit

Apple's CloudKit provides built-in KV sync with encryption, background sync, and push notifications — seemingly a perfect fit for iOS. We reject it because:

- **Apple-only.** CloudKit has no real non-Apple API. The JavaScript API is limited and poorly maintained. A Linux CLI can't use it at all. saku is AGPL, multi-platform by design — tying sync to Apple's infrastructure contradicts this.
- **KV quota is tiny.** `NSUbiquitousKeyValueStore` caps at 1MB total, 1024 keys. A task store could hit that with ~500 tasks.
- **No control.** Can't debug sync issues, can't self-host, can't migrate away. If Apple changes behavior, you're stuck.
- **Doesn't eliminate the server.** Even with CloudKit for iOS-to-Mac sync, Linux CLI users still need the HTTP server. Supporting two sync backends doubles the maintenance burden.

### Replicache-style mutation replay

Replicache's model — push mutations to the server, let the server re-execute them authoritatively, pull diffs — is elegant and well-proven. We reject it because:

- **Incompatible with E2E encryption.** The server can't re-execute mutations it can't read. Our server stores opaque encrypted blobs — it has no concept of "complete task" or "rename project."
- **Server complexity.** The server would need to understand every entity type and every mutation. Our current server is a thin blob store with auth. The mutation-replay model turns it into an application server.
- **Overkill for single-user.** Mutation replay shines for multi-user collaboration (Google Docs, Figma). saku is personal — one user, multiple devices. LWW per entry handles this without the complexity.

### CRDTs (Automerge, Yjs, cr-sqlite)

CRDTs guarantee eventual consistency without a central authority. We reject them because:

- **Overkill for the data model.** saku's data is simple — tasks with flat fields, projects as name references, areas as name references. LWW per entry is sufficient. CRDTs add per-field metadata and unbounded history growth.
- **Don't solve E2E encryption.** You still need a relay server to exchange CRDT operations between devices. The relay is effectively our KV server.
- **Operational complexity.** CRDT documents grow over time (compaction is non-trivial), debugging merge behavior requires understanding the CRDT semantics, and library choices lock you into specific data structures.

### `?since=timestamp` (instead of cookie-based diffing)

The original draft of this RFC used `?since={server_timestamp_ms}` for incremental pulls. We replaced it with cookie-based diffing because:

- **Clock drift.** If the server clock drifts backward (NTP correction, VM migration), entries could be missed entirely. Monotonic sequence numbers don't have this problem.
- **Ambiguous checkpoint.** After a multi-step sync (pull, merge, push), which timestamp should the client save? The one from the pull? The one from the last push? Cookies make this unambiguous — save the `next_cookie` from the pull response, done.
- **Future flexibility.** Cookies are opaque to the client. The server can change the internal representation (e.g., composite cursor for sharding, Lamport clock, hash-based cursor) without breaking any client.

---

## References

### Internal

- [RFC: Natural-Key KV Store](rfc-natural-key-kv-store.md) — the storage migration this builds on
- [Sync Architecture](sync-architecture.md) — current sync design decisions
- [Architecture](architecture.md) — overall saku suite architecture
- `crates/saku-storage/src/kv_store.rs` — KvStore, lww_merge_kv, reconcile_renames, repair_references
- `crates/saku-storage/src/entity.rs` — Entity trait
- `crates/saku-storage/src/timestamp.rs` — HybridTimestamp
- `crates/saku-sync/src/sync_engine.rs` — SyncEngine (whole-file sync loop)
- `crates/saku-sync/src/merkle.rs` — Merkle tree
- `crates/saku-sync/src/conflict.rs` — merge_store_json, tdo_entity_schemas
- `crates/saku-sync/src/backend/mod.rs` — SyncBackend trait
- `crates/saku-sync/src/backend/server.rs` — ServerSyncBackend (presigned URLs + JWT)
- `crates/saku-crypto/src/lib.rs` — encrypt/decrypt, MasterKey
- `crates/saku-server/src/sync/handlers.rs` — presigned URL endpoints
- `crates/saku-server/src/sync/storage.rs` — opendal S3 operator

### External

- [RFC 2119: Key words for use in RFCs](https://www.ietf.org/rfc/rfc2119.txt) — MUST/SHOULD/MAY terminology used in the normative Client SDK Contract section
- [How Replicache Works](https://doc.replicache.dev/concepts/how-it-works) — server-authoritative sync with mutation replay and cookie-based diffing
- [Replicache Row Version Strategy](https://doc.replicache.dev/strategies/row-version) — CVR-based diffing mechanism (basis for our cookie design)
- [Zero Sync Engine](https://zero.rocicorp.dev/) — Replicache successor with partial sync
- [ElectricSQL Durable Streams](https://electric-sql.com/blog/2025/12/09/announcing-durable-streams) — HTTP protocol for resumable real-time streaming with monotonic offsets
- [PowerSync Rust Implementation](https://www.powersync.com/blog/speeding-up-powersync-with-a-sqlite-extension-written-in-rust) — cross-platform sync architecture patterns
- [IndexedDB API](https://developer.mozilla.org/en-US/docs/Web/API/IndexedDB_API) — browser-native structured storage
- [Origin Private File System (OPFS)](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system) — high-performance browser filesystem (potential future optimization)
- [libsodium.js](https://github.com/nicedoc/nicedoc.io) — Argon2id + XChaCha20Poly1305 for TypeScript
- [swift-sodium](https://github.com/nicedoc/nicedoc.io) — Argon2id + XChaCha20Poly1305 for Swift
- [GRDB.swift](https://github.com/groue/GRDB.swift) — SQLite toolkit for Swift (iOS local storage)
