---
name: apple-books-export-rust
description: Export Apple Books notes and highlights to Markdown on macOS. Use when the user asks to list annotated Apple Books titles, export a specific book's notes or highlights, or save Apple Books annotations for Obsidian or plain Markdown.
---

# Export Apple Books Notes

Use the bundled CLI to select one book from the current Apple Books library, export its annotations, and verify the generated Markdown.

## Steps

### 1. Resolve the CLI

Resolve `<skill-directory>` to the directory containing this `SKILL.md`, then use this executable for every command:

```bash
EXPORTER="<skill-directory>/scripts/apple-books-exporter"
"$EXPORTER" --help
```

Continue when the executable exists and its help lists both `list` and `export`.

This data source is macOS-only. On another operating system, explain that the Apple Books databases are unavailable and stop. If the executable is missing or incompatible with the Mac's architecture, report that a compatible build is required; build or install it only when the user asks.

### 2. Select one book

Always refresh the numbered list before exporting because the CLI exports by the index from the current list:

```bash
"$EXPORTER" list
```

Match the requested title against the command output:

- Use the row when exactly one title matches.
- For multiple exact or partial matches, show the matching title, author, note count, and index, then ask the user to choose.
- For no match, report that no annotated book matched and stop.
- When the user supplies an index, confirm that the index exists in this fresh list.

Continue when exactly one current row—including its index, title, author, and note count—has been selected.

### 3. Export

Use `~/books-exported` unless the user supplies another output directory. Use `obsidian` unless the user explicitly requests plain Markdown.

```bash
OUTPUT_DIR="$HOME/books-exported"
"$EXPORTER" export <index> --output "$OUTPUT_DIR" --format obsidian
```

For plain Markdown, replace the final argument with `markdown`. Treat the title as selection input only: the current CLI has no `-t` or `--title` export option.

Continue when the command exits successfully and identifies the selected book and output directory.

### 4. Verify and report

Inspect the selected book's directory below `OUTPUT_DIR`. Confirm that it contains a non-empty `.md` file before reporting success.

Report:

- the exact book title;
- the annotation count printed by the export;
- the format;
- the absolute path to the generated Markdown file.

If the database cannot be opened, preserve the error text and tell the user to grant Full Disk Access to the terminal or agent host in **System Settings → Privacy & Security → Full Disk Access**, then rerun from step 2 after the permission is granted.
