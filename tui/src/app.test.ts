import { afterEach, describe, expect, test } from "bun:test";
import { createTestRenderer } from "@opentui/core/testing";
import type { TestRendererSetup } from "@opentui/core/testing";
import { createBookBrowser } from "./app";
import {
  BackendCommandError,
  type AnnotationResponse,
  type Book,
} from "./backend";

const books: Book[] = [
  {
    asset_id: "book-1",
    title: "深入理解计算机系统",
    author: "Randal E. Bryant",
    note_count: 2,
  },
  {
    asset_id: "book-2",
    title: "纳瓦尔宝典",
    author: "Eric Jorgenson",
    note_count: 1,
  },
];

const detailsByAssetId: Record<string, AnnotationResponse> = {
  "book-1": {
    asset_id: "book-1",
    title: "深入理解计算机系统",
    author: "Randal E. Bryant",
    annotation_count: 2,
    annotations: [
      {
        id: "annotation-41",
        type: "highlight",
        content_text: "程序的生命周期从源文件开始。",
        note_text: null,
        chapter_title: "第 1 章 计算机系统漫游",
        location: "epubcfi(/6/4!/4/2/2)",
        created_at: "2026-08-01T10:30:00Z",
      },
      {
        id: "annotation-42",
        type: "note",
        content_text: "信息就是位加上下文。",
        note_text: "上下文决定解释。",
        chapter_title: null,
        location: null,
        created_at: null,
      },
    ],
  },
  "book-2": {
    asset_id: "book-2",
    title: "纳瓦尔宝典",
    author: "Eric Jorgenson",
    annotation_count: 1,
    annotations: [
      {
        id: "annotation-7",
        type: "note",
        content_text: "选择长期主义。",
        note_text: "复利需要时间。",
        chapter_title: "判断力",
        location: "epubcfi(/6/8!/4/2)",
        created_at: "2026-08-02T09:00:00Z",
      },
    ],
  },
};

let testSetup: TestRendererSetup | undefined;

afterEach(() => {
  testSetup?.renderer.destroy();
  testSetup = undefined;
});

const loadAnnotations = async (assetId: string): Promise<AnnotationResponse> => {
  const details = detailsByAssetId[assetId];
  if (!details) throw new Error(`missing fixture for ${assetId}`);
  return details;
};

describe("book browser", () => {
  test("loads and renders the selected book annotation details", async () => {
    testSetup = await createTestRenderer({ width: 120, height: 30 });
    const browser = createBookBrowser(testSetup.renderer, books, {
      loadAnnotations,
    });

    await browser.waitForIdle();
    const frame = await testSetup.waitForFrame((value) =>
      value.includes("程序的生命周期从源文件开始。"),
    );

    expect(frame).toContain("深入理解计算机系统");
    expect(frame).toContain("Randal E. Bryant");
    expect(frame).toContain("标注：2 条");
    expect(frame).toContain("第 1 章 计算机系统漫游");
    expect(frame).toContain("2026-08-01 10:30 UTC");
    expect(frame).toContain("上下文决定解释。");
  });

  test("loads annotation details while navigating books", async () => {
    testSetup = await createTestRenderer({ width: 100, height: 24 });
    const browser = createBookBrowser(testSetup.renderer, books, {
      loadAnnotations,
    });
    await browser.waitForIdle();

    testSetup.mockInput.pressArrow("down");
    await browser.waitForIdle();
    const frame = await testSetup.waitForFrame((value) =>
      value.includes("选择长期主义。"),
    );

    expect(frame).toContain("Eric Jorgenson");
    expect(frame).toContain("复利需要时间。");
    expect(frame).not.toContain("程序的生命周期从源文件开始。");
  });

  test("switches compact mode between list and loaded detail", async () => {
    testSetup = await createTestRenderer({
      width: 45,
      height: 24,
      kittyKeyboard: true,
    });
    const browser = createBookBrowser(testSetup.renderer, books, {
      loadAnnotations,
    });
    await browser.waitForIdle();

    testSetup.mockInput.pressEnter();
    const detailFrame = await testSetup.waitForFrame((value) =>
      value.includes("程序的生命周期从源文件开始。"),
    );

    expect(detailFrame).toContain("标注详情");
    expect(detailFrame).not.toContain("搜索结果");

    testSetup.mockInput.pressEscape();
    await testSetup.renderOnce();
    expect(testSetup.captureCharFrame()).toContain("搜索结果");
  });

  test("shows stable backend errors and remediation in the detail", async () => {
    testSetup = await createTestRenderer({ width: 120, height: 24 });
    const browser = createBookBrowser(testSetup.renderer, books, {
      loadAnnotations: async () => {
        throw new BackendCommandError({
          code: "FULL_DISK_ACCESS_REQUIRED",
          message: "Permission denied.",
          remediation: "Grant Full Disk Access, then retry.",
          exitCode: 1,
        });
      },
    });

    await browser.waitForIdle();
    const frame = await testSetup.waitForFrame((value) =>
      value.includes("FULL_DISK_ACCESS_REQUIRED"),
    );

    expect(frame).toContain("Permission denied.");
    expect(frame).toContain("Grant Full Disk Access");
  });

  test("filters books from the search input", async () => {
    testSetup = await createTestRenderer({ width: 100, height: 24 });
    const browser = createBookBrowser(testSetup.renderer, books, {
      loadAnnotations,
    });
    await browser.waitForIdle();

    testSetup.mockInput.pressKey("/");
    await testSetup.mockInput.typeText("纳瓦尔");
    await browser.waitForIdle();
    const frame = await testSetup.waitForFrame((value) =>
      value.includes("搜索结果 (1)"),
    );

    expect(frame).toContain("纳瓦尔宝典");
    expect(frame).not.toContain("深入理解计算机系统");
  });

  test("ignores stale annotation responses after rapid navigation", async () => {
    testSetup = await createTestRenderer({ width: 100, height: 24 });
    let releaseFirst: (() => void) | undefined;
    const first = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    const browser = createBookBrowser(testSetup.renderer, books, {
      loadAnnotations: async (assetId) => {
        if (assetId === "book-1") await first;
        return detailsByAssetId[assetId]!;
      },
    });

    testSetup.mockInput.pressArrow("down");
    await testSetup.waitForFrame((value) => value.includes("选择长期主义。"));
    releaseFirst?.();
    await browser.waitForIdle();
    await testSetup.renderOnce();

    const frame = testSetup.captureCharFrame();
    expect(frame).toContain("选择长期主义。");
    expect(frame).not.toContain("程序的生命周期从源文件开始。");
  });

  test("reflows when the terminal becomes compact", async () => {
    testSetup = await createTestRenderer({ width: 120, height: 24 });
    const browser = createBookBrowser(testSetup.renderer, books, {
      loadAnnotations,
    });
    await browser.waitForIdle();

    testSetup.resize(45, 24);
    await testSetup.renderOnce();
    const frame = testSetup.captureCharFrame();

    expect(frame).not.toContain("导航");
    expect(frame).toContain("搜索结果");
    expect(
      frame.split("\n").every((line) => Array.from(line).length <= 45),
    ).toBe(true);
  });

  test("destroys the renderer when q is pressed outside search", async () => {
    testSetup = await createTestRenderer({ width: 80, height: 24 });
    createBookBrowser(testSetup.renderer, books, { loadAnnotations });
    let destroyed = false;
    testSetup.renderer.on("destroy", () => {
      destroyed = true;
    });

    testSetup.mockInput.pressKey("q");

    expect(destroyed).toBe(true);
    testSetup = undefined;
  });
});
