"""
预览窗口 - macOS 原生风格
"""
import customtkinter as ctk
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
from books_exporter import apple_timestamp_to_datetime, parse_cfi_chapter, format_chapter_display


class PreviewWindow(ctk.CTkToplevel):
    """笔记预览窗口"""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.title("笔记预览")
        self.geometry("700x600")
        self.minsize(500, 400)

        self.transient(parent)
        self.grab_set()
        self.after(200, self._center_on_parent)

    def _center_on_parent(self):
        if self.master:
            self.update_idletasks()
            x = self.master.winfo_x() + (self.master.winfo_width() - self.winfo_width()) // 2
            y = self.master.winfo_y() + (self.master.winfo_height() - self.winfo_height()) // 2
            self.geometry(f"+{x}+{y}")

    def show(self, book, annotations):
        if not annotations:
            return

        self.title(f"笔记预览 — {book['title'][:30]}")
        self.configure(fg_color="#ffffff")

        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(2, weight=1)

        # 头部 - 书名
        ctk.CTkLabel(
            self, text=book['title'],
            font=ctk.CTkFont(size=15, weight="bold"),
            text_color="#1d1d1f",
            wraplength=640, anchor="nw", justify="left"
        ).grid(row=0, column=0, sticky="nw", padx=(20, 20), pady=(16, 2))

        # 头部 - 作者 + 笔记数
        author = book['author']
        sub = f"{author}  ·  {len(annotations)} 条笔记" if author and author != '未知作者' else f"{len(annotations)} 条笔记"
        ctk.CTkLabel(
            self, text=sub,
            font=ctk.CTkFont(size=12),
            text_color="#86868b",
            anchor="w"
        ).grid(row=1, column=0, sticky="w", padx=(20, 20), pady=(0, 8))

        # 分隔线
        ctk.CTkFrame(self, height=1, fg_color="#e5e5ea").grid(
            row=1, column=0, sticky="ew", padx=(20, 20), pady=(28, 0)
        )

        # 内容
        content = self._build_preview(book, annotations)

        text_box = ctk.CTkTextbox(
            self,
            font=ctk.CTkFont(size=13, family="Menlo"),
            fg_color="#ffffff",
            text_color="#1d1d1f",
            corner_radius=0,
            border_width=0,
            wrap="word",
            activate_scrollbars=True
        )
        text_box.grid(row=2, column=0, sticky="nsew", padx=(24, 24), pady=(12, 8))
        text_box.insert("1.0", content)
        text_box.configure(state="disabled")

        # 底部
        bottom = ctk.CTkFrame(self, fg_color="transparent", height=40)
        bottom.grid(row=3, column=0, sticky="ew", padx=(20, 20), pady=(0, 12))

        ctk.CTkButton(
            bottom, text="关闭",
            width=80, height=32,
            font=ctk.CTkFont(size=13),
            corner_radius=6,
            fg_color="#e5e5ea",
            text_color="#1d1d1f",
            hover_color="#d1d1d6",
            command=self.destroy
        ).pack(side="right")

        self.protocol("WM_DELETE_WINDOW", self.destroy)

    def _build_preview(self, book, annotations):
        lines = []
        lines.append(f"# {book['title']}")
        lines.append(f"作者: {book['author']}")
        lines.append("─" * 50)
        lines.append("")

        highlights = [
            ann for ann in annotations
            if ann.get('selected_text') or ann.get('note')
        ]

        if highlights:
            lines.append(f"## 高亮与笔记 ({len(highlights)} 条)")
            lines.append("")
            for i, ann in enumerate(highlights[:50], 1):
                chapter = parse_cfi_chapter(ann.get('location')) if ann.get('location') else None
                chapter_display = format_chapter_display(chapter, i)
                lines.append(f"### {i}. {chapter_display}")
                lines.append("")

                if ann.get('selected_text'):
                    text = ann['selected_text'][:200]
                    if len(ann['selected_text']) > 200:
                        text += "..."
                    lines.append(f'  "{text}"')

                if ann.get('note'):
                    lines.append(f"  笔记: {ann['note'][:100]}")

                if ann.get('created_date'):
                    try:
                        date = apple_timestamp_to_datetime(ann['created_date'])
                        local = date.astimezone()
                        lines.append(f"  {local.strftime('%Y-%m-%d %H:%M')}")
                    except Exception:
                        pass

                lines.append("")

            if len(highlights) > 50:
                lines.append(f"... 还有 {len(highlights) - 50} 条未显示")
            lines.append("")

        return '\n'.join(lines)
