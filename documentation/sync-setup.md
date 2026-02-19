# Saku Sync — Server Setup & Client Configuration

This guide covers how to deploy `saku-server` and configure `tdo` to sync through it.

## Overview

Saku sync uses a self-hosted coordination server that issues presigned S3-compatible URLs. Your encrypted data is stored in an S3-compatible bucket (AWS S3, Cloudflare R2, or MinIO for local dev). The server never sees your unencrypted data — all encryption happens client-side.

**Components:**

- **saku-server** — Auth + coordination server (issues presigned URLs, manages users/devices)
- **S3-compatible storage** — Where encrypted sync data lives (MinIO, AWS S3, Cloudflare R2)
- **tdo** — CLI client that authenticates and syncs via presigned URLs

## Quick Start (Docker, local dev)

The fastest way to try sync locally. This starts `saku-server` and MinIO together.

```bash
cd crates/saku-server
docker compose up -d
```

This starts:
- `saku-server` on `http://localhost:8080`
- MinIO (S3-compatible) on `http://localhost:9000` (console at `http://localhost:9001`)
- Auto-creates the `saku-sync` bucket in MinIO

Create a user account:

```bash
docker compose exec saku-server saku-server \
  --config /etc/saku-server/config.toml \
  create-user --email you@example.com
```

You'll be prompted for a password (twice to confirm). Then configure `tdo`:

```bash
tdo sync login --server http://localhost:8080 --email you@example.com
```

You'll be prompted for:
1. **Password** — the account password you just set
2. **Encryption passphrase** — a separate passphrase used to encrypt your data end-to-end (the server never sees this)

That's it. From now on, every mutating `tdo` command (`add`, `done`, `move`, `delete`, etc.) automatically syncs in the background.

## Production Deployment

### 1. Prepare storage

Create an S3-compatible bucket for sync data. Any S3-compatible service works:

| Provider | Endpoint format |
|---|---|
| AWS S3 | `https://s3.<region>.amazonaws.com` |
| Cloudflare R2 | `https://<account-id>.r2.cloudflarestorage.com` |
| MinIO | `http://<host>:9000` |

Create a bucket (e.g., `saku-sync`) and an access key with read/write permissions on that bucket.

### 2. Write the config file

Copy the example config and edit it:

```bash
cp crates/saku-server/config.example.toml /etc/saku-server/config.toml
```

```toml
[server]
host = "0.0.0.0"
port = 8080

[auth]
# Generate a strong random secret: openssl rand -hex 32
jwt_secret = "<random-secret>"
access_token_mins = 15
refresh_token_days = 90

[database]
path = "/data/saku-server.db"

[storage]
bucket = "saku-sync"
region = "auto"                    # or "us-east-1", etc.
endpoint = "https://..."           # your S3-compatible endpoint
access_key_id = "..."
secret_access_key = "..."
```

Every config value can be overridden with environment variables using the `SAKU__SECTION__KEY` pattern:

| Config key | Environment variable |
|---|---|
| `server.host` | `SAKU__SERVER__HOST` |
| `server.port` | `SAKU__SERVER__PORT` |
| `auth.jwt_secret` | `SAKU__AUTH__JWT_SECRET` |
| `database.path` | `SAKU__DATABASE__PATH` |
| `storage.bucket` | `SAKU__STORAGE__BUCKET` |
| `storage.endpoint` | `SAKU__STORAGE__ENDPOINT` |
| `storage.access_key_id` | `SAKU__STORAGE__ACCESS_KEY_ID` |
| `storage.secret_access_key` | `SAKU__STORAGE__SECRET_ACCESS_KEY` |

### 3. Start the server

**Option A: Docker**

```bash
docker run -d \
  --name saku-server \
  -p 8080:8080 \
  -v /path/to/config.toml:/etc/saku-server/config.toml:ro \
  -v saku-data:/data \
  saku-server
```

**Option B: Binary**

```bash
cargo install --path crates/saku-server
saku-server --config /etc/saku-server/config.toml
```

Or run directly with `cargo`:

```bash
cargo run --release --package saku-server -- --config /path/to/config.toml
```

The server logs to stderr. Set `RUST_LOG=saku_server=debug` for verbose output.

