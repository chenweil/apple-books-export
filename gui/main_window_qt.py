"""
主窗口 - Apple Books 笔记导出工具 (PyQt6 + macOS 风格)
"""
import sys
import threading
import queue
from pathlib import Path

from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QLineEdit, QListWidget, QListWidgetItem, QPushButton,
    QFrame, QSplitter, QMessageBox, QFileDialog, QProgressDialog,
    QGraphicsDropShadowEffect, QScrollArea, QSizePolicy, QStackedWidget
)
from PyQt6.QtCore import Qt, QThread, pyqtSignal, QSize, QPropertyAnimation, QEasingCurve, QRect
from PyQt6.QtGui import QColor, QFont, QPalette, QCursor

sys.path.insert(0, str(Path(__file__).parent.parent))
from services.book_service import BookService
from books_exporter import apple_timestamp_to_datetime, parse_cfi_chapter, format_chapter_display, export_book_to_markdown


# macOS 风格配色
COLORS = {
    'bg': '#f6f6f6',
    'white': '#ffffff',
    'text': '#1d1d1f',
    'text_secondary': '#86868b',
    'text_tertiary': '#aeaeb2',
    'border': '#d2d2d7',
    'divider': '#e5e5ea',
    'accent': '#007aff',
    'accent_hover': '#0056b3',
    'hover': '#f0f0f5',
    'card': '#f5f5f7',
    'scrollbar': '#c7c7cc',
    'selected': '#007aff',
}


class BookListWidget(QListWidget):
    """自定义列表控件 - 支持触控板滚动和自定义样式"""
    
    def __init__(self):
        super().__init__()
        self.setStyleSheet(f"""
            QListWidget {{
                background-color: {COLORS['white']};
                border: none;
                outline: none;
                font-family: 'PingFang SC', sans-serif;
                font-size: 13px;
            }}
            QListWidget::item {{
                padding: 8px 12px;
                border-radius: 6px;
                margin: 2px 8px;
                color: {COLORS['text']};
            }}
            QListWidget::item:hover {{
                background-color: {COLORS['hover']};
            }}
            QListWidget::item:selected {{
                background-color: {COLORS['selected']};
                color: white;
            }}
            QScrollBar:vertical {{
                background: transparent;
                width: 8px;
                margin: 4px 2px 4px 0;
            }}
            QScrollBar::handle:vertical {{
                background: {COLORS['scrollbar']};
                border-radius: 4px;
                min-height: 30px;
            }}
            QScrollBar::handle:vertical:hover {{
                background: {COLORS['text_tertiary']};
            }}
            QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{
                height: 0;
            }}
            QScrollBar::add-page:vertical, QScrollBar::sub-page:vertical {{
                background: transparent;
            }}
        """)
        self.setVerticalScrollMode(QListWidget.ScrollMode.ScrollPerPixel)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setSpacing(4)


class SearchInput(QLineEdit):
    """搜索输入框"""
    
    def __init__(self):
        super().__init__()
        self.setPlaceholderText("搜索书名或作者…")
        self.setStyleSheet(f"""
            QLineEdit {{
                background-color: {COLORS['white']};
                border: 1px solid {COLORS['border']};
                border-radius: 8px;
                padding: 8px 12px;
                font-family: 'PingFang SC', sans-serif;
                font-size: 13px;
                color: {COLORS['text']};
            }}
            QLineEdit:focus {{
                border-color: {COLORS['accent']};
            }}
            QLineEdit::placeholder {{
                color: {COLORS['text_tertiary']};
            }}
        """)


