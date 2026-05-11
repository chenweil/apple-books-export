"""
书籍详情面板 - macOS 原生风格
"""
import customtkinter as ctk


class DetailPanel(ctk.CTkFrame):
    """书籍详情面板"""

    def __init__(self, parent, on_preview_callback=None, on_export_callback=None, **kwargs):
        super().__init__(parent, corner_radius=0, fg_color="transparent", **kwargs)

        self.on_preview_callback = on_preview_callback
        self.on_export_callback = on_export_callback
        self.current_book = None
        self.current_annotations = None

        self._build_ui()

    def _build_ui(self):
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(0, weight=1)

        # 空状态
        self.empty_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.empty_frame.grid(row=0, column=0, sticky="nsew")
        self.empty_frame.grid_columnconfigure(0, weight=1)
        self.empty_frame.grid_rowconfigure(0, weight=1)

        ctk.CTkLabel(
            self.empty_frame, text="📖",
            font=ctk.CTkFont(size=40), text_color="#c7c7cc"
        ).grid(row=0, column=0)

        ctk.CTkLabel(
            self.empty_frame, text="选择一本书查看详情",
            font=ctk.CTkFont(size=13), text_color="#aeaeb2"
        ).grid(row=1, column=0, pady=(8, 0))

        # 详情内容（初始隐藏）
        self.detail_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.detail_frame.grid_columnconfigure(0, weight=1)

        # 书名
        self.title_label = ctk.CTkLabel(
            self.detail_frame, text="",
            font=ctk.CTkFont(size=17, weight="bold"),
            text_color="#1d1d1f",
            wraplength=320, anchor="nw", justify="left"
        )
        self.title_label.grid(row=0, column=0, sticky="nw", padx=(20, 20), pady=(24, 2))

        # 作者
        self.author_label = ctk.CTkLabel(
            self.detail_frame, text="",
            font=ctk.CTkFont(size=13),
            text_color="#86868b",
            anchor="w"
        )
        self.author_label.grid(row=1, column=0, sticky="w", padx=(20, 20), pady=(0, 20))

        # 分隔线
        ctk.CTkFrame(
            self.detail_frame, height=1, fg_color="#e5e5ea"
        ).grid(row=2, column=0, sticky="ew", padx=(20, 20), pady=(0, 20))

        # 统计信息 - 简洁的两列布局
        stats_frame = ctk.CTkFrame(self.detail_frame, fg_color="transparent")
        stats_frame.grid(row=3, column=0, sticky="ew", padx=(20, 20), pady=(0, 20))
        stats_frame.grid_columnconfigure((0, 1), weight=1)

        # 笔记数量
        note_card = ctk.CTkFrame(stats_frame, corner_radius=10, fg_color="#f5f5f7")
        note_card.grid(row=0, column=0, sticky="nsew", padx=(0, 6))
        note_card.grid_columnconfigure(0, weight=1)
        ctk.CTkLabel(
            note_card, text="笔记数量",
            font=ctk.CTkFont(size=11), text_color="#86868b", anchor="center"
        ).grid(row=0, column=0, pady=(12, 0))
        self.note_count_label = ctk.CTkLabel(
            note_card, text="—",
            font=ctk.CTkFont(size=24, weight="bold"), text_color="#1d1d1f", anchor="center"
        )
        self.note_count_label.grid(row=1, column=0, pady=(2, 12))

        # 阅读进度
        progress_card = ctk.CTkFrame(stats_frame, corner_radius=10, fg_color="#f5f5f7")
        progress_card.grid(row=0, column=1, sticky="nsew", padx=(6, 0))
        progress_card.grid_columnconfigure(0, weight=1)
        ctk.CTkLabel(
            progress_card, text="阅读进度",
            font=ctk.CTkFont(size=11), text_color="#86868b", anchor="center"
        ).grid(row=0, column=0, pady=(12, 0))
        self.progress_label = ctk.CTkLabel(
            progress_card, text="—",
            font=ctk.CTkFont(size=24, weight="bold"), text_color="#1d1d1f", anchor="center"
        )
        self.progress_label.grid(row=1, column=0, pady=(2, 12))

        # 时间信息
        self.time_info_frame = ctk.CTkFrame(self.detail_frame, fg_color="transparent")
        self.time_info_frame.grid(row=4, column=0, sticky="ew", padx=(20, 20), pady=(0, 20))
        self.time_info_frame.grid_columnconfigure(0, weight=1)

        self.last_open_label = ctk.CTkLabel(
            self.time_info_frame, text="",
            font=ctk.CTkFont(size=12), text_color="#86868b", anchor="w"
        )
        self.last_open_label.grid(row=0, column=0, sticky="w", pady=(0, 4))

        self.added_label = ctk.CTkLabel(
            self.time_info_frame, text="",
            font=ctk.CTkFont(size=12), text_color="#86868b", anchor="w"
        )
        self.added_label.grid(row=1, column=0, sticky="w")

        # 分隔线
        ctk.CTkFrame(
            self.detail_frame, height=1, fg_color="#e5e5ea"
        ).grid(row=5, column=0, sticky="ew", padx=(20, 20), pady=(0, 20))

        # 操作按钮 - macOS 风格蓝色主按钮
        btn_frame = ctk.CTkFrame(self.detail_frame, fg_color="transparent")
        btn_frame.grid(row=6, column=0, sticky="ew", padx=(20, 20), pady=(0, 24))
        btn_frame.grid_columnconfigure((0, 1), weight=1)

        self.preview_btn = ctk.CTkButton(
            btn_frame, text="预览笔记",
            height=36,
            font=ctk.CTkFont(size=13, weight="bold"),
            corner_radius=8,
            fg_color="#007aff",
            hover_color="#0056b3",
            text_color="#ffffff",
            command=self._on_preview,
            state="disabled"
        )
        self.preview_btn.grid(row=0, column=0, padx=(0, 6), sticky="ew")

        self.export_btn = ctk.CTkButton(
            btn_frame, text="导出 Markdown",
            height=36,
            font=ctk.CTkFont(size=13, weight="bold"),
            corner_radius=8,
            fg_color="#007aff",
            hover_color="#0056b3",
            text_color="#ffffff",
            command=self._on_export,
            state="disabled"
        )
        self.export_btn.grid(row=0, column=1, padx=(6, 0), sticky="ew")

    def _on_preview(self):
        if self.on_preview_callback and self.current_book:
            self.on_preview_callback(self.current_book, self.current_annotations)

    def _on_export(self):
        if self.on_export_callback and self.current_book:
            self.on_export_callback(self.current_book)

    def set_book(self, book, annotations=None):
        self.current_book = book
        self.current_annotations = annotations

    def update_display(self, stats=None):
        if self.current_book:
            book = self.current_book
            self.empty_frame.grid_forget()
            self.detail_frame.grid(row=0, column=0, sticky="nsew")

            self.title_label.configure(text=book['title'])
            author = book['author']
            self.author_label.configure(
                text=f"作者：{author}" if author and author != '未知作者' else ""
            )

            # 笔记数量
            if stats and isinstance(stats.get('highlights'), int):
                self.note_count_label.configure(text=str(stats['highlights']))
            else:
                self.note_count_label.configure(text="…")

            # 阅读进度
            is_finished = book.get('is_finished')
            if is_finished == 1:
                self.progress_label.configure(text="已读完")
            else:
                progress = book.get('reading_progress')
                if progress is not None:
                    self.progress_label.configure(text=f"{int(progress * 100)}%")
                else:
                    self.progress_label.configure(text="—")

            # 时间信息
            from books_exporter import apple_timestamp_to_datetime

            last_open = book.get('last_open_date')
            if last_open:
                try:
                    dt = apple_timestamp_to_datetime(last_open)
                    local = dt.astimezone()
                    self.last_open_label.configure(text=f"最后打开：{local.strftime('%Y-%m-%d %H:%M')}")
                except Exception:
                    self.last_open_label.configure(text="")
            else:
                self.last_open_label.configure(text="")

            added = book.get('creation_date')
            if added:
                try:
                    dt = apple_timestamp_to_datetime(added)
                    local = dt.astimezone()
                    self.added_label.configure(text=f"添加时间：{local.strftime('%Y-%m-%d')}")
                except Exception:
                    self.added_label.configure(text="")
            else:
                self.added_label.configure(text="")

            # 按钮状态
            if stats and all(isinstance(v, int) for v in stats.values()):
                self.preview_btn.configure(state="normal")
                self.export_btn.configure(state="normal")
            else:
                self.preview_btn.configure(state="disabled")
                self.export_btn.configure(state="disabled")
        else:
            self.detail_frame.grid_forget()
            self.empty_frame.grid(row=0, column=0, sticky="nsew")
