import {
  BoxRenderable,
  InputRenderable,
  InputRenderableEvents,
  ScrollBoxRenderable,
  SelectRenderable,
  SelectRenderableEvents,
  TextAttributes,
  TextRenderable,
  type CliRenderer,
  type KeyEvent,
  type SelectOption,
} from "@opentui/core";
import {
  BackendCommandError,
  loadAnnotations as loadAnnotationsFromBackend,
  type Annotation,
  type AnnotationResponse,
  type Book,
} from "./backend";
import { getLayoutMode, type LayoutMode } from "./layout";

type CompactScreen = "list" | "detail";

type AnnotationLoader = (assetId: string) => Promise<AnnotationResponse>;

interface BookBrowserOptions {
  loadAnnotations?: AnnotationLoader;
}

export interface BookBrowser {
  applyLayout(width: number): void;
  waitForIdle(): Promise<void>;
}

function optionsFor(books: Book[]): SelectOption[] {
  return books.map((book) => ({
    name: book.title,
    description: `${book.author} · ${book.note_count} 条标注`,
    value: book,
  }));
}

function formatCreatedAt(value: string | null): string {
  if (!value) return "未知";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return `${parsed.toISOString().slice(0, 16).replace("T", " ")} UTC`;
}

function formatAnnotation(annotation: Annotation, index: number): string {
  const kind = annotation.type === "note" ? "笔记" : "高亮";
  return [
    `#${index + 1} · ${kind}`,
    `正文：${annotation.content_text ?? "（无正文）"}`,
    `笔记：${annotation.note_text ?? "（无笔记）"}`,
    `章节：${annotation.chapter_title ?? "未知"}`,
    `位置：${annotation.location ?? "未知"}`,
    `时间：${formatCreatedAt(annotation.created_at)}`,
  ].join("\n");
}

function loadingDetail(book: Book): string {
  return [
    book.title,
    `作者：${book.author || "未知"}`,
    `标注：${book.note_count} 条`,
    "",
    "正在读取标注详情…",
  ].join("\n");
}

function emptyDetail(): string {
  return "没有匹配的书籍。\n\n按 / 修改搜索条件。";
}

function annotationDetail(response: AnnotationResponse): string {
  const annotations = response.annotations.length
    ? response.annotations.map(formatAnnotation).join("\n\n────────\n\n")
    : "没有可显示的标注。";

  return [
    response.title,
    `作者：${response.author || "未知"}`,
    `标注：${response.annotation_count} 条`,
    "",
    annotations,
  ].join("\n");
}

function errorDetail(book: Book, error: unknown): string {
  if (error instanceof BackendCommandError) {
    return [
      book.title,
      `作者：${book.author || "未知"}`,
      "",
      `错误：${error.code}`,
      error.message,
      error.remediation ? `处理：${error.remediation}` : undefined,
    ]
      .filter((line): line is string => line !== undefined)
      .join("\n");
  }

  return [
    book.title,
    `作者：${book.author || "未知"}`,
    "",
    "读取标注失败。",
    String(error),
  ].join("\n");
}

