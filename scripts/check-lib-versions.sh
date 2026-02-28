#!/usr/bin/env bash
set -euo pipefail

# Checks if library crate versions need to be published to crates.io
# Usage: ./scripts/check-lib-versions.sh
#
# Two checks per crate:
#   1. Version number differs from crates.io → needs publish
#   2. Version matches, but source files changed since the version was last
#      bumped → forgot to bump version (needs version bump + publish)

LIBS=("saku-crypto" "saku-storage" "saku-sync")

echo "Checking library crate versions..."
echo ""

NEEDS_PUBLISH=()
NEEDS_BUMP=()

for crate in "${LIBS[@]}"; do
  CRATE_DIR="crates/${crate}"

  # Get local version
  LOCAL_VERSION=$(grep '^version = ' "${CRATE_DIR}/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/')

  # Get published version from crates.io
  PUBLISHED_VERSION=$(curl -s "https://crates.io/api/v1/crates/${crate}" 2>/dev/null | jq -r '.versions[0].num // "none"')

  if [[ "$PUBLISHED_VERSION" == "none" ]]; then
    echo "❌ ${crate} v${LOCAL_VERSION} - NOT PUBLISHED (first time)"
    NEEDS_PUBLISH+=("$crate")
  elif [[ "$LOCAL_VERSION" != "$PUBLISHED_VERSION" ]]; then
    echo "⚠️  ${crate} v${LOCAL_VERSION} - local differs from published v${PUBLISHED_VERSION}"
    NEEDS_PUBLISH+=("$crate")
  else
    # Version matches — check if source changed since the version was last bumped.
    # Find the commit that last changed the version line in this crate's Cargo.toml.
    LAST_BUMP_COMMIT=$(git log -1 --format="%H" -G '^version = ' -- "${CRATE_DIR}/Cargo.toml" 2>/dev/null || true)

    if [[ -n "$LAST_BUMP_COMMIT" ]]; then
      # Check if any source files changed since that commit
      CHANGED_FILES=$(git diff --name-only "${LAST_BUMP_COMMIT}" HEAD -- "${CRATE_DIR}/src/" 2>/dev/null || true)

      if [[ -n "$CHANGED_FILES" ]]; then
        NUM_CHANGED=$(echo "$CHANGED_FILES" | wc -l | tr -d ' ')
        echo "❌ ${crate} v${LOCAL_VERSION} - version matches crates.io but ${NUM_CHANGED} source file(s) changed since last version bump"
        NEEDS_BUMP+=("$crate")
      else
        echo "✓ ${crate} v${LOCAL_VERSION} - up to date"
      fi
    else
      echo "✓ ${crate} v${LOCAL_VERSION} - up to date"
    fi
  fi
done

echo ""

if [[ ${#NEEDS_BUMP[@]} -gt 0 ]]; then
  echo "The following crates have unpublished source changes (bump version first):"
  for crate in "${NEEDS_BUMP[@]}"; do
    echo "  - $crate"
  done
  echo ""
fi

if [[ ${#NEEDS_PUBLISH[@]} -gt 0 ]]; then
  echo "The following crates need to be published:"
  for crate in "${NEEDS_PUBLISH[@]}"; do
    echo "  - $crate"
  done
  echo ""
fi

if [[ ${#NEEDS_BUMP[@]} -gt 0 || ${#NEEDS_PUBLISH[@]} -gt 0 ]]; then
  if [[ ${#NEEDS_BUMP[@]} -gt 0 ]]; then
    echo "First bump versions, then run: ./scripts/publish-all-libs.sh"
  else
    echo "Run: ./scripts/publish-all-libs.sh"
  fi
  exit 1
else
  echo "✓ All library crates are published and up to date!"
fi
