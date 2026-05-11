"""
书籍列表组件 - 现代化简洁列表，一行显示书名 + 笔记数量
"""
import customtkinter as ctk


class BookListPanel(ctk.CTkFrame):
    """书籍列表面板，简洁的一行式列表"""

    PAGE_SIZE = 50

    def __init__(self, parent, on_select_callback=None, **kwargs):
        super().__init__(parent, **kwargs)

        self.on_select_callback = on_select_callback
        self.books = []
        self.current_page = 1
        self.total_pages = 1
        self.selected_index = None
        self.item_frames = []

        self._configure_grid()
        self._build_ui()

    def _configure_grid(self):
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(1, weight=1)

    def _build_ui(self):
        # 标题
        header = ctk.CTkFrame(self, fg_color="transparent")
        header.grid(row=0, column=0, sticky="ew", padx=(16, 16), pady=(12, 4))
        header.grid_columnconfigure(0, weight=1)

        ctk.CTkLabel(
            header, text="📚 我的书籍",
            font=ctk.CTkFont(size=16, weight="bold"),
            anchor="w"
        ).grid(row=0, column=0, sticky="w")

        self.count_label = ctk.CTkLabel(
            header, text="",
            font=ctk.CTkFont(size=12),
            text_color="#888888",
            anchor="e"
        )
        self.count_label.grid(row=0, column=1, sticky="e")

        # 搜索框
        self.search_var = ctk.StringVar()
        self.search_var.trace_add("write", self._on_search)
        self.search_entry = ctk.CTkEntry(
            self,
            textvariable=self.search_var,
            placeholder_text="搜索书名或作者...",
            height=36,
            font=ctk.CTkFont(size=13),
            corner_radius=8,
            border_width=0
        )
        self.search_entry.grid(row=0, column=1, sticky="ew", padx=(16, 16), pady=(12, 4))

        # 可滚动列表区域
        self.scroll_frame = ctk.CTkScrollableFrame(
            self,
            fg_color="transparent",
            scrollbar_button_color="#d0d0d0",
            scrollbar_button_hover_color="#b0b0b0"
        )
        self.scroll_frame.grid(row=1, column=0, columnspan=2, sticky="nsew", padx=(8, 8), pady=(4, 4))
        self.scroll_frame.grid_columnconfigure(0, weight=1)

        # 分页
        self.page_frame = ctk.CTkFrame(self, fg_color="transparent", height=36)
        self.page_frame.grid(row=2, column=0, columnspan=2, sticky="ew", padx=(16, 16), pady=(4, 8))
        self.page_frame.grid_columnconfigure(0, weight=1)
        self.page_frame.grid_columnconfigure(1, weight=0)
        self.page_frame.grid_columnconfigure(2, weight=0)
        self.page_frame.grid_columnconfigure(3, weight=0)
        self.page_frame.grid_columnconfigure(4, weight=1)

        self.prev_btn = ctk.CTkButton(
            self.page_frame, text="上一页", width=70, height=30,
            font=ctk.CTkFont(size=12),
            corner_radius=6,
            fg_color="#e0e0e0",
            text_color="#333333",
            hover_color="#d0d0d0",
            command=self._prev_page
        )
        self.prev_btn.grid(row=0, column=1, padx=(0, 8))

        # 页码输入框
        self.page_entry = ctk.CTkEntry(
            self.page_frame,
            width=50,
            height=30,
            font=ctk.CTkFont(size=12),
            justify="center"
        )
        self.page_entry.grid(row=0, column=2)
        self.page_entry.bind("<Return>", self._on_page_jump)

        self.page_label = ctk.CTkLabel(
            self.page_frame, text="/ 1",
            font=ctk.CTkFont(size=12),
            text_color="#888888"
        )
        self.page_label.grid(row=0, column=3, padx=(4, 8))

        self.next_btn = ctk.CTkButton(
            self.page_frame, text="下一页", width=70, height=30,
            font=ctk.CTkFont(size=12),
            corner_radius=6,
            fg_color="#e0e0e0",
            text_color="#333333",
            hover_color="#d0d0d0",
            command=self._next_page
        )
        self.next_btn.grid(row=0, column=4)

        # 空状态提示
        self.empty_label = ctk.CTkLabel(
            self.scroll_frame,
            text="暂无书籍数据\n请确保 Apple Books 中有笔记",
            font=ctk.CTkFont(size=13),
            text_color="#aaaaaa",
            justify="center"
        )
        self.empty_label.grid(row=0, column=0, pady=40)

    def _on_search(self, *args):
        self.current_page = 1
        self._refresh_list()

    def _get_filtered_books(self):
        query = self.search_var.get().strip().lower()
        if not query:
            return self.books
        return [
            b for b in self.books
            if query in b['title'].lower() or query in b['author'].lower()
        ]

    def _refresh_list(self):
        # 清除旧列表项
        for frame in self.item_frames:
            frame.destroy()
        self.item_frames = []
        self.empty_label.grid_forget()

        filtered = self._get_filtered_books()
        if not filtered:
            self.empty_label.grid(row=0, column=0, pady=40)
            self.page_entry.configure(state="disabled")
            self.page_label.configure(text="/ 1")
            self.prev_btn.configure(state="disabled")
            self.next_btn.configure(state="disabled")
            return

        self.total_pages = max(1, (len(filtered) + self.PAGE_SIZE - 1) // self.PAGE_SIZE)
        if self.current_page > self.total_pages:
            self.current_page = self.total_pages

        start = (self.current_page - 1) * self.PAGE_SIZE
        end = start + self.PAGE_SIZE
        page_books = filtered[start:end]

        for i, book in enumerate(page_books):
            global_index = start + i
            row_frame = ctk.CTkFrame(
                self.scroll_frame,
                corner_radius=8,
                fg_color="transparent",
                height=48
            )
            row_frame.grid(row=i, column=0, sticky="ew", padx=(8, 8), pady=(3, 3))
            row_frame.grid_columnconfigure(1, weight=1)
            row_frame.grid_rowconfigure(0, weight=1)

            # 绑定点击事件
            for widget in [row_frame]:
                widget.bind("<Button-1>", lambda e, idx=global_index: self._on_click(idx))
                widget.bind("<Enter>", lambda e, f=row_frame: self._on_hover(f, True))
                widget.bind("<Leave>", lambda e, f=row_frame: self._on_hover(f, False))

            # 序号
            num_label = ctk.CTkLabel(
                row_frame,
                text=f"{global_index + 1}",
                font=ctk.CTkFont(size=11),
                text_color="#aaaaaa",
                width=28,
                anchor="e"
            )
            num_label.grid(row=0, column=0, padx=(4, 8))
            num_label.bind("<Button-1>", lambda e, idx=global_index: self._on_click(idx))

            # 书名 + 作者
            title_text = book['title']
            if len(title_text) > 40:
                title_text = title_text[:40] + "…"
            author_text = book['author']
            if author_text and author_text != '未知作者':
                display = f"{title_text}  ·  {author_text}"
            else:
                display = title_text

            title_label = ctk.CTkLabel(
                row_frame,
                text=display,
                font=ctk.CTkFont(size=13),
                anchor="w"
            )
            title_label.grid(row=0, column=1, sticky="w", padx=(0, 8))
            title_label.bind("<Button-1>", lambda e, idx=global_index: self._on_click(idx))
            title_label.bind("<Enter>", lambda e, f=row_frame: self._on_hover(f, True))
            title_label.bind("<Leave>", lambda e, f=row_frame: self._on_hover(f, False))

            # 笔记数量 badge
            note_count = book['note_count']
            badge = ctk.CTkLabel(
                row_frame,
                text=f" {note_count} 条笔记 ",
                font=ctk.CTkFont(size=11),
                corner_radius=10,
                fg_color="#e8f4fd",
                text_color="#2196F3"
            )
            badge.grid(row=0, column=2, padx=(0, 8))
            badge.bind("<Button-1>", lambda e, idx=global_index: self._on_click(idx))

            self.item_frames.append(row_frame)

        # 更新分页
        if self.total_pages > 1:
            self.page_entry.configure(state="normal")
            self.page_entry.delete(0, "end")
            self.page_entry.insert(0, str(self.current_page))
            self.page_label.configure(text=f"/ {self.total_pages}")
        else:
            self.page_entry.configure(state="disabled")
            self.page_label.configure(text="/ 1")
        self.prev_btn.configure(state="normal" if self.current_page > 1 else "disabled")
        self.next_btn.configure(state="normal" if self.current_page < self.total_pages else "disabled")

    def _on_hover(self, frame, entering):
        if entering:
            frame.configure(fg_color="#f0f0f0")
        else:
            frame.configure(fg_color="transparent")

    def _on_click(self, index):
        filtered = self._get_filtered_books()
        if 0 <= index < len(filtered):
            self.selected_index = index
            # 高亮选中项
            for i, frame in enumerate(self.item_frames):
                start = (self.current_page - 1) * self.PAGE_SIZE
                if start + i == index:
                    frame.configure(fg_color="#e3f2fd")
                else:
                    frame.configure(fg_color="transparent")
            if self.on_select_callback:
                self.on_select_callback(filtered[index], index)

    def _on_page_jump(self, event):
        """页码跳转"""
        try:
            page = int(self.page_entry.get())
            if 1 <= page <= self.total_pages:
                self.current_page = page
                self.selected_index = None
                self._refresh_list()
            else:
                # 恢复当前页码
                self.page_entry.delete(0, "end")
                self.page_entry.insert(0, str(self.current_page))
        except ValueError:
            # 输入无效，恢复当前页码
            self.page_entry.delete(0, "end")
            self.page_entry.insert(0, str(self.current_page))

    def _prev_page(self):
        if self.current_page > 1:
            self.current_page -= 1
            self.selected_index = None
            self._refresh_list()

    def _next_page(self):
        if self.current_page < self.total_pages:
            self.current_page += 1
            self.selected_index = None
            self._refresh_list()

    def update_books(self, books):
        """更新书籍列表"""
        self.books = books
        self.current_page = 1
        self.selected_index = None
        total_notes = sum(b['note_count'] for b in books)
        self.count_label.configure(text=f"共 {len(books)} 本 · {total_notes} 条笔记")
        self._refresh_list()

    def refresh(self):
        """刷新显示"""
        self._refresh_list()
