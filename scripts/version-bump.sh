#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/version-bump.sh <new-version>
# Examples:
#   ./scripts/version-bump.sh 0.2.0          # stable release
#   ./scripts/version-bump.sh 0.2.0-alpha.1  # pre-release
#   ./scripts/version-bump.sh 0.2.0-beta.1   # pre-release
#   ./scripts/version-bump.sh 0.2.0-rc.1     # release candidate

VERSION="${1:?Usage: $0 <version>}"

# Validate semver format
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
  echo "Error: '$VERSION' is not a valid semver version"
  echo "Expected: MAJOR.MINOR.PATCH[-PRERELEASE]"
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# Ensure clean working tree
if [ -n "$(git status --porcelain)" ]; then
  echo "Error: working tree is not clean. Commit or stash changes first."
  exit 1
fi

CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
echo "Bumping version: $CURRENT -> $VERSION"

# Update workspace version in root Cargo.toml
sed -i "s/^version = \"$CURRENT\"/version = \"$VERSION\"/" Cargo.toml

# Verify the change
NEW=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
if [ "$NEW" != "$VERSION" ]; then
  echo "Error: failed to update Cargo.toml"
  exit 1
fi

# Update Cargo.lock
cargo check --workspace 2>/dev/null || true

echo ""
echo "Version updated to $VERSION"
echo ""
echo "Next steps:"
echo "  1. Update CHANGELOG.md (or run: git-cliff --unreleased --tag v$VERSION --prepend CHANGELOG.md)"
echo "  2. git add Cargo.toml Cargo.lock CHANGELOG.md"
echo "  3. git commit -m 'chore: release v$VERSION'"
echo "  4. git tag v$VERSION"
echo "  5. git push origin main --tags"
