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
  if grep -Fq -- "$forbidden" "$RELEASE_WORKFLOW"; then
    printf 'legacy GUI release path remains: %s\n' "$forbidden" >&2
    exit 1
  fi
done

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
