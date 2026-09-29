import { join } from "node:path";

const SUPPORTED_SCHEMA_VERSION = 1;

type NullableString = string | null;

export interface Book {
  asset_id: string;
  title: string;
  author: string;
  note_count: number;
}

export interface Annotation {
  id: string;
  type: "highlight" | "note";
  content_text: NullableString;
  note_text: NullableString;
  chapter_title: NullableString;
  location: NullableString;
  created_at: NullableString;
}

export interface AnnotationResponse {
  asset_id: string;
  title: string;
  author: string;
  annotation_count: number;
  annotations: Annotation[];
}

interface MachineErrorPayload {
  code: string;
  message: string;
  remediation?: string;
}

export class BackendCommandError extends Error {
  readonly code: string;
  readonly remediation?: string;
  readonly exitCode: number;

  constructor({
    code,
    message,
    remediation,
    exitCode,
  }: MachineErrorPayload & { exitCode: number }) {
    super(message);
    this.name = "BackendCommandError";
    this.code = code;
    this.remediation = remediation;
    this.exitCode = exitCode;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseJson(output: string, label: string): unknown {
  try {
    return JSON.parse(output);
  } catch (error) {
    throw new Error(`invalid ${label} JSON: ${String(error)}`);
  }
}

function requireSupportedSchema(value: unknown): Record<string, unknown> {
  if (!isRecord(value)) throw new Error("invalid machine protocol payload");
  if (value.schema_version !== SUPPORTED_SCHEMA_VERSION) {
    throw new BackendCommandError({
      code: "UNSUPPORTED_SCHEMA_VERSION",
      message: `unsupported machine protocol schema: ${String(value.schema_version)}`,
      remediation:
        "请升级 TUI，或使用支持 schema_version 1 的 apple-books-exporter binary。",
      exitCode: 1,
    });
  }
  return value;
}

function isNullableString(value: unknown): value is NullableString {
  return typeof value === "string" || value === null;
}

function isBook(value: unknown): value is Book {
  if (!isRecord(value)) return false;

  return (
    typeof value.asset_id === "string" &&
    typeof value.title === "string" &&
    typeof value.author === "string" &&
    typeof value.note_count === "number" &&
    Number.isInteger(value.note_count) &&
    value.note_count >= 0
  );
}

function isAnnotation(value: unknown): value is Annotation {
  if (!isRecord(value)) return false;

  return (
    typeof value.id === "string" &&
    (value.type === "highlight" || value.type === "note") &&
    isNullableString(value.content_text) &&
    isNullableString(value.note_text) &&
    isNullableString(value.chapter_title) &&
    isNullableString(value.location) &&
    isNullableString(value.created_at)
  );
}

export function parseBookList(output: string): Book[] {
  const value = requireSupportedSchema(parseJson(output, "book-list"));
  if (!Array.isArray(value.books) || !value.books.every(isBook)) {
    throw new Error("invalid book-list payload");
  }

  return value.books;
}

export function parseAnnotationResponse(output: string): AnnotationResponse {
  const value = requireSupportedSchema(parseJson(output, "annotation"));
  if (
    typeof value.asset_id !== "string" ||
    typeof value.title !== "string" ||
    typeof value.author !== "string" ||
    typeof value.annotation_count !== "number" ||
    !Number.isInteger(value.annotation_count) ||
    value.annotation_count < 0 ||
    !Array.isArray(value.annotations) ||
    !value.annotations.every(isAnnotation) ||
    value.annotation_count !== value.annotations.length
  ) {
    throw new Error("invalid annotation payload");
  }

  return {
    asset_id: value.asset_id,
    title: value.title,
    author: value.author,
    annotation_count: value.annotation_count,
    annotations: value.annotations,
  };
}

export function parseMachineError(
  output: string,
  exitCode: number,
): BackendCommandError {
  const value = requireSupportedSchema(parseJson(output, "machine error"));
  if (!isRecord(value.error)) {
    throw new Error("invalid machine error payload");
  }

  const { code, message, remediation } = value.error;
  if (
    typeof code !== "string" ||
    typeof message !== "string" ||
    (remediation !== undefined && typeof remediation !== "string")
  ) {
    throw new Error("invalid machine error payload");
  }

  return new BackendCommandError({
    code,
    message,
    remediation,
    exitCode,
  });
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

function spawnFailure(error: unknown): BackendCommandError {
  const message = String(error);
  const incompatible = /bad cpu type|exec format|cannot execute binary/i.test(
    message,
  );
  return new BackendCommandError({
    code: incompatible ? "BINARY_INCOMPATIBLE" : "BACKEND_UNAVAILABLE",
    message: `无法启动 Rust 后端：${message}`,
    remediation: incompatible
      ? "请使用与当前 macOS CPU 架构兼容的 apple-books-exporter binary。"
      : "请先构建 apple-books-exporter，或用 APPLE_BOOKS_EXPORTER_BIN 指定 binary。",
    exitCode: 1,
  });
}

async function runMachineCommand(args: string[]): Promise<string> {
  const backend = await resolveBackend();
  let processHandle: Bun.ReadableSubprocess;
  try {
    processHandle = Bun.spawn([backend, ...args], {
      stdout: "pipe",
      stderr: "pipe",
    });
  } catch (error) {
    throw spawnFailure(error);
  }

  const [exitCode, stdout, stderr] = await Promise.all([
    processHandle.exited,
    new Response(processHandle.stdout).text(),
    new Response(processHandle.stderr).text(),
  ]);

  if (exitCode !== 0) {
    const errorOutput = stderr.trim();
    if (errorOutput) {
      try {
        throw parseMachineError(errorOutput, exitCode);
      } catch (error) {
        if (error instanceof BackendCommandError) throw error;
      }
    }
    throw new BackendCommandError({
      code: "BACKEND_COMMAND_FAILED",
      message: `Rust 后端退出码: ${exitCode}`,
      remediation: "运行 apple-books-exporter doctor --json 检查环境。",
      exitCode,
    });
  }

  return stdout;
}

export async function loadBooks(): Promise<Book[]> {
  return parseBookList(await runMachineCommand(["list", "--json"]));
}

export async function loadAnnotations(
  assetId: string,
): Promise<AnnotationResponse> {
  return parseAnnotationResponse(
    await runMachineCommand([
      "annotations",
      "--asset-id",
      assetId,
      "--json",
    ]),
  );
}
