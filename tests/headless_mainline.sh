#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOW="$ROOT_DIR/.github/workflows/ci.yml"
RELEASE_WORKFLOW="$ROOT_DIR/.github/workflows/release.yml"
README="$ROOT_DIR/README.md"
AGENTS="$ROOT_DIR/AGENTS.md"

cd "$ROOT_DIR"

# ---------------------------------------------------------------------------
# Release contract: the CLI artifacts and their checksums.
# ---------------------------------------------------------------------------

required_release_text=(
  'cargo build --release --target'
  'OUTPUT="${{ matrix.binary_name }}"'
  'cp "$CLI" "$OUTPUT"'
  'apple-books-exporter-aarch64-apple-darwin'
  'apple-books-exporter-x86_64-apple-darwin'
  'SHA256SUMS'
)

for text in "${required_release_text[@]}"; do
  if ! grep -Fq -- "$text" "$RELEASE_WORKFLOW"; then
    printf 'missing headless release contract text: %s\n' "$text" >&2
    exit 1
  fi
done

# Every matrix leg needs a timeout. A leg whose runner label no longer exists is
# not rejected by Actions: it is accepted, then sits queued with no runner
# assigned, forever. The rehearsal tag on 2026-09-30 proved it -- the x86_64
# leg had been queued for 80 minutes against a retired label, and because the
# release job declares `needs: build` that stall blocks the entire release
# while looking like nothing is wrong. A timeout converts the hang into a
# visible red run.
if ! grep -Eq '^[[:space:]]*timeout-minutes:[[:space:]]*[0-9]+' "$RELEASE_WORKFLOW"; then
  printf 'the release build job has no timeout, so a stalled leg blocks the release silently\n' >&2
  exit 1
fi

# An x86_64 target needs an Intel runner, and a bare `macos-<version>` label is
# no longer one: macos-latest, macos-14 and macos-15 are all arm64, and Intel
# is spelled `macos-<version>-intel`. Checking the shape rather than an
# allow-list of live labels is deliberate -- an allow-list of runner labels
# rots the moment GitHub retires another image, and a guard that silently
# blocks a legitimate new image is worse than no guard. This shape invariant
# is the thing that actually broke: `macos-13` was Intel when it was written.
python3 - "$RELEASE_WORKFLOW" <<'PY'
import re
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    text = handle.read()

# Each matrix include entry is a block of `key: value` lines under a dash.
entries = re.findall(r'(?m)^\s*-\s+(\S+)\s*:\s*(\S+)\s*$(.*?)(?=^\s*-\s+\S+\s*:|\Z)',
                     text, re.S)
seen_x86 = False
for first_key, first_value, body in entries:
    fields = {first_key: first_value}
    for key, value in re.findall(r'(?m)^\s*([\w-]+)\s*:\s*(\S+)\s*$', body):
        fields.setdefault(key, value)
    if fields.get("target") != "x86_64-apple-darwin":
        continue
    seen_x86 = True
    runner = fields.get("os", "")
    if not runner.endswith("-intel"):
        print(f"release.yml: the x86_64-apple-darwin leg runs on '{runner}', "
              f"which is not an Intel runner. Since macos-latest/macos-14/macos-15 "
              f"are arm64, only a 'macos-<version>-intel' label builds x86_64.",
              file=sys.stderr)
        sys.exit(1)

if not seen_x86:
    print("release.yml: no matrix leg builds x86_64-apple-darwin, so the "
          "Intel CLI binary would never ship", file=sys.stderr)
    sys.exit(1)
PY

# These forbid the Tauri release path coming back.
#
# `.dmg` used to be in this list on its own, because the only disk image the
# workflow ever produced was Tauri's. AppKit now ships a DMG too, so the bare
# extension no longer distinguishes anything and would fail on a legitimate
# artifact. The Tauri path is caught by its own markers instead. A bare `tauri`
# is deliberately absent, because it also matches the comments explaining the
# rollback that this file contains -- the specific invocations and artifact
# names are what matter, and they are listed individually.
for forbidden in \
  'cargo tauri' \
  'npm ci' \
  'Setup Node.js' \
  'Build GUI' \
  'locate-gui' \
  'gui-app-' \
  'gui-dmg-' \
  'tauri.conf.json' \
  'tauri-apps/tauri-action' \
  'tauri-action@' \
  '.app.zip' \
  'src-tauri/target'; do
  if grep -Fq -- "$forbidden" "$RELEASE_WORKFLOW"; then
    printf 'legacy GUI release path remains: %s\n' "$forbidden" >&2
    exit 1
  fi
done

