"""
书籍列表组件 - 分页显示书籍列表
"""
import PySimpleGUI as sg


class BookListPanel:
    """书籍列表面板，支持分页"""

    PAGE_SIZE = 20  # 每页显示20条

    def __init__(self, on_select_callback=None):
        """
        Args:
            on_select_callback: 选中书籍时的回调函数，签名: callback(book, index)
        """
        self.on_select_callback = on_select_callback
        self.books = []
        self.current_page = 1
        self.total_pages = 1
        self.selected_index = None

        # 定义表格列
        self.headers = ['#', '书名', '作者', '笔记']
        self.column_widths = [4, 35, 15, 6]

        # 创建布局
        self.layout = self._create_layout()

    def _create_layout(self):
        """创建布局"""
        return [
            [sg.Text('书籍列表', font=('Helvetica', 12, 'bold'))],
            [sg.HorizontalSeparator()],
            [
                sg.Table(
                    key='-BOOK_TABLE-',
                    headings=self.headers,
                    values=self._get_page_data(),
                    col_widths=self.column_widths,
                    auto_size_columns=False,
                    justification='left',
                    font=('Helvetica', 10),
                    num_rows=18,
                    row_height=22,
                    enable_events=True,
                    select_mode=sg.TABLE_SELECT_MODE_BROWSE,
                    bind_return_key=True,
                    visible=True,
                )
            ],
            [sg.HorizontalSeparator()],
            [
                sg.Button('◄ 上一页', key='-PREV_PAGE-', size=(10, 1), disabled=True),
                sg.Text('', key='-PAGE_INFO-', size=(15, 1), justification='center'),
                sg.Button('下一页 ►', key='-NEXT_PAGE-', size=(10, 1), disabled=True),
            ],
        ]

    def _get_page_data(self):
        """获取当前页的数据"""
        if not self.books:
            return []

        start = (self.current_page - 1) * self.PAGE_SIZE
        end = start + self.PAGE_SIZE
        page_books = self.books[start:end]

        data = []
        for i, book in enumerate(page_books):
            global_index = start + i + 1
            title = book['title'][:33] + '…' if len(book['title']) > 33 else book['title']
            author = book['author'][:13] + '…' if len(book['author']) > 13 else book['author']
            data.append([
                global_index,
                title,
                author,
                book['note_count']
            ])

        return data

    def update_books(self, books):
        """更新书籍列表"""
        self.books = books
        self.total_pages = max(1, (len(books) + self.PAGE_SIZE - 1) // self.PAGE_SIZE)
        self.current_page = 1
        self.selected_index = None

    def get_layout(self):
        """获取布局"""
        return self.layout

    def refresh(self, window):
        """刷新显示"""
        if window is None:
            return

        # 更新表格数据
        window['-BOOK_TABLE-'].update(values=self._get_page_data())

        # 更新页码信息
        window['-PAGE_INFO-'].update(f'{self.current_page} / {self.total_pages}')

        # 更新翻页按钮状态
        window['-PREV_PAGE-'].update(disabled=(self.current_page <= 1))
        window['-NEXT_PAGE-'].update(disabled=(self.current_page >= self.total_pages))

    def handle_event(self, event, window):
        """
        处理事件

        Returns:
            tuple: (handled, book) - 是否处理了事件，选中的书籍
        """
        if event == '-PREV_PAGE-':
            if self.current_page > 1:
                self.current_page -= 1
                self.selected_index = None
                self.refresh(window)
            return True, None

        elif event == '-NEXT_PAGE-':
            if self.current_page < self.total_pages:
                self.current_page += 1
                self.selected_index = None
                self.refresh(window)
            return True, None

        elif event == '-BOOK_TABLE-':
            # 选中书籍
            try:
                selected_rows = window['-BOOK_TABLE-'].SelectedRows
                if selected_rows:
                    row_idx = selected_rows[0]
                    global_index = (self.current_page - 1) * self.PAGE_SIZE + row_idx
                    if 0 <= global_index < len(self.books):
                        self.selected_index = global_index
                        book = self.books[global_index]
                        if self.on_select_callback:
                            self.on_select_callback(book, global_index)
                        return True, book
            except Exception:
                pass
            return True, None

        return False, None
