#!/usr/bin/env bash
#
# Assert that a packaged .app resolves SwiftPM's Bundle.module.
#
# v0.3.4 shipped an app that crashed on its first Share Card render with
# EXC_BREAKPOINT. The stack was not a memory fault: `Bundle.module` hit the
# `fatalError` in the accessor SwiftPM generates, because the resource bundle
# was not anywhere the accessor looks. Nothing in CI noticed, because inside
# the build tree the accessor's second candidate -- an absolute path into
# .build -- always resolves, so `Bundle.module` works in tests and only fails
# once the app is copied somewhere else.
#
# The generated accessor (see the DerivedSources/resource_bundle_accessor.swift
# that `swift build` emits) tries exactly two paths:
#
#   1. Bundle.main.bundleURL + "<name>.bundle"   <- the .app ROOT
#   2. an absolute path into the build directory <- only valid on the build machine
#
# So for a shipped app the resource bundle has to sit at the app root. The
# packer used to put it in Contents/, which is neither candidate. This script
# inspects a real .app directory rather than grepping the packaging script,
# because the wrong destination and the right one differ only in a path
# fragment of the same `cp -R` line -- a text check cannot tell them apart, and
# grepping for the buggy string is exactly the check that would have passed.
#
# Usage: verify-resource-bundle-layout.sh <path/to/Foo.app> [expected-bundle-name]

set -euo pipefail

if [[ $# -lt 1 ]]; then
  printf 'usage: %s <path/to/Foo.app> [expected-bundle-name]\n' "$(basename "$0")" >&2
  exit 2
fi

APP_DIR="$1"
EXPECTED="${2:-}"

if [[ ! -d "$APP_DIR" ]]; then
  printf 'not an app directory: %s\n' "$APP_DIR" >&2
  exit 1
fi

fail() {
  printf 'resource bundle layout is wrong: %s\n' "$1" >&2
  printf 'Bundle.module only looks at <app root>/<name>.bundle and at an absolute\n' >&2
  printf 'path inside the build directory. A bundle in Contents/ is unreachable:\n' >&2
  printf 'Bundle(path:) happily returns a Bundle for it, which is why this went\n' >&2
  printf 'unnoticed -- but the generated accessor never tries that path.\n' >&2
  exit 1
}

# Anything the packer put somewhere else, so the failure message can name it
# instead of leaving the reader to go looking.
found_elsewhere=()
while IFS= read -r candidate; do
  found_elsewhere+=("${candidate#$APP_DIR/}")
done < <(find "$APP_DIR" -maxdepth 3 -name '*.bundle' -type d 2>/dev/null | sort)

if [[ -n "$EXPECTED" ]]; then
  if [[ ! -d "$APP_DIR/$EXPECTED" ]]; then
    if [[ ${#found_elsewhere[@]} -gt 0 ]]; then
      fail "expected $EXPECTED at the app root; found instead: ${found_elsewhere[*]}"
    fi
    fail "expected $EXPECTED at the app root; it is not in the app at all"
  fi
  target="$APP_DIR/$EXPECTED"
else
  # No name given: exactly one .bundle at the root is the only unambiguous case.
  shopt -s nullglob
  roots=("$APP_DIR"/*.bundle)
  shopt -u nullglob
  if [[ ${#roots[@]} -eq 0 ]]; then
    if [[ ${#found_elsewhere[@]} -gt 0 ]]; then
      fail "no .bundle at the app root; found instead: ${found_elsewhere[*]}"
    fi
    fail "no .bundle anywhere under $APP_DIR"
  fi
  if [[ ${#roots[@]} -gt 1 ]]; then
    fail "expected exactly one .bundle at the app root, found ${#roots[@]}: ${roots[*]##*/}"
  fi
  target="${roots[0]}"
fi

if [[ ! -d "$target" ]]; then
  fail "$target is not a directory"
fi

count="$(find "$target" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')"
if [[ "$count" -eq 0 ]]; then
  fail "$target is empty; the Share Card fonts and templates would be missing"
fi

# A stray copy under Contents/ is dead weight but not a crash: the accessor
# finds the root one first. Say so, rather than leaving it to be discovered
# later as "why is the app 170 MB of duplicated fonts".
stray=()
for path in "${found_elsewhere[@]}"; do
  if [[ "$path" != "${target#$APP_DIR/}" ]]; then
    stray+=("$path")
  fi
done

printf 'resource bundle OK: %s (%s entries) at the app root\n' "${target##*/}" "$count"
if [[ ${#stray[@]} -gt 0 ]]; then
  printf 'note: a duplicate also exists at %s -- unreachable, safe to drop\n' "${stray[*]}"
fi