### 4. Create user accounts

There is no public registration. An admin creates accounts via the CLI:

```bash
saku-server --config /etc/saku-server/config.toml create-user --email user@example.com
```

This prompts for a password (entered twice). The password is hashed with bcrypt before storage.

### 5. Verify the server is running

```bash
curl http://localhost:8080/api/v1/health
# {"status":"ok"}
```

## Client Configuration

### Log in

```bash
tdo sync login --server https://your-server.example.com --email user@example.com
```

You'll be prompted for two things:

1. **Password** — your account password (authenticates with the server)
2. **Encryption passphrase** — encrypts your data client-side before it leaves your machine

Both are stored securely in your OS keychain (macOS Keychain, GNOME Keyring, Windows Credential Manager).

The login command also writes `~/.config/saku/sync.toml` with the server URL and your device ID.

### Check sync status

```bash
tdo sync status
```

Shows:
- Server URL
- Device ID
- Whether the encryption passphrase is stored in the keychain

### Log out

```bash
tdo sync logout
```

Clears all keychain entries (access token, refresh token, encryption passphrase) and deletes `~/.config/saku/sync.toml`.

## How Sync Works

After login, sync is automatic and transparent:

1. You run a mutating command (e.g., `tdo add "Buy groceries"`)
2. The command executes locally as usual
3. After the mutation, `tdo` automatically syncs in the background:
   - Detects local changes via file hashing
   - Compares Merkle tree roots with the server to detect remote changes
   - Pulls remote changes and merges (LWW for JSON, conflict copies for other files)
   - Encrypts and pushes local changes via presigned S3 URLs
4. If the server is unreachable, sync is silently skipped — your data is always local-first

Sync errors are printed as warnings but never block your workflow.

## Multi-Device Setup

To sync between multiple machines:

1. Install `tdo` on each machine
2. Run `tdo sync login` on each with the **same email** and **same encryption passphrase**
3. Each device gets its own device ID (auto-generated on first run at `~/.local/share/saku/device_id`)
4. Changes sync automatically after every mutation

The encryption passphrase must match across devices — using different passphrases means devices cannot decrypt each other's data.

## Security Model

- **End-to-end encryption**: All data is encrypted client-side with ChaCha20-Poly1305 before upload. The server and S3 storage only see ciphertext.
- **Key derivation**: Your encryption passphrase is run through Argon2 to produce a master key (KEK). Each file gets a unique random data encryption key (DEK).
- **Auth tokens**: Short-lived JWT access tokens (15 min) with long-lived refresh tokens (90 days). Refresh tokens are stored as SHA-256 hashes in the server DB.
- **No public registration**: Only an admin with server access can create accounts.
- **Credential storage**: Tokens and passphrase are stored in the OS keychain, not on disk.

## API Reference

All endpoints are under `/api/v1/`. Auth endpoints are public; sync endpoints require a `Bearer` JWT.

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/health` | No | Health check |
| `POST` | `/auth/login` | No | Device login → access + refresh tokens |
| `POST` | `/auth/refresh` | No | Rotate access token |
| `DELETE` | `/auth/devices/{id}` | Yes | Revoke a device |
| `GET` | `/sync/{tool}/download-url?path=...` | Yes | Get presigned download URL |
| `POST` | `/sync/{tool}/upload-url` | Yes | Get presigned upload URL |
| `POST` | `/sync/{tool}/confirm-upload` | Yes | Confirm upload + update quota |
| `GET` | `/sync/{tool}/metadata?path=...` | Yes | Get object metadata |

## Troubleshooting

**"sync failed: No refresh token available"**
Your session has expired or you haven't logged in. Run `tdo sync login` again.

**"sync failed: ServerSyncBackend requires the 'server' feature"**
`tdo` was compiled without the `sync` feature. Reinstall with: `cargo install --path crates/tdo` (the `sync` feature is on by default).

**Sync silently does nothing**
Check `tdo sync status`. If sync is not configured, run `tdo sync login`. If it is configured but the server is unreachable, sync is silently skipped (local-first design).

**Different data on different devices**
Make sure all devices use the same encryption passphrase. Different passphrases produce different encryption keys and devices cannot decrypt each other's data.