# An AppKit DMG in the workflow must be the one package-dmg.sh produces, and it
# must be unsigned. This checks the *invocation*, not a mention: a comment
# naming the script satisfies a bare grep, and so does the legitimate AppKit
# step being cited as if it vouched for an unrelated disk image.
if grep -Fq -- '.dmg' "$RELEASE_WORKFLOW"; then
  if ! grep -Eq '^[[:space:]]*(\./)?appkit/Scripts/package-dmg\.sh' "$RELEASE_WORKFLOW"; then
    printf 'a DMG is released but never built by running appkit/Scripts/package-dmg.sh\n' >&2
    exit 1
  fi
  # ADR 0004 requires every stable Release to carry a valid latest.json
  # alongside the DMG; dropping it leaves the app's version discovery broken.
  # Matched on a non-comment line, since the explanatory comment above the
  # upload names the file too.
  if ! grep -v '^[[:space:]]*#' "$RELEASE_WORKFLOW" | grep -Fq -- 'dist/latest.json'; then
    printf 'the AppKit DMG ships without the latest.json ADR 0004 requires\n' >&2
    exit 1
  fi
fi

# The manifest's channel must describe the version it ships with. It used to be
# a literal "stable" in package-dmg.sh, so a prerelease release published a
# manifest claiming to be stable.
#
# This is a behavioural test, not a lint. An earlier version of this guard
# grepped for the absence of one literal, and five of six mutations that
# reintroduced the defect -- including the same bug reformatted, and a comment
# naming the right words -- sailed past it. Grepping script text cannot tell the
# bug from the fix. So the rules live in one script that both the packaging
# step and the release workflow call, and those rules are executed here against
# a table of versions.
CHANNEL_SCRIPT="$ROOT_DIR/appkit/Scripts/release-channel.sh"
if [[ ! -x "$CHANNEL_SCRIPT" ]]; then
  printf 'missing %s; the channel rules must have exactly one implementation\n' "$CHANNEL_SCRIPT" >&2
  exit 1
fi

# version:expected. The +build cases are the interesting ones: SemVer build
# metadata is ignored for precedence, so 1.0.0+build-1 is a stable version that
# happens to contain a hyphen, and a naive "contains a -" test misclassifies it.
channel_cases=(
  '0.3.3:stable'
  '0.3.4-rc1:prerelease'
  '0.3.4-rc.1:prerelease'
  '1.0.0:stable'
  '0.3.4+build7:stable'
  '1.0.0+build-1:stable'
  '1.0.0-alpha+build7:prerelease'
  '1.0.0-x-y-z:prerelease'
)
for case_spec in "${channel_cases[@]}"; do
  version="${case_spec%%:*}"
  expected="${case_spec##*:}"
  actual="$(bash "$CHANNEL_SCRIPT" "$version" 2>/dev/null || printf '<error>')"
  if [[ "$actual" != "$expected" ]]; then
    printf 'channel for %s should be %s, got %s\n' "$version" "$expected" "$actual" >&2
    exit 1
  fi
done

# The packaging script must take its channel from that one implementation and
# hand it to plutil unchanged. These two are invocation-shape checks, not
# behaviour: package-dmg.sh builds with swift and shells out to hdiutil, so
# running it just to watch what channel it passes to plutil is not something
# this guard should do -- it would be a release build in a contract check. They
# are
# anchored to the exact line and the exact variable name on purpose: a looser
# "does the file mention release-channel.sh" check passed a mutation that
# renamed the variable to MANIFEST_CHANNEL, leaving plutil to write an empty
# channel, and a comment containing the required words satisfied one outright.
# Comment lines are stripped first so the explanation above the assignment
# cannot satisfy them.
assignment_pattern='^CHANNEL="\$\(bash .*release-channel\.sh.*\)"[[:space:]]*$'
plutil_pattern='^plutil -insert channel -string "\$CHANNEL" "\$MANIFEST_PLIST"[[:space:]]*$'

body="$(grep -v '^[[:space:]]*#' "$ROOT_DIR/appkit/Scripts/package-dmg.sh")"
if ! grep -Eq "$assignment_pattern" <<<"$body"; then
  printf 'package-dmg.sh does not assign CHANNEL from release-channel.sh\n' >&2
  exit 1
fi
if ! grep -Eq "$plutil_pattern" <<<"$body"; then
  printf 'package-dmg.sh does not pass CHANNEL through to plutil verbatim\n' >&2
  exit 1
fi

# The release workflow has to *call* that same script, in both places it
# classifies a version, rather than carrying its own copy of the rule. Two call
# sites, because there are two jobs: the arm64 leg derives the expected channel
# to check the produced manifest against, and the release job decides whether
# the Release is flagged as a prerelease. A divergent inline copy would let a
# prerelease publish as a full release with the local guard still green --
# both sides would be deriving their expectation from the same wrong rule.
# The literal invocation is required: a comment naming the script would
# satisfy a looser check, which is the mistake this file has made before.
for call_site in \
  'bash appkit/Scripts/release-channel.sh "$APP_VERSION"' \
  'bash appkit/Scripts/release-channel.sh "${{ steps.version.outputs.VERSION }}"'; do
  if ! grep -Fq -- "$call_site" "$RELEASE_WORKFLOW"; then
    printf 'release.yml does not invoke release-channel.sh: %s\n' "$call_site" >&2
    exit 1
  fi