export function createBookBrowser(
  renderer: CliRenderer,
  books: Book[],
  options: BookBrowserOptions = {},
): BookBrowser {
  const loadAnnotations =
    options.loadAnnotations ?? loadAnnotationsFromBackend;
  const annotationCache = new Map<string, AnnotationResponse>();
  let filteredBooks = books;
  let layoutMode: LayoutMode = getLayoutMode(renderer.width);
  let compactScreen: CompactScreen = "list";
  let detailFocused = false;
  let detailRequestId = 0;
  let activeLoad: Promise<void> = Promise.resolve();
  let destroyed = false;

  const root = new BoxRenderable(renderer, {
    id: "app",
    width: "100%",
    height: "100%",
    flexDirection: "column",
    backgroundColor: "#10131c",
  });

  const header = new BoxRenderable(renderer, {
    id: "header",
    height: 3,
    paddingX: 1,
    alignItems: "center",
    border: ["bottom"],
    borderColor: "#3a4258",
  });
  header.add(
    new TextRenderable(renderer, {
      content: "Apple Books · 终端浏览",
      attributes: TextAttributes.BOLD,
      fg: "#7dcfff",
    }),
  );

  const searchBar = new BoxRenderable(renderer, {
    id: "search-bar",
    height: 3,
    flexDirection: "row",
    alignItems: "center",
    paddingX: 1,
    gap: 1,
  });
  searchBar.add(
    new TextRenderable(renderer, {
      content: "搜索",
      width: 5,
      fg: "#a9b1d6",
    }),
  );
  const searchInput = new InputRenderable(renderer, {
    id: "search",
    flexGrow: 1,
    placeholder: "按 / 输入书名或作者",
    backgroundColor: "#1a1f2e",
    focusedBackgroundColor: "#252b3d",
    textColor: "#c0caf5",
    cursorColor: "#7aa2f7",
  });
  searchBar.add(searchInput);

  const body = new BoxRenderable(renderer, {
    id: "body",
    flexGrow: 1,
    flexDirection: "row",
    minHeight: 1,
    gap: 1,
    paddingX: 1,
  });

  const navigationPanel = new BoxRenderable(renderer, {
    id: "navigation",
    width: 20,
    height: "100%",
    flexDirection: "column",
    border: true,
    borderStyle: "rounded",
    borderColor: "#3a4258",
    title: "导航",
    padding: 1,
  });
  navigationPanel.add(
    new TextRenderable(renderer, {
      content: [
        "/      搜索",
        "↑/↓    浏览",
        "Enter  打开详情",
        "Esc    返回",
        "Tab    切换焦点",
        "q      退出",
      ].join("\n"),
      fg: "#a9b1d6",
    }),
  );

  const listPanel = new BoxRenderable(renderer, {
    id: "book-list-panel",
    height: "100%",
    flexDirection: "column",
    border: true,
    borderStyle: "rounded",
    borderColor: "#3a4258",
    title: `搜索结果 (${books.length})`,
  });
  const bookSelect = new SelectRenderable(renderer, {
    id: "book-list",
    flexGrow: 1,
    options: optionsFor(books),
    showScrollIndicator: true,
    showDescription: true,
    selectedBackgroundColor: "#283457",
    selectedTextColor: "#ffffff",
    descriptionColor: "#787c99",
    selectedDescriptionColor: "#a9b1d6",
  });
  listPanel.add(bookSelect);

  const detailPanel = new ScrollBoxRenderable(renderer, {
    id: "book-detail-panel",
    height: "100%",
    flexGrow: 1,
    focusable: true,
    scrollY: true,
    border: true,
    borderStyle: "rounded",
    borderColor: "#3a4258",
    title: "标注详情",
    padding: 1,
  });
  const detailText = new TextRenderable(renderer, {
    id: "book-detail",
    content: books[0] ? loadingDetail(books[0]) : emptyDetail(),
    width: "100%",
    fg: "#c0caf5",
    selectable: true,
  });
  detailPanel.add(detailText);

  body.add(navigationPanel);
  body.add(listPanel);
  body.add(detailPanel);

  const footer = new BoxRenderable(renderer, {
    id: "footer",
    height: 1,
    paddingX: 1,
  });
  const footerText = new TextRenderable(renderer, {
    content: "/ 搜索  ↑↓ 浏览  Enter 详情  q 退出",
    attributes: TextAttributes.DIM,
    fg: "#787c99",
  });
  footer.add(footerText);

  root.add(header);
  root.add(searchBar);
  root.add(body);
  root.add(footer);
  renderer.root.add(root);

  function selectedBook(): Book | undefined {
    return bookSelect.getSelectedOption()?.value as Book | undefined;
  }

  function setDetailContent(content: string): void {
    if (destroyed) return;
    detailPanel.scrollTo(0);
    detailText.content = content;
  }

  function requestDetails(book: Book | undefined): void {
    const requestId = ++detailRequestId;
    if (!book) {
      setDetailContent(emptyDetail());
      activeLoad = Promise.resolve();
      return;
    }

    const cached = annotationCache.get(book.asset_id);
    if (cached) {
      setDetailContent(annotationDetail(cached));
      activeLoad = Promise.resolve();
      return;
    }

    setDetailContent(loadingDetail(book));
    activeLoad = loadAnnotations(book.asset_id)
      .then((response) => {
        if (response.asset_id !== book.asset_id) {
          throw new Error(
            `标注响应 asset_id 不匹配：${response.asset_id}`,
          );
        }
        annotationCache.set(book.asset_id, response);
        if (requestId === detailRequestId) {
          setDetailContent(annotationDetail(response));
        }
      })
      .catch((error: unknown) => {
        if (requestId === detailRequestId) {
          setDetailContent(errorDetail(book, error));
        }
      });
  }

  function showCompactScreen(screen: CompactScreen): void {
    compactScreen = screen;
    if (layoutMode !== "compact") return;

    listPanel.visible = screen === "list";
    detailPanel.visible = screen === "detail";
    footerText.content =
      screen === "list"
        ? "/ 搜索  ↑↓ 浏览  Enter 详情  q 退出"
        : "↑↓ 滚动  Esc 返回列表  q 退出";
  }

  function filterBooks(query: string): void {
    const normalized = query.trim().toLocaleLowerCase();
    filteredBooks = normalized
      ? books.filter(
          (book) =>
            book.title.toLocaleLowerCase().includes(normalized) ||
            book.author.toLocaleLowerCase().includes(normalized),
        )
      : books;

    bookSelect.options = optionsFor(filteredBooks);
    if (filteredBooks.length > 0) bookSelect.setSelectedIndex(0);
    listPanel.title = `搜索结果 (${filteredBooks.length})`;
    requestDetails(filteredBooks[0]);
  }

  function focusList(): void {
    detailFocused = false;
    searchInput.blur();
    detailPanel.blur();
    bookSelect.focus();
    if (layoutMode !== "compact") {
      footerText.content = "/ 搜索  ↑↓ 浏览  Enter 详情  Tab 切换  q 退出";
    }
  }

  function focusDetail(): void {
    const book = selectedBook();
    if (!book) return;
    detailFocused = true;
    searchInput.blur();
    bookSelect.blur();
    if (layoutMode === "compact") showCompactScreen("detail");
    detailPanel.focus();
    footerText.content = "↑↓ 滚动  Esc 返回列表  q 退出";
  }

  function applyLayout(width: number): void {
    layoutMode = getLayoutMode(width);
    navigationPanel.visible = layoutMode === "wide";
    body.flexDirection = "row";

    if (layoutMode === "compact") {
      listPanel.width = "100%";
      detailPanel.width = "100%";
      showCompactScreen(compactScreen);
      return;
    }

    listPanel.visible = true;
    detailPanel.visible = true;
    listPanel.width = layoutMode === "medium" ? "45%" : 36;
    detailPanel.flexGrow = 1;
    footerText.content = detailFocused
      ? "↑↓ 滚动  Esc 返回列表  q 退出"
      : "/ 搜索  ↑↓ 浏览  Enter 详情  Tab 切换  q 退出";
  }

  detailPanel.onKeyDown = (key: KeyEvent) => {
    if (key.name === "escape") {
      if (layoutMode === "compact") showCompactScreen("list");
      focusList();
    }
  };

  searchInput.on(InputRenderableEvents.INPUT, filterBooks);
  searchInput.on(InputRenderableEvents.ENTER, focusList);
  bookSelect.on(
    SelectRenderableEvents.SELECTION_CHANGED,
    (_index: number, option: SelectOption) => {
      requestDetails(option.value as Book);
    },
  );
  bookSelect.on(SelectRenderableEvents.ITEM_SELECTED, focusDetail);

  renderer.keyInput.on("keypress", (key: KeyEvent) => {
    if (key.name === "q" && !searchInput.focused) {
      renderer.destroy();
      return;
    }

    if (key.name === "/" && !searchInput.focused) {
      key.preventDefault();
      key.stopPropagation();
      detailFocused = false;
      bookSelect.blur();
      detailPanel.blur();
      searchInput.focus();
      return;
    }

    if (key.name === "tab") {
      if (searchInput.focused || detailFocused) {
        focusList();
      } else {
        bookSelect.blur();
        searchInput.focus();
      }
      return;
    }

    if (key.name === "escape") {
      if (searchInput.focused) {
        focusList();
      } else if (detailFocused) {
        if (layoutMode === "compact") showCompactScreen("list");
        focusList();
      }
    }
  });

  renderer.on("resize", applyLayout);
  renderer.on("destroy", () => {
    destroyed = true;
    detailRequestId += 1;
  });
  applyLayout(renderer.width);
  bookSelect.focus();
  requestDetails(books[0]);

  return {
    applyLayout,
    waitForIdle: () => activeLoad,
  };
}
