#!/usr/bin/env bash
#
# Verify that a release tag names the version the crate actually declares.
#
# release.yml used to derive the GitHub Release name from the tag without ever
# comparing it to Cargo.toml, so `git tag v0.4.0` published a release page
# reading 0.4.0 around a binary whose --version reported 0.3.3, and nothing
# failed. This script is the single implementation, called both by the
# workflow and by tests/headless_mainline.sh, so the two cannot drift.
#
# Usage: check-release-tag.sh <tag>   (with or without a leading "v")

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v python3 >/dev/null 2>&1; then
  printf 'python3 is required to read the crate version\n' >&2
  exit 1
fi

# Only the [package] version, never a dependency's: the first top-level
# `version = "..."` in Cargo.toml belongs to the package itself.
crate_version() {
  python3 - "$REPO_DIR/Cargo.toml" <<'PY'
import re
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    for line in handle:
        match = re.match(r'^version\s*=\s*"([^"]+)"', line)
        if match:
            print(match.group(1))
            break
    else:
        raise SystemExit("no [package] version in Cargo.toml")
PY
}

if [[ $# -lt 1 ]]; then
  printf 'usage: %s <tag>\n' "$(basename "$0")" >&2
  exit 2
fi

tag="$1"
tag_version="${tag#v}"
declared="$(crate_version)"

if [[ "$tag_version" != "$declared" ]]; then
  printf 'tag must match Cargo.toml version: tag is v%s, Cargo.toml is %s\n' \
    "$tag_version" "$declared" >&2
  printf 'Either update Cargo.toml, or move the tag to v%s.\n' "$declared" >&2
  exit 1
fi

printf 'tag v%s matches Cargo.toml\n' "$tag_version"
