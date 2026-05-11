# Apple Books 笔记导出工具 - OpenCode 指南

## 架构概览

**单仓库 Python 项目**，用于从 macOS Apple Books 导出笔记/标注为 Markdown。

- **入口点**：
  - CLI: `books_exporter.py` (数据层 + CLI 逻辑)
  - GUI: `gui/main.py` (通过 `run-gui.sh` 启动)
- **核心模块**：
  - `books_exporter.py`: SQLite 数据访问、EPUB CFI 解析、Markdown 导出
  - `services/book_service.py`: 业务逻辑封装（缓存、同步/异步加载）
  - `gui/`: PySimpleGUI 界面（主窗口、列表、详情、预览、导出）

## 开发环境

**系统要求**：macOS only（Apple Books 数据仅存在于 macOS）

**Python 版本**：3.14（必须使用 Homebrew 安装，需要 Tkinter 支持）

```bash
# 安装 Python 和 Tkinter
brew install python@3.14 python-tk@3.14

# 安装依赖（必须使用 --break-system-packages，不使用虚拟环境）
/opt/homebrew/bin/python3.14 -m pip install PySimpleGUI --break-system-packages
```

## 运行命令

```bash
# GUI 模式
./run-gui.sh
# 或
/opt/homebrew/bin/python3.14 -m gui.main

# CLI 模式
python3 books_exporter.py list              # 列出所有书籍
python3 books_exporter.py export            # 交互式选择导出
python3 books_exporter.py export 1          # 导出第1本书
python3 books_exporter.py export 1 -o ~/Desktop  # 指定导出目录
```

## 数据源

Apple Books 数据存储在：
```
~/Library/Containers/com.apple.iBooksX/Data/Documents/
├── BKLibrary/BKLibrary-*.sqlite      # 书籍元数据
└── AEAnnotation/AEAnnotation_*.sqlite # 笔记/标注数据
```

**权限要求**：需要完全磁盘访问权限（系统设置 → 隐私与安全性 → 完全磁盘访问权限 → 添加终端或 Python）

## 核心功能

### EPUB CFI 解析

`ZANNOTATIONLOCATION` 字段存储 EPUB CFI（Canonical Fragment Identifier），不是章节标题。

**解析函数**：
- `parse_cfi_chapter(cfi)`: 从 CFI 提取章节信息
- `format_chapter_display(chapter, index)`: 格式化显示

**示例**：
- `epubcfi(...[Section0001.xhtml]...)` → "第1章"
- `epubcfi(...[id20]...)` → "位置 20"
- `epubcfi(...[15-面向并发的内存模型]...)` → "面向并发的内存模型"

**注意**：
- CLI 导出和 GUI 预览都使用这些函数
- 不是所有 EPUB 都有完整章节元数据，部分只显示 ID 标识

### 笔记分类

`ANNOTATION_TYPE_MAP`:
- 0: 书签
- 1: 笔记
- 2: 高亮
- 3: 标注

### Apple 时间戳转换

Apple CoreData 时间戳从 2001-01-01 UTC 开始：
```python
APPLE_EPOCH = datetime(2001, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
```

## 项目约束

- **不使用虚拟环境**：依赖通过 `--break-system-packages` 安装到系统 Python
- **GUI 线程安全**：所有 DB 操作在 worker 线程，通过 `window.write_event_value()` 回调主线程
- **main 分支仅 Python**：Swift/Xcode 代码在 `swiftui` 分支，不要合并到 main
- **导出文件忽略**：`*.md` 在 `.gitignore` 中（除了 README.md 和 .gitignore 本身）

## 常见问题

**权限错误**：确保终端/Python 有完全磁盘访问权限

**Python 版本错误**：必须使用 Python 3.14，因为依赖 `python-tk@3.14`

**GUI 无法启动**：检查 Tkinter 是否安装（`brew install python-tk@3.14`）

**章节显示为原始 CFI**：确保使用 `parse_cfi_chapter()` 和 `format_chapter_display()` 函数
