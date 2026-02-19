#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/release.sh <crate> <bump>
#   crate: tdo | nte
#   bump:  patch | minor | major

CRATE="${1:-}"
BUMP="${2:-}"

# 1. Validate args
if [[ -z "$CRATE" || -z "$BUMP" ]]; then
  echo "Usage: $0 <crate> <bump>"
  echo "  crate: tdo | nte"
  echo "  bump:  patch | minor | major"
  exit 1
fi

if [[ "$CRATE" != "tdo" && "$CRATE" != "nte" ]]; then
  echo "Error: crate must be 'tdo' or 'nte', got '$CRATE'"
  exit 1
fi

if [[ "$BUMP" != "patch" && "$BUMP" != "minor" && "$BUMP" != "major" ]]; then
  echo "Error: bump must be 'patch', 'minor', or 'major', got '$BUMP'"
  exit 1
fi

# 2. Check clean working tree
if [[ -n "$(git status --porcelain)" ]]; then
  echo "Error: working tree is dirty. Commit or stash changes before releasing."
  git status --short
  exit 1
fi

# 3. Ensure on main and up to date
git checkout main
git pull origin main

# 4. Set crate info
if [[ "$CRATE" == "tdo" ]]; then
  PACKAGE="saku-tdo"
  CRATE_PATH="crates/tdo"
  TAG_PREFIX="saku-tdo-v"
elif [[ "$CRATE" == "nte" ]]; then
  PACKAGE="saku-nte"
  CRATE_PATH="crates/nte"
  TAG_PREFIX="saku-nte-v"
fi

# 5. Get current version
CURRENT_VERSION=$(grep '^version = ' "${CRATE_PATH}/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "Current version: $CURRENT_VERSION"

# 6. Compute new version
MAJOR=$(echo "$CURRENT_VERSION" | cut -d. -f1)
MINOR=$(echo "$CURRENT_VERSION" | cut -d. -f2)
PATCH=$(echo "$CURRENT_VERSION" | cut -d. -f3)

case "$BUMP" in
  major) NEW_VERSION="$((MAJOR + 1)).0.0" ;;
  minor) NEW_VERSION="${MAJOR}.$((MINOR + 1)).0" ;;
  patch) NEW_VERSION="${MAJOR}.${MINOR}.$((PATCH + 1))" ;;
esac

TAG="${TAG_PREFIX}${NEW_VERSION}"
echo "New version: $NEW_VERSION  (tag: $TAG)"

# 7. Get last tag
LAST_TAG=$(git tag --list "${TAG_PREFIX}*" --sort=-version:refname | head -1)
echo "Last tag: ${LAST_TAG:-(none, first release)}"

# 8. Run checks before touching anything
echo ""
echo "Running build, tests, and clippy..."
cargo build --release -p "$PACKAGE"
cargo test -p "$PACKAGE"
cargo clippy -p "$PACKAGE" -- -D warnings

# 9. Generate changelog
if [[ -z "$LAST_TAG" ]]; then
  ALL_COMMITS=$(git log --pretty=format:"- %s (%h)" --no-merges)
else
  ALL_COMMITS=$(git log "${LAST_TAG}..HEAD" --pretty=format:"- %s (%h)" --no-merges)
fi

# Filter commits for this crate (matching prefix or no prefix)
COMMITS=$(echo "$ALL_COMMITS" | grep -E "^- \(${CRATE}\)" || true)

REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)

CHANGELOG="## What's Changed

"
if [[ -z "$COMMITS" ]]; then
  CHANGELOG+="- No changes since last release"
else
  CHANGELOG+="$COMMITS"
fi
CHANGELOG+="
"
if [[ -n "$LAST_TAG" ]]; then
  CHANGELOG+="
**Full Changelog**: https://github.com/${REPO}/compare/${LAST_TAG}...${TAG}"
fi

echo ""
echo "--- Changelog ---"
echo "$CHANGELOG"
echo "-----------------"
echo ""

# 10. Bump version in Cargo.toml
sed -i '' "s/^version = \"${CURRENT_VERSION}\"/version = \"${NEW_VERSION}\"/" "${CRATE_PATH}/Cargo.toml"

# 11. Update Cargo.lock
cargo update -p "$PACKAGE"

# 12. Commit
git add "${CRATE_PATH}/Cargo.toml" Cargo.lock
git commit -m "chore(release): bump ${CRATE} to v${NEW_VERSION}"

# 13. Push directly to main
git push origin main

# 14. Create and push tag
git tag -a "$TAG" -m "Release $TAG"
git push origin "$TAG"

# 15. Create GitHub Release
gh release create "$TAG" \
  --title "$TAG" \
  --notes "$CHANGELOG"

# 16. Publish to crates.io
echo ""
echo "Publishing $PACKAGE to crates.io..."
cargo publish -p "$PACKAGE"

echo ""
echo "Released and published $TAG successfully."
