"""
导出进度对话框 - macOS 原生风格
"""
import customtkinter as ctk


class ExportDialog(ctk.CTkToplevel):
    """导出进度对话框"""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.title("导出")
        self.geometry("420x200")
        self.resizable(False, False)

        self.cancelled = False
        self.parent_window = parent

        self.transient(parent)
        self.grab_set()
        self.protocol("WM_DELETE_WINDOW", self._on_cancel)
        self.after(200, self._center_on_parent)

    def _center_on_parent(self):
        if self.master:
            self.update_idletasks()
            x = self.master.winfo_x() + (self.master.winfo_width() - self.winfo_width()) // 2
            y = self.master.winfo_y() + (self.master.winfo_height() - self.winfo_height()) // 2
            self.geometry(f"+{x}+{y}")

    def show(self, book, output_dir):
        self.cancelled = False
        self.configure(fg_color="#ffffff")
        self.grid_columnconfigure(0, weight=1)

        # 标题
        ctk.CTkLabel(
            self, text="正在导出",
            font=ctk.CTkFont(size=14, weight="bold"),
            text_color="#1d1d1f", anchor="w"
        ).grid(row=0, column=0, sticky="w", padx=(20, 20), pady=(20, 4))

        # 书名
        title = book['title'][:40] + "…" if len(book['title']) > 40 else book['title']
        ctk.CTkLabel(
            self, text=title,
            font=ctk.CTkFont(size=12),
            text_color="#86868b", anchor="w"
        ).grid(row=1, column=0, sticky="w", padx=(20, 20), pady=(0, 2))

        # 输出路径
        ctk.CTkLabel(
            self, text=output_dir,
            font=ctk.CTkFont(size=11),
            text_color="#aeaeb2", anchor="w"
        ).grid(row=2, column=0, sticky="w", padx=(20, 20), pady=(0, 16))

        # 状态
        self.status_label = ctk.CTkLabel(
            self, text="准备中…",
            font=ctk.CTkFont(size=12),
            text_color="#86868b", anchor="w"
        )
        self.status_label.grid(row=3, column=0, sticky="w", padx=(20, 20), pady=(0, 8))

        # 进度条
        self.progress_bar = ctk.CTkProgressBar(
            self, height=4, corner_radius=2,
            fg_color="#e5e5ea",
            progress_color="#007aff"
        )
        self.progress_bar.grid(row=4, column=0, sticky="ew", padx=(20, 20), pady=(0, 4))
        self.progress_bar.set(0)

        # 百分比
        self.percent_label = ctk.CTkLabel(
            self, text="",
            font=ctk.CTkFont(size=11),
            text_color="#aeaeb2", anchor="e"
        )
        self.percent_label.grid(row=5, column=0, sticky="e", padx=(20, 20), pady=(0, 12))

        # 取消按钮
        ctk.CTkButton(
            self, text="取消",
            width=72, height=30,
            font=ctk.CTkFont(size=12),
            corner_radius=6,
            fg_color="#e5e5ea",
            text_color="#1d1d1f",
            hover_color="#d1d1d6",
            command=self._on_cancel
        ).grid(row=6, column=0, sticky="e", padx=(20, 20), pady=(0, 16))

    def _on_cancel(self):
        self.cancelled = True
        self.destroy()

    def update_progress(self, status, current, total):
        try:
            if not self.winfo_exists():
                return
        except Exception:
            return

        if status == 'loading':
            self.status_label.configure(text="正在加载笔记…")
            self.progress_bar.set(0)
            self.percent_label.configure(text="")
        elif status == 'exporting':
            pct = current / total if total > 0 else 0
            self.status_label.configure(text=f"正在导出 ({current}/{total})")
            self.progress_bar.set(pct)
            self.percent_label.configure(text=f"{int(pct * 100)}%")
        elif status == 'done':
            self.status_label.configure(text="导出完成")
            self.progress_bar.set(1.0)
            self.percent_label.configure(text="100%")

    def close(self):
        try:
            if self.winfo_exists():
                self.destroy()
        except Exception:
            pass
