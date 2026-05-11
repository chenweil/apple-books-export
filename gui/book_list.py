"""
书籍列表组件 - macOS 原生风格，全部展示 + 搜索
"""
import customtkinter as ctk


class BookListPanel(ctk.CTkFrame):
    """书籍列表面板"""

    def __init__(self, parent, on_select_callback=None, **kwargs):
        super().__init__(parent, corner_radius=0, fg_color="transparent", **kwargs)

        self.on_select_callback = on_select_callback
        self.books = []
        self.selected_index = None
        self.item_frames = []

        self._build_ui()

    def _build_ui(self):
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(1, weight=1)

        # 搜索栏
        search_bar = ctk.CTkFrame(self, fg_color="transparent", height=40)
        search_bar.grid(row=0, column=0, sticky="ew", padx=(12, 12), pady=(10, 6))
        search_bar.grid_columnconfigure(0, weight=1)
        search_bar.grid_propagate(False)

        self.search_var = ctk.StringVar()
        self.search_var.trace_add("write", self._on_search)
        self.search_entry = ctk.CTkEntry(
            search_bar,
            textvariable=self.search_var,
            placeholder_text="🔍  搜索书名或作者…",
            height=32,
            font=ctk.CTkFont(size=13),
            corner_radius=8,
            border_width=1,
            border_color="#d2d2d7",
            fg_color="#ffffff",
            text_color="#1d1d1f",
            placeholder_text_color="#aeaeb2"
        )
        self.search_entry.grid(row=0, column=0, sticky="ew")

        # 可滚动列表
        self.scroll_frame = ctk.CTkScrollableFrame(
            self,
            fg_color="transparent",
            scrollbar_button_color="#c7c7cc",
            scrollbar_button_hover_color="#aeaeb2"
        )
        self.scroll_frame.grid(row=1, column=0, sticky="nsew", padx=(4, 4), pady=(0, 0))
        self.scroll_frame.grid_columnconfigure(0, weight=1)

        # 空状态
        self.empty_label = ctk.CTkLabel(
            self.scroll_frame,
            text="暂无书籍",
            font=ctk.CTkFont(size=13),
            text_color="#aeaeb2"
        )
        self.empty_label.grid(row=0, column=0, pady=60)

    def _on_search(self, *args):
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
        for frame in self.item_frames:
            frame.destroy()
        self.item_frames = []
        self.empty_label.grid_forget()

        filtered = self._get_filtered_books()
        if not filtered:
            self.empty_label.grid(row=0, column=0, pady=60)
            return

        for i, book in enumerate(filtered):
            is_selected = (i == self.selected_index)

            row = ctk.CTkFrame(
                self.scroll_frame,
                corner_radius=6,
                height=40,
                fg_color="#007aff" if is_selected else "transparent"
            )
            row.grid(row=i, column=0, sticky="ew", padx=(8, 8), pady=(2, 2))
            row.grid_columnconfigure(1, weight=1)
            row.grid_propagate(False)

            # 点击 & 悬停
            row.bind("<Button-1>", lambda e, idx=i: self._on_click(idx))
            row.bind("<Enter>", lambda e, f=row, idx=i: self._on_hover(f, idx, True))
            row.bind("<Leave>", lambda e, f=row, idx=i: self._on_hover(f, idx, False))

            # 书名
            title_text = book['title']
            if len(title_text) > 50:
                title_text = title_text[:50] + "…"

            title_color = "#ffffff" if is_selected else "#1d1d1f"
            title_lbl = ctk.CTkLabel(
                row, text=title_text,
                font=ctk.CTkFont(size=13),
                text_color=title_color,
                anchor="w"
            )
            title_lbl.grid(row=0, column=1, sticky="w", padx=(10, 8), pady=(0, 0))
            title_lbl.bind("<Button-1>", lambda e, idx=i: self._on_click(idx))

            # 笔记数量
            count_color = "#ffffff" if is_selected else "#86868b"
            count_lbl = ctk.CTkLabel(
                row, text=f"{book['note_count']} 条笔记",
                font=ctk.CTkFont(size=11),
                text_color=count_color,
                anchor="e"
            )
            count_lbl.grid(row=0, column=2, padx=(0, 10))
            count_lbl.bind("<Button-1>", lambda e, idx=i: self._on_click(idx))

            self.item_frames.append((row, i))

    def _on_hover(self, frame, index, entering):
        if index == self.selected_index:
            return
        if entering:
            frame.configure(fg_color="#f0f0f5")
        else:
            frame.configure(fg_color="transparent")

    def _on_click(self, index):
        self.selected_index = index
        for row, idx in self.item_frames:
            if idx == index:
                row.configure(fg_color="#007aff")
                for widget in row.winfo_children():
                    if isinstance(widget, ctk.CTkLabel):
                        widget.configure(text_color="#ffffff")
            else:
                row.configure(fg_color="transparent")
                for widget in row.winfo_children():
                    if isinstance(widget, ctk.CTkLabel):
                        if widget.cget("text").endswith("条笔记"):
                            widget.configure(text_color="#86868b")
                        else:
                            widget.configure(text_color="#1d1d1f")

        filtered = self._get_filtered_books()
        if 0 <= index < len(filtered) and self.on_select_callback:
            self.on_select_callback(filtered[index], index)

    def update_books(self, books):
        self.books = books
        self.selected_index = None
        self._refresh_list()

    def refresh(self):
        self._refresh_list()
