#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/publish-lib.sh <crate>
#   crate: saku-crypto | saku-storage | saku-sync

CRATE="${1:-}"

# 1. Validate args
if [[ -z "$CRATE" ]]; then
  echo "Usage: $0 <crate>"
  echo "  crate: saku-crypto | saku-storage | saku-sync"
  exit 1
fi

if [[ "$CRATE" != "saku-crypto" && "$CRATE" != "saku-storage" && "$CRATE" != "saku-sync" ]]; then
  echo "Error: crate must be 'saku-crypto', 'saku-storage', or 'saku-sync', got '$CRATE'"
  exit 1
fi

# 2. Set crate path
CRATE_PATH="crates/${CRATE}"

if [[ ! -d "$CRATE_PATH" ]]; then
  echo "Error: crate directory not found: $CRATE_PATH"
  exit 1
fi

# 3. Get current version
CURRENT_VERSION=$(grep '^version = ' "${CRATE_PATH}/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "Publishing ${CRATE} v${CURRENT_VERSION}"

# 4. Run checks
echo ""
echo "Running build, tests, and clippy..."
cargo build --release -p "$CRATE"
cargo test -p "$CRATE"
cargo clippy -p "$CRATE" -- -D warnings

# 5. Dry run
echo ""
echo "Running dry-run publish..."
cargo publish -p "$CRATE" --dry-run

# 6. Confirm
echo ""
read -p "Publish ${CRATE} v${CURRENT_VERSION} to crates.io? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
  echo "Cancelled."
  exit 1
fi

# 7. Publish
echo ""
echo "Publishing ${CRATE} v${CURRENT_VERSION} to crates.io..."
cargo publish -p "$CRATE"

echo ""
echo "✓ Published ${CRATE} v${CURRENT_VERSION} successfully."
echo ""
echo "Note: Wait 30-60 seconds before publishing crates that depend on this one."
