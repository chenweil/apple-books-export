# Apple Books TUI

只读 OpenTUI 前端，用于搜索和浏览 Apple Books 中包含笔记的书籍。

## Requirements

- macOS with Full Disk Access granted to the terminal
- Bun
- A built `apple-books-exporter` Rust binary

## Run

From the repository root:

```bash
cargo build
cd tui
bun install
bun run start
```

Set `APPLE_BOOKS_EXPORTER_BIN` to override the Rust backend path.

## Controls

- `/`: focus search
- `↑` / `↓`: browse books
- `Enter`: open the detail view on narrow terminals
- `Esc`: return or leave search
- `Tab`: switch between search and the list
- `q`: cleanly destroy the renderer and quit

This first phase is intentionally read-only. Export, AI, card generation, and configuration remain in the existing CLI and Tauri GUI.
