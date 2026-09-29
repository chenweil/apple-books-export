import { createCliRenderer, type CliRenderer } from "@opentui/core";
import { createBookBrowser } from "./app";
import { BackendCommandError, loadBooks } from "./backend";

let renderer: CliRenderer | undefined;

try {
  const books = await loadBooks();
  renderer = await createCliRenderer({ exitOnCtrlC: true });
  createBookBrowser(renderer, books);
} catch (error) {
  renderer?.destroy();
  if (error instanceof BackendCommandError) {
    console.error(`无法启动 Apple Books TUI [${error.code}]：${error.message}`);
    if (error.remediation) console.error(error.remediation);
  } else {
    console.error(`无法启动 Apple Books TUI：${String(error)}`);
    console.error(
      "请先构建 Rust CLI，并确认终端已获得 Full Disk Access。可用 APPLE_BOOKS_EXPORTER_BIN 指定后端路径。",
    );
  }
  process.exitCode = 1;
}
