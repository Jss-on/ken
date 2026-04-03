#!/usr/bin/env bash
set -euo pipefail

# Full release flow: bump version, update changelog, commit, tag, push.
#
# Usage:
#   ./scripts/release.sh 0.2.0          # stable release
#   ./scripts/release.sh 0.2.0-rc.1     # release candidate
#   ./scripts/release.sh --dry-run 0.2.0 # preview without committing

DRY_RUN=false
if [ "${1:-}" = "--dry-run" ]; then
  DRY_RUN=true
  shift
fi

VERSION="${1:?Usage: $0 [--dry-run] <version>}"

# Validate semver
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
  echo "Error: '$VERSION' is not valid semver"
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# Must be on main
BRANCH=$(git branch --show-current)
if [ "$BRANCH" != "main" ]; then
  echo "Error: releases must be cut from main (currently on '$BRANCH')"
  exit 1
fi

# Must be clean
if [ -n "$(git status --porcelain)" ]; then
  echo "Error: working tree not clean"
  exit 1
fi

# Must be up to date
git fetch origin main --quiet
LOCAL=$(git rev-parse HEAD)
REMOTE=$(git rev-parse origin/main)
if [ "$LOCAL" != "$REMOTE" ]; then
  echo "Error: local main is not up to date with origin/main"
  echo "  Run: git pull origin main"
  exit 1
fi

CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
echo "=== Ken Release ==="
echo "  Current: v$CURRENT"
echo "  New:     v$VERSION"
echo "  Branch:  $BRANCH"
echo ""

if $DRY_RUN; then
  echo "[dry-run] Would update Cargo.toml, CHANGELOG.md, commit, tag v$VERSION, and push."
  exit 0
fi

read -rp "Proceed? [y/N] " confirm
if [[ ! "$confirm" =~ ^[yY]$ ]]; then
  echo "Aborted."
  exit 0
fi

# 1. Bump version
sed -i "s/^version = \"$CURRENT\"/version = \"$VERSION\"/" Cargo.toml
cargo check --workspace 2>/dev/null || true

# 2. Update changelog
if command -v git-cliff >/dev/null 2>&1; then
  git-cliff --tag "v$VERSION" -o CHANGELOG.md
  echo "Changelog updated via git-cliff"
else
  echo "Warning: git-cliff not found. Update CHANGELOG.md manually."
fi

# 3. Commit and tag
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: release v$VERSION"
git tag -a "v$VERSION" -m "v$VERSION"

# 4. Push
git push origin main --tags

echo ""
echo "Released v$VERSION"
echo "  GitHub will build artifacts and create the release automatically."
echo "  https://github.com/Jss-on/ken/releases/tag/v$VERSION"