done

# The Release's prerelease flag has to read that step's output, not re-derive
# the answer with an inline expression. `contains(VERSION, '-')` is the rule
# this replaced, and it disagreed with the shipped manifest for a version like
# 1.0.0+build-1, which has a hyphen in its build metadata but is stable.
if ! grep -Eq 'prerelease:[[:space:]]*\$\{\{[[:space:]]*steps\.channel\.outputs\.is_prerelease' \
     "$RELEASE_WORKFLOW"; then
  printf 'the Release prerelease flag does not come from the shared channel step\n' >&2
  exit 1
fi

# Claiming to sign is worse than not signing: there is no Developer ID identity
# in this repository, so such a step is a false statement about the artifact.
# Matched on the actual command forms rather than one literal, because
# `codesign --force --deep --sign` and the short `-s` flag are the same step
# written differently. Comments are stripped first so that a line documenting
# why the repo does *not* notarize does not trip the guard.
if ! command -v python3 >/dev/null 2>&1; then
  printf 'python3 is required to inspect release.yml\n' >&2
  exit 1
fi

if ! python3 - "$RELEASE_WORKFLOW" <<'PY'
import re
import sys

SIGNING = re.compile(
    r'\b(codesign|notarytool|stapler|spctl)\b'
    r'|\bDeveloper\s+ID\b'
    r'|--sign\b'
    r'|(?<![\w-])-s(?![\w-])'
)

with open(sys.argv[1], encoding="utf-8") as handle:
    for number, line in enumerate(handle, 1):
        stripped = line.strip()
        if stripped.startswith("#"):
            continue
        if SIGNING.search(line):
            print(f"release.yml:{number}: claims a signing step this repository "
                  f"cannot perform: {stripped}")
            sys.exit(1)
sys.exit(0)
PY
then
  exit 1
fi

# ---------------------------------------------------------------------------
# One tag, one product version.
#
# release.yml only ever used the tag to name the GitHub Release; Cargo.toml's
# version was never compared against it, so `git tag v0.4.0` would publish a
# release page reading 0.4.0 around a binary whose --version reports 0.3.3,
# with nothing failing. The tag and the manifest must now agree.
# ---------------------------------------------------------------------------

if ! command -v python3 >/dev/null 2>&1; then
  printf 'python3 is required to read the crate version\n' >&2
  exit 1
fi

# Read the crate version through the shared script, not through a parser of its
# own. This used to be an inline regex taking the first `version = "..."` in the
# file, with a comment claiming it read only [package] and never a dependency's
# -- which it could not do. Putting a [workspace.package] block above [package]
# made it return 9.9.9 while the shared script returned the right answer, so the
# guard would have been the one component reading a different field than
# everything it is supposed to check. A guard that re-derives the value it is
# checking is a second implementation wearing a checker's clothes.
CRATE_VERSION="$(bash "$ROOT_DIR/scripts/crate-version.sh")"

# The workflow has to *invoke* the check, not merely mention it: a comment
# saying "see scripts/check-release-tag.sh" satisfies a bare grep. The literal
# invocation is required, and it must appear before the CLI build, so a
# mismatched tag still costs seconds rather than a full build.
if ! grep -Fq -- 'bash scripts/check-release-tag.sh "$TAG"' "$RELEASE_WORKFLOW"; then
  printf 'release.yml does not invoke check-release-tag.sh\n' >&2
  exit 1
fi

tag_check_line="$(grep -Fn -- 'bash scripts/check-release-tag.sh "$TAG"' "$RELEASE_WORKFLOW" | head -1 | cut -d: -f1)"
cli_build_line="$(grep -Fn -- 'cargo build --release --target' "$RELEASE_WORKFLOW" | head -1 | cut -d: -f1)"

if [[ -z "$tag_check_line" || -z "$cli_build_line" ]]; then
  printf 'cannot locate the tag check or the CLI build in release.yml\n' >&2
  exit 1
fi

if (( tag_check_line > cli_build_line )); then
  printf 'release.yml builds the CLI (line %d) before verifying the tag (line %d)\n' \
    "$cli_build_line" "$tag_check_line" >&2
  exit 1
fi

# And the script itself has to work in both directions. A script that always
# exits 0 would satisfy the invocation check above.
if ! bash "$ROOT_DIR/scripts/check-release-tag.sh" "v${CRATE_VERSION}" >/dev/null; then
  printf 'check-release-tag.sh rejects the current version v%s\n' "$CRATE_VERSION" >&2
  exit 1
fi

if bash "$ROOT_DIR/scripts/check-release-tag.sh" 'v99.99.99' >/dev/null 2>&1; then
  printf 'check-release-tag.sh accepts a mismatched tag\n' >&2
  exit 1
