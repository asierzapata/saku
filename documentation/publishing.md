# Releasing Saku (`tdo` and `saku-server`)

This guide covers cutting a new `tdo` release with `cargo-release` and `cargo-dist` (Homebrew tap + shell installer) and deploying `saku-server` via GHCR.

## Prerequisites

- Push access to `asierzapata/saku`.
- The `HOMEBREW_TAP_TOKEN` repository secret must be set on `asierzapata/saku`
  (a fine-grained PAT scoped to `asierzapata/homebrew-saku` with
  **Contents: Read and write**). Without it, the formula bump step fails and
  falls back to a manual step — see [If the formula bump fails](#if-the-formula-bump-fails).

## Cut a `tdo` release

1. Cut a release using `cargo release`:
   ```bash
   cargo release patch --execute # or minor / major
   ```
   Or manually bump `version` in `Cargo.toml` / workspace crates, commit, tag, and push:
   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
   The tag must point at a commit already on `main` — pushing the tag triggers `.github/workflows/release.yml`.
2. `.github/workflows/release.yml` runs `cargo-dist`, which:
   - builds `tdo` for `x86_64-apple-darwin`, `aarch64-apple-darwin`,
     `x86_64-unknown-linux-gnu`, and `aarch64-unknown-linux-gnu`,
   - publishes a GitHub Release with archives, checksums, and `saku-tdo-installer.sh`,
   - updates `Formula/saku.rb` in `asierzapata/homebrew-saku` with the new version,
     download URLs, and checksums, and pushes that commit directly to the tap repository's default branch.
3. Watch the workflow run:
   ```bash
   gh run watch --workflow=release.yml
   # or
   gh run list --workflow=release.yml
   ```

## Install or upgrade on a machine

Via Homebrew:

```bash
brew update
brew install asierzapata/saku/saku
# or upgrade an existing install:
brew upgrade asierzapata/saku/saku
```

`brew update` is required before `brew upgrade` so Homebrew refreshes the tap repository.

Via the shell installer:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/asierzapata/saku/releases/latest/download/saku-tdo-installer.sh | sh
```

Verify the installed version:

```bash
tdo --version
```

## If the formula bump fails

If the `publish-homebrew-formula` job fails (for example, an expired or missing `HOMEBREW_TAP_TOKEN`), update `Formula/saku.rb` in `asierzapata/homebrew-saku` manually:

1. Download the `.sha256` files or generated `saku.rb` artifact from the GitHub Release / workflow run.
2. In `Formula/saku.rb`, bump `version` and update the `url`/`sha256` pairs under `on_macos` and `on_linux`.
3. Commit and push directly to the tap repository's default branch.

## Deploying `saku-server`

`saku-server` is published to GitHub Container Registry (`ghcr.io/asierzapata/saku-server`) via `.github/workflows/docker-publish.yml`, which runs automatically on pushes to `main` touching server/sync crates and on `v*` tags.

To deploy or update the server where `deploy/docker-compose.yml` is hosted:

```bash
docker compose pull
docker compose up -d
```

See [documentation/sync-setup.md](sync-setup.md) for coordinated server/client upgrades, backup, and recovery procedures.
