"""
主窗口 - Apple Books 笔记导出工具主界面
所有 UI 更新都在主线程的事件循环中处理，确保线程安全
"""
import PySimpleGUI as sg
from gui.book_list import BookListPanel
from gui.detail_panel import DetailPanel
from gui.preview_window import PreviewWindow
from gui.export_dialog import ExportDialog
from services.book_service import BookService


class MainWindow:
    """主窗口类"""

    TITLE = "Apple Books 笔记导出工具"

    def __init__(self):
        # 初始化服务
        self.book_service = BookService()

        # 初始化组件
        self.book_list_panel = BookListPanel(on_select_callback=self._on_book_selected)
        self.detail_panel = DetailPanel(
            on_preview_callback=self._on_preview,
            on_export_callback=self._on_export
        )

        # 初始化子窗口
        self.preview_window = PreviewWindow()
        self.export_dialog = ExportDialog()

        # 状态
        self.selected_book = None
        self.selected_annotations = None
        self.loading_annotations = False

        # 创建主窗口
        self.window = self._create_window()

    def _create_window(self):
        """创建主窗口"""
        # 主布局
        layout = [
            [sg.Text(self.TITLE, font=('Helvetica', 16, 'bold'), size=(60, 1))],
            [sg.HorizontalSeparator()],
            [
                # 左侧：书籍列表
                sg.Column(
                    self.book_list_panel.get_layout(),
                    key='-LEFT_PANEL-',
                    size=(400, 500),
                    scrollable=True,
                    vertical_scroll_only=True
                ),
                sg.VerticalSeparator(),
                # 右侧：详情面板
                sg.Column(
                    self.detail_panel.get_layout(),
                    key='-RIGHT_PANEL-',
                    size=(400, 500),
                    scrollable=True,
                    vertical_scroll_only=True
                ),
            ],
            [sg.HorizontalSeparator()],
            [
                sg.StatusBar('', key='-STATUS_BAR-', size=(60, 1)),
            ],
        ]

        window = sg.Window(
            self.TITLE,
            layout,
            size=(850, 600),
            resizable=True,
            finalize=True
        )

        # 初始状态
        window['-STATUS_BAR-'].update('正在加载书籍列表...')

        return window

    def run(self):
        """运行主窗口"""
        # 异步加载书籍（结果通过事件回到主线程）
        self.book_service.load_books_async(self.window)

        # 事件循环（所有 UI 更新都在主线程）
        while True:
            event, values = self.window.read()

            if event in (None, 'Exit'):
                break

            self._handle_event(event, values)

        self.window.close()

    def _handle_event(self, event, values):
        """处理事件"""
        # ---- 书籍加载完成（来自 worker 线程）----
        if event == '-BOOKS_LOADED-':
            books, error = values[event]
            if error:
                self.window['-STATUS_BAR-'].update(f'加载失败: {error}')
                sg.popup_error(f'加载书籍失败:\n{error}', title='错误')
                return
            if not books:
                self.window['-STATUS_BAR-'].update('未找到任何书籍笔记数据')
                sg.popup_info(
                    '未找到任何书籍笔记数据\n\n请确保 Apple Books 中有书籍并已经做了笔记/标注',
                    title='提示'
                )
                return
            self.book_list_panel.update_books(books)
            self.book_list_panel.refresh(self.window)
            total_notes = sum(b['note_count'] for b in books)
            self.window['-STATUS_BAR-'].update(
                f'已加载 {len(books)} 本书, 共 {total_notes} 条笔记'
            )
            return

        # ---- 笔记加载完成（来自 worker 线程，带 asset_id 校验）----
        if event == '-ANNOTATIONS_LOADED-':
            payload = values[event]
            asset_id = payload['asset_id']
            error = payload['error']

            # P1-1 防抖：检查这本书是否还是当前选中的
            if self.selected_book is None or self.selected_book['asset_id'] != asset_id:
                return  # 书已切换，忽略过期回调

            self.loading_annotations = False

            if error:
                sg.popup_error(f'加载笔记失败:\n{error}', title='错误')
                return

            annotations = payload['annotations']
            stats = payload['stats']
            self.selected_annotations = annotations

            # 更新详情面板
            self.detail_panel.set_book(self.selected_book, annotations)
            self.detail_panel.update_display(self.window, stats)
            return

        # ---- 导出进度（来自 worker 线程）----
        if event == '-EXPORT_PROGRESS-':
            status, current, total = values[event]
            self.export_dialog.update_progress(status, current, total)
            return

        # ---- 导出完成（来自 worker 线程）----
        if event == '-EXPORT_COMPLETE-':
            payload = values[event]
            self.export_dialog.on_complete()
            success = payload['success']
            filepath = payload['filepath']
            error = payload['error']
            if success:
                sg.popup_ok(
                    f'导出成功!\n\n文件已保存至:\n{filepath}',
                    title='导出完成',
                    modal=True
                )
            elif error:
                sg.popup_error(f'导出失败:\n{error}', title='错误', modal=True)
            return

        # ---- 书籍列表内部事件 ----
        handled, book = self.book_list_panel.handle_event(event, self.window)
        if handled:
            return

        # ---- 详情面板事件 ----
        handled, action, book = self.detail_panel.handle_event(event, self.window)
        if handled:
            return

    def _on_book_selected(self, book, index):
        """书籍选中回调（主线程）"""
        self.selected_book = book
        self.selected_annotations = None
        self.loading_annotations = True

        # 显示加载状态
        self.detail_panel.set_book(book)
        self.detail_panel.update_display(self.window, {
            'highlights': '...',
            'notes': '...',
            'bookmarks': '...'
        })

        # 异步加载笔记详情（结果通过事件回到主线程）
        self.book_service.get_annotations_async(self.window, book['asset_id'])

    def _on_preview(self, book, annotations):
        """预览按钮回调"""
        if not annotations:
            sg.popup_info('这本书没有任何笔记', title='预览')
            return
        self.preview_window.show(book, annotations)

    def _on_export(self, book):
        """导出按钮回调"""
        output_dir = sg.popup_get_folder(
            '选择导出目录',
            title='选择导出位置',
            default_path='.',
            modal=True
        )
        if not output_dir:
            return

        self.selected_output_dir = output_dir
        self.export_dialog.show(
            book,
            output_dir,
            window=self.window  # 传递 window 用于事件回调
        )
