#!/bin/bash
# Apple Books 笔记导出工具 - GUI 启动脚本
cd "$(dirname "$0")"

/opt/homebrew/bin/python3.14 -m gui.main