fi

# Three things read Cargo.toml's [package] version, and two of them deciding
# differently is the failure the crate has already had once. They share
# scripts/crate-version.sh. This checks what that script actually returns,
# against synthetic manifests, because a guard that re-derives the answer the
# same way the script does proves nothing.
CRATE_VERSION_SCRIPT="$ROOT_DIR/scripts/crate-version.sh"
if [[ ! -f "$CRATE_VERSION_SCRIPT" ]]; then
  printf 'missing %s; the crate version must have one implementation\n' \
    "$CRATE_VERSION_SCRIPT" >&2
  exit 1
fi

# ...including this file. The guard's own CRATE_VERSION read is pinned the same
# way: reverting it to a private reader is invisible whenever the two happen to
# agree today, which is the exact condition the shared script was introduced to
# eliminate.
#
# The assignment is extracted first and the pattern is the path, never the whole
# line. An earlier version grepped this file for the entire assignment text --
# a string that therefore also appeared inside the grep's own arguments, so the
# assertion matched itself and could not fail. Matching against the extracted
# line removes the file from the search space entirely, and requiring exactly
# one assignment stops a second plain assignment from being added underneath.
# "A second plain assignment" is the whole claim: `eval "CRATE_VERSION=..."` on
# the next line overrides the value, adds no second '^CRATE_VERSION=', and
# passes both assertions below. Measured, not assumed. Pinning a value in shell
# against a determined override would need execution rather than pattern
# matching, which is why the extraction checks the read and this one checks the
# spelling -- each within what its own mechanism can actually enforce.
guard_assignments="$(grep -c '^CRATE_VERSION=' "$ROOT_DIR/tests/headless_mainline.sh" || true)"
if [[ "$guard_assignments" -ne 1 ]]; then
  printf 'headless_mainline.sh should assign CRATE_VERSION exactly once, found %s\n' \
    "$guard_assignments" >&2
  exit 1
fi
# Matched on the invocation, not on the path appearing anywhere in the line: a
# private reader with "# crate-version.sh" in a trailing comment passes a bare
# substring search. The whole point is that a decoupled reader must not slip
# through, and this is the last assertion standing in its way.
crate_version_read="$(grep -m1 '^CRATE_VERSION=' "$ROOT_DIR/tests/headless_mainline.sh")"
if ! grep -Eq '^CRATE_VERSION="\$\(bash .*crate-version\.sh"\)"[[:space:]]*$' \
     <<<"$crate_version_read"; then
  printf 'headless_mainline.sh does not read CRATE_VERSION through crate-version.sh\n' >&2
  exit 1
fi

# The tag check has to read the manifest through that same script. Its own
# accept/reject tests above cannot tell the difference: a version hardcoded
# into it still accepts the current tag and still rejects v99.99.99, and passes
# both. The call site is what has to be pinned -- though as a substring check
# like the one below, so it too is defeated by leaving the call in place beside
# a hardcoded value.
if ! grep -v '^[[:space:]]*#' "$ROOT_DIR/scripts/check-release-tag.sh" \
     | grep -Fq -- 'bash "$CRATE_VERSION_SCRIPT"'; then
  printf 'check-release-tag.sh does not read the manifest through crate-version.sh\n' >&2
  exit 1
fi

manifest_fixtures="$(mktemp -d)"
trap 'rm -rf "$manifest_fixtures"' EXIT

# A plain manifest returns what it declares.
cat >"$manifest_fixtures/plain.toml" <<'FIXTURE'
[package]
name = "example"
version = "1.2.3"
edition = "2021"
FIXTURE

# The case a positional regex gets wrong: a version-looking line above
# [package] must not be read as the crate's own version, in either direction.
cat >"$manifest_fixtures/workspace.toml" <<'FIXTURE'
[workspace.package]
version = "9.9.9"

[dependencies]
serde = { version = "8.0.1" }

[package]
name = "example"
version = "1.2.3"
edition = "2021"
FIXTURE

# TOML allows a trailing comment and both quote styles. A parser that strips
# '"' naively turns these into the literal 1.2.3" # bump, which then gets
# stamped into a public artifact and offered to users as an update.
cat >"$manifest_fixtures/comment.toml" <<'FIXTURE'
[package]
version = "1.2.3" # bump me
FIXTURE
cat >"$manifest_fixtures/single.toml" <<'FIXTURE'
[package]
version = '1.2.3'
FIXTURE

