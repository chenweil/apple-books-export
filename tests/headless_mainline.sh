#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOW="$ROOT_DIR/.github/workflows/release.yml"
README="$ROOT_DIR/README.md"
AGENTS="$ROOT_DIR/AGENTS.md"
PACKAGE_JSON="$ROOT_DIR/package.json"
TAURI_CONF="$ROOT_DIR/src-tauri/tauri.conf.json"

required_workflow_text=(
  'cargo build --release --target'
  'OUTPUT="${{ matrix.binary_name }}"'
  'cp "$CLI" "$OUTPUT"'
  'apple-books-exporter-aarch64-apple-darwin'
  'apple-books-exporter-x86_64-apple-darwin'
  'SHA256SUMS'
)

for text in "${required_workflow_text[@]}"; do
  if ! grep -Fq -- "$text" "$WORKFLOW"; then
    printf 'missing headless release contract text: %s\n' "$text" >&2
    exit 1
  fi
done

for forbidden in \
  'cargo tauri' \
  'npm ci' \
  'Setup Node.js' \
  'Build GUI' \
  'locate-gui' \
  'gui-app-' \
  'gui-dmg-' \
  '.app.zip' \
  '.dmg'; do
  if grep -Fq -- "$forbidden" "$WORKFLOW"; then
    printf 'legacy GUI release path remains: %s\n' "$forbidden" >&2
    exit 1
  fi
done

for text in \
  'Headless Mainline' \
  'Tauri Legacy GUI' \
  'Rust CLI' \
  'Read-only TUI' \
  'Agent Data Skill'; do
  if ! grep -Fq -- "$text" "$README"; then
    printf 'missing README headless boundary text: %s\n' "$text" >&2
    exit 1
  fi
done

if grep -Fq -- '### 方式三：GUI 应用' "$README"; then
  printf 'legacy GUI remains in the README quick-start path\n' >&2
  exit 1
fi

for text in \
  'Headless Mainline' \
  'Tauri Legacy GUI' \
  'Rust CLI' \
  'Read-only TUI' \
  'Agent Data Skill'; do
  if ! grep -Fq -- "$text" "$AGENTS"; then
    printf 'missing AGENTS headless boundary text: %s\n' "$text" >&2
    exit 1
  fi
done

if [[ ! -d "$ROOT_DIR/src-tauri" ]]; then
  printf 'Tauri source was removed instead of retained\n' >&2
  exit 1
fi

# package.json must not hand the default entry to the legacy Tauri frontend.
# The keys are parsed rather than grepped: re-pointing `build` at
# `npm run legacy-gui:build` would restore the retired GUI as the default entry
# while every literal command-string guard still passed.
if ! command -v python3 >/dev/null 2>&1; then
  printf 'python3 is required to inspect package.json\n' >&2
  exit 1
fi

if ! python3 - "$PACKAGE_JSON" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    scripts = json.load(handle)["scripts"]

retired = {"dev", "build", "preview", "tauri"}
present = sorted(retired & set(scripts))
if present:
    print("package.json still exposes the retired default entry: " + ", ".join(present))
    sys.exit(1)

required = {
    "cli": "cargo build",
    "tui": "bun run --cwd tui start",
    "legacy-gui:dev": "vite",
    "legacy-gui:build": "vite build",
    "legacy-gui:tauri": "tauri",
}
for name, prefix in required.items():
    value = scripts.get(name)
    if value is None:
        print(f"package.json is missing the {name} entry")
        sys.exit(1)
    if not value.startswith(prefix):
        print(f"package.json {name} must start with {prefix!r}, got {value!r}")
        sys.exit(1)
PY
then
  exit 1
fi

# Renaming the scripts must not silently break the retained legacy build path,
# so tauri.conf.json has to invoke the namespaced scripts itself.
for text in \
  '"beforeDevCommand": "npm run legacy-gui:dev"' \
  '"beforeBuildCommand": "npm run legacy-gui:build"'; do
  if ! grep -Fq -- "$text" "$TAURI_CONF"; then
    printf 'legacy GUI build path is broken, tauri.conf.json must use: %s\n' "$text" >&2
    exit 1
  fi
done

# The documented rollback has to keep resolving. Grepping the README for the tag
# name only proves it is *mentioned*, so resolve the ref and pin its target.
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

# The README has to carry the capability matrix, a named rollback ref, and the
# explicit legacy build path.
for text in \
  '## Headless 能力矩阵' \
  "$ROLLBACK_TAG" \
  'npm run legacy-gui:build'; do
  if ! grep -Fq -- "$text" "$README"; then
    printf 'missing README deprecation text: %s\n' "$text" >&2
    exit 1
  fi
done

# Every top-level command has to have its own row in the matrix, so a new CLI
# capability cannot be added without saying whether it still works without the
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

# A removed script must not be documented as a working command. The pattern is
# anchored to the start of a line so a truthful prose mention such as
# "旧的 `npm run build` 已移除" does not fail the gate.
for stale in \
  'npm run dev' \
  'npm run build' \
  'npm run preview' \
  'npm run tauri dev' \
  'npm run tauri build'; do
  if grep -Eq -- "^[[:space:]]*${stale}([[:space:]]|$)" "$README"; then
    printf 'README still documents a removed script: %s\n' "$stale" >&2
    exit 1
  fi
done

printf 'headless mainline contract passed\n'
