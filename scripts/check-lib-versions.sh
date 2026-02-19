#!/usr/bin/env bash
set -euo pipefail

# Checks if library crate versions need to be published to crates.io
# Usage: ./scripts/check-lib-versions.sh

LIBS=("saku-crypto" "saku-storage" "saku-sync")

echo "Checking library crate versions..."
echo ""

NEEDS_PUBLISH=()

for crate in "${LIBS[@]}"; do
  # Get local version
  LOCAL_VERSION=$(grep '^version = ' "crates/${crate}/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/')
  
  # Get published version from crates.io
  PUBLISHED_VERSION=$(curl -s "https://crates.io/api/v1/crates/${crate}" 2>/dev/null | jq -r '.versions[0].num // "none"')
  
  if [[ "$PUBLISHED_VERSION" == "none" ]]; then
    echo "❌ ${crate} v${LOCAL_VERSION} - NOT PUBLISHED (first time)"
    NEEDS_PUBLISH+=("$crate")
  elif [[ "$LOCAL_VERSION" != "$PUBLISHED_VERSION" ]]; then
    echo "⚠️  ${crate} v${LOCAL_VERSION} - local differs from published v${PUBLISHED_VERSION}"
    NEEDS_PUBLISH+=("$crate")
  else
    echo "✓ ${crate} v${LOCAL_VERSION} - up to date"
  fi
done

echo ""

if [[ ${#NEEDS_PUBLISH[@]} -eq 0 ]]; then
  echo "✓ All library crates are published and up to date!"
else
  echo "The following crates need to be published:"
  for crate in "${NEEDS_PUBLISH[@]}"; do
    echo "  - $crate"
  done
  echo ""
  echo "Run: ./scripts/publish-all-libs.sh"
  exit 1
fi
