#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/version-bump.sh <new-version>
#        ./scripts/version-bump.sh --minor
#        ./scripts/version-bump.sh --patch
# Examples:
#   ./scripts/version-bump.sh 0.3.0          # stable release
#   ./scripts/version-bump.sh 0.3.0-alpha.1  # pre-release
#   ./scripts/version-bump.sh --minor         # auto-bump minor (0.2.0 -> 0.3.0)
#   ./scripts/version-bump.sh --patch         # auto-bump patch (0.2.0 -> 0.2.1)

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

case "${1:?Usage: $0 <version> | --major | --minor | --patch}" in
  --major)
    IFS='.' read -r MAJOR MINOR PATCH <<< "${CURRENT%%-*}"
    VERSION="$((MAJOR + 1)).0.0"
    ;;
  --minor)
    IFS='.' read -r MAJOR MINOR PATCH <<< "${CURRENT%%-*}"
    VERSION="$MAJOR.$((MINOR + 1)).0"
    ;;
  --patch)
    IFS='.' read -r MAJOR MINOR PATCH <<< "${CURRENT%%-*}"
    VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
    ;;
  *)
    VERSION="$1"
    ;;
esac

# Validate semver format
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
  echo "Error: '$VERSION' is not a valid semver version"
  echo "Expected: MAJOR.MINOR.PATCH[-PRERELEASE]"
  exit 1
fi

# Ensure clean working tree
if [ -n "$(git status --porcelain)" ]; then
  echo "Error: working tree is not clean. Commit or stash changes first."
  exit 1
fi

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
echo "  3. git commit -m 'chore(release): v$VERSION'"
echo "  4. git tag -a v$VERSION -m 'v$VERSION'"
echo "  5. git push origin main --tags"
echo ""
echo "Or just run: ./scripts/release.sh $VERSION"
