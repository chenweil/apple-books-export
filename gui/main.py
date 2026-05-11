#!/usr/bin/env python3
"""
Apple Books 笔记导出工具 - GUI 入口

运行方式:
    cd ~/books-exporter
    pip install PySimpleGUI
    python -m gui.main
"""
import sys
from pathlib import Path

# 确保可以导入 gui 和 services 包
sys.path.insert(0, str(Path(__file__).parent.parent))

from gui.main_window import MainWindow


def main():
    """主函数"""
    print("正在启动 Apple Books 笔记导出工具...")
    print("GUI 窗口即将打开...")

    app = MainWindow()
    app.run()

    print("程序已退出")


if __name__ == '__main__':
    main()
