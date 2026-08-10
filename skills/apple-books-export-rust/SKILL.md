---
name: apple-books-export-rust
description: Use when an AI agent needs to list, filter, inspect, or export Apple Books annotations to Obsidian or plain Markdown on macOS.
---

# Apple Books Data Skill

Use the Rust Machine JSON Protocol as the only data interface. Keep listing,
reading, and Markdown export local to the user's machine.

## 1. Resolve and validate the binary

Resolve `<skill-directory>` to the directory containing this `SKILL.md`. On
non-macOS systems, explain that Apple Books data is unavailable and stop.

Run the repository validator before any data command:

```bash
VALIDATOR="<skill-directory>/scripts/validate.sh"
EXPORTER="$("$VALIDATOR" --print-path)"
```

In short, the preflight entry point is `validate.sh --print-path`.

The validator must complete successfully. It verifies that the selected file
is executable, is a Mach-O binary compatible with the current CPU, and that
`--help` declares `list`, `annotations`, `export`, and `doctor`.

The resolver uses this order:

1. `APPLE_BOOKS_EXPORTER_BIN`, when set;
2. the repository's `target/release/apple-books-exporter`;
3. the repository's `target/debug/apple-books-exporter`;
4. `apple-books-exporter` found through `PATH`;
5. the installed Skill's bundled `scripts/apple-books-exporter`, only when
   `PATH` has no usable binary.

The final PATH lookup is `command -v apple-books-exporter`.

The bundled fallback is for an installed Skill copy; repository release and
debug binaries, then PATH, always take precedence. Do not download a binary,
build one, or replace one unless the user explicitly asks. If validation fails,
report its stable error code and remediation, then stop.

`--help` is used only for capability validation. It is never a data source.

## 2. Refresh and select by stable identity

Refresh the library immediately before selection:

```bash
LIST_JSON="$("$EXPORTER" list --json)"
```

The command must succeed with JSON on stdout and no human table parsing. Parse
the response as a JSON object and require:

- integer `schema_version` equal to the supported version;
- a `books` array;
- each selected book's `asset_id`, `title`, `author`, and `note_count`.

Match the user's title or author request against the parsed DTOs. For multiple
matches, show title, author, annotation count, and `asset_id`, then ask the
user to choose. For no match, report that no annotated book matched and stop.

If the user supplies a display index, use it only against this freshly
parsed list and immediately convert the choice to its `asset_id`. Never pass a
display index to a machine command. A refreshed list invalidates an earlier
display index.

Reject an unsupported `schema_version` explicitly. Do not silently fall back
to human CLI output or guess a field meaning.

## 3. Read annotations when requested

Use the selected stable identity:

```bash
ANNOTATIONS_JSON="$("$EXPORTER" annotations --asset-id "$ASSET_ID" --json)"
```

Require JSON on stdout, the supported integer `schema_version`, and a response
whose `asset_id` equals `ASSET_ID`. Read `annotation_count` and the
`annotations` array from the response. Each annotation exposes `id`, `type`,
`content_text`, `note_text`, `chapter_title`, `location`, and `created_at`;
nullable values remain `null`.

When presenting annotations, preserve the distinction between highlighted
content and a personal note. Keep the content on the local machine.

## 4. Export Markdown when requested

Use `~/books-exported` unless the user supplies another output directory. Use
`obsidian` unless the user explicitly requests plain `markdown`.
The machine form is `export --asset-id <asset_id> --json`.

```bash
OUTPUT_DIR="$HOME/books-exported"
EXPORT_JSON="$("$EXPORTER" export \
  --asset-id "$ASSET_ID" \
  --json \
  --output "$OUTPUT_DIR" \
  --format obsidian)"
```

Parse the successful JSON response and require a `receipt` containing
`asset_id`, title, annotation count, format, output directory, and
`generated_files`. Confirm the receipt's `asset_id` and output directory match
the current request. Pass `--overwrite` only when the user explicitly
authorizes replacing existing files; otherwise preserve the default refusal.

The Skill can list, filter, read, and export. It does not modify Apple Books,
invoke `enrich`, invoke `card`, change cache/configuration, upload note
content, or call remote AI as part of this workflow.

## 5. Verify before reporting success

Before claiming an export succeeded, inspect the receipt's `generated_files`:

- every reported path exists;
- every reported export file has the `.md` suffix;
- at least one Markdown file in the selected output directory is non-empty;
- the files remain inside the user-selected output directory.

Report only after these checks pass:

- exact book title;
- annotation count from the receipt;
- format;
- absolute path to each generated non-empty Markdown file.

## Errors and permissions

For a failed machine command, parse structured JSON from stderr and preserve
its stable `code`, human-readable `message`, and `remediation`. In particular,
preserve `FULL_DISK_ACCESS_REQUIRED` and tell the user to grant Full Disk
Access to the terminal or agent host in **System Settings → Privacy & Security
→ Full Disk Access**, then rerun the workflow from step 2.

Use `doctor --json` when diagnosing binary, database, or permission failures.
Its response is diagnostic data; do not treat human CLI output as a fallback.
