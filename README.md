# Apple Books 笔记导出工具

导出 macOS Apple Books 中的笔记、标注、书签为 Markdown 文件。

## 功能

- 列出 Apple Books 书库中所有做过笔记的书籍
- 显示每本书的笔记数量（高亮、标注、笔记、书签分类统计）
- 交互式选择书籍并导出为 Markdown
- 支持预览笔记内容后再决定是否导出
- 进度条显示导出过程

## 系统要求

- macOS（Apple Books 数据仅存在于 macOS）
- Python 3.14（需要 python-tk@3.14）
- Apple Books 中有做过笔记/标注的书籍

## 安装

```bash
# 1. 安装 Python 和 Tkinter
brew install python@3.14 python-tk@3.14

# 2. 安装依赖
/opt/homebrew/bin/python3.14 -m pip install PySimpleGUI --break-system-packages

# 3. 进入项目目录
cd ~/books-exporter
```

## 使用

### GUI 模式

```bash
./run-gui.sh
```

或

```bash
/opt/homebrew/bin/python3.14 -m gui.main
```

界面左侧显示书籍列表（按笔记数排序），右侧显示选中书的详情和统计。点击"预览"查看笔记内容，点击"导出"选择保存位置。

### CLI 模式

```bash
# 列出所有书籍
python3 books_exporter.py list

# 交互式选择导出
python3 books_exporter.py export

# 导出第 N 本书
python3 books_exporter.py export 1

# 指定导出目录
python3 books_exporter.py export 3 -o ~/Desktop
```

## 项目结构

```
books-exporter/
├── books_exporter.py        # CLI 核心 + 数据层
├── gui/
│   ├── main.py             # GUI 入口
│   ├── main_window.py      # 主窗口
│   ├── book_list.py        # 书籍列表面板
│   ├── detail_panel.py     # 详情面板
│   ├── preview_window.py   # 预览弹窗
│   └── export_dialog.py    # 导出进度弹窗
├── services/
│   └── book_service.py    # 业务逻辑封装
├── run-gui.sh             # GUI 启动脚本
├── requirements.txt        # 依赖
└── README.md
```

## 数据来源

Apple Books 的笔记数据存储在：
```
~/Library/Containers/com.apple.iBooksX/Data/Documents/
├── BKLibrary/BKLibrary-*.sqlite      # 书籍元数据
└── AEAnnotation/AEAnnotation_*.sqlite # 笔记/标注数据
```

## 导出格式

导出的 Markdown 文件按笔记类型分组：

```markdown
# 书名

**作者**: 作者名

**笔记数量**: N

---

## 高亮与标注

### 1. 位置: epubcfi(...)
*2025-01-15 10:30*

> 高亮文字

**笔记**: 我的笔记内容

---

## 独立笔记

## 书签
```

## macOS 权限

如果遇到"无法读取数据"提示，确保在 **系统设置 → 隐私与安全性 → 完全磁盘访问权限** 中给予终端或 Python 完全磁盘访问权限。

## License

MIT
