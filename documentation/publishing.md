# Publishing Crates to crates.io

## Prerequisites

- `gh` CLI authenticated (`gh auth login`)
- `cargo` authenticated (`cargo login`)

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
2. Build, test, and run clippy (fails fast before touching anything)
3. Bump the version in `Cargo.toml` and update `Cargo.lock`
4. Commit and push directly to `main`
5. Create and push the version tag (e.g. `saku-tdo-v0.2.1`)
6. Create a GitHub Release with an auto-generated changelog
7. Publish to crates.io

## Tag format

- `saku-tdo-v{version}` — e.g. `saku-tdo-v0.2.1`
- `saku-nte-v{version}` — e.g. `saku-nte-v0.1.1`

## Troubleshooting

- **Dirty working tree**: commit or stash changes before running the script
- **Publish fails (not logged in)**: run `cargo login` with your crates.io token
- **Build/test/clippy failure**: fix the issues — the script aborts before making any changes
