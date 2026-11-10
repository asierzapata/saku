# Set up and recover `tdo` sync

This guide describes the **unreleased checkout** of `tdo` and `saku-server`. Published v0.13 binaries may not have the SQLite storage and guarded KV v2 protocol described here. Use matching builds of both before connecting any devices.

Sync is manual: local task commands do not contact the server. `tdo sync run` pulls and merges, then pushes encrypted entries to the server's SQLite database. The server cannot read entry plaintext. Current `tdo` KV sync does not request objects from S3; the server still requires a `[storage]` configuration for its legacy routes.

## Build and prerequisites

From the repository root:

```bash
cargo build -p saku-tdo -p saku-server --locked
```

The binaries are `target/debug/tdo` and `target/debug/saku-server`. A C compiler is needed for bundled SQLite and D-Bus. Other native dependencies, such as OpenSSL, may require their development packages and `pkg-config` on your platform. Linux bundles the D-Bus client library; libdbus development headers and custom D-Bus pkg-config metadata are not required.

Sync credentials use the OS keychain: on Linux, an unlocked Secret Service session such as GNOME Keyring or KWallet; on macOS and Windows, their native credential stores. Bundling the D-Bus client does not start a Secret Service. Local task commands work without one. There is no plaintext credential fallback.

## Configure a server

Create a private server data directory and config file. The following is a loopback-only example; choose a writable absolute database path and replace the placeholder JWT secret:

```toml
[server]
host = "127.0.0.1"
port = 8080

[auth]
jwt_secret = "replace-with-a-strong-random-secret"
access_token_mins = 15
refresh_token_days = 90

[database]
path = "/absolute/path/to/private/server.sqlite3"

[storage]
bucket = "unused-by-tdo-kv"
region = "auto"
endpoint = "http://127.0.0.1:9"
access_key_id = "unused-by-tdo-kv"
secret_access_key = "unused-by-tdo-kv"
```

Protect the config and preserve the JWT secret and auth configuration with the database backup. If legacy object routes are used, configure their storage separately. The complete fields and environment overrides are in `crates/saku-server/config.example.toml` and `crates/saku-server/src/config.rs`.

Create an account interactively, then start the server:

```bash
target/debug/saku-server --config /path/to/config.toml create-user --email user@example.com
target/debug/saku-server --config /path/to/config.toml serve
```

The first command prompts twice for the account password. In another terminal, verify the loopback server:

```bash
curl http://127.0.0.1:8080/api/v1/health
```

For access across machines, configure the server binding and a reverse proxy with TLS appropriate to your environment. Do not expose the loopback example's placeholder secrets. The repository's Compose setup is a legacy development option: it maps host port 4060 to container port 8080 and includes MinIO dependencies; inspect and adapt its configuration before use.

## Rehearse migration on a copy

Existing v9 `store.json` data requires explicit migration. `tdo migrate` imports it only into a pristine SQLite store and retains the JSON source. First rehearse using a copy and inspect the export:

```bash
umask 077
rehearsal_dir=$(mktemp -d)
mkdir -p "$rehearsal_dir"/{tasks,data,config,cache,runtime}
cp /path/to/copied-v9.json "$rehearsal_dir/tasks/store.json"
(
  export TDO_DATA_DIR="$rehearsal_dir/tasks"
  export XDG_DATA_HOME="$rehearsal_dir/data"
  export XDG_CONFIG_HOME="$rehearsal_dir/config"
  export XDG_CACHE_HOME="$rehearsal_dir/cache"
  export XDG_RUNTIME_DIR="$rehearsal_dir/runtime"
  tdo migrate &&
    tdo export "$rehearsal_dir/migrated.json"
)
```

The subshell keeps rehearsal environment variables out of subsequent commands. The output file must not already exist. Compare the export with the source copy and inspect the tasks before continuing. Migration can normalize legacy schedules; do not assume a byte-for-byte export. A non-pristine database cannot be migrated or imported over.

`TDO_DATA_DIR` isolates task data, not sync config, device identity, or keychain credentials. The XDG paths above isolate the first two on Linux, but they do **not** create a private D-Bus/keychain session. Do not log in to a test server from this shell using a real keychain. A sync rehearsal also needs a separate private D-Bus session and keychain.

## Log in and sync

On each device, with the keychain unlocked:

```bash
tdo sync login --server https://your-server.example.com --email user@example.com
tdo sync status
tdo sync run
```

Login prompts for the account password and a separate encryption passphrase. Use the same encryption passphrase on all devices for this account. The password authenticates the login; the keychain stores tokens and the encryption passphrase, not the account password. The sync config stores the server URL and device ID. Check status for a stored passphrase and pending changes.

Run `tdo sync run` whenever synchronization is wanted. Ordinary local commands do not sync. If `TDO_NO_SYNC` is present in the environment, explicit sync runs are disabled. `tdo sync logout` clears local sync credentials and config.

