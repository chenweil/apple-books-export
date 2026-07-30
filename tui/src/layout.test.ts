import { describe, expect, test } from "bun:test";
import { getLayoutMode } from "./layout";

describe("getLayoutMode", () => {
  test.each([
    [35, "compact"],
    [80, "medium"],
    [120, "wide"],
  ] as const)("uses the expected layout at %i columns", (width, mode) => {
    expect(getLayoutMode(width)).toBe(mode);
  });
});
