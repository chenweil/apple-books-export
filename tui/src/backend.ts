import { join } from "node:path";

export interface Book {
  asset_id: string;
  title: string;
  author: string;
  note_count: number;
}

interface BookListResponse {
  schema_version: number;
  books: unknown;
}

function isBook(value: unknown): value is Book {
  if (typeof value !== "object" || value === null) return false;

  const book = value as Record<string, unknown>;
  return (
    typeof book.asset_id === "string" &&
    typeof book.title === "string" &&
    typeof book.author === "string" &&
    typeof book.note_count === "number"
  );
}

export function parseBookList(output: string): Book[] {
  const value = JSON.parse(output) as BookListResponse;
  if (value.schema_version !== 1) {
    throw new Error(`unsupported book-list schema: ${value.schema_version}`);
  }
  if (!Array.isArray(value.books) || !value.books.every(isBook)) {
    throw new Error("invalid book-list payload");
  }

  return value.books;
}

async function existingBackendCandidates(): Promise<string[]> {
  const repositoryRoot = join(import.meta.dir, "..", "..");
  const candidates = [
    join(repositoryRoot, "target", "release", "apple-books-exporter"),
    join(repositoryRoot, "target", "debug", "apple-books-exporter"),
  ];

  const existing: string[] = [];
  for (const candidate of candidates) {
    if (await Bun.file(candidate).exists()) existing.push(candidate);
  }
  return existing;
}

export async function resolveBackend(): Promise<string> {
  const configured = process.env.APPLE_BOOKS_EXPORTER_BIN;
  if (configured) return configured;

  const candidates = await existingBackendCandidates();
  return candidates[0] ?? "apple-books-exporter";
}

export async function loadBooks(): Promise<Book[]> {
  const backend = await resolveBackend();
  const processHandle = (() => {
    try {
      return Bun.spawn([backend, "list", "--json"], {
        stdout: "pipe",
        stderr: "pipe",
      });
    } catch (error) {
      throw new Error(`无法启动 Rust 后端 ${backend}: ${String(error)}`);
    }
  })();

  const [exitCode, stdout, stderr] = await Promise.all([
    processHandle.exited,
    new Response(processHandle.stdout).text(),
    new Response(processHandle.stderr).text(),
  ]);

  if (exitCode !== 0) {
    throw new Error(stderr.trim() || `Rust 后端退出码: ${exitCode}`);
  }

  return parseBookList(stdout);
}
