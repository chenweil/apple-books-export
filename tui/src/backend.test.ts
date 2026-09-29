import { describe, expect, test } from "bun:test";
import {
  BackendCommandError,
  parseAnnotationResponse,
  parseBookList,
  parseMachineError,
} from "./backend";

const fixture = (name: string): Promise<string> =>
  Bun.file(new URL(`../test/fixtures/${name}`, import.meta.url)).text();

describe("machine protocol fixtures", () => {
  test("parses the versioned Rust book-list protocol", async () => {
    const result = parseBookList(await fixture("books.json"));

    expect(result).toEqual([
      {
        asset_id: "book-1",
        title: "深入理解计算机系统",
        author: "Randal E. Bryant",
        note_count: 2,
      },
    ]);
  });

  test("parses annotation details including nullable fields", async () => {
    const result = parseAnnotationResponse(await fixture("annotations.json"));

    expect(result.asset_id).toBe("book-1");
    expect(result.annotation_count).toBe(2);
    expect(result.annotations[0]).toEqual({
      id: "annotation-41",
      type: "highlight",
      content_text: "程序的生命周期从源文件开始。",
      note_text: null,
      chapter_title: "第 1 章 计算机系统漫游",
      location: "epubcfi(/6/4!/4/2/2)",
      created_at: "2026-08-01T10:30:00Z",
    });
    expect(result.annotations[1]?.note_text).toBe("上下文决定位序列的解释。");
  });

  test("parses stable machine errors", async () => {
    const error = parseMachineError(await fixture("error.json"), 1);

    expect(error).toBeInstanceOf(BackendCommandError);
    expect(error.code).toBe("FULL_DISK_ACCESS_REQUIRED");
    expect(error.remediation).toContain("Full Disk Access");
    expect(error.exitCode).toBe(1);
  });

  test("rejects an unknown protocol version with a stable error", () => {
    for (const parse of [
      () => parseBookList(JSON.stringify({ schema_version: 2, books: [] })),
      () =>
        parseAnnotationResponse(
          JSON.stringify({ schema_version: 2, annotations: [] }),
        ),
    ]) {
      try {
        parse();
        throw new Error("expected unsupported schema failure");
      } catch (error) {
        expect(error).toBeInstanceOf(BackendCommandError);
        expect((error as BackendCommandError).code).toBe(
          "UNSUPPORTED_SCHEMA_VERSION",
        );
      }
    }
  });

  test("rejects malformed annotation payloads", () => {
    expect(() =>
      parseAnnotationResponse(
        JSON.stringify({
          schema_version: 1,
          asset_id: "book-1",
          title: "Book",
          author: "Author",
          annotation_count: 1,
          annotations: [{ id: "annotation-1", type: "bookmark" }],
        }),
      ),
    ).toThrow("invalid annotation payload");
  });
});
