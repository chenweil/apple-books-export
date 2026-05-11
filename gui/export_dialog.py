"""
导出进度对话框 - 现代化进度弹窗
"""
import customtkinter as ctk
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
from services.book_service import BookService


class ExportDialog(ctk.CTkToplevel):
    """导出进度对话框"""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.title("导出进度")
        self.geometry("460x220")
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

    def show(self, book, output_dir, main_window):
        """创建导出进度弹窗并启动异步导出"""
        self.cancelled = False
        self.parent_window = main_window

        self.configure(fg_color="#fafafa")
        self.grid_columnconfigure(0, weight=1)

        # 标题
        ctk.CTkLabel(
            self, text="正在导出",
            font=ctk.CTkFont(size=15, weight="bold"),
            anchor="w"
        ).grid(row=0, column=0, sticky="w", padx=(24, 24), pady=(20, 4))

        # 书名
        title_text = book['title']
        if len(title_text) > 40:
            title_text = title_text[:40] + "…"
        ctk.CTkLabel(
            self, text=f"书名：{title_text}",
            font=ctk.CTkFont(size=12),
            text_color="#666666",
            anchor="w"
        ).grid(row=1, column=0, sticky="w", padx=(24, 24), pady=(0, 2))

        # 输出路径
        ctk.CTkLabel(
            self, text=f"输出：{output_dir}",
            font=ctk.CTkFont(size=12),
            text_color="#666666",
            anchor="w"
        ).grid(row=2, column=0, sticky="w", padx=(24, 24), pady=(0, 12))

        # 状态文字
        self.status_label = ctk.CTkLabel(
            self, text="准备中...",
            font=ctk.CTkFont(size=12),
            text_color="#888888",
            anchor="w"
        )
        self.status_label.grid(row=3, column=0, sticky="w", padx=(24, 24), pady=(0, 8))

        # 进度条
        self.progress_bar = ctk.CTkProgressBar(
            self, height=8, corner_radius=4,
            fg_color="#e0e0e0",
            progress_color="#2196F3"
        )
        self.progress_bar.grid(row=4, column=0, sticky="ew", padx=(24, 24), pady=(0, 4))
        self.progress_bar.set(0)

        # 百分比
        self.percent_label = ctk.CTkLabel(
            self, text="",
            font=ctk.CTkFont(size=11),
            text_color="#aaaaaa",
            anchor="e"
        )
        self.percent_label.grid(row=5, column=0, sticky="e", padx=(24, 24), pady=(0, 12))

        # 取消按钮
        ctk.CTkButton(
            self, text="取消",
            width=80, height=32,
            font=ctk.CTkFont(size=12),
            corner_radius=8,
            fg_color="#e0e0e0",
            text_color="#333333",
            hover_color="#d0d0d0",
            command=self._on_cancel
        ).grid(row=6, column=0, sticky="e", padx=(24, 24), pady=(0, 16))

        # 启动异步导出
        BookService().export_async(main_window, book, output_dir)

    def _on_cancel(self):
        self.cancelled = True
        self.destroy()

    def update_progress(self, status, current, total):
        """更新进度显示"""
        try:
            if not self.winfo_exists():
                return
        except Exception:
            return

        if status == 'loading':
            self.status_label.configure(text="正在加载笔记...")
            self.progress_bar.set(0)
            self.percent_label.configure(text="")
        elif status == 'exporting':
            percent = current / total if total > 0 else 0
            self.status_label.configure(text=f"正在导出 ({current}/{total})")
            self.progress_bar.set(percent)
            self.percent_label.configure(text=f"{int(percent * 100)}%")
        elif status == 'done':
            self.status_label.configure(text="导出完成！")
            self.progress_bar.set(1.0)
            self.percent_label.configure(text="100%")

    def close(self):
        """关闭窗口"""
        try:
            if self.winfo_exists():
                self.destroy()
        except Exception:
            pass
