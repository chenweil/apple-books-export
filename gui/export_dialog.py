"""
导出进度对话框 - 完全事件驱动，不在 worker 线程直接操作 UI
"""
import PySimpleGUI as sg
import threading
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
from services.book_service import BookService


class ExportDialog:
    """导出进度对话框，事件驱动"""

    def __init__(self):
        self.window = None
        self.cancelled = False
        self.export_done = False
        self._book = None
        self._output_dir = None
        self._result = None  # (success, filepath, error)

    def show(self, book, output_dir, window):
        """
        显示导出进度对话框并开始导出

        Args:
            book: 书籍信息
            output_dir: 输出目录
            window: 主窗口（事件发到这里）
        """
        self.cancelled = False
        self.export_done = False
        self._book = book
        self._output_dir = output_dir
        self._result = None

        # 创建布局
        layout = [
            [sg.Text('正在导出', font=('Helvetica', 12, 'bold'))],
            [sg.Text(f'书名: {book["title"][:40]}', key='-EXPORT_TITLE-', size=(50, 1))],
            [sg.Text(f'输出: {output_dir}', key='-OUTPUT_DIR-', size=(50, 1))],
            [sg.HorizontalSeparator()],
            [sg.Text('', key='-STATUS_TEXT-', size=(50, 1))],
            [
                sg.ProgressBar(100, key='-PROGRESS-', size=(40, 20), bar_color=('blue', 'lightgray')),
                sg.Text('', key='-PROGRESS_TEXT-', size=(10, 1))
            ],
            [sg.HorizontalSeparator()],
            [sg.Button('取消', key='-CANCEL-', size=(10, 1))]
        ]

        self.window = sg.Window(
            '导出进度',
            layout,
            modal=True,
            finalize=True,
            disable_close=False,
            size=(500, 200)
        )

        # 通过主窗口启动异步导出（结果发回主窗口）
        BookService().export_async(window, book, output_dir)

        # 事件循环（modal 窗口自己的循环）
        self._event_loop(window)

    def _event_loop(self, main_window):
        """事件循环"""
        while True:
            # 优先检查主窗口的事件
            event, values = main_window.read(timeout=100)

            # 主窗口的事件（导出进度/完成）
            if event == '-EXPORT_PROGRESS-':
                status, current, total = values[event]
                self._update_progress(status, current, total)
                if status == 'done':
                    break
            elif event == '-EXPORT_COMPLETE-':
                self._result = (
                    values[event]['success'],
                    values[event]['filepath'],
                    values[event]['error']
                )
                break

            # 检查我们的 modal 窗口事件
            if event in (None, '-CANCEL-'):
                self.cancelled = True
                self._result = (False, None, '用户取消')
                break

        if self.window:
            self.window.close()
            self.window = None

    def _update_progress(self, status, current, total):
        """更新进度（主线程调用，安全）"""
        if not self.window or self.cancelled:
            return
        if status == 'loading':
            self.window['-STATUS_TEXT-'].update('正在加载笔记...')
            self.window['-PROGRESS-'].update(0)
            self.window['-PROGRESS_TEXT-'].update('')
        elif status == 'exporting':
            percent = int(current / total * 100) if total > 0 else 0
            self.window['-STATUS_TEXT-'].update(f'正在导出 ({current}/{total})')
            self.window['-PROGRESS-'].update(percent)
            self.window['-PROGRESS_TEXT-'].update(f'{percent}%')
        elif status == 'done':
            self.window['-STATUS_TEXT-'].update('导出完成!')
            self.window['-PROGRESS-'].update(100)
            self.window['-PROGRESS_TEXT-'].update('100%')

    def update_progress(self, status, current, total):
        """外部进度更新接口（兼容旧回调式调用）"""
        self._update_progress(status, current, total)

    def on_complete(self):
        """导出完成回调（由 main_window 调用）"""
        self.export_done = True

    def close(self):
        """关闭窗口"""
        if self.window:
            self.window.close()
            self.window = None
