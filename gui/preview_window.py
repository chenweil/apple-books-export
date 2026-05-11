"""
预览窗口 - 现代化笔记预览弹窗
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
        self.geometry("720x620")
        self.minsize(500, 400)

        # 使窗口居中
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
        """显示预览窗口"""
        if not annotations:
            return

        self.title(f"笔记预览 - {book['title'][:30]}")
        self.configure(fg_color="#fafafa")

        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(1, weight=1)

        # 头部
        header = ctk.CTkFrame(self, fg_color="transparent", height=60)
        header.grid(row=0, column=0, sticky="ew", padx=(20, 20), pady=(16, 8))
        header.grid_columnconfigure(0, weight=1)

        ctk.CTkLabel(
            header, text=book['title'],
            font=ctk.CTkFont(size=16, weight="bold"),
            wraplength=600, anchor="w", justify="left"
        ).grid(row=0, column=0, sticky="w")

        author = book['author']
        if author and author != '未知作者':
            sub_text = f"{author}  ·  共 {len(annotations)} 条笔记"
        else:
            sub_text = f"共 {len(annotations)} 条笔记"

        ctk.CTkLabel(
            header, text=sub_text,
            font=ctk.CTkFont(size=12),
            text_color="#888888",
            anchor="w"
        ).grid(row=1, column=0, sticky="w", pady=(2, 0))

        # 内容区域
        content = self._build_preview(book, annotations)

        text_frame = ctk.CTkFrame(self, corner_radius=10, fg_color="white")
        text_frame.grid(row=1, column=0, sticky="nsew", padx=(20, 20), pady=(0, 8))
        text_frame.grid_columnconfigure(0, weight=1)
        text_frame.grid_rowconfigure(0, weight=1)

        text_box = ctk.CTkTextbox(
            text_frame,
            font=ctk.CTkFont(size=13, family="Courier"),
            fg_color="white",
            text_color="#333333",
            corner_radius=10,
            border_width=0,
            wrap="word",
            activate_scrollbars=True
        )
        text_box.grid(row=0, column=0, sticky="nsew", padx=(16, 16), pady=(12, 12))
        text_box.insert("1.0", content)
        text_box.configure(state="disabled")

        # 底部关闭按钮
        btn_frame = ctk.CTkFrame(self, fg_color="transparent")
        btn_frame.grid(row=2, column=0, sticky="ew", padx=(20, 20), pady=(0, 16))

        ctk.CTkButton(
            btn_frame,
            text="关闭",
            width=100,
            height=36,
            font=ctk.CTkFont(size=13),
            corner_radius=8,
            fg_color="#e0e0e0",
            text_color="#333333",
            hover_color="#d0d0d0",
            command=self.destroy
        ).pack(side="right")

        self.protocol("WM_DELETE_WINDOW", self.destroy)

    def _build_preview(self, book, annotations):
        """构建预览文本"""
        lines = []
        lines.append(f"# {book['title']}")
        lines.append(f"作者: {book['author']}")
        lines.append("─" * 50)
        lines.append("")

        # 分类笔记
        highlights = []
        notes = []
        bookmarks = []

        for ann in annotations:
            ann_type = ann['type']
            if ann_type == 0:
                bookmarks.append(ann)
            elif ann_type == 1:
                notes.append(ann)
            elif ann_type in (2, 3):
                highlights.append(ann)

        # 高亮与标注
        if highlights:
            lines.append(f"## 高亮与标注 ({len(highlights)} 条)")
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
                lines.append(f"... 还有 {len(highlights) - 50} 条高亮未显示")
            lines.append("")

        # 独立笔记
        if notes:
            lines.append(f"## 独立笔记 ({len(notes)} 条)")
            lines.append("")
            for i, ann in enumerate(notes[:50], 1):
                chapter = parse_cfi_chapter(ann.get('location')) if ann.get('location') else None
                chapter_display = format_chapter_display(chapter, i)
                lines.append(f"### {i}. {chapter_display}")
                lines.append("")

                if ann.get('note'):
                    note_text = ann['note'][:300]
                    if len(ann['note']) > 300:
                        note_text += "..."
                    lines.append(f"  {note_text}")

                if ann.get('created_date'):
                    try:
                        date = apple_timestamp_to_datetime(ann['created_date'])
                        local = date.astimezone()
                        lines.append(f"  {local.strftime('%Y-%m-%d %H:%M')}")
                    except Exception:
                        pass

                lines.append("")

            if len(notes) > 50:
                lines.append(f"... 还有 {len(notes) - 50} 条笔记未显示")
            lines.append("")

        # 书签
        if bookmarks:
            lines.append(f"## 书签 ({len(bookmarks)} 条)")
            lines.append("")
            for i, ann in enumerate(bookmarks[:50], 1):
                chapter = parse_cfi_chapter(ann.get('location')) if ann.get('location') else None
                chapter_display = format_chapter_display(chapter, i)
                note = ann.get('note', '')
                if note:
                    lines.append(f"  {i}. {chapter_display} - {note[:50]}")
                else:
                    lines.append(f"  {i}. {chapter_display}")

            if len(bookmarks) > 50:
                lines.append(f"... 还有 {len(bookmarks) - 50} 条书签未显示")
            lines.append("")

        return '\n'.join(lines)
