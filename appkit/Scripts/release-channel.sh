#!/usr/bin/env bash
#
# Print the release channel a version belongs on.
#
# Called by the packaging script and by the release workflow, and table-tested
# by tests/headless_mainline.sh. This mirrors the arrangement already used for
# the tag check: one implementation, called from every place that needs it, so
# that a change to the rules only has to be made once.
#
# The guard in headless_mainline.sh is a regression guard, not a proof. It
# executes this script against eight versions and separately asserts that the
# callers really invoke it, but both the table and the release workflow derive
# their expectation from this same file -- so a version of this script that
# hardcoded those eight answers would satisfy both. The eight cases are the
# only independent statement of what the rules are, which is why they are
# spelled out literally in the guard rather than generated from here.
#
# Usage: release-channel.sh <version>   -> prints "stable" or "prerelease"
#
# Why this exists at all, stated precisely. UpdateChecker already refuses a
# manifest whose version is a prerelease, independently of the channel field,
# so a prerelease build was never actually offered to stable users -- that
# would be a false claim, and the guard in UpdateChecker is the real
# protection. What was wrong before this script existed is that a prerelease
# release published a manifest claiming `channel: stable`, which is a false
# statement inside a public artifact. Two reasons to stop saying it: the
# artifact should describe itself honestly, and a manifest that is honest
# about being a prerelease keeps working if the version-based guard is ever
# loosened or removed.
#
# SemVer 2.0.0: `+` introduces build metadata, which comes last and is
# ignored for precedence, so it cannot make a version a prerelease. `1.0.0+build-1`
# is a stable version that happens to contain a hyphen. Checking for a hyphen
# in the whole string would misclassify it, so the build metadata is stripped
# first.

set -euo pipefail

if [[ $# -lt 1 ]]; then
  printf 'usage: %s <version>\n' "$(basename "$0")" >&2
  exit 2
fi

version="$1"
core="${version%%+*}"

if [[ "$core" == *-* ]]; then
  printf 'prerelease\n'
else
  printf 'stable\n'
fi
