#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SKILL_DIR="$ROOT_DIR/skills/apple-books-export-rust"
SKILL_FILE="$SKILL_DIR/SKILL.md"
VALIDATOR="$SKILL_DIR/scripts/validate.sh"

required_text=(
  'APPLE_BOOKS_EXPORTER_BIN'
  'target/release/apple-books-exporter'
  'target/debug/apple-books-exporter'
  'command -v apple-books-exporter'
  'list --json'
  'annotations --asset-id'
  'export --asset-id'
  'doctor --json'
  'schema_version'
  'asset_id'
  'FULL_DISK_ACCESS_REQUIRED'
  'generated_files'
  '--overwrite'
  'non-empty'
  'validate.sh --print-path'
)

for text in "${required_text[@]}"; do
  if ! grep -Fq -- "$text" "$SKILL_FILE"; then
    printf 'missing Skill contract text: %s\n' "$text" >&2
    exit 1
  fi
done

if grep -Eq '^[[:space:]]*"\$EXPORTER"[[:space:]]+list[[:space:]]*$' "$SKILL_FILE"; then
  printf 'Skill must not select books from human list output\n' >&2
  exit 1
fi

if grep -Eq '^[[:space:]]*"\$EXPORTER"[[:space:]]+export[[:space:]]+<index>[[:space:]]*$' "$SKILL_FILE"; then
  printf 'Skill must export by asset_id, not a display index\n' >&2
  exit 1
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  printf 'contract text checks passed; runtime binary check skipped outside macOS\n'
  exit 0
fi

debug_binary="$ROOT_DIR/target/debug/apple-books-exporter"
if [[ ! -x "$debug_binary" ]]; then
  printf 'runtime check requires %s; run cargo build first\n' "$debug_binary" >&2
  exit 1
fi

resolved_path="$(
  APPLE_BOOKS_EXPORTER_BIN="$debug_binary" \
    "$VALIDATOR" --print-path
)"

if [[ "$resolved_path" != "$debug_binary" ]]; then
  printf 'validator selected unexpected binary: %s\n' "$resolved_path" >&2
  exit 1
fi

release_binary="$ROOT_DIR/target/release/apple-books-exporter"
if [[ ! -x "$release_binary" ]]; then
  printf 'resolver order check requires %s; run cargo build --release first\n' "$release_binary" >&2
  exit 1
fi

default_path="$(env -u APPLE_BOOKS_EXPORTER_BIN "$VALIDATOR" --print-path)"
if [[ "$default_path" != "$release_binary" ]]; then
  printf 'resolver did not prefer the repository release binary: %s\n' "$default_path" >&2
  exit 1
fi

printf 'Skill contract and runtime validation passed\n'