# Malformed input has to fail rather than answer.
# Cargo accepts every one of these, so the fallback has to read them the way
# tomllib does. If it does not, a manifest that builds fine fails only on a
# machine with an older system python -- the local-versus-CI split this script
# exists to remove, just moved inside the unifier.
#
# This is an enumeration of cases checked to agree, not a claim that the two
# branches accept the same set of documents. They do not: a quoted table header
# like ["package"] and spacing around the dot in package . version are still
# refused by the fallback where tomllib accepts them. The fallback is a line
# scanner and closing that gap means writing a TOML parser.
#
# It is also not the case that the fallback never answers a different value.
# It does not decode TOML escape sequences, so on a manifest cargo builds
# happily the two can disagree:
#
#   version = "1.0.0\u002Drc1"   cargo 1.0.0-rc1   tomllib 1.0.0-rc1
#                                        fallback 1.0.0\u002Drc1
#
# What does hold, on every such case tried, is that the fallback's answer
# contains a backslash. A backslash cannot occur in a legal semver, so the
# answer is not a version at all: check-release-tag.sh compares it to the tag,
# mismatches, and exits 1 with the difference printed. The release path fails
# closed and never publishes the wrong number -- checked by running it, not by
# reading it. The residue is narrower and worth naming: release-channel.sh fed
# such a value returns "stable", so a prerelease could be labelled stable by a
# developer running package-dmg.sh by hand on a machine with a pre-3.11 python.
# That is a local packaging path, not the release workflow, and the DMG it
# produces carries the backslash in its filename where it is visible.
cat >"$manifest_fixtures/dotted.toml" <<'FIXTURE'
package.version = "1.2.3"
FIXTURE
cat >"$manifest_fixtures/space-header.toml" <<'FIXTURE'
[ package ]
version = "1.2.3"
FIXTURE
cat >"$manifest_fixtures/header-comment.toml" <<'FIXTURE'
[package] # the crate
version = "1.2.3"
FIXTURE
cat >"$manifest_fixtures/multiline.toml" <<'FIXTURE'
[package]
version = """1.2.3"""
FIXTURE
# A `version = "..."` inside a multi-line string is a value, not a key. Reading
# it as a key made the fallback answer 9.9.9 for a manifest cargo reads as
# 0.0.0 -- a wrong version stamped into a public artifact. The multi-line can
# be any key's, so the fixture uses `description`, not `version`.
cat >"$manifest_fixtures/multiline-decoy.toml" <<'FIXTURE'
[package]
version = "0.0.0"
description = """
version = "9.9.9"
"""
FIXTURE
cat >"$manifest_fixtures/multiline-spanning.toml" <<'FIXTURE'
[package]
version = """
1.2.3
"""
FIXTURE
# The single-quoted triple is here because it was once written as a four
# apostrophe literal, so ''' never opened a multi-line string and every decoy below
# passed by default. A fixture for a delimiter only discriminates if the
# delimiter is the one the code actually compares against.
cat >"$manifest_fixtures/single-quoted-decoy.toml" <<'FIXTURE'
[package]
version = "1.2.3"
description = '''
version = "9.9.9"
'''
FIXTURE
# A fake table header inside the string would otherwise flip in_package off and
# make the real version invisible.
cat >"$manifest_fixtures/single-quoted-fake-header.toml" <<'FIXTURE'
[package]
version = "1.2.3"
description = '''
[dependencies]
'''
FIXTURE
# A quoted key is a key.
cat >"$manifest_fixtures/quoted-key.toml" <<'FIXTURE'
[package]
"version" = "1.2.3"
FIXTURE
# cargo normalises surrounding whitespace out of a version, so both branches
# must strip or they disagree quietly on this.
cat >"$manifest_fixtures/padded-multiline.toml" <<'FIXTURE'
[package]
version = """
   1.2.3   """
FIXTURE
# Two multi-line versions goes through the collector, not the one-liner path.
cat >"$manifest_fixtures/duplicate-multiline.toml" <<'FIXTURE'
[package]
version = """
1.2.2
"""
version = """
1.2.3
"""
FIXTURE

# A dotted key after any table header is a different key, not [package].
cat >"$manifest_fixtures/dotted-after-table.toml" <<'FIXTURE'
[workspace]
members = []

package.version = "9.9.9"

[package]
version = "1.2.3"
FIXTURE

# A dependency table above [package] is the other direction of the same trap.
cat >"$manifest_fixtures/deps-first.toml" <<'FIXTURE'
[dependencies]
serde = { version = "8.0.1" }

[package]
version = "1.2.3"
FIXTURE
# Defining the version twice is something cargo rejects; the fallback must not
# answer from the first hit.
cat >"$manifest_fixtures/duplicate.toml" <<'FIXTURE'
package.version = "9.9.9"

[package]
version = "1.2.3"
FIXTURE

# An empty version is accepted by the TOML parser and would otherwise become a
# blank APP_VERSION and a disk image called Books-Exporter--unsigned.dmg.
cat >"$manifest_fixtures/empty.toml" <<'FIXTURE'
[package]
version = ""
FIXTURE
cat >"$manifest_fixtures/unquoted.toml" <<'FIXTURE'
[package]
version = 1.2.3.1
FIXTURE
cat >"$manifest_fixtures/garbage.toml" <<'FIXTURE'
[package]
version = "1.2.3" junk
FIXTURE
cat >"$manifest_fixtures/brace.toml" <<'FIXTURE'
[package]
version = { workspace = true }
FIXTURE
cat >"$manifest_fixtures/absent.toml" <<'FIXTURE'
[workspace]
members = []
FIXTURE

