#!/usr/bin/env bash
# ─── bump-version.sh ──────────────────────────────────────────────────────────
# Set the crate version, commit, and optionally tag.
#
# Usage:
#   ./scripts/bump-version.sh <version>
#
# Examples:
#   ./scripts/bump-version.sh 0.5.8-alpha.2   # pre-release bump (no tag)
#   ./scripts/bump-version.sh 0.5.8           # release — commits + creates tag v0.5.8
#
# Convention:
#   MAJOR.MINOR.PATCH-alpha.N  / -rc.N   on develop  (no tag created)
#   MAJOR.MINOR.PATCH                    release tag  (tag v<version> created)
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
  echo "Usage: $0 <version>"
  echo "  e.g. $0 0.5.8-alpha.2"
  echo "  e.g. $0 0.5.8"
  exit 1
fi

# Validate semver-ish format: digits, dots, hyphens, alphanumerics only
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
  echo "ERROR: version must be MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-pre.N"
  exit 1
fi

CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "  Current: $CURRENT"
echo "  New:     $VERSION"

# Update Cargo.toml (first occurrence — the [package] version)
sed -i "0,/^version = \"${CURRENT}\"/s//version = \"${VERSION}\"/" Cargo.toml

# Regenerate lock file so it reflects the new version
cargo generate-lockfile --quiet 2>/dev/null || cargo update --quiet 2>/dev/null || true

# Commit
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to ${VERSION}"

# Tag only for clean releases (no pre-release suffix)
if [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  git tag -a "v${VERSION}" -m "Release v${VERSION}"
  echo "  Tagged: v${VERSION}"
  echo "  Push with: git push && git push origin v${VERSION}"
else
  echo "  Pre-release: no tag created"
  echo "  Push with: git push"
fi

echo "  Done."
