import { describe, expect, test } from "bun:test";
import { parseBookList } from "./backend";

describe("parseBookList", () => {
  test("accepts the versioned Rust book-list protocol", () => {
    const result = parseBookList(
      JSON.stringify({
        schema_version: 1,
        books: [
          {
            asset_id: "book-1",
            title: "纳瓦尔宝典",
            author: "Eric Jorgenson",
            note_count: 218,
          },
        ],
      }),
    );

    expect(result).toEqual([
      {
        asset_id: "book-1",
        title: "纳瓦尔宝典",
        author: "Eric Jorgenson",
        note_count: 218,
      },
    ]);
  });

  test("rejects an unknown protocol version", () => {
    expect(() =>
      parseBookList(JSON.stringify({ schema_version: 2, books: [] })),
    ).toThrow("unsupported book-list schema");
  });
});