# The fallback parser, for interpreters without tomllib, runs twice over the
# same fixtures. It is not a corner case: /usr/bin/python3 on macOS is 3.9 and
# has no tomllib, so anyone packaging locally takes that branch, and it was
# previously covered by nothing at all.
no_tomllib="$manifest_fixtures/no-tomllib"
mkdir -p "$no_tomllib"
printf 'raise ModuleNotFoundError("blocked for testing")\n' >"$no_tomllib/tomllib.py"

for parser_mode in tomllib fallback; do
  if [[ "$parser_mode" == fallback ]]; then
    export PYTHONPATH="$no_tomllib"
    if python3 -c 'import tomllib' 2>/dev/null; then
      printf 'the tomllib shim is not working; the fallback round is not testing the fallback\n' >&2
      exit 1
    fi
  else
    unset PYTHONPATH
    # Without this the round named "tomllib" would silently run the fallback
    # again on a runner whose python3 has no tomllib, and the tomllib path
    # would go uncovered while the log still said it had been tested.
    if ! python3 -c 'import tomllib' 2>/dev/null; then
      # Skipped, not failed. A python3 without tomllib is the reason the
      # fallback exists -- /usr/bin/python3 on macOS is 3.9 -- so refusing to
      # run the guard there would invert its purpose. The fallback round below
      # is the one that matters on such a machine; it is reported as skipped
      # rather than silently passing.
      printf 'SKIP: this python3 has no tomllib, so the tomllib parser branch is not exercised here\n' >&2
      continue
    fi
  fi

  # fixture:expected, because one fixture is a manifest whose correct answer
  # is deliberately not 1.2.3.
  for spec in \
    'plain:1.2.3' \
    'workspace:1.2.3' \
    'comment:1.2.3' \
    'single:1.2.3' \
    'dotted:1.2.3' \
    'space-header:1.2.3' \
    'header-comment:1.2.3' \
    'multiline:1.2.3' \
    'multiline-spanning:1.2.3' \
    'deps-first:1.2.3' \
    'dotted-after-table:1.2.3' \
    'multiline-decoy:0.0.0' \
    'single-quoted-decoy:1.2.3' \
    'single-quoted-fake-header:1.2.3' \
    'quoted-key:1.2.3' \
    'padded-multiline:1.2.3'; do
    fixture="${spec%%:*}"
    expected="${spec##*:}"
    got="$(bash "$CRATE_VERSION_SCRIPT" "$manifest_fixtures/$fixture.toml" 2>/dev/null || printf '<error>')"
    if [[ "$got" != "$expected" ]]; then
      printf 'crate-version.sh (%s) read %s from the %s fixture, expected %s\n' \
        "$parser_mode" "$got" "$fixture" "$expected" >&2
      exit 1
    fi
  done

  for fixture in garbage brace absent unquoted duplicate empty duplicate-multiline; do
    if bash "$CRATE_VERSION_SCRIPT" "$manifest_fixtures/$fixture.toml" >/dev/null 2>&1; then
      printf 'crate-version.sh (%s) accepts the malformed %s fixture\n' \
        "$parser_mode" "$fixture" >&2
      exit 1
    fi
  done
done
unset PYTHONPATH

# The packaging script's default has to come from that script, not from a
# literal that can go stale again. This is a substring check, not a wiring
# check: it is satisfied by the path appearing anywhere in the file, so a
# version restored to a literal while a dead line elsewhere still mentions the
# script would pass. Nothing downstream catches that on this path: the workflow
# always passes APP_VERSION explicitly, so this default branch never runs in CI
# and the produced-manifest assertion never observes it. This check catches the
# common case and nothing more.
# Comment lines are stripped first so the explanation above the assignment
# cannot satisfy it on its own.
if ! grep -v '^[[:space:]]*#' "$ROOT_DIR/appkit/Scripts/package-dmg.sh" \
     | grep -Fq -- 'scripts/crate-version.sh'; then
  printf 'package-dmg.sh does not take its default version from crate-version.sh\n' >&2
  exit 1
fi

# The AppKit bundle version has to be stamped from the same tag rather than
# kept as a hand-edited literal, otherwise the GUI and the CLI drift apart
# again after this change.
for required in 'APP_VERSION' 'BUILD_VERSION'; do
  if ! grep -Fq -- "$required" "$ROOT_DIR/appkit/Scripts/package-dmg.sh"; then
    printf 'package-dmg.sh no longer honours %s\n' "$required" >&2
    exit 1
  fi
done


