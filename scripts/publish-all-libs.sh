#!/usr/bin/env bash
set -euo pipefail

# Publishes all library crates in dependency order
# Usage: ./scripts/publish-all-libs.sh

echo "This will publish all library crates to crates.io in order:"
echo "  1. saku-crypto"
echo "  2. saku-storage"  
echo "  3. saku-sync"
echo ""
read -p "Continue? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
  echo "Cancelled."
  exit 1
fi

# 1. Publish saku-crypto (no internal deps)
echo ""
echo "========================================="
echo "Publishing saku-crypto..."
echo "========================================="
./scripts/publish-lib.sh saku-crypto

# 2. Publish saku-storage (no internal deps)
echo ""
echo "========================================="
echo "Publishing saku-storage..."
echo "========================================="
./scripts/publish-lib.sh saku-storage

# 3. Wait for crates.io to index
echo ""
echo "Waiting 60 seconds for crates.io to index..."
sleep 60

# 4. Publish saku-sync (depends on crypto + storage)
echo ""
echo "========================================="
echo "Publishing saku-sync..."
echo "========================================="
./scripts/publish-lib.sh saku-sync

echo ""
echo "========================================="
echo "✓ All library crates published!"
echo "========================================="
echo ""
echo "You can now publish saku-tdo using: ./scripts/release.sh tdo patch"
