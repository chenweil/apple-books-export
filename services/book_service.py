"""
书籍服务 - 封装现有函数，提供线程安全的异步加载
所有 DB 操作在 worker 线程，通过 window.write_event_value() 回调到主线程
"""
import threading
import sys
from pathlib import Path

# 添加父目录到路径以导入 books_exporter
sys.path.insert(0, str(Path(__file__).parent.parent))

from books_exporter import (
    get_books_with_notes,
    get_annotations_for_book,
    export_book_to_markdown
)


class BookService:
    """书籍服务类，提供线程安全的异步加载功能"""

    def __init__(self):
        self._books_cache = None
        self._loading = False

    def load_books_async(self, window):
        """
        异步加载书籍列表，结果通过 window.write_event_value('-BOOKS_LOADED-', books) 发回

        Args:
            window: PySimpleGUI window 对象
        """
        if self._books_cache is not None:
            window.write_event_value('-BOOKS_LOADED-', (self._books_cache, None))
            return

        if self._loading:
            return

        self._loading = True

        def worker():
            try:
                books = get_books_with_notes()
                self._books_cache = books
                window.write_event_value('-BOOKS_LOADED-', (books, None))
            except Exception as e:
                window.write_event_value('-BOOKS_LOADED-', ([], str(e)))
            finally:
                self._loading = False

        thread = threading.Thread(target=worker, daemon=True)
        thread.start()

    def get_annotations_async(self, window, asset_id):
        """
        异步获取书籍的笔记，结果通过 window.write_event_value('-ANNOTATIONS_LOADED-', {...}) 发回

        Args:
            window: PySimpleGUI window 对象
            asset_id: 书籍资产ID，用于防抖校验
        """
        def worker():
            try:
                annotations = get_annotations_for_book(asset_id)
                stats = BookService.classify_annotations(annotations)
                window.write_event_value(
                    '-ANNOTATIONS_LOADED-',
                    {'asset_id': asset_id, 'annotations': annotations, 'stats': stats, 'error': None}
                )
            except Exception as e:
                window.write_event_value(
                    '-ANNOTATIONS_LOADED-',
                    {'asset_id': asset_id, 'annotations': [], 'stats': None, 'error': str(e)}
                )

        thread = threading.Thread(target=worker, daemon=True)
        thread.start()

    def export_async(self, window, book, output_dir):
        """
        异步导出书籍笔记，进度和结果通过 window.write_event_value() 发回

        Args:
            window: PySimpleGUI window 对象
            book: 书籍信息字典
            output_dir: 输出目录路径
        """
        def worker():
            try:
                window.write_event_value('-EXPORT_PROGRESS-', ('loading', 0, 1))

                annotations = get_annotations_for_book(book['asset_id'])
                total = len(annotations)

                window.write_event_value('-EXPORT_PROGRESS-', ('exporting', 0, total))

                output_path = Path(output_dir)
                output_path.mkdir(parents=True, exist_ok=True)

                filepath = export_book_to_markdown(book, annotations, output_path)

                window.write_event_value('-EXPORT_PROGRESS-', ('done', total, total))
                window.write_event_value(
                    '-EXPORT_COMPLETE-',
                    {'success': True, 'filepath': str(filepath), 'error': None}
                )

            except Exception as e:
                window.write_event_value('-EXPORT_PROGRESS-', ('error', 0, 0))
                window.write_event_value(
                    '-EXPORT_COMPLETE-',
                    {'success': False, 'filepath': None, 'error': str(e)}
                )

        thread = threading.Thread(target=worker, daemon=True)
        thread.start()

    @staticmethod
    def classify_annotations(annotations):
        """
        分类笔记统计

        Returns:
            dict: {
                'highlights': 高亮数,
                'notes': 独立笔记数,
                'bookmarks': 书签数,
                'total': 总数
            }
        """
        highlights = 0
        notes = 0
        bookmarks = 0

        for ann in annotations:
            ann_type = ann['type']
            if ann_type == 0:
                bookmarks += 1
            elif ann_type == 1:
                notes += 1
            elif ann_type in (2, 3):
                highlights += 1

        return {
            'highlights': highlights,
            'notes': notes,
            'bookmarks': bookmarks,
            'total': len(annotations)
        }
