# Apple Books TUI

只读 OpenTUI 前端，通过 Rust Machine JSON Protocol 搜索书籍并浏览完整标注详情。

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
- `Enter`: open and focus the annotation detail view
- `↑` / `↓` in detail: scroll annotation content
- `Esc`: return to the book list or leave search
- `Tab`: switch between search and the list
- `q`: cleanly destroy the renderer and quit

The detail view shows the selected book, author, annotation count, highlight text, note text, chapter, location, and timestamp. This first phase is intentionally read-only: it does not export, call AI, generate cards, or write configuration.
