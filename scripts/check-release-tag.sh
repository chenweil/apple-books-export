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

# Read the [package] version with a real TOML parser rather than a positional
# regex. A regex for the first `version = "..."` inverts silently the moment the
# manifest is restructured -- a [workspace.package] block or a table placed
# above [package] would be read as the crate's own version, and the check would
# pass against the wrong field. tomllib is stdlib from 3.11; older
# interpreters fall back to a section-aware line scan rather than failing, so
# the release does not become uncuttable on an older runner.
crate_version() {
  python3 - "$REPO_DIR/Cargo.toml" <<'PY'
import sys

try:
    import tomllib
except ModuleNotFoundError:
    tomllib = None

if tomllib is not None:
    with open(sys.argv[1], "rb") as handle:
        data = tomllib.load(handle)
    version = data.get("package", {}).get("version")
    if not isinstance(version, str):
        raise SystemExit("Cargo.toml has no [package] version")
    print(version)
    raise SystemExit(0)

# Fallback for Python < 3.11: track which section each line belongs to, so a
# dependency table above [package] cannot be mistaken for it.
in_package = False
with open(sys.argv[1], encoding="utf-8") as handle:
    for line in handle:
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            in_package = stripped == "[package]"
            continue
        if not in_package:
            continue
        key, _, value = stripped.partition("=")
        if key.strip() == "version":
            value = value.strip().strip('"')
            if not value or value.startswith("{"):
                raise SystemExit("Cargo.toml [package] version is not a literal")
            print(value)
            raise SystemExit(0)
raise SystemExit("Cargo.toml has no [package] version")
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
