#!/bin/bash
set -euo pipefail

# Usage: ./scripts/release.sh <version>
# Example: ./scripts/release.sh 0.2.0

VERSION="${1:?Usage: $0 <version> (e.g. 0.2.0)}"
TAG="v${VERSION}"

# ─── Validations ──────────────────────────────────────────────────────────

# Must be on main
BRANCH=$(git branch --show-current)
if [ "$BRANCH" != "main" ]; then
  echo "Error: must be on 'main' branch (currently on '${BRANCH}')"
  exit 1
fi

# Working tree must be clean
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Error: working tree is dirty — commit or stash changes first"
  exit 1
fi

# Must be up to date with remote
git fetch origin main --quiet
LOCAL=$(git rev-parse HEAD)
REMOTE=$(git rev-parse origin/main)
if [ "$LOCAL" != "$REMOTE" ]; then
  echo "Error: local main is not up to date with origin/main"
  echo "  local:  $LOCAL"
  echo "  remote: $REMOTE"
  exit 1
fi

# Tag must not already exist
if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Error: tag '${TAG}' already exists"
  exit 1
fi

# Version format check
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "Error: version must be semver (e.g. 1.2.3), got '${VERSION}'"
  exit 1
fi

# ─── Bump, commit, tag, push ─────────────────────────────────────────────

CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
echo "Releasing: ${CURRENT} -> ${VERSION}"

# Update Cargo.toml version
sed -i '' "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml

# Update Cargo.lock to reflect the new version
cargo generate-lockfile --quiet

# Commit + tag + push
git add Cargo.toml Cargo.lock
git commit --allow-empty -m "Release ${TAG}"
git tag -a "$TAG" -m "Release ${TAG}"
git push origin main "$TAG"

echo ""
echo "Done! GitHub Actions will now build and publish the release."
echo "  Track progress: https://github.com/liranbg/cliphop/actions"
echo "  Release page:   https://github.com/liranbg/cliphop/releases/tag/${TAG}"
