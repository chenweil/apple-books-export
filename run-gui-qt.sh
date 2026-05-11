#!/bin/bash
# 启动 Apple Books 笔记导出工具 (PyQt6 版本)

cd "$(dirname "$0")"
/opt/homebrew/bin/python3.14 -m gui.main_qt