# The checked-in Info.plist is a template that package-dmg.sh overwrites, so its
# literal does not have to equal the crate version. It does have to be a real
# version string rather than a placeholder, because a developer who packages by
# hand reads that file.
if ! grep -Eq '<string>[0-9]+\.[0-9]+\.[0-9]+</string>' "$ROOT_DIR/appkit/Resources/Info.plist"; then
  printf 'appkit/Resources/Info.plist has no concrete CFBundleShortVersionString\n' >&2
  exit 1
fi
# The release runbook quotes the tag to cut. That literal is exactly the kind
# of copy that goes stale silently the day the crate version moves, so it is
# pinned to Cargo.toml rather than trusted.
if ! grep -Fq -- "git tag v${CRATE_VERSION}" "$AGENTS"; then
  printf 'the release runbook does not tag the current crate version v%s\n' \
    "$CRATE_VERSION" >&2
  exit 1
fi

# The AppKit gate is the only GUI that ships. Removing Tauri must not take the
# AppKit coverage with it.
if ! grep -Fq -- 'AppKit contracts' "$WORKFLOW"; then
  printf 'the AppKit CI gate is missing from ci.yml\n' >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# The Tauri / Svelte frontend is gone.
#
# src/ also holds the entire Rust CLI, so these checks are path-precise.
# `src/lib.rs` is a Rust file that happens to share a basename with the former
# frontend directory `src/lib/`. Deleting the whole of src/ would remove the
# CLI, and no other assertion in this file would notice -- hence the survival
# checks further down.
# ---------------------------------------------------------------------------

for removed_path in \
  'src-tauri' \
  'svelte.config.js' \
  'vite.config.ts' \
  'tsconfig.json' \
  'package.json' \
  'package-lock.json'; do
  if [[ -e "$ROOT_DIR/$removed_path" ]]; then
    printf 'the removed Tauri frontend is still present: %s\n' "$removed_path" >&2
    exit 1
  fi
done

if [[ -n "$(git ls-files '*.svelte')" ]]; then
  printf 'Svelte components remain tracked:\n%s\n' "$(git ls-files '*.svelte')" >&2
  exit 1
fi

for removed_entry in 'src/App.svelte' 'src/main.ts' 'src/app.css' 'src/index.html'; do
  if [[ -e "$ROOT_DIR/$removed_entry" ]]; then
    printf 'the removed frontend entry is still present: %s\n' "$removed_entry" >&2
    exit 1
  fi
done

if [[ -d "$ROOT_DIR/src/lib" ]]; then
  printf 'the removed frontend directory src/lib/ is still present\n' >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# The Rust CLI survived the frontend removal.
#
# This is the guard that makes the deletion above safe to perform. Existence is
# checked on disk rather than through `git ls-files`, because the mistake this
# guards against is a working-tree operation: a file removed from disk but still
# listed in the index would otherwise pass. The count is a floor rather than a
# spot check so a partial wipe that leaves src/main.rs behind still fails.
# ---------------------------------------------------------------------------

for required_rust in 'src/main.rs' 'src/lib.rs' 'src/exporter.rs' 'src/machine.rs'; do
  if [[ ! -f "$ROOT_DIR/$required_rust" ]]; then
    printf 'the Rust CLI is missing after the frontend removal: %s\n' "$required_rust" >&2
    exit 1
  fi
done

# The floor is deliberately below the size of the speech module (14 files), so
# that losing src/speech/ trips the specific speech assertion below rather than
# this one, and a partial wipe is still caught. It is a floor rather than a
# spot check because a spot check alone would pass with four files left.
rust_file_count="$(find "$ROOT_DIR/src" -name '*.rs' -type f | wc -l | tr -d ' ')"
if [[ "$rust_file_count" -lt 12 ]]; then
  printf 'expected the Rust CLI under src/, found only %s files\n' "$rust_file_count" >&2
  exit 1
fi

if [[ -z "$(find "$ROOT_DIR/src/speech" -name '*.rs' -type f 2>/dev/null)" ]]; then
  printf 'the speech module is missing from src/\n' >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# The rollback anchor still resolves.
#
# Removing the source must not remove the ability to get it back, so this is
# resolved with rev-parse rather than grepped for a mention, and the resolved
# commit is pinned.
# ---------------------------------------------------------------------------

ROLLBACK_TAG='legacy/tauri-gui-mainline'
ROLLBACK_COMMIT='6bac3e5509cc33702e96873b5701c5b17c7dfe02'

if ! resolved_commit="$(git -C "$ROOT_DIR" rev-parse --verify --quiet "refs/tags/${ROLLBACK_TAG}^{commit}")"; then
  printf 'the documented Tauri GUI rollback tag does not resolve: %s\n' "$ROLLBACK_TAG" >&2
  exit 1
fi

