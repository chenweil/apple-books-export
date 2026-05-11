#!/usr/bin/env python3
"""
Apple Books 笔记导出工具 - GUI 入口 (PyQt6)

运行方式:
    cd ~/books-exporter
    pip install PyQt6
    python -m gui.main_qt
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from gui.main_window_qt import main


if __name__ == '__main__':
    main()
