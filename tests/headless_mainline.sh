#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOW="$ROOT_DIR/.github/workflows/release.yml"
README="$ROOT_DIR/README.md"
AGENTS="$ROOT_DIR/AGENTS.md"

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

printf 'headless mainline contract passed\n'