class CardWidget(QFrame):
    """卡片组件"""
    
    def __init__(self, title: str):
        super().__init__()
        self.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['card']};
                border-radius: 10px;
            }}
        """)
        
        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 12, 16, 12)
        layout.setSpacing(4)
        
        title_label = QLabel(title)
        title_label.setStyleSheet(f"""
            color: {COLORS['text_secondary']};
            font-size: 11px;
            border: none;
            background: transparent;
        """)
        title_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        self.value_label = QLabel("—")
        self.value_label.setStyleSheet(f"""
            color: {COLORS['text']};
            font-size: 24px;
            font-weight: bold;
            border: none;
            background: transparent;
        """)
        self.value_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        layout.addWidget(title_label)
        layout.addWidget(self.value_label)


class PrimaryButton(QPushButton):
    """主按钮 - macOS 蓝色风格"""
    
    def __init__(self, text: str):
        super().__init__(text)
        self.setCursor(QCursor(Qt.CursorShape.PointingHandCursor))
        self.setStyleSheet(f"""
            QPushButton {{
                background-color: {COLORS['accent']};
                color: white;
                border: none;
                border-radius: 8px;
                padding: 10px 20px;
                font-family: 'PingFang SC', sans-serif;
                font-size: 13px;
                font-weight: bold;
            }}
            QPushButton:hover {{
                background-color: {COLORS['accent_hover']};
            }}
            QPushButton:pressed {{
                background-color: #003d7a;
            }}
            QPushButton:disabled {{
                background-color: {COLORS['border']};
                color: {COLORS['text_tertiary']};
            }}
        """)


class DetailPanel(QWidget):
    """详情面板"""
    
    preview_clicked = pyqtSignal()
    export_clicked = pyqtSignal()
    
    def __init__(self):
        super().__init__()
        self.current_book = None
        self._build_ui()
    
    def _build_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)
        
        # 使用 QStackedWidget 管理空状态和详情
        self.stacked = QStackedWidget()
        self.stacked.setStyleSheet(f"background-color: {COLORS['white']};")
        
        # 空状态
        self.empty_widget = QWidget()
        empty_layout = QVBoxLayout(self.empty_widget)
        empty_layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        empty_icon = QLabel("📖")
        empty_icon.setStyleSheet("font-size: 40px; border: none; background: transparent;")
        empty_icon.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        empty_text = QLabel("选择一本书查看详情")
        empty_text.setStyleSheet(f"""
            color: {COLORS['text_tertiary']};
            font-size: 13px;
            border: none;
            background: transparent;
        """)
        empty_text.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        empty_layout.addWidget(empty_icon)
        empty_layout.addWidget(empty_text)
        
        # 详情内容
        self.detail_widget = QWidget()
        detail_layout = QVBoxLayout(self.detail_widget)
        detail_layout.setContentsMargins(20, 24, 20, 24)
        detail_layout.setSpacing(0)
        
        # 书名
        self.title_label = QLabel()
        self.title_label.setStyleSheet(f"""
            color: {COLORS['text']};
            font-size: 17px;
            font-weight: bold;
            border: none;
            background: transparent;
        """)
        self.title_label.setWordWrap(True)
        self.title_label.setAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop)
        
        # 作者
        self.author_label = QLabel()
        self.author_label.setStyleSheet(f"""
            color: {COLORS['text_secondary']};
            font-size: 13px;
            border: none;
            background: transparent;
        """)
        
        # 分隔线
        divider1 = QFrame()
        divider1.setFrameShape(QFrame.Shape.HLine)
        divider1.setStyleSheet(f"background-color: {COLORS['divider']}; border: none; max-height: 1px;")
        divider1.setFixedHeight(1)
        
        # 统计卡片
        stats_layout = QHBoxLayout()
        stats_layout.setSpacing(12)
        
        self.note_card = CardWidget("笔记数量")
        self.progress_card = CardWidget("阅读进度")
        
        stats_layout.addWidget(self.note_card)
        stats_layout.addWidget(self.progress_card)
        
        # 时间信息
        self.last_open_label = QLabel()
        self.last_open_label.setStyleSheet(f"""
            color: {COLORS['text_secondary']};
            font-size: 12px;
            border: none;
            background: transparent;
        """)
        
        self.added_label = QLabel()
        self.added_label.setStyleSheet(f"""
            color: {COLORS['text_secondary']};
            font-size: 12px;
            border: none;
            background: transparent;
        """)
        
        # 分隔线 2
        divider2 = QFrame()
        divider2.setFrameShape(QFrame.Shape.HLine)
        divider2.setStyleSheet(f"background-color: {COLORS['divider']}; border: none; max-height: 1px;")
        divider2.setFixedHeight(1)
        
        # 按钮
        btn_layout = QHBoxLayout()
        btn_layout.setSpacing(12)
        
        self.preview_btn = PrimaryButton("预览笔记")
        self.preview_btn.clicked.connect(self.preview_clicked.emit)
        self.preview_btn.setEnabled(False)
        
        self.export_btn = PrimaryButton("导出 Markdown")
        self.export_btn.clicked.connect(self.export_clicked.emit)
        self.export_btn.setEnabled(False)
        
        btn_layout.addWidget(self.preview_btn)
        btn_layout.addWidget(self.export_btn)
        
        # 组装布局
        detail_layout.addWidget(self.title_label)
        detail_layout.addSpacing(2)
        detail_layout.addWidget(self.author_label)
        detail_layout.addSpacing(20)
        detail_layout.addWidget(divider1)
        detail_layout.addSpacing(20)
        detail_layout.addLayout(stats_layout)
        detail_layout.addSpacing(20)
        detail_layout.addWidget(self.last_open_label)
        detail_layout.addSpacing(4)
        detail_layout.addWidget(self.added_label)
        detail_layout.addSpacing(20)
        detail_layout.addWidget(divider2)
        detail_layout.addSpacing(20)
        detail_layout.addLayout(btn_layout)
        detail_layout.addStretch()
        
        # 添加到 StackedWidget
        self.stacked.addWidget(self.empty_widget)
        self.stacked.addWidget(self.detail_widget)
        
        layout.addWidget(self.stacked)
    
    def set_book(self, book, stats=None):
        self.current_book = book
        if not book:
            self.stacked.setCurrentWidget(self.empty_widget)
            return
        
        self.stacked.setCurrentWidget(self.detail_widget)
        
        # 书名限制长度
        title = book['title'][:20] + '…' if len(book['title']) > 20 else book['title']
        self.title_label.setText(title)
        
        author = book['author']
        self.author_label.setText(f"作者：{author}" if author and author != '未知作者' else "")
        
        # 笔记数量
        if stats and isinstance(stats.get('highlights'), int):
            self.note_card.value_label.setText(str(stats['highlights']))
        else:
            self.note_card.value_label.setText("…")
        
        # 阅读进度
        is_finished = book.get('is_finished')
        if is_finished == 1:
            self.progress_card.value_label.setText("已读完")
        else:
            progress = book.get('reading_progress')
            if progress is not None:
                self.progress_card.value_label.setText(f"{int(progress * 100)}%")
            else:
                self.progress_card.value_label.setText("—")
        
        # 时间信息
        last_open = book.get('last_open_date')
        if last_open:
            try:
                dt = apple_timestamp_to_datetime(last_open)
                local = dt.astimezone()
                self.last_open_label.setText(f"最后打开：{local.strftime('%Y-%m-%d %H:%M')}")
            except Exception:
                self.last_open_label.setText("")
        else:
            self.last_open_label.setText("")
        
        added = book.get('creation_date')
        if added:
            try:
                dt = apple_timestamp_to_datetime(added)
                local = dt.astimezone()
                self.added_label.setText(f"添加时间：{local.strftime('%Y-%m-%d')}")
            except Exception:
                self.added_label.setText("")
        else:
            self.added_label.setText("")
        
        # 按钮状态
        if stats and all(isinstance(v, int) for v in stats.values()):
            self.preview_btn.setEnabled(True)
            self.export_btn.setEnabled(True)
        else:
            self.preview_btn.setEnabled(False)
            self.export_btn.setEnabled(False)


class MainWindow(QMainWindow):
    """主窗口"""
    
    def __init__(self):
        super().__init__()
        
        self.setWindowTitle("Apple Books 笔记导出工具")
        self.setMinimumSize(960, 640)
        self.resize(960, 640)
        
        # 设置全局样式
        self.setStyleSheet(f"""
            QMainWindow {{
                background-color: {COLORS['bg']};
            }}
        """)
        
        self.book_service = BookService()
        self.books = []
        self._filtered_books = []
        self.selected_book = None
        self.selected_annotations = None
        self.event_queue = queue.Queue()
        
        self._build_ui()
        self._load_books()
        
        # 定时器处理事件队列
        from PyQt6.QtCore import QTimer
        self.timer = QTimer()
        self.timer.timeout.connect(self._poll_events)
        self.timer.start(100)
    
    def _build_ui(self):
        central = QWidget()
        self.setCentralWidget(central)
        
        layout = QVBoxLayout(central)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)
        
        # 顶部标题栏
        title_bar = QFrame()
        title_bar.setFixedHeight(52)
        title_bar.setStyleSheet(f"background-color: {COLORS['bg']};")
        title_layout = QHBoxLayout(title_bar)
        title_layout.setContentsMargins(16, 0, 16, 0)
        
        title_label = QLabel("📚  笔记导出")
        title_label.setStyleSheet(f"""
            color: {COLORS['text']};
            font-size: 15px;
            font-weight: bold;
            border: none;
            background: transparent;
        """)
        
        self.status_label = QLabel()
        self.status_label.setStyleSheet(f"""
            color: {COLORS['text_secondary']};
            font-size: 12px;
            border: none;
            background: transparent;
        """)
        
        title_layout.addWidget(title_label)
        title_layout.addStretch()
        title_layout.addWidget(self.status_label)
        
        # 分隔线
        divider1 = QFrame()
        divider1.setFrameShape(QFrame.Shape.HLine)
        divider1.setStyleSheet(f"background-color: {COLORS['border']}; border: none;")
        divider1.setFixedHeight(1)
        
        # 内容区 - 使用 Splitter
        content = QSplitter(Qt.Orientation.Horizontal)
        content.setStyleSheet(f"""
            QSplitter {{
                background-color: {COLORS['white']};
            }}
            QSplitter::handle {{
                background-color: {COLORS['divider']};
                width: 1px;
            }}
        """)
        
        # 左侧列表
        left_panel = QWidget()
        left_panel.setStyleSheet(f"background-color: {COLORS['white']};")
        left_layout = QVBoxLayout(left_panel)
        left_layout.setContentsMargins(12, 10, 12, 0)
        left_layout.setSpacing(6)
        
        self.search_input = SearchInput()
        self.search_input.textChanged.connect(self._on_search)
        
        self.book_list = BookListWidget()
        self.book_list.itemClicked.connect(self._on_book_selected)
        
        left_layout.addWidget(self.search_input)
        left_layout.addWidget(self.book_list)
        
        # 右侧详情
        self.detail_panel = DetailPanel()
        self.detail_panel.preview_clicked.connect(self._on_preview)
        self.detail_panel.export_clicked.connect(self._on_export)
        
        content.addWidget(left_panel)
        content.addWidget(self.detail_panel)
        content.setSizes([386, 574])
        
        # 分隔线
        divider2 = QFrame()
        divider2.setFrameShape(QFrame.Shape.HLine)
        divider2.setStyleSheet(f"background-color: {COLORS['border']}; border: none;")
        divider2.setFixedHeight(1)
        
        # 底部状态栏
        bottom_bar = QFrame()
        bottom_bar.setFixedHeight(24)
        bottom_bar.setStyleSheet(f"background-color: {COLORS['bg']};")
        bottom_layout = QHBoxLayout(bottom_bar)
        bottom_layout.setContentsMargins(12, 0, 12, 0)
        
        self.bottom_status = QLabel("正在加载...")
        self.bottom_status.setStyleSheet(f"""
            color: {COLORS['text_secondary']};
            font-size: 11px;
            border: none;
            background: transparent;
        """)
        bottom_layout.addWidget(self.bottom_status)
        bottom_layout.addStretch()
        
        # 组装
        layout.addWidget(title_bar)
        layout.addWidget(divider1)
        layout.addWidget(content, 1)
        layout.addWidget(divider2)
        layout.addWidget(bottom_bar)
    
    def _load_books(self):
        def worker():
            try:
                books = self.book_service.load_books()
                self.event_queue.put(('BOOKS_LOADED', {'books': books, 'error': None}))
            except Exception as e:
                self.event_queue.put(('BOOKS_LOADED', {'books': [], 'error': str(e)}))
        threading.Thread(target=worker, daemon=True).start()
    
    def _poll_events(self):
        try:
            while True:
                event_type, data = self.event_queue.get_nowait()
                self._handle_event(event_type, data)
        except queue.Empty:
            pass
    
    def _handle_event(self, event_type, data):
        if event_type == 'BOOKS_LOADED':
            books = data['books']
            error = data.get('error')
            if error:
                self.bottom_status.setText(f'加载失败: {error}')
                QMessageBox.critical(self, '错误', f'加载书籍失败:\n{error}')
                return
            if not books:
                self.bottom_status.setText('未找到书籍笔记数据')
                QMessageBox.information(self, '提示', '未找到任何书籍笔记数据\n\n请确保 Apple Books 中有书籍并已经做了笔记/标注')
                return
            self.books = books
            self._update_book_list()
            total_notes = sum(b['note_count'] for b in books)
            self.status_label.setText(f"{len(books)} 本书 · {total_notes} 条笔记")
            self.bottom_status.setText(f'已加载 {len(books)} 本书, 共 {total_notes} 条笔记')
        
        elif event_type == 'ANNOTATIONS_LOADED':
            asset_id = data['asset_id']
            error = data.get('error')
            if self.selected_book is None or self.selected_book['asset_id'] != asset_id:
                return
            if error:
                QMessageBox.critical(self, '错误', f'加载笔记失败:\n{error}')
                return
            annotations = data['annotations']
            stats = data['stats']
            self.selected_annotations = annotations
            self.detail_panel.set_book(self.selected_book, stats)
    
    def _update_book_list(self, query=''):
        filtered = self.books
        if query:
            q = query.lower()
            filtered = [b for b in self.books if q in b['title'].lower() or q in b['author'].lower()]
        
        self.book_list.clear()
        for b in filtered:
            title = b['title'][:15] + '…' if len(b['title']) > 15 else b['title']
            item = QListWidgetItem(f"  {title}  ·  {b['note_count']} 条笔记")
            self.book_list.addItem(item)
        
        self._filtered_books = filtered
    
    def _on_search(self, text):
        self._update_book_list(text)
    
    def _on_book_selected(self, item):
        row = self.book_list.row(item)
        if row < 0 or row >= len(self._filtered_books):
            return
        self.selected_book = self._filtered_books[row]
        self.selected_annotations = None
        self.detail_panel.set_book(self.selected_book, {'highlights': '...', 'total': '...'})
        
        def worker():
            try:
                annotations = self.book_service.get_annotations(self.selected_book['asset_id'])
                stats = BookService.classify_annotations(annotations)
                self.event_queue.put(('ANNOTATIONS_LOADED', {
                    'asset_id': self.selected_book['asset_id'],
                    'annotations': annotations,
                    'stats': stats,
                    'error': None
                }))
            except Exception as e:
                self.event_queue.put(('ANNOTATIONS_LOADED', {
                    'asset_id': self.selected_book['asset_id'],
                    'annotations': [],
                    'stats': None,
                    'error': str(e)
                }))
        threading.Thread(target=worker, daemon=True).start()
    
    def _on_preview(self):
        if not self.selected_annotations:
            QMessageBox.information(self, '预览', '这本书没有任何笔记')
            return
        self._show_preview_window()
    
    def _show_preview_window(self):
        from PyQt6.QtWidgets import QDialog, QTextEdit
        
        dialog = QDialog(self)
        dialog.setWindowTitle(f"笔记预览 — {self.selected_book['title'][:30]}")
        dialog.resize(700, 600)
        dialog.setStyleSheet(f"background-color: {COLORS['white']};")
        
        layout = QVBoxLayout(dialog)
        layout.setContentsMargins(20, 16, 20, 12)
        
        # 标题
        title_label = QLabel(self.selected_book['title'])
        title_label.setStyleSheet(f"""
            color: {COLORS['text']};
            font-size: 15px;
            font-weight: bold;
            border: none;
            background: transparent;
        """)
        title_label.setWordWrap(True)
        
        # 副标题
        author = self.selected_book['author']
        # 只统计有实际内容的笔记
        highlights = [ann for ann in self.selected_annotations if ann.get('selected_text') or ann.get('note')]
        sub = f"{author}  ·  {len(highlights)} 条笔记" if author and author != '未知作者' else f"{len(highlights)} 条笔记"
        sub_label = QLabel(sub)
        sub_label.setStyleSheet(f"""
            color: {COLORS['text_secondary']};
            font-size: 12px;
            border: none;
            background: transparent;
        """)
        
        # 分隔线
        divider = QFrame()
        divider.setFrameShape(QFrame.Shape.HLine)
        divider.setStyleSheet(f"background-color: {COLORS['divider']}; border: none;")
        divider.setFixedHeight(1)
        
        # 内容
        content = self._build_preview_content()
        
        text_edit = QTextEdit()
        text_edit.setReadOnly(True)
        text_edit.setStyleSheet(f"""
            QTextEdit {{
                background-color: {COLORS['white']};
                color: {COLORS['text']};
                border: none;
                font-family: 'Menlo', 'Monaco', 'Courier New', monospace;
                font-size: 12px;
            }}
            QScrollBar:vertical {{
                background: transparent;
                width: 8px;
            }}
            QScrollBar::handle:vertical {{
                background: {COLORS['scrollbar']};
                border-radius: 4px;
                min-height: 30px;
            }}
        """)
        text_edit.setPlainText(content)
        
        # 关闭按钮
        close_btn = QPushButton("关闭")
        close_btn.setCursor(QCursor(Qt.CursorShape.PointingHandCursor))
        close_btn.setStyleSheet(f"""
            QPushButton {{
                background-color: {COLORS['divider']};
                color: {COLORS['text']};
                border: none;
                border-radius: 6px;
                padding: 8px 20px;
                font-size: 13px;
            }}
            QPushButton:hover {{
                background-color: {COLORS['scrollbar']};
            }}
        """)
        close_btn.clicked.connect(dialog.accept)
        
        btn_layout = QHBoxLayout()
        btn_layout.addStretch()
        btn_layout.addWidget(close_btn)
        
        layout.addWidget(title_label)
        layout.addSpacing(2)
        layout.addWidget(sub_label)
        layout.addSpacing(8)
        layout.addWidget(divider)
        layout.addSpacing(12)
        layout.addWidget(text_edit)
        layout.addSpacing(8)
        layout.addLayout(btn_layout)
        
        dialog.exec()
    
    def _build_preview_content(self):
        book = self.selected_book
        annotations = self.selected_annotations
        
        lines = [f"# {book['title']}", f"作者: {book['author']}", "─" * 50, ""]
        highlights = [ann for ann in annotations if ann.get('selected_text') or ann.get('note')]
        
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
        
        return '\n'.join(lines)
    
    def _on_export(self):
        output_dir = QFileDialog.getExistingDirectory(self, '选择导出目录')
        if not output_dir:
            return
        
        book = self.selected_book
        
        progress = QProgressDialog("正在导出...", "取消", 0, 100, self)
        progress.setWindowTitle("导出")
        progress.setWindowModality(Qt.WindowModality.WindowModal)
        progress.setStyleSheet(f"""
            QProgressDialog {{
                background-color: {COLORS['white']};
            }}
            QLabel {{
                color: {COLORS['text']};
                font-size: 13px;
            }}
        """)
        progress.setMinimumDuration(0)
        progress.setValue(0)
        
        success = False
        filepath = None
        error_msg = None
        
        def worker():
            nonlocal success, filepath, error_msg
            try:
                annotations = self.book_service.get_annotations(book['asset_id'])
                total = len(annotations)
                
                from PyQt6.QtCore import QMetaObject, Qt
                QMetaObject.invokeMethod(progress, 'setValue', Qt.ConnectionType.QueuedConnection,
                                        Q_ARG(int, 10))
                
                output_path = Path(output_dir)
                output_path.mkdir(parents=True, exist_ok=True)
                
                QMetaObject.invokeMethod(progress, 'setValue', Qt.ConnectionType.QueuedConnection,
                                        Q_ARG(int, 50))
                
                filepath = export_book_to_markdown(book, annotations, output_path)
                success = True
                
                QMetaObject.invokeMethod(progress, 'setValue', Qt.ConnectionType.QueuedConnection,
                                        Q_ARG(int, 100))
            except Exception as e:
                error_msg = str(e)
        
        thread = threading.Thread(target=worker, daemon=True)
        thread.start()
        
        while thread.is_alive():
            QApplication.processEvents()
            thread.join(0.05)
        
        progress.close()
        
        if success:
            QMessageBox.information(self, '导出完成', f'导出成功!\n\n文件已保存至:\n{filepath}')
        elif error_msg:
            QMessageBox.critical(self, '错误', f'导出失败:\n{error_msg}')


def main():
    app = QApplication(sys.argv)
    app.setStyle('Fusion')  # 使用 Fusion 风格获得更现代的外观
    
    window = MainWindow()
    window.show()
    
    sys.exit(app.exec())


if __name__ == '__main__':
    main()
