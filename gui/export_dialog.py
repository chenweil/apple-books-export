"""
导出进度对话框 - 纯进度显示器，不含事件循环
所有事件由 MainWindow 统一处理
"""
import PySimpleGUI as sg
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
from services.book_service import BookService


class ExportDialog:
    """导出进度对话框，纯 UI 组件"""

    def __init__(self):
        self.window = None
        self.cancelled = False

    def show(self, book, output_dir, main_window):
        """
        创建导出进度弹窗并启动异步导出。
        不阻塞，由 MainWindow 事件循环统一驱动。

        Args:
            book: 书籍信息
            output_dir: 输出目录
            main_window: 主窗口（导出事件发到这里）
        """
        self.cancelled = False

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
            modal=False,
            finalize=True,
            disable_close=False,
            size=(500, 200)
        )

        BookService().export_async(main_window, book, output_dir)

    def update_progress(self, status, current, total):
        """更新进度显示"""
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

    def close(self):
        """关闭窗口"""
        if self.window:
            self.window.close()
            self.window = None
