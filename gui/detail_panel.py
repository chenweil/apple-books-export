"""
书籍详情面板 - 现代化卡片式详情展示
"""
import customtkinter as ctk


class DetailPanel(ctk.CTkFrame):
    """书籍详情面板"""

    def __init__(self, parent, on_preview_callback=None, on_export_callback=None, **kwargs):
        super().__init__(parent, **kwargs)

        self.on_preview_callback = on_preview_callback
        self.on_export_callback = on_export_callback
        self.current_book = None
        self.current_annotations = None

        self._configure_grid()
        self._build_ui()

    def _configure_grid(self):
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(1, weight=1)

    def _build_ui(self):
        # 空状态占位
        self.empty_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.empty_frame.grid(row=0, column=0, rowspan=10, sticky="nsew")
        self.empty_frame.grid_columnconfigure(0, weight=1)
        self.empty_frame.grid_rowconfigure(0, weight=1)

        ctk.CTkLabel(
            self.empty_frame,
            text="📖",
            font=ctk.CTkFont(size=48),
            text_color="#cccccc"
        ).grid(row=0, column=0)

        ctk.CTkLabel(
            self.empty_frame,
            text="选择一本书查看详情",
            font=ctk.CTkFont(size=14),
            text_color="#aaaaaa"
        ).grid(row=1, column=0, pady=(8, 0))

        # 详情内容（初始隐藏）
        self.detail_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.detail_frame.grid_columnconfigure(0, weight=1)

        # 书名
        self.title_label = ctk.CTkLabel(
            self.detail_frame,
            text="",
            font=ctk.CTkFont(size=18, weight="bold"),
            wraplength=350,
            anchor="w",
            justify="left"
        )
        self.title_label.grid(row=0, column=0, sticky="w", padx=(20, 20), pady=(20, 2))

        # 作者
        self.author_label = ctk.CTkLabel(
            self.detail_frame,
            text="",
            font=ctk.CTkFont(size=13),
            text_color="#888888",
            anchor="w"
        )
        self.author_label.grid(row=1, column=0, sticky="w", padx=(20, 20), pady=(0, 16))

        # 分隔线
        ctk.CTkFrame(
            self.detail_frame, height=1, fg_color="#e8e8e8"
        ).grid(row=2, column=0, sticky="ew", padx=(20, 20), pady=(0, 16))

        # 统计卡片（2个）
        stats_frame = ctk.CTkFrame(self.detail_frame, fg_color="transparent")
        stats_frame.grid(row=3, column=0, sticky="ew", padx=(20, 20), pady=(0, 16))
        stats_frame.grid_columnconfigure((0, 1), weight=1)

        # 笔记数量统计
        highlight_card = ctk.CTkFrame(stats_frame, corner_radius=10, fg_color="#fff8e1")
        highlight_card.grid(row=0, column=0, sticky="nsew", padx=(0, 8))
        highlight_card.grid_columnconfigure(0, weight=1)
        ctk.CTkLabel(
            highlight_card, text="笔记数量",
            font=ctk.CTkFont(size=11), text_color="#f9a825",
            anchor="center"
        ).grid(row=0, column=0, pady=(10, 0))
        self.highlight_count = ctk.CTkLabel(
            highlight_card, text="—",
            font=ctk.CTkFont(size=22, weight="bold"), text_color="#f57f17",
            anchor="center"
        )
        self.highlight_count.grid(row=1, column=0, pady=(0, 10))

        # 阅读进度
        progress_card = ctk.CTkFrame(stats_frame, corner_radius=10, fg_color="#e3f2fd")
        progress_card.grid(row=0, column=1, sticky="nsew", padx=(8, 0))
        progress_card.grid_columnconfigure(0, weight=1)
        ctk.CTkLabel(
            progress_card, text="阅读进度",
            font=ctk.CTkFont(size=11), text_color="#1e88e5",
            anchor="center"
        ).grid(row=0, column=0, pady=(10, 0))
        self.progress_label = ctk.CTkLabel(
            progress_card, text="—",
            font=ctk.CTkFont(size=22, weight="bold"), text_color="#1565c0",
            anchor="center"
        )
        self.progress_label.grid(row=1, column=0, pady=(0, 10))

        # 阅读时间信息
        self.time_info_frame = ctk.CTkFrame(self.detail_frame, fg_color="transparent")
        self.time_info_frame.grid(row=4, column=0, sticky="ew", padx=(20, 20), pady=(0, 12))
        self.time_info_frame.grid_columnconfigure(1, weight=1)

        self.last_open_label = ctk.CTkLabel(
            self.time_info_frame,
            text="",
            font=ctk.CTkFont(size=12),
            text_color="#888888",
            anchor="w"
        )
        self.last_open_label.grid(row=0, column=0, columnspan=2, sticky="w", pady=(0, 4))

        self.added_label = ctk.CTkLabel(
            self.time_info_frame,
            text="",
            font=ctk.CTkFont(size=12),
            text_color="#888888",
            anchor="w"
        )
        self.added_label.grid(row=1, column=0, columnspan=2, sticky="w")

        # 分隔线
        ctk.CTkFrame(
            self.detail_frame, height=1, fg_color="#e8e8e8"
        ).grid(row=6, column=0, sticky="ew", padx=(20, 20), pady=(0, 16))

        # 操作按钮
        btn_frame = ctk.CTkFrame(self.detail_frame, fg_color="transparent")
        btn_frame.grid(row=7, column=0, sticky="ew", padx=(20, 20), pady=(0, 20))
        btn_frame.grid_columnconfigure((0, 1), weight=1)

        self.preview_btn = ctk.CTkButton(
            btn_frame,
            text="预览笔记",
            height=40,
            font=ctk.CTkFont(size=14, weight="bold"),
            corner_radius=10,
            fg_color="#2196F3",
            hover_color="#1976D2",
            command=self._on_preview,
            state="disabled"
        )
        self.preview_btn.grid(row=0, column=0, padx=(0, 8), sticky="ew")

        self.export_btn = ctk.CTkButton(
            btn_frame,
            text="导出 Markdown",
            height=40,
            font=ctk.CTkFont(size=14, weight="bold"),
            corner_radius=10,
            fg_color="#4CAF50",
            hover_color="#388E3C",
            command=self._on_export,
            state="disabled"
        )
        self.export_btn.grid(row=0, column=1, padx=(8, 0), sticky="ew")

    def _on_preview(self):
        if self.on_preview_callback and self.current_book:
            self.on_preview_callback(self.current_book, self.current_annotations)

    def _on_export(self):
        if self.on_export_callback and self.current_book:
            self.on_export_callback(self.current_book)

    def set_book(self, book, annotations=None):
        """设置当前书籍"""
        self.current_book = book
        self.current_annotations = annotations

    def update_display(self, stats=None):
        """更新显示"""
        if self.current_book:
            book = self.current_book
            # 隐藏空状态，显示详情
            self.empty_frame.grid_forget()
            self.detail_frame.grid(row=0, column=0, rowspan=10, sticky="nsew")

            self.title_label.configure(text=book['title'])
            author = book['author']
            if author and author != '未知作者':
                self.author_label.configure(text=f"作者：{author}")
            else:
                self.author_label.configure(text="")

            # 更新阅读进度
            is_finished = book.get('is_finished')
            if is_finished == 1:
                self.progress_label.configure(text="已读完")
            else:
                progress = book.get('reading_progress')
                if progress is not None:
                    progress_pct = int(progress * 100)
                    self.progress_label.configure(text=f"{progress_pct}%")
                else:
                    self.progress_label.configure(text="—")

            # 更新时间信息
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

            if stats:
                self.highlight_count.configure(text=str(stats.get('highlights', 0)))

                # 只有数据真正加载完成才启用按钮
                stats_loaded = all(isinstance(v, int) for v in stats.values())
                if stats_loaded:
                    self.preview_btn.configure(state="normal")
                    self.export_btn.configure(state="normal")
                else:
                    self.preview_btn.configure(state="disabled")
                    self.export_btn.configure(state="disabled")
            else:
                self.highlight_count.configure(text="—")
                self.preview_btn.configure(state="disabled")
                self.export_btn.configure(state="disabled")
        else:
            # 显示空状态
            self.detail_frame.grid_forget()
            self.empty_frame.grid(row=0, column=0, rowspan=10, sticky="nsew")
