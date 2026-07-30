import { afterEach, describe, expect, test } from "bun:test";
import { createTestRenderer } from "@opentui/core/testing";
import type { TestRendererSetup } from "@opentui/core/testing";
import { createBookBrowser } from "./app";
import type { Book } from "./backend";

const books: Book[] = [
  {
    asset_id: "book-1",
    title: "深入理解计算机系统",
    author: "Randal E. Bryant",
    note_count: 45,
  },
  {
    asset_id: "book-2",
    title: "纳瓦尔宝典",
    author: "Eric Jorgenson",
    note_count: 218,
  },
];

let testSetup: TestRendererSetup | undefined;

afterEach(() => {
  testSetup?.renderer.destroy();
  testSetup = undefined;
});

describe("book browser", () => {
  test("renders a list and the selected book detail", async () => {
    testSetup = await createTestRenderer({ width: 80, height: 24 });
    createBookBrowser(testSetup.renderer, books);

    await testSetup.renderOnce();
    const frame = testSetup.captureCharFrame();

    expect(frame).toContain("Apple Books");
    expect(frame).toContain("深入理解计算机系统");
    expect(frame).toContain("Randal E. Bryant");
    expect(frame).toContain("45");
  });

  test("updates the detail while navigating", async () => {
    testSetup = await createTestRenderer({ width: 80, height: 24 });
    createBookBrowser(testSetup.renderer, books);

    testSetup.mockInput.pressArrow("down");
    await testSetup.renderOnce();

    expect(testSetup.captureCharFrame()).toContain("Eric Jorgenson");
    expect(testSetup.captureCharFrame()).toContain("218");
  });

  test("switches compact mode between list and detail", async () => {
    testSetup = await createTestRenderer({
      width: 35,
      height: 20,
      kittyKeyboard: true,
    });
    createBookBrowser(testSetup.renderer, books);

    testSetup.mockInput.pressEnter();
    await testSetup.renderOnce();
    let frame = testSetup.captureCharFrame();

    expect(frame).toContain("书籍详情");
    expect(frame).not.toContain("搜索结果");

    testSetup.mockInput.pressEscape();
    await testSetup.renderOnce();
    frame = testSetup.captureCharFrame();

    expect(frame).toContain("搜索结果");
  });

  test("filters books from the search input", async () => {
    testSetup = await createTestRenderer({ width: 80, height: 24 });
    createBookBrowser(testSetup.renderer, books);

    testSetup.mockInput.pressKey("/");
    await testSetup.mockInput.typeText("纳瓦尔");
    await testSetup.renderOnce();
    const frame = testSetup.captureCharFrame();

    expect(frame).toContain("搜索结果 (1)");
    expect(frame).toContain("纳瓦尔宝典");
    expect(frame).not.toContain("深入理解计算机系统");
  });

  test("reflows when the terminal becomes compact", async () => {
    testSetup = await createTestRenderer({ width: 120, height: 24 });
    createBookBrowser(testSetup.renderer, books);

    await testSetup.renderOnce();
    expect(testSetup.captureCharFrame()).toContain("导航");

    testSetup.resize(35, 20);
    await testSetup.renderOnce();
    const frame = testSetup.captureCharFrame();

    expect(frame).not.toContain("导航");
    expect(frame).toContain("搜索结果");
    expect(
      frame.split("\n").every((line) => Array.from(line).length <= 35),
    ).toBe(true);
  });

  test("destroys the renderer when q is pressed outside search", async () => {
    testSetup = await createTestRenderer({ width: 80, height: 24 });
    createBookBrowser(testSetup.renderer, books);
    let destroyed = false;
    testSetup.renderer.on("destroy", () => {
      destroyed = true;
    });

    testSetup.mockInput.pressKey("q");

    expect(destroyed).toBe(true);
    testSetup = undefined;
  });
});
