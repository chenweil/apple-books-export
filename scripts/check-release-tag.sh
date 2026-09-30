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

# The manifest is read by scripts/crate-version.sh, which the AppKit packaging
# script and tests/headless_mainline.sh also call, so a restructured Cargo.toml
# cannot make this check and the packaging step read different fields.
CRATE_VERSION_SCRIPT="$REPO_DIR/scripts/crate-version.sh"
# Existence, not executability: the script is invoked through bash, so a lost
# exec bit is not a problem, and reporting it as "missing" is actively
# misleading when the real failure is something else further down.
if [[ ! -f "$CRATE_VERSION_SCRIPT" ]]; then
  printf 'missing %s; the crate version must have one implementation\n' \
    "$CRATE_VERSION_SCRIPT" >&2
  exit 1
fi

if [[ $# -lt 1 ]]; then
  printf 'usage: %s <tag>\n' "$(basename "$0")" >&2
  exit 2
fi

tag="$1"
tag_version="${tag#v}"
declared="$(bash "$CRATE_VERSION_SCRIPT")"

if [[ "$tag_version" != "$declared" ]]; then
  printf 'tag must match Cargo.toml version: tag is v%s, Cargo.toml is %s\n' \
    "$tag_version" "$declared" >&2
  printf 'Either update Cargo.toml, or move the tag to v%s.\n' "$declared" >&2
  exit 1
fi

printf 'tag v%s matches Cargo.toml\n' "$tag_version"
