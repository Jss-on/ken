#!/usr/bin/env bash
set -euo pipefail

# Full release flow: bump version, update changelog, commit, tag, push.
#
# Usage:
#   ./scripts/release.sh 0.3.0              # stable release
#   ./scripts/release.sh 0.3.0-alpha.1      # pre-release
#   ./scripts/release.sh 0.3.0-rc.1         # release candidate
#   ./scripts/release.sh --dry-run 0.3.0    # preview without committing
#   ./scripts/release.sh --minor            # auto-bump minor version
#   ./scripts/release.sh --patch            # auto-bump patch version

DRY_RUN=false
AUTO_BUMP=""

while [[ "${1:-}" == --* ]]; do
  case "$1" in
    --dry-run) DRY_RUN=true; shift ;;
    --minor)   AUTO_BUMP="minor"; shift ;;
    --patch)   AUTO_BUMP="patch"; shift ;;
    --major)   AUTO_BUMP="major"; shift ;;
    *) echo "Unknown flag: $1"; exit 1 ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

# Auto-bump if requested
if [ -n "$AUTO_BUMP" ]; then
  IFS='.' read -r MAJOR MINOR PATCH <<< "${CURRENT%%-*}"
  case "$AUTO_BUMP" in
    major) VERSION="$((MAJOR + 1)).0.0" ;;
    minor) VERSION="$MAJOR.$((MINOR + 1)).0" ;;
    patch) VERSION="$MAJOR.$MINOR.$((PATCH + 1))" ;;
  esac
else
  VERSION="${1:?Usage: $0 [--dry-run] [--major|--minor|--patch] <version>}"
fi

# Validate semver
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
  echo "Error: '$VERSION' is not valid semver"
  exit 1
fi

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

# Determine pre-release status
PRERELEASE=false
PRERELEASE_LABEL=""
if echo "$VERSION" | grep -qE '-(alpha|beta|rc)'; then
  PRERELEASE=true
  PRERELEASE_LABEL=$(echo "$VERSION" | sed 's/.*-\([a-z]*\).*/\1/')
fi

# Collect stats
COMMIT_COUNT=$(git log "v${CURRENT}..HEAD" --oneline 2>/dev/null | wc -l || echo "?")
CONTRIBUTORS=$(git log "v${CURRENT}..HEAD" --format='%aN' 2>/dev/null | sort -u | wc -l || echo "?")
FEAT_COUNT=$(git log "v${CURRENT}..HEAD" --oneline --grep='^feat' 2>/dev/null | wc -l || echo "0")
FIX_COUNT=$(git log "v${CURRENT}..HEAD" --oneline --grep='^fix' 2>/dev/null | wc -l || echo "0")

echo "=== Ken Release ==="
echo ""
echo "  Current version:  v$CURRENT"
echo "  New version:      v$VERSION"
echo "  Branch:           $BRANCH"
echo "  Pre-release:      $PRERELEASE${PRERELEASE_LABEL:+ ($PRERELEASE_LABEL)}"
echo ""
echo "  Since v$CURRENT:"
echo "    Commits:        $COMMIT_COUNT"
echo "    Contributors:   $CONTRIBUTORS"
echo "    Features:       $FEAT_COUNT"
echo "    Fixes:          $FIX_COUNT"
echo ""

if $DRY_RUN; then
  echo "[dry-run] Would update Cargo.toml, CHANGELOG.md, commit, tag v$VERSION, and push."
  echo ""
  echo "Commits since v$CURRENT:"
  git log "v${CURRENT}..HEAD" --oneline 2>/dev/null || echo "  (no previous tag found)"
  exit 0
fi

read -rp "Proceed? [y/N] " confirm
if [[ ! "$confirm" =~ ^[yY]$ ]]; then
  echo "Aborted."
  exit 0
fi

# 1. Bump version
echo ""
echo "==> Bumping version in Cargo.toml..."
sed -i "s/^version = \"$CURRENT\"/version = \"$VERSION\"/" Cargo.toml
cargo check --workspace 2>/dev/null || true

# 2. Update changelog
echo "==> Updating CHANGELOG.md..."
if command -v git-cliff >/dev/null 2>&1; then
  git-cliff --tag "v$VERSION" -o CHANGELOG.md
  echo "    Changelog generated via git-cliff"
else
  echo "    Warning: git-cliff not found. Update CHANGELOG.md manually."
  echo "    Install: cargo install git-cliff"
fi

# 3. Commit and tag
echo "==> Committing and tagging..."
git add Cargo.toml Cargo.lock CHANGELOG.md

TAG_MSG="v$VERSION"
if [ "$PRERELEASE" = true ]; then
  TAG_MSG="v$VERSION ($PRERELEASE_LABEL)"
fi

git commit -m "chore(release): v$VERSION

Release v$VERSION with $COMMIT_COUNT commits since v$CURRENT.
Features: $FEAT_COUNT | Fixes: $FIX_COUNT | Contributors: $CONTRIBUTORS"

git tag -a "v$VERSION" -m "$TAG_MSG

Changes since v$CURRENT:
$(git log "v${CURRENT}..HEAD~1" --oneline 2>/dev/null || echo "  Initial release")"

# 4. Push
echo "==> Pushing to origin..."
git push origin main --tags

echo ""
echo "=== Released v$VERSION ==="
echo ""
echo "  GitHub will build artifacts and create the release automatically."
echo "  https://github.com/Jss-on/ken/releases/tag/v$VERSION"
if [ "$PRERELEASE" = true ]; then
  echo ""
  echo "  Note: This is a $PRERELEASE_LABEL pre-release."
  echo "  It will be marked as pre-release on GitHub."
fi