if [[ "$resolved_commit" != "$ROLLBACK_COMMIT" ]]; then
  printf 'rollback tag %s resolves to %s, expected %s\n' \
    "$ROLLBACK_TAG" "$resolved_commit" "$ROLLBACK_COMMIT" >&2
  exit 1
fi

if ! git -C "$ROOT_DIR" merge-base --is-ancestor "$ROLLBACK_COMMIT" HEAD; then
  printf 'rollback commit %s is no longer on this branch\n' "$ROLLBACK_COMMIT" >&2
  exit 1
fi

# The documented recovery command has to be real. Grepping the README for the
# tag proves only that it is mentioned, so the paths it restores are resolved
# against the tag itself.
for restored_path in \
  'src-tauri' \
  'package.json' \
  'package-lock.json' \
  'svelte.config.js' \
  'vite.config.ts' \
  'tsconfig.json' \
  'src/App.svelte' \
  'src/main.ts' \
  'src/app.css' \
  'src/index.html' \
  'src/lib'; do
  if ! git -C "$ROOT_DIR" cat-file -e "${ROLLBACK_TAG}:${restored_path}" 2>/dev/null; then
    printf 'the rollback tag no longer carries %s, so the documented recovery is wrong\n' \
      "$restored_path" >&2
    exit 1
  fi
done

# ---------------------------------------------------------------------------
# Documentation describes the state that actually exists.
# ---------------------------------------------------------------------------

for text in \
  'Headless Mainline' \
  'Rust CLI' \
  'Read-only TUI' \
  'Agent Data Skill' \
  'AppKit GUI'; do
  if ! grep -Fq -- "$text" "$README"; then
    printf 'missing README headless boundary text: %s\n' "$text" >&2
    exit 1
  fi
done

for text in \
  'Headless Mainline' \
  'Rust CLI' \
  'Read-only TUI' \
  'Agent Data Skill'; do
  if ! grep -Fq -- "$text" "$AGENTS"; then
    printf 'missing AGENTS headless boundary text: %s\n' "$text" >&2
    exit 1
  fi
done

if grep -Fq -- '### 方式三：GUI 应用' "$README"; then
  printf 'legacy GUI remains in the README quick-start path\n' >&2
  exit 1
fi

# The README must keep naming the rollback ref, since the source is gone and the
# tag is now the only way back.
for text in \
  '## Headless 能力矩阵' \
  "$ROLLBACK_TAG" \
  'Tauri'; do
  if ! grep -Fq -- "$text" "$README"; then
    printf 'missing README deprecation text: %s\n' "$text" >&2
    exit 1
  fi
done

# Requiring the tag name proves the ref is mentioned, not that the surrounding
# prose agrees with the tree. A document can satisfy every check above while
# still telling the reader the source is retained, so the retained-source
# phrasing is rejected outright. CONTEXT.md is included because it is the live
# domain glossary that every agent is instructed to read as current state.
for doc in "$README" "$AGENTS" "$ROOT_DIR/CONTEXT.md"; do
  while IFS= read -r stale_claim; do
    printf '%s still claims the Tauri source is retained: %s\n' \
      "$(basename "$doc")" "$stale_claim" >&2
    exit 1
  done < <(grep -En '源码(仍)?保留|source is retained|source remains|remains in source' "$doc" || true)
done

# A removed script must not still be documented as a working command. The
# pattern is anchored to the start of a line so a truthful prose mention such as
# "旧的 `npm run build` 已移除" does not fail the gate.
for stale in \
  'npm run dev' \
  'npm run build' \
  'npm run preview' \
  'npm run tauri dev' \
  'npm run tauri build' \
  'npm run legacy-gui:dev' \
  'npm run legacy-gui:build' \
  'npm run legacy-gui:tauri' \
  'npm install'; do
  if grep -Eq -- "^[[:space:]]*${stale}([[:space:]]|$)" "$README"; then
    printf 'README still documents a removed script: %s\n' "$stale" >&2
    exit 1
  fi
done

# Every top-level command has to have its own row in the matrix, so a new CLI
# capability cannot be added without saying whether it still works without a
# GUI. Only the table rows are inspected: these names also appear in unrelated
# prose and in the closing footnote, which would satisfy a looser check.
MATRIX_ROWS="$(awk '
  /^## Headless 能力矩阵/ { inside = 1; next }
  /^## / { inside = 0 }
  inside && /^\|/ { print }
' "$README")"

if [[ -z "$MATRIX_ROWS" ]]; then
  printf 'the README capability matrix has no table rows\n' >&2
  exit 1
fi

for command_name in \
  'list' 'annotations' 'export' 'doctor' 'enrich' 'card' 'config' 'cache' 'speech'; do
  if ! printf '%s' "${MATRIX_ROWS}" | grep -Fq -- "$command_name"; then
    printf 'capability matrix has no row for the %s command\n' "$command_name" >&2
    exit 1
  fi
done

printf 'headless mainline contract passed\n'
