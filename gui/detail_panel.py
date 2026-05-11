"""
书籍详情面板 - 显示选中书籍的详细信息
"""
import PySimpleGUI as sg


class DetailPanel:
    """书籍详情面板"""

    def __init__(self, on_preview_callback=None, on_export_callback=None):
        """
        Args:
            on_preview_callback: 预览按钮回调，签名: callback(book)
            on_export_callback: 导出按钮回调，签名: callback(book)
        """
        self.on_preview_callback = on_preview_callback
        self.on_export_callback = on_export_callback
        self.current_book = None
        self.current_annotations = None

        # 创建布局
        self.layout = self._create_layout()

    def _create_layout(self):
        """创建布局"""
        return [
            [sg.Text('书籍详情', font=('Helvetica', 12, 'bold'))],
            [sg.HorizontalSeparator()],
            [sg.Text('书名:', size=(10, 1), font=('Helvetica', 10, 'bold')),
             sg.Text('', key='-BOOK_TITLE-', size=(35, 1), font=('Helvetica', 10))],
            [sg.Text('作者:', size=(10, 1), font=('Helvetica', 10, 'bold')),
             sg.Text('', key='-BOOK_AUTHOR-', size=(35, 1), font=('Helvetica', 10))],
            [sg.HorizontalSeparator()],
            [sg.Text('统计信息', font=('Helvetica', 11, 'bold'))],
            [sg.Text('笔记总数:', size=(12, 1)), sg.Text('', key='-NOTE_COUNT-', size=(10, 1))],
            [sg.Text('高亮与标注:', size=(12, 1)), sg.Text('', key='-HIGHLIGHT_COUNT-', size=(10, 1))],
            [sg.Text('独立笔记:', size=(12, 1)), sg.Text('', key='-ANNOTATION_COUNT-', size=(10, 1))],
            [sg.Text('书签:', size=(12, 1)), sg.Text('', key='-BOOKMARK_COUNT-', size=(10, 1))],
            [sg.HorizontalSeparator()],
            [sg.Text('', key='-HINT_TEXT-', size=(45, 2), text_color='gray')],
            [
                sg.Button('预览笔记', key='-PREVIEW-', size=(15, 1), disabled=True,
                          button_color=('white', 'gray')),
                sg.Button('导出 Markdown', key='-EXPORT-', size=(15, 1), disabled=True,
                          button_color=('white', 'gray')),
            ],
        ]

    def get_layout(self):
        """获取布局"""
        return self.layout

    def set_book(self, book, annotations=None):
        """设置当前书籍"""
        self.current_book = book
        self.current_annotations = annotations

    def update_display(self, window, stats=None):
        """更新显示"""
        if window is None:
            return

        if self.current_book:
            book = self.current_book
            window['-BOOK_TITLE-'].update(book['title'])
            window['-BOOK_AUTHOR-'].update(book['author'])
            window['-NOTE_COUNT-'].update(str(book['note_count']))
            window['-HINT_TEXT-'].update('选中一本书后可以预览笔记或导出')

            # 更新统计信息
            if stats:
                window['-HIGHLIGHT_COUNT-'].update(str(stats.get('highlights', 0)))
                window['-ANNOTATION_COUNT-'].update(str(stats.get('notes', 0)))
                window['-BOOKMARK_COUNT-'].update(str(stats.get('bookmarks', 0)))

            # 启用按钮
            window['-PREVIEW-'].update(disabled=False, button_color=('white', 'blue'))
            window['-EXPORT-'].update(disabled=False, button_color=('white', 'green'))
        else:
            # 清空显示
            window['-BOOK_TITLE-'].update('')
            window['-BOOK_AUTHOR-'].update('')
            window['-NOTE_COUNT-'].update('')
            window['-HIGHLIGHT_COUNT-'].update('')
            window['-ANNOTATION_COUNT-'].update('')
            window['-BOOKMARK_COUNT-'].update('')
            window['-HINT_TEXT-'].update('请从左侧列表选择一本书')
            window['-PREVIEW-'].update(disabled=True, button_color=('white', 'gray'))
            window['-EXPORT-'].update(disabled=True, button_color=('white', 'gray'))

    def handle_event(self, event, window):
        """
        处理事件

        Returns:
            tuple: (handled, action, book)
                handled: 是否处理了事件
                action: 'preview' | 'export' | None
                book: 当前书籍
        """
        if event == '-PREVIEW-' and self.current_book:
            if self.on_preview_callback:
                self.on_preview_callback(self.current_book, self.current_annotations)
            return True, 'preview', self.current_book

        elif event == '-EXPORT-' and self.current_book:
            if self.on_export_callback:
                self.on_export_callback(self.current_book)
            return True, 'export', self.current_book

        return False, None, None
