"""
预览窗口 - 显示书籍的所有笔记内容
"""
import PySimpleGUI as sg
from datetime import datetime
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
from books_exporter import apple_timestamp_to_datetime


class PreviewWindow:
    """笔记预览窗口"""

    def __init__(self):
        self.window = None

    def show(self, book, annotations):
        """
        显示预览窗口

        Args:
            book: 书籍信息字典
            annotations: 笔记列表
        """
        if not annotations:
            sg.popup('提示', '这本书没有任何笔记', title='预览')
            return

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

        # 构建预览内容
        content = self._build_preview(book, highlights, notes, bookmarks)

        # 创建窗口布局
        layout = [
            [sg.Text(f'{book["title"]}', font=('Helvetica', 14, 'bold'), size=(60, 1))],
            [sg.Text(f'作者: {book["author"]}', font=('Helvetica', 10), text_color='gray')],
            [sg.Text(f'共 {len(annotations)} 条笔记', font=('Helvetica', 10), text_color='gray')],
            [sg.HorizontalSeparator()],
            [sg.Multiline(
                content,
                key='-PREVIEW_TEXT-',
                size=(70, 25),
                font=('Courier', 10),
                disabled=True,
                autoscroll=True,
                background_color='white',
                text_color='black'
            )],
            [sg.HorizontalSeparator()],
            [sg.Button('关闭', key='-CLOSE-', size=(10, 1))]
        ]

        self.window = sg.Window(
            f'笔记预览 - {book["title"][:30]}',
            layout,
            modal=True,
            finalize=True,
            size=(700, 600)
        )

        # 事件循环
        while True:
            event, values = self.window.read()
            if event in (None, '-CLOSE-'):
                break

        self.window.close()
        self.window = None

    def _build_preview(self, book, highlights, notes, bookmarks):
        """构建预览文本"""
        lines = []
        lines.append('=' * 60)
        lines.append(f'# {book["title"]}')
        lines.append(f'# 作者: {book["author"]}')
        lines.append('=' * 60)
        lines.append('')

        # 高亮与标注
        if highlights:
            lines.append('## 高亮与标注 (共 {} 条)'.format(len(highlights)))
            lines.append('-' * 40)
            for i, ann in enumerate(highlights[:50], 1):  # 限制显示50条
                lines.append('')
                lines.append(f'### {i}.')

                if ann.get('location'):
                    lines.append(f'位置: {ann["location"]}')

                if ann.get('selected_text'):
                    text = ann['selected_text'][:200]
                    if len(ann['selected_text']) > 200:
                        text += '...'
                    lines.append(f'"{text}"')

                if ann.get('note'):
                    lines.append(f'笔记: {ann["note"][:100]}')
                    if len(ann['note']) > 100:
                        lines[-1] += '...'

                if ann.get('created_date'):
                    try:
                        date = apple_timestamp_to_datetime(ann['created_date'])
                        lines.append(f'时间: {date.strftime("%Y-%m-%d %H:%M")}')
                    except Exception:
                        pass

                lines.append('-' * 20)

            if len(highlights) > 50:
                lines.append(f'... (还有 {len(highlights) - 50} 条高亮未显示)')
            lines.append('')

        # 独立笔记
        if notes:
            lines.append('## 独立笔记 (共 {} 条)'.format(len(notes)))
            lines.append('-' * 40)
            for i, ann in enumerate(notes[:50], 1):
                lines.append('')
                lines.append(f'### {i}.')

                if ann.get('location'):
                    lines.append(f'位置: {ann["location"]}')

                if ann.get('note'):
                    note_text = ann['note'][:300]
                    if len(ann['note']) > 300:
                        note_text += '...'
                    lines.append(note_text)

                if ann.get('created_date'):
                    try:
                        date = apple_timestamp_to_datetime(ann['created_date'])
                        lines.append(f'时间: {date.strftime("%Y-%m-%d %H:%M")}')
                    except Exception:
                        pass

                lines.append('-' * 20)

            if len(notes) > 50:
                lines.append(f'... (还有 {len(notes) - 50} 条笔记未显示)')
            lines.append('')

        # 书签
        if bookmarks:
            lines.append('## 书签 (共 {} 条)'.format(len(bookmarks)))
            lines.append('-' * 40)
            for i, ann in enumerate(bookmarks[:50], 1):
                location = ann.get('location', '未知位置')
                note = ann.get('note', '')
                if note:
                    lines.append(f'{i}. 位置: {location} - {note[:50]}')
                else:
                    lines.append(f'{i}. 位置: {location}')

            if len(bookmarks) > 50:
                lines.append(f'... (还有 {len(bookmarks) - 50} 条书签未显示)')
            lines.append('')

        return '\n'.join(lines)
