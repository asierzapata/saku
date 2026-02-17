# Publishing Crates to crates.io

This project uses GitHub Actions to automatically publish crates to crates.io when version tags are pushed.

## Prerequisites

1. **Add crates.io token to GitHub Secrets**:
   - Go to crates.io and generate an API token at https://crates.io/me
   - Add it to your GitHub repository secrets as `CRATES_IO_TOKEN`
   - Navigate to: Repository Settings → Secrets and variables → Actions → New repository secret

## How to Publish

### For saku-tdo crate:

1. Update the version in `crates/tdo/Cargo.toml`
2. Commit the changes
3. Create and push a tag:
   ```bash
   git tag saku-tdo-v0.1.0
   git push origin saku-tdo-v0.1.0
   ```

### For saku-nte crate:

1. Update the version in `crates/nte/Cargo.toml`
2. Commit the changes
3. Create and push a tag:
   ```bash
   git tag saku-nte-v0.1.0
   git push origin saku-nte-v0.1.0
   ```

## What the Workflow Does

When you push a version tag, the workflow will:
1. ✅ Determine which crate to publish based on the tag name
2. ✅ Verify the tag version matches the version in `Cargo.toml`
3. ✅ Build the crate in release mode
4. ✅ Run all tests
5. ✅ Run clippy checks
6. ✅ Publish to crates.io (if all checks pass)

## Tag Format

- **saku-tdo**: `saku-tdo-v{version}` (e.g., `saku-tdo-v0.1.0`)
- **saku-nte**: `saku-nte-v{version}` (e.g., `saku-nte-v0.1.0`)

The version number must match what's in the crate's `Cargo.toml` file.

## Troubleshooting

- **401 Unauthorized**: Check that `CRATES_IO_TOKEN` is set correctly in GitHub Secrets
- **403 Forbidden**: The crate name is already taken or you don't have permission
- **Version mismatch**: Ensure the tag version matches `Cargo.toml`
- **Build/test failures**: Fix the issues before the crate can be published