## Coordinate a live rollout

1. Pause **all** KV writers and take consistent backups of each client's complete data directory: legacy v9 JSON, or SQLite including history, pending changes, and cursor. Back up server SQLite and preserve its auth/JWT configuration too.
2. Stage matching server and client binaries. The new server rejects v1 writes with HTTP 428; do not leave old writers active.
3. Upgrade the server and clients. Explicitly migrate existing v9 clients, retaining their JSON sources.
4. Verify each client's local contents before any manual sync. Log in with its unlocked keychain and matching account/passphrase.
5. Sync one client, then the next; repeat as needed to converge. Inspect `tdo sync status` and `tdo history list --json` for pending changes and preserved divergent values.

This is a preparation procedure, not a deployment record. If a client reports a different sync target, export its data and import it into a separate data directory; do not clear or edit `sync_state`.

## Back up and restore

Portable `tdo export` is v9 task JSON, not a full operational backup: it omits history, outbox, cursor, and metadata. `tdo history export` produces a separate local history archive; it is not importable with `tdo import`. Protect both exports as plaintext.

For an operational backup, either stop **all** processes using the data directory and copy the entire directory, including any SQLite WAL and sidecars, or use SQLite's backup API. For example, with an existing source and a destination that does not exist:

```bash
umask 077
python3 - /path/to/store.sqlite3 /path/to/new-backup.sqlite3 <<'PY'
import os
import sqlite3
import sys
from pathlib import Path

source = Path(sys.argv[1]).resolve()
destination = Path(sys.argv[2])
fd = os.open(destination, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
os.close(fd)
with sqlite3.connect(source.as_uri() + "?mode=ro", uri=True) as db:
    with sqlite3.connect(destination) as backup:
        db.backup(backup)
PY
```

Also back up relevant config and keychain access separately. To restore a client operational backup, stop processes using its data directory and restore the complete matching backup. Importing portable JSON into a pristine database restores entries, not operational history or sync state.

To restore the server, stop it first and retain a consistent snapshot of the current database before replacement. Replace the database and remove or replace its stale WAL/sidecars together while the service is stopped. Start the matching v2 server normally. Never edit or restore the live database in place, and do not downgrade to v1 while new writes exist.

## Recovery and conflict behavior

KV v2 cookies have the form `<instance>:<sequence>`. The server generates a new random instance on **every startup**, outside the database. A restart or stop/restore/start invalidates old cursors with HTTP 410: the client atomically requeues all local entries, clears the cursor, and performs a full pull and merge before guarded pushes. Entries absent remotely are not deleted locally; deletions are represented by tombstones.

Each batch checks its keys against the last completed pull. HTTP 409 causes a pull, merge, and retry. Stale-write and cursor-recovery retries share a limit of three retries. Push responses do not advance the pull cursor. Every restart therefore causes a full reconciliation, even if the database did not change. Run only one server instance: alternating independent instances can exhaust the retry limit.

Recovery history retains divergent values locally in plaintext, without automatic pruning or restoration. It cannot reconstruct a version absent from every client and server, including historical unseen overwrites.

## API reference

KV endpoints require a bearer access token. The old v1 KV reads remain available with decimal cookies, but v1 writes return 428. Current clients do not fall back to v1.

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/v1/health` | Health check; no authentication |
| `POST` | `/api/v1/auth/login` | Authenticate and issue tokens |
| `POST` | `/api/v1/auth/refresh` | Rotate tokens |
| `DELETE` | `/api/v1/auth/devices/{device_id}` | Revoke a device |
| `GET` | `/api/v2/kv/{tool}` | Incremental pull |
| `GET` | `/api/v2/kv/{tool}/snapshot` | Full snapshot page |
| `PUT` | `/api/v2/kv/{tool}` | Guarded batch with `base_cookie` |
| `GET` | `/api/v1/kv/{tool}` and `/api/v1/kv/{tool}/snapshot` | Legacy reads |
| `PUT` | `/api/v1/kv/{tool}` and `/api/v1/kv/{tool}/{key}` | Rejected legacy writes |

Legacy object-sync routes remain available separately; see the historical [distributed KV RFC](rfc-distributed-kv-sync.md) and server routes.

## Troubleshooting

- **Undecryptable entry:** the failed page does not advance the cursor and no push follows it. Check that devices use the same passphrase; do not overwrite the entry to bypass the error.
- **Guarded KV v2 unavailable:** upgrade the server and clients together; there is no fallback to old-server writes.
- **Credentials or keychain unavailable:** unlock the OS keychain, run `tdo sync status`, and log in again if needed. On Linux, ensure a Secret Service is running in the intended session.
- **Target changed:** export and import into a separate data directory rather than editing internal sync state.
- **Repeated 410 or 409:** check for alternating server instances or concurrent writers, then run sync again after resolving the cause. Local changes remain pending.
