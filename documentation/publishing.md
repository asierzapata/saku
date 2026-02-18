# Publishing Crates to crates.io

## Prerequisites

- `gh` CLI authenticated (`gh auth login`)
- `CRATES_IO_TOKEN` set in GitHub repository secrets (Settings → Secrets → Actions)

## Releasing a crate

```bash
./scripts/release.sh <crate> <bump>
```

- `crate`: `tdo` or `nte`
- `bump`: `patch`, `minor`, or `major`

**Example:**
```bash
./scripts/release.sh tdo patch
```

The script will:
1. Verify the working tree is clean and `main` is up to date
2. Bump the version in `Cargo.toml` and update `Cargo.lock`
3. Commit and push directly to `main`
4. Create and push the version tag (e.g. `saku-tdo-v0.2.1`)
5. Create a GitHub Release with an auto-generated changelog

The `publish.yml` workflow then triggers automatically on the pushed tag and publishes the crate to crates.io.

## Tag format

- `saku-tdo-v{version}` — e.g. `saku-tdo-v0.2.1`
- `saku-nte-v{version}` — e.g. `saku-nte-v0.1.1`

## Troubleshooting

- **Dirty working tree**: commit or stash changes before running the script
- **401 Unauthorized on publish**: check `CRATES_IO_TOKEN` in GitHub Secrets
- **Version mismatch on publish**: the tag version must match `Cargo.toml` — this is handled automatically by the script
