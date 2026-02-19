# Release Scripts

This directory contains scripts for publishing crates to crates.io and creating GitHub releases.

## Quick Reference

```bash
# Check if library crates need publishing
./scripts/check-lib-versions.sh

# Publish all library crates (first time or when versions change)
./scripts/publish-all-libs.sh

# Release a binary crate (automatically checks library versions)
./scripts/release.sh tdo patch   # or minor, or major
./scripts/release.sh nte patch   # or minor, or major
```

## How It Works

### Automatic Version Checking

The `release.sh` script **automatically checks** if library crates need to be published:
- Compares local versions in `Cargo.toml` with versions on crates.io
- Stops the release if library versions are out of sync
- Tells you to run `./scripts/publish-all-libs.sh` first

### When Do Library Crates Need Publishing?

Library crates need to be published when:
1. **First time**: Never published before
2. **Version changed**: You manually bumped the version in the library's `Cargo.toml`

You can check anytime by running:
```bash
./scripts/check-lib-versions.sh
```

## Library Crates (Internal Dependencies)

Library crates (`saku-crypto`, `saku-storage`, `saku-sync`) are published to crates.io but don't get GitHub releases or tags.

### First-time setup: Publish all libraries

```bash
./scripts/publish-all-libs.sh
```

This publishes all library crates in the correct dependency order. You only need to do this once (or when versions change).

### Publish a single library (after version bump)

When you make changes to a library and manually bump its version in `Cargo.toml`:

```bash
./scripts/publish-lib.sh saku-crypto
# or
./scripts/publish-lib.sh saku-storage
# or  
./scripts/publish-lib.sh saku-sync
```

## Binary Crates (User-facing applications)

Binary crates (`tdo`, `nte`) get full releases with:
- Version bump in Cargo.toml
- Git commit and tag
- GitHub release with changelog
- Published to crates.io
- **Automatic check** that library dependencies are published

### Release a binary crate

```bash
./scripts/release.sh tdo patch   # or minor, or major
# or
./scripts/release.sh nte patch   # or minor, or major
```

The script will automatically:
1. Check if library crates need publishing (and stop if they do)
2. Run tests and checks
3. Bump version
4. Create tag and GitHub release
5. Publish to crates.io

## Workflow Examples

### First time publishing everything

```bash
# 1. Publish library crates first
./scripts/publish-all-libs.sh

# 2. Then release the binary
./scripts/release.sh tdo patch
```

### Regular release (no library changes)

```bash
# Just release the binary - script checks libraries automatically
./scripts/release.sh tdo patch
```

### After updating a library

```bash
# 1. Make changes to saku-storage
# 2. Manually bump version in crates/saku-storage/Cargo.toml
# 3. Publish the library
./scripts/publish-lib.sh saku-storage

# 4. Release the binary that uses it
./scripts/release.sh tdo minor
```

## Version Strategy

- **Library crates**: Manual versioning, publish only when needed
  - Breaking changes: Bump major version
  - New features: Bump minor version  
  - Bug fixes: Bump patch version
- **Binary crates**: Automated versioning via release script
  - User-facing changes determine the bump type
