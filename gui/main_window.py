"""
主窗口 - Apple Books 笔记导出工具主界面 (CustomTkinter 现代化版本)
"""
import customtkinter as ctk
import tkinter as tk
from tkinter import filedialog, messagebox
import threading
import queue

from gui.book_list import BookListPanel
from gui.detail_panel import DetailPanel
from gui.preview_window import PreviewWindow
from gui.export_dialog import ExportDialog
from services.book_service import BookService


class MainWindow(ctk.CTk):
    """主窗口类"""

    TITLE = "Apple Books 笔记导出工具"

    def __init__(self):
        super().__init__()

        # 设置外观
        ctk.set_appearance_mode("Light")
        ctk.set_default_color_theme("blue")

        # 窗口配置
        self.title(self.TITLE)
        self.geometry("900x620")
        self.minsize(700, 500)
        self.configure(fg_color="#fafafa")

        # 初始化服务
        self.book_service = BookService()

        # 事件队列（用于线程间通信）
        self.event_queue = queue.Queue()

        # 状态
        self.selected_book = None
        self.selected_annotations = None
        self.loading_annotations = False

        # 子窗口引用
        self.preview_window = None
        self.export_dialog = None

        # 创建 UI
        self._create_ui()

        # 启动事件轮询
        self._poll_events()

        # 异步加载书籍
        self._load_books()

    def _create_ui(self):
        """创建主界面"""
        self.grid_columnconfigure(0, weight=2)
        self.grid_columnconfigure(1, weight=1)
        self.grid_rowconfigure(0, weight=1)

        # 左侧：书籍列表
        self.book_list_panel = BookListPanel(
            self,
            on_select_callback=self._on_book_selected
        )
        self.book_list_panel.grid(row=0, column=0, sticky="nsew", padx=(12, 6), pady=(12, 12))

        # 右侧：详情面板
        self.detail_panel = DetailPanel(
            self,
            on_preview_callback=self._on_preview,
            on_export_callback=self._on_export
        )
        self.detail_panel.grid(row=0, column=1, sticky="nsew", padx=(6, 12), pady=(12, 12))

        # 底部状态栏
        self.status_bar = ctk.CTkLabel(
            self,
            text="正在加载书籍列表...",
            font=ctk.CTkFont(size=11),
            text_color="#888888",
            anchor="w",
            height=28,
            fg_color="#f0f0f0",
            corner_radius=0
        )
        self.status_bar.grid(row=1, column=0, columnspan=2, sticky="ew", padx=0, pady=0)

    def _poll_events(self):
        """轮询事件队列"""
        try:
            while True:
                event_type, data = self.event_queue.get_nowait()
                self._handle_event(event_type, data)
        except queue.Empty:
            pass

        # 继续轮询
        self.after(100, self._poll_events)

    def _load_books(self):
        """异步加载书籍"""
        def worker():
            try:
                books = self.book_service.load_books()
                self.event_queue.put(('BOOKS_LOADED', {'books': books, 'error': None}))
            except Exception as e:
                self.event_queue.put(('BOOKS_LOADED', {'books': [], 'error': str(e)}))

        thread = threading.Thread(target=worker, daemon=True)
        thread.start()

    def _handle_event(self, event_type, data):
        """处理事件"""
        if event_type == 'BOOKS_LOADED':
            books = data['books']
            error = data.get('error')

            if error:
                self.status_bar.configure(text=f'加载失败: {error}')
                messagebox.showerror('错误', f'加载书籍失败:\n{error}')
                return

            if not books:
                self.status_bar.configure(text='未找到任何书籍笔记数据')
                messagebox.showinfo('提示', '未找到任何书籍笔记数据\n\n请确保 Apple Books 中有书籍并已经做了笔记/标注')
                return

            self.book_list_panel.update_books(books)
            total_notes = sum(b['note_count'] for b in books)
            self.status_bar.configure(text=f'已加载 {len(books)} 本书, 共 {total_notes} 条笔记')

        elif event_type == 'ANNOTATIONS_LOADED':
            asset_id = data['asset_id']
            error = data.get('error')

            # 防抖：检查这本书是否还是当前选中的
            if self.selected_book is None or self.selected_book['asset_id'] != asset_id:
                return

            self.loading_annotations = False

            if error:
                messagebox.showerror('错误', f'加载笔记失败:\n{error}')
                return

            annotations = data['annotations']
            stats = data['stats']
            self.selected_annotations = annotations

            # 更新详情面板
            self.detail_panel.set_book(self.selected_book, annotations)
            self.detail_panel.update_display(stats)

        elif event_type == 'EXPORT_PROGRESS':
            status = data['status']
            current = data['current']
            total = data['total']
            if self.export_dialog:
                self.export_dialog.update_progress(status, current, total)

        elif event_type == 'EXPORT_COMPLETE':
            if self.export_dialog:
                self.export_dialog.close()
                self.export_dialog = None

            success = data['success']
            filepath = data.get('filepath')
            error = data.get('error')

            if success:
                messagebox.showinfo('导出完成', f'导出成功!\n\n文件已保存至:\n{filepath}')
            elif error:
                messagebox.showerror('错误', f'导出失败:\n{error}')

    def _on_book_selected(self, book, index):
        """书籍选中回调"""
        self.selected_book = book
        self.selected_annotations = None
        self.loading_annotations = True

        # 显示加载状态
        self.detail_panel.set_book(book)
        self.detail_panel.update_display({
            'highlights': '...',
            'notes': '...',
            'bookmarks': '...'
        })

        # 异步加载笔记详情
        def worker():
            try:
                annotations = self.book_service.get_annotations(book['asset_id'])
                stats = BookService.classify_annotations(annotations)
                self.event_queue.put(('ANNOTATIONS_LOADED', {
                    'asset_id': book['asset_id'],
                    'annotations': annotations,
                    'stats': stats,
                    'error': None
                }))
            except Exception as e:
                self.event_queue.put(('ANNOTATIONS_LOADED', {
                    'asset_id': book['asset_id'],
                    'annotations': [],
                    'stats': None,
                    'error': str(e)
                }))

        thread = threading.Thread(target=worker, daemon=True)
        thread.start()

    def _on_preview(self, book, annotations):
        """预览按钮回调"""
        if not annotations:
            messagebox.showinfo('预览', '这本书没有任何笔记')
            return

        self.preview_window = PreviewWindow(self)
        self.preview_window.show(book, annotations)

    def _on_export(self, book):
        """导出按钮回调"""
        output_dir = filedialog.askdirectory(title='选择导出目录')
        if not output_dir:
            return

        self.export_dialog = ExportDialog(self)

        # 异步导出
        def worker():
            try:
                self.event_queue.put(('EXPORT_PROGRESS', {'status': 'loading', 'current': 0, 'total': 1}))

                annotations = self.book_service.get_annotations(book['asset_id'])
                total = len(annotations)

                self.event_queue.put(('EXPORT_PROGRESS', {'status': 'exporting', 'current': 0, 'total': total}))

                from pathlib import Path
                from books_exporter import export_book_to_markdown

                output_path = Path(output_dir)
                output_path.mkdir(parents=True, exist_ok=True)

                filepath = export_book_to_markdown(book, annotations, output_path)

                self.event_queue.put(('EXPORT_PROGRESS', {'status': 'done', 'current': total, 'total': total}))
                self.event_queue.put(('EXPORT_COMPLETE', {'success': True, 'filepath': str(filepath), 'error': None}))

            except Exception as e:
                self.event_queue.put(('EXPORT_PROGRESS', {'status': 'error', 'current': 0, 'total': 0}))
                self.event_queue.put(('EXPORT_COMPLETE', {'success': False, 'filepath': None, 'error': str(e)}))

        self.export_dialog.show(book, output_dir)
        thread = threading.Thread(target=worker, daemon=True)
        thread.start()


def main():
    """主函数"""
    print("正在启动 Apple Books 笔记导出工具...")
    print("GUI 窗口即将打开...")

    app = MainWindow()
    app.mainloop()

    print("程序已退出")


if __name__ == '__main__':
    main()
