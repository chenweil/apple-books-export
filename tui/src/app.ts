import {
  BoxRenderable,
  InputRenderable,
  InputRenderableEvents,
  SelectRenderable,
  SelectRenderableEvents,
  TextAttributes,
  TextRenderable,
  type CliRenderer,
  type KeyEvent,
  type SelectOption,
} from "@opentui/core";
import type { Book } from "./backend";
import { getLayoutMode, type LayoutMode } from "./layout";

type CompactScreen = "list" | "detail";

export interface BookBrowser {
  applyLayout(width: number): void;
}

function optionsFor(books: Book[]): SelectOption[] {
  return books.map((book) => ({
    name: book.title,
    description: `${book.author} · ${book.note_count} 条笔记`,
    value: book,
  }));
}

function bookDetail(book: Book | undefined): string {
  if (!book) {
    return "没有匹配的书籍。\n\n按 / 修改搜索条件。";
  }

  return [
    book.title,
    "",
    `作者：${book.author || "未知"}`,
    `笔记：${book.note_count} 条`,
    "",
    "当前为只读原型。",
    "导出与 AI 写操作尚未接入。",
  ].join("\n");
}

export function createBookBrowser(
  renderer: CliRenderer,
  books: Book[],
): BookBrowser {
  let filteredBooks = books;
  let layoutMode: LayoutMode = getLayoutMode(renderer.width);
  let compactScreen: CompactScreen = "list";

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
        "Enter  详情",
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

  const detailPanel = new BoxRenderable(renderer, {
    id: "book-detail-panel",
    height: "100%",
    flexGrow: 1,
    flexDirection: "column",
    focusable: true,
    border: true,
    borderStyle: "rounded",
    borderColor: "#3a4258",
    title: "书籍详情",
    padding: 1,
  });
  const detailText = new TextRenderable(renderer, {
    id: "book-detail",
    content: bookDetail(books[0]),
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

  function updateDetail(book = selectedBook()): void {
    detailText.content = bookDetail(book);
  }

  function showCompactScreen(screen: CompactScreen): void {
    compactScreen = screen;
    if (layoutMode !== "compact") return;

    listPanel.visible = screen === "list";
    detailPanel.visible = screen === "detail";
    footerText.content =
      screen === "list"
        ? "/ 搜索  ↑↓ 浏览  Enter 详情  q 退出"
        : "Esc 返回列表  q 退出";
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
    updateDetail(filteredBooks[0]);
  }

  function focusList(): void {
    searchInput.blur();
    bookSelect.focus();
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
    footerText.content = "/ 搜索  ↑↓ 浏览  Enter 详情  Tab 切换  q 退出";
  }

  detailPanel.onKeyDown = (key: KeyEvent) => {
    if (key.name === "escape" && layoutMode === "compact") {
      showCompactScreen("list");
      bookSelect.focus();
    }
  };

  searchInput.on(InputRenderableEvents.INPUT, filterBooks);
  searchInput.on(InputRenderableEvents.ENTER, focusList);
  bookSelect.on(
    SelectRenderableEvents.SELECTION_CHANGED,
    (_index: number, option: SelectOption) => {
      updateDetail(option.value as Book);
    },
  );
  bookSelect.on(SelectRenderableEvents.ITEM_SELECTED, () => {
    updateDetail();
    if (layoutMode === "compact") {
      bookSelect.blur();
      showCompactScreen("detail");
      detailPanel.focus();
    }
  });

  renderer.keyInput.on("keypress", (key: KeyEvent) => {
    if (key.name === "q" && !searchInput.focused) {
      renderer.destroy();
      return;
    }

    if (key.name === "/" && !searchInput.focused) {
      key.preventDefault();
      key.stopPropagation();
      bookSelect.blur();
      searchInput.focus();
      return;
    }

    if (key.name === "tab") {
      if (searchInput.focused) {
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
      } else if (layoutMode === "compact" && compactScreen === "detail") {
        showCompactScreen("list");
        bookSelect.focus();
      }
    }
  });

  renderer.on("resize", applyLayout);
  applyLayout(renderer.width);
  bookSelect.focus();

  return { applyLayout };
}
