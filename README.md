# Apple Books 笔记导出工具

导出 macOS Apple Books 中的笔记、标注、书签为 Markdown 文件。

## 功能

- 列出 Apple Books 书库中所有做过笔记的书籍
- 显示每本书的笔记数量
- **按书名搜索导出**：支持模糊匹配书名
- 交互式选择书籍并导出为 Markdown
- 支持预览笔记内容后再决定是否导出
- 进度条显示导出过程
- **AI Agent 支持**：提供 skill 文件，支持 AI 助手直接调用

## 系统要求

- macOS（Apple Books 数据仅存在于 macOS）
- Python 3.14（需要 python-tk@3.14）或使用独立二进制
- Apple Books 中有做过笔记/标注的书籍

## 安装

### 方式一：使用独立二进制（推荐）

无需安装 Python，直接使用打包好的二进制：

```bash
# 下载项目
git clone https://github.com/yourname/books-exporter.git
cd books-exporter

# 直接运行
./skills/apple-books-export/scripts/books-exporter list
```

### 方式二：Python 源码运行

```bash
# 1. 安装 Python 和 Tkinter
brew install python@3.14 python-tk@3.14

# 2. 安装依赖
/opt/homebrew/bin/python3.14 -m pip install PySimpleGUI --break-system-packages

# 3. 进入项目目录
cd books-exporter
```

## 使用

### CLI 模式

```bash
# 列出所有书籍
./skills/apple-books-export/scripts/books-exporter list

# 交互式选择导出
./skills/apple-books-export/scripts/books-exporter export

# 导出第 N 本书
./skills/apple-books-export/scripts/books-exporter export 1

# 按书名搜索导出（模糊匹配）
./skills/apple-books-export/scripts/books-exporter export -t "纳瓦尔"

# 指定导出目录
./skills/apple-books-export/scripts/books-exporter export -t "宝典" -o ~/Desktop
```

### GUI 模式

```bash
./run-gui.sh
```

或

```bash
/opt/homebrew/bin/python3.14 -m gui.main
```

界面左侧显示书籍列表（按笔记数排序），右侧显示选中书的详情和统计。点击"预览"查看笔记内容，点击"导出"选择保存位置。

## AI Agent 支持

本项目提供 `apple-books-export` skill，支持 AI 助手（Claude Code、Gemini CLI 等）直接调用：

**安装 skill**：
```bash
# 复制到 agent skills 目录
cp -r skills/apple-books-export ~/.agents/skills/
```

**使用示例**：
```
用户: 导出纳瓦尔宝典的笔记
AI: 正在搜索包含"纳瓦尔"的书籍...
    找到 1 本匹配：
    1. 纳瓦尔宝典 - 217 条笔记
    确认导出到桌面？
```

Skill 文件位置：`skills/apple-books-export/SKILL.md`

## 项目结构

```
books-exporter/
├── books_exporter.py           # CLI 核心 + 数据层
├── gui/                        # GUI 模块
├── services/                   # 业务逻辑封装
├── skills/                     # AI Agent skill
│   └── apple-books-export/
│       ├── SKILL.md            # Skill 文档
│       └── scripts/
│           └── books-exporter  # 独立二进制
├── build.sh                    # 打包脚本
├── run-gui.sh                  # GUI 启动脚本
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
