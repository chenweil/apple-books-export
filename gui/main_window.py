"""
主窗口 - Apple Books 笔记导出工具 (PySimpleGUI + macOS 风格)
"""
import PySimpleGUI as sg
import threading
import queue
from pathlib import Path
import sys

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
}


class MainWindow:
    """主窗口类"""

    TITLE = "Apple Books 笔记导出工具"

    def __init__(self):
        sg.theme('LightGrey1')
        sg.set_options(
            font=('PingFang SC', 13),
            button_element_size=(10, 1),
            auto_size_buttons=False,
        )

        self.book_service = BookService()
        self.event_queue = queue.Queue()
        self.books = []
        self._filtered_books = []
        self.selected_book = None
        self.selected_annotations = None
        self.selected_index = None

        self.layout = self._create_layout()
        self.window = sg.Window(
            self.TITLE,
            self.layout,
            size=(960, 640),
            background_color=COLORS['bg'],
            finalize=True,
        )

        self._bind_events()
        self._load_books()

    def _create_layout(self):
        return [
            # 顶部标题栏
            [
                sg.Column(
                    [[
                        sg.Text("📚  笔记导出", font=('PingFang SC', 15, 'bold'), 
                               text_color=COLORS['text'], background_color=COLORS['bg']),
                        sg.Push(background_color=COLORS['bg']),
                        sg.Text("", key='-STATUS-', font=('PingFang SC', 12),
                               text_color=COLORS['text_secondary'], background_color=COLORS['bg']),
                    ]],
                    pad=(16, 12),
                    background_color=COLORS['bg'],
                    expand_x=True,
                )
            ],
            # 分隔线
            [sg.HorizontalSeparator(color=COLORS['border'], pad=(0, 0))],
            # 内容区
            [
                # 左侧书籍列表
                sg.Column(
                    self._create_book_list_layout(),
                    key='-BOOK_LIST_COL-',
                    element_justification='c',
                    expand_x=True,
                    expand_y=True,
                    background_color=COLORS['white'],
                    pad=(0, 0),
                ),
                # 分隔线
                sg.VerticalSeparator(color=COLORS['divider'], pad=(0, 0)),
                # 右侧详情面板
                sg.Column(
                    self._create_detail_layout(),
                    key='-DETAIL_COL-',
                    element_justification='c',
                    expand_x=True,
                    expand_y=True,
                    background_color=COLORS['white'],
                    pad=(0, 0),
                ),
            ],
            # 分隔线
            [sg.HorizontalSeparator(color=COLORS['border'], pad=(0, 0))],
            # 底部状态栏
            [
                sg.Text("正在加载...", key='-BOTTOM_STATUS-', font=('PingFang SC', 11),
                       text_color=COLORS['text_secondary'], background_color=COLORS['bg'],
                       pad=(12, 6)),
            ],
        ]

    def _create_book_list_layout(self):
        return [
            # 搜索框
            [
                sg.Input(
                    "",
                    key='-SEARCH-',
                    size=(30, 1),
                    enable_events=True,
                    expand_x=True,
                    font=('PingFang SC', 13),
                    background_color=COLORS['white'],
                    text_color=COLORS['text'],
                    pad=(12, 8),
                ),
            ],
            # 书籍列表
            [
                sg.Listbox(
                    values=[],
                    key='-BOOK_LIST-',
                    enable_events=True,
                    select_mode=sg.LISTBOX_SELECT_MODE_SINGLE,
                    size=(None, 20),
                    expand_x=True,
                    expand_y=True,
                    font=('PingFang SC', 13),
                    text_color=COLORS['text'],
                    background_color=COLORS['white'],
                    highlight_background_color=COLORS['accent'],
                    highlight_text_color='#ffffff',
                    no_scrollbar=True,
                    pad=(8, 4),
                ),
            ],
        ]

    def _create_detail_layout(self):
        return [
            # 空状态
            [
                sg.Column(
                    [
                        [sg.Text("📖", font=('PingFang SC', 40), text_color=COLORS['text_tertiary'],
                                background_color=COLORS['white'])],
                        [sg.Text("选择一本书查看详情", font=('PingFang SC', 13),
                                text_color=COLORS['text_tertiary'], background_color=COLORS['white'])],
                    ],
                    key='-EMPTY_STATE-',
                    element_justification='c',
                    vertical_alignment='c',
                    expand_x=True,
                    expand_y=True,
                    background_color=COLORS['white'],
                    visible=True,
                ),
            ],
            # 详情内容 (初始隐藏)
            [
                sg.Column(
                    self._create_detail_content(),
                    key='-DETAIL_CONTENT-',
                    visible=False,
                    expand_x=True,
                    expand_y=True,
                    background_color=COLORS['white'],
                ),
            ],
        ]

    def _create_detail_content(self):
        return [
            # 书名
            [sg.Text("", key='-BOOK_TITLE-', font=('PingFang SC', 17, 'bold'),
                    text_color=COLORS['text'], background_color=COLORS['white'],
                    pad=(20, (24, 2)), expand_x=True)],
            # 作者
            [sg.Text("", key='-BOOK_AUTHOR-', font=('PingFang SC', 13),
                    text_color=COLORS['text_secondary'], background_color=COLORS['white'],
                    pad=(20, (0, 20)))],
            # 分隔线
            [sg.HorizontalSeparator(color=COLORS['divider'], pad=(20, 20))],
            # 统计卡片
            [
                sg.Column(
                    [
                        [
                            sg.Column(
                                [
                                    [sg.Text("笔记数量", font=('PingFang SC', 11),
                                            text_color=COLORS['text_secondary'], 
                                            background_color=COLORS['card'],
                                            justification='c', expand_x=True)],
                                    [sg.Text("—", key='-NOTE_COUNT-', font=('PingFang SC', 24, 'bold'),
                                            text_color=COLORS['text'], background_color=COLORS['card'],
                                            justification='c', expand_x=True)],
                                ],
                                background_color=COLORS['card'],
                                element_justification='c',
                                pad=(0, 0),
                                expand_x=True,
                            ),
                            sg.Column(
                                [
                                    [sg.Text("阅读进度", font=('PingFang SC', 11),
                                            text_color=COLORS['text_secondary'],
                                            background_color=COLORS['card'],
                                            justification='c', expand_x=True)],
                                    [sg.Text("—", key='-PROGRESS-', font=('PingFang SC', 24, 'bold'),
                                            text_color=COLORS['text'], background_color=COLORS['card'],
                                            justification='c', expand_x=True)],
                                ],
                                background_color=COLORS['card'],
                                element_justification='c',
                                pad=(12, 0),
                                expand_x=True,
                            ),
                        ],
                    ],
                    background_color=COLORS['white'],
                    pad=(20, (0, 20)),
                    expand_x=True,
                ),
            ],
            # 时间信息
            [sg.Text("", key='-LAST_OPEN-', font=('PingFang SC', 12),
                    text_color=COLORS['text_secondary'], background_color=COLORS['white'],
                    pad=(20, (0, 4)))],
            [sg.Text("", key='-ADDED-', font=('PingFang SC', 12),
                    text_color=COLORS['text_secondary'], background_color=COLORS['white'],
                    pad=(20, (0, 20)))],
            # 分隔线
            [sg.HorizontalSeparator(color=COLORS['divider'], pad=(20, 20))],
            # 按钮
            [
                sg.Button("预览笔记", key='-PREVIEW-', size=(12, 1),
                         font=('PingFang SC', 13, 'bold'),
                         button_color=(COLORS['white'], COLORS['accent']),
                         disabled=True, pad=(20, (0, 24))),
                sg.Button("导出 Markdown", key='-EXPORT-', size=(14, 1),
                         font=('PingFang SC', 13, 'bold'),
                         button_color=(COLORS['white'], COLORS['accent']),
                         disabled=True, pad=(6, (0, 24))),
            ],
        ]

    def _bind_events(self):
        self.window.bind('<MouseWheel>', '_MOUSEWHEEL_')

    def _load_books(self):
        def worker():
            try:
                books = self.book_service.load_books()
                self.event_queue.put(('BOOKS_LOADED', {'books': books, 'error': None}))
            except Exception as e:
                self.event_queue.put(('BOOKS_LOADED', {'books': [], 'error': str(e)}))
        threading.Thread(target=worker, daemon=True).start()

    def _handle_event(self, event_type, data):
        if event_type == 'BOOKS_LOADED':
            books = data['books']
            error = data.get('error')
            if error:
                self.window['-BOTTOM_STATUS-'].update(f'加载失败: {error}')
                sg.popup_error(f'加载书籍失败:\n{error}', title='错误')
                return
            if not books:
                self.window['-BOTTOM_STATUS-'].update('未找到书籍笔记数据')
                sg.popup('未找到任何书籍笔记数据\n\n请确保 Apple Books 中有书籍并已经做了笔记/标注', title='提示')
                return
            self.books = books
            self._update_book_list()
            total_notes = sum(b['note_count'] for b in books)
            self.window['-STATUS-'].update(f"{len(books)} 本书 · {total_notes} 条笔记")
            self.window['-BOTTOM_STATUS-'].update(f'已加载 {len(books)} 本书, 共 {total_notes} 条笔记')

        elif event_type == 'ANNOTATIONS_LOADED':
            asset_id = data['asset_id']
            error = data.get('error')
            if self.selected_book is None or self.selected_book['asset_id'] != asset_id:
                return
            if error:
                sg.popup_error(f'加载笔记失败:\n{error}', title='错误')
                return
            annotations = data['annotations']
            stats = data['stats']
            self.selected_annotations = annotations
            self._update_detail(stats)

    def _update_book_list(self, query=''):
        if not self.books:
            return
        filtered = self.books
        if query:
            q = query.lower()
            filtered = [b for b in self.books if q in b['title'].lower() or q in b['author'].lower()]
        
        display_values = []
        for b in filtered:
            title = b['title'][:15] + '…' if len(b['title']) > 15 else b['title']
            display_values.append(f"  {title}  ·  {b['note_count']} 条笔记")
        
        self.window['-BOOK_LIST-'].update(display_values)
        self._filtered_books = filtered

    def _update_detail(self, stats=None):
        if not self.selected_book:
            return
        
        book = self.selected_book
        self.window['-EMPTY_STATE-'].update(visible=False)
        self.window['-DETAIL_CONTENT-'].update(visible=True)

        title = book['title'][:15] + '…' if len(book['title']) > 15 else book['title']
        self.window['-BOOK_TITLE-'].update(title)
        author = book['author']
        self.window['-BOOK_AUTHOR-'].update(
            f"作者：{author}" if author and author != '未知作者' else ""
        )

        if stats and isinstance(stats.get('highlights'), int):
            self.window['-NOTE_COUNT-'].update(str(stats['highlights']))
        else:
            self.window['-NOTE_COUNT-'].update("…")

        is_finished = book.get('is_finished')
        if is_finished == 1:
            self.window['-PROGRESS-'].update("已读完")
        else:
            progress = book.get('reading_progress')
            if progress is not None:
                self.window['-PROGRESS-'].update(f"{int(progress * 100)}%")
            else:
                self.window['-PROGRESS-'].update("—")

        last_open = book.get('last_open_date')
        if last_open:
            try:
                dt = apple_timestamp_to_datetime(last_open)
                local = dt.astimezone()
                self.window['-LAST_OPEN-'].update(f"最后打开：{local.strftime('%Y-%m-%d %H:%M')}")
            except Exception:
                self.window['-LAST_OPEN-'].update("")
        else:
            self.window['-LAST_OPEN-'].update("")

        added = book.get('creation_date')
        if added:
            try:
                dt = apple_timestamp_to_datetime(added)
                local = dt.astimezone()
                self.window['-ADDED-'].update(f"添加时间：{local.strftime('%Y-%m-%d')}")
            except Exception:
                self.window['-ADDED-'].update("")
        else:
            self.window['-ADDED-'].update("")

        if stats and all(isinstance(v, int) for v in stats.values()):
            self.window['-PREVIEW-'].update(disabled=False)
            self.window['-EXPORT-'].update(disabled=False)
        else:
            self.window['-PREVIEW-'].update(disabled=True)
            self.window['-EXPORT-'].update(disabled=True)

    def _on_book_selected(self, index):
        if index < 0 or index >= len(self._filtered_books):
            return
        self.selected_book = self._filtered_books[index]
        self.selected_annotations = None
        self._update_detail({'highlights': '...', 'total': '...'})

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
            sg.popup('这本书没有任何笔记', title='预览')
            return
        self._show_preview_window()

    def _show_preview_window(self):
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
        
        content = '\n'.join(lines)
        
        preview_layout = [
            [sg.Text(book['title'], font=('PingFang SC', 15, 'bold'),
                    text_color=COLORS['text'], background_color=COLORS['white'],
                    pad=(20, (16, 2)))],
            [sg.Text(f"{book['author']}  ·  {len(annotations)} 条笔记" if book.get('author') and book['author'] != '未知作者' else f"{len(annotations)} 条笔记",
                    font=('PingFang SC', 12), text_color=COLORS['text_secondary'],
                    background_color=COLORS['white'], pad=(20, (0, 8)))],
            [sg.HorizontalSeparator(color=COLORS['divider'], pad=(20, 20))],
            [sg.Multiline(content, size=(70, 25), font=('Menlo', 12),
                         text_color=COLORS['text'], background_color=COLORS['white'],
                         disabled=True, expand_x=True, expand_y=True, pad=(24, 12))],
            [sg.Push(background_color=COLORS['white']),
             sg.Button("关闭", key='-CLOSE-', size=(10, 1),
                      font=('PingFang SC', 13), button_color=(COLORS['text'], COLORS['divider']),
                      pad=(20, 12))],
        ]
        
        preview_window = sg.Window(
            f"笔记预览 — {book['title'][:30]}",
            preview_layout,
            size=(700, 600),
            background_color=COLORS['white'],
            modal=True,
        )
        
        while True:
            event, values = preview_window.read()
            if event in (sg.WIN_CLOSED, '-CLOSE-'):
                break
        preview_window.close()

    def _on_export(self):
        output_dir = sg.popup_get_folder('选择导出目录', title='导出')
        if not output_dir:
            return
        
        book = self.selected_book
        
        progress_layout = [
            [sg.Text("正在导出", font=('PingFang SC', 14, 'bold'),
                    text_color=COLORS['text'], background_color=COLORS['white'],
                    pad=(20, (20, 4)))],
            [sg.Text(book['title'][:40] + "…" if len(book['title']) > 40 else book['title'],
                    font=('PingFang SC', 12), text_color=COLORS['text_secondary'],
                    background_color=COLORS['white'], pad=(20, (0, 2)))],
            [sg.Text(output_dir, font=('PingFang SC', 11), text_color=COLORS['text_tertiary'],
                    background_color=COLORS['white'], pad=(20, (0, 16)))],
            [sg.Text("准备中…", key='-PROG_STATUS-', font=('PingFang SC', 12),
                    text_color=COLORS['text_secondary'], background_color=COLORS['white'],
                    pad=(20, (0, 8)))],
            [sg.ProgressBar(100, key='-PROG_BAR-', size=(40, 20), bar_color=(COLORS['accent'], COLORS['divider']),
                           pad=(20, (0, 4)))],
            [sg.Text("", key='-PROG_PERCENT-', font=('PingFang SC', 11),
                    text_color=COLORS['text_tertiary'], background_color=COLORS['white'],
                    pad=(20, (0, 12)), justification='r', expand_x=True)],
            [sg.Push(background_color=COLORS['white']),
             sg.Button("取消", key='-CANCEL-', size=(10, 1),
                      font=('PingFang SC', 12), button_color=(COLORS['text'], COLORS['divider']),
                      pad=(20, 16))],
        ]
        
        progress_window = sg.Window(
            "导出",
            progress_layout,
            size=(420, 200),
            background_color=COLORS['white'],
            modal=True,
        )
        
        cancelled = False
        success = False
        filepath = None
        error_msg = None
        
        def worker():
            nonlocal success, filepath, error_msg
            try:
                annotations = self.book_service.get_annotations(book['asset_id'])
                total = len(annotations)
                
                def update_progress(status, current, total):
                    try:
                        if status == 'loading':
                            progress_window.write_event_value('-UPDATE-', ('loading', 0, total))
                        elif status == 'exporting':
                            progress_window.write_event_value('-UPDATE-', ('exporting', current, total))
                        elif status == 'done':
                            progress_window.write_event_value('-UPDATE-', ('done', total, total))
                    except:
                        pass
                
                update_progress('loading', 0, total)
                output_path = Path(output_dir)
                output_path.mkdir(parents=True, exist_ok=True)
                update_progress('exporting', 0, total)
                filepath = export_book_to_markdown(book, annotations, output_path)
                update_progress('done', total, total)
                success = True
                progress_window.write_event_value('-DONE-', None)
            except Exception as e:
                error_msg = str(e)
                progress_window.write_event_value('-DONE-', None)
        
        threading.Thread(target=worker, daemon=True).start()
        
        while True:
            event, values = progress_window.read(timeout=100)
            if event == sg.WIN_CLOSED:
                cancelled = True
                break
            if event == '-CANCEL-':
                cancelled = True
                break
            if event == '-UPDATE-':
                status, current, total = values['-UPDATE-']
                if status == 'loading':
                    progress_window['-PROG_STATUS-'].update("正在加载笔记…")
                    progress_window['-PROG_BAR-'].update(0, max=total if total > 0 else 1)
                elif status == 'exporting':
                    progress_window['-PROG_STATUS-'].update(f"正在导出 ({current}/{total})")
                    pct = int((current / total) * 100) if total > 0 else 0
                    progress_window['-PROG_BAR-'].update(current, max=total)
                    progress_window['-PROG_PERCENT-'].update(f"{pct}%")
                elif status == 'done':
                    progress_window['-PROG_STATUS-'].update("导出完成")
                    progress_window['-PROG_BAR-'].update(total, max=total)
                    progress_window['-PROG_PERCENT-'].update("100%")
            if event == '-DONE-':
                break
        
        progress_window.close()
        
        if success:
            sg.popup(f'导出成功!\n\n文件已保存至:\n{filepath}', title='导出完成')
        elif error_msg:
            sg.popup_error(f'导出失败:\n{error_msg}', title='错误')

    def run(self):
        while True:
            event, values = self.window.read(timeout=100)
            
            # 处理异步事件队列
            try:
                while True:
                    event_type, data = self.event_queue.get_nowait()
                    self._handle_event(event_type, data)
            except queue.Empty:
                pass
            
            if event == sg.WIN_CLOSED:
                break
            
            if event == '-SEARCH-':
                query = values['-SEARCH-']
                self._update_book_list(query)
            
            if event == '-BOOK_LIST-':
                selection = values['-BOOK_LIST-']
                if selection:
                    selected_item = selection[0]
                    for idx, book in enumerate(self._filtered_books):
                        title = book['title'][:15] + '…' if len(book['title']) > 15 else book['title']
                        display_text = f"  {title}  ·  {book['note_count']} 条笔记"
                        if display_text == selected_item:
                            self._on_book_selected(idx)
                            break
            
            if event == '-PREVIEW-':
                self._on_preview()
            
            if event == '-EXPORT-':
                self._on_export()
        
        self.window.close()


def main():
    print("正在启动 Apple Books 笔记导出工具...")
    app = MainWindow()
    app.run()
    print("程序已退出")


if __name__ == '__main__':
    main()
