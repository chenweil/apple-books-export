"""
书籍服务 - 提供书籍和笔记数据访问
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from books_exporter import (
    get_books_with_notes,
    get_annotations_for_book,
    export_book_to_markdown
)


class BookService:
    """书籍服务类"""

    def __init__(self):
        self._books_cache = None

    def load_books(self):
        """加载书籍列表（同步）"""
        if self._books_cache is not None:
            return self._books_cache
        self._books_cache = get_books_with_notes()
        return self._books_cache

    def get_annotations(self, asset_id):
        """获取书籍的笔记（同步）"""
        return get_annotations_for_book(asset_id)

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
