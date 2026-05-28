# Apple Books 笔记导出工具 - GUI 版本实现方案

## 一、现有代码分析

### 1.1 代码结构

```
books_exporter.py (307 行)
├── 常量定义
│   ├── IBOOKS_PATH = ~/Library/Containers/com.apple.iBooksX/Data/Documents
│   ├── BK_LIBRARY_DB = BKLibrary/BKLibrary-1-091020131601.sqlite
│   └── AE_ANNOTATION_DB = AEAnnotation/AEAnnotation_v10312011_1727_local.sqlite
├── 数据模型
│   └── ANNOTATION_TYPE_MAP = {0:书签, 1:笔记, 2:高亮, 3:标注}
├── 核心函数
│   ├── get_books_with_notes()      # 查询所有书籍及其笔记数
│   ├── get_annotations_for_book()  # 获取指定书籍的所有笔记
│   └── export_book_to_markdown()   # 导出为 Markdown 文件
├── CLI 命令
│   ├── list_books()                # 列出书籍列表
│   ├── export_book()                # 导出单本书
│   └── interactive_select_and_export()  # 交互式选择导出
└── main()                          # CLI 入口
```

### 1.2 数据流分析

```
BKLibrary DB                          AEAnnotation DB
     │                                      │
     ▼                                      ▼
ZASSETID, ZTITLE, ZAUTHOR, ZPATH    ZANNOTATIONASSETID, ZANNOTATIONTYPE,
     │                                      │   ZANNOTATIONSELECTEDTEXT,
     │◄──── asset_id ─────────────────►     │   ZANNOTATIONNOTE,
     │                                      │   ZANNOTATIONCREATIONDATE,
     │                                      │   ZANNOTATIONLOCATION
     │                                      │
     └──────────┬───────────────────────────┘
                ▼
        按 asset_id 关联
                │
                ▼
        ┌───────────────┐
        │ books[]       │
        │ - asset_id    │
        │ - title       │
        │ - author      │
        │ - note_count  │
        └───────────────┘
```

### 1.3 关键实现细节

| 特性 | 实现 |
|------|------|
| 数据库连接 | sqlite3.connect() 同步阻塞 |
| 笔记计数 | GROUP BY ZANNOTATIONASSETID |
| 删除过滤 | ZANNOTATIONDELETED IS NULL OR = 0 |
| 排序方式 | 按笔记数量降序 |
| 文件命名 | 书名(50字符) + asset_id前8位 + .md |
| Markdown结构 | ## 高亮与标注 / ## 独立笔记 / ## 书签 |

---

## 二、GUI 框架推荐

### 2.1 推荐方案: Tkinter (内置) + PySimpleGUI

**推荐理由:**

| 评估项 | Tkinter+PySimpleGUI | PyQt6/PySide6 | Go+WASM |
|--------|---------------------|---------------|---------|
| 安装便利 | ★★★★★ 内置无需安装 | ★★ 需要pip install | ★ 需安装Go编译器 |
| macOS兼容性 | ★★★★ | ★★★★★ | ★★★★ |
| 学习曲线 | ★★★ | ★★★★★ | ★★★ |
| 原生界面 | ★★★ 稍显陈旧 | ★★★★ 现代风格 | ★★★ |
| 与现有代码整合 | ★★★★★ 直接复用 | ★★★★ 需调整import | ★ 需完全重写 |
| 依赖管理 | ★★★★★ 零依赖 | ★★★ 多个包 | ★★★ |

**结论:** 选用 **Tkinter + PySimpleGUI** 方案，理由：
1. 零外部依赖，现有 Python 代码可直接复用
2. PySimpleGUI 封装了 Tkinter 的繁琐 API，开发效率高
3. 完全满足当前需求，无过度设计
4. 如未来需更好看的界面，可迁移至 PyQt

### 2.2 备选方案: PySide6 (Qt)

如对界面质量要求高，可采用 PySide6：
```bash
pip install PySide6
```

---

## 三、GUI 架构设计

### 3.1 窗口布局

```
┌─────────────────────────────────────────────────────────────────┐
│  Apple Books 笔记导出工具                              [─][□][×] │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────┐  ┌───────────────────────────┐│
│  │ 书籍列表                     │  │ 书籍详情                   ││
│  │ ─────────────────────────── │  │ ──────────────────────────  ││
│  │ [#] 书名           作者 笔记 │  │ 书名: xxx                  ││
│  │ [1] 深入理解计算机... 李...  5 │  │ 作者: xxx                  ││
│  │ [2] Python编程...   张... 12 │  │ 笔记数: 12                 ││
│  │ [3] 算法导论...     王...  8 │  │                             ││
│  │ [4] ...                   ... │  │ 高亮与标注: 8               ││
│  │                             │  │ 独立笔记: 3                 ││
│  │ ◄  [1/11]  ►               │  │ 书签: 1                     ││
│  │                             │  │                             ││
│  └─────────────────────────────┘  │ [预览笔记]  [导出 Markdown] ││
│                                   └───────────────────────────┘│
├─────────────────────────────────────────────────────────────────┤
│  状态: 就绪                                      [进度条 ▓▓▓░░] │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 组件规格

| 组件 | 类型 | 规格 |
|------|------|------|
| 书籍列表 | Table | 300宽, 每页20条, 可滚动翻页 |
| 详情面板 | Column | 350宽, 显示选中书信息 |
| 预览按钮 | Button | 触发笔记预览弹窗 |
| 导出按钮 | Button | 导出到选择目录 |
| 状态栏 | StatusBar | 显示当前状态和进度 |
| 进度条 | ProgressBar | 显示导出进度 |

### 3.3 数据流设计

```
┌──────────────────────────────────────────────────────────────────┐
│                            GUI 层                                │
│  ┌────────────┐    ┌────────────┐    ┌────────────────────────┐   │
│  │ BookList   │    │ DetailPanel│    │ ProgressWindow         │   │
│  │ - 显示列表  │───►│ - 显示详情  │───►│ - 显示导出进度         │   │
│  │ - 翻页控制  │    │ - 预览笔记  │    │ - 完成通知             │   │
│  └────────────┘    └────────────┘    └────────────────────────┘   │
└────────────────────────────┬─────────────────────────────────────┘
                             │ 事件/回调
┌────────────────────────────▼─────────────────────────────────────┐
│                          业务逻辑层                                │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────────┐   │
│  │ BookService    │  │ ExportService  │  │ PreviewService     │   │
│  │ - load_books() │  │ - export()     │  │ - get_notes()      │   │
│  │ - paginate()   │  │ - progress_cb  │  │ - format_preview() │   │
│  └───────┬────────┘  └───────┬────────┘  └────────────────────┘   │
└──────────┼───────────────────┼────────────────────────────────────┘
           │                   │
┌──────────▼───────────────────▼────────────────────────────────────┐
│                          数据层 (复用现有代码)                       │
│  ┌────────────────────────────────────────────────────────────┐   │
│  │ books_exporter.py                                          │   │
│  │ - get_books_with_notes()      -> books[]                   │   │
│  │ - get_annotations_for_book()  -> annotations[]             │   │
│  │ - export_book_to_markdown()   -> filepath                  │   │
│  └────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────┘
```

### 3.4 交互逻辑

```
用户操作                    系统响应
─────────────────────────────────────────────────────────
启动程序                   1. 显示加载中...
                           2. 后台线程加载书籍列表
                           3. 加载完成后显示第一页

点击书籍项                 1. 高亮选中项
                           2. 右侧详情面板显示书籍信息
                           3. 笔记统计: 高亮数/笔记数/书签数

点击"预览笔记"             1. 弹出新窗口
                           2. 显示该书所有笔记预览(前100字)
                           3. 支持滚动查看

点击"导出 Markdown"       1. 弹出目录选择对话框
                           2. 后台线程执行导出
                           3. 实时更新进度条
                           4. 完成后显示文件路径

点击上一页/下一页         1. 加载对应页数据
                           2. 更新页码显示
                           3. 更新列表显示
```

### 3.5 线程模型

```
┌─────────────────┐       ┌─────────────────┐
│   Main Thread   │       │  Worker Thread   │
│   (GUI Event    │       │  (DB + Export)   │
│    Loop)        │       │                  │
└────────┬────────┘       └────────┬─────────┘
         │                         │
         │  load_books_async()     │
         │────────────────────────►│
         │                         │ 执行 SQL 查询
         │                         │ 整理数据
         │◄────────────────────────│ 返回 books[]
         │                         │
         │ 更新 UI                 │
         │                         │
         │ export_async()          │
         │────────────────────────►│
         │                         │ 读取 annotations
         │                         │ 写入 markdown
         │◄─ progress ─────────────│
         │◄─ progress ─────────────│
         │◄─ complete(filepath) ───│
         │                         │
         │ 更新进度条               │
         │ 显示完成对话框            │
         └─────────────────────────┘
```

---

## 四、关键实现步骤

### 4.1 模块划分

```
books-exporter/
├── books_exporter.py          # 现有代码 (最小改动)
├── gui/
│   ├── __init__.py
│   ├── main_window.py         # 主窗口, App 入口
│   ├── book_list_panel.py     # 书籍列表面板 (分页Table)
│   ├── detail_panel.py        # 书籍详情面板
│   ├── preview_window.py       # 笔记预览弹窗
│   └── export_dialog.py       # 导出进度弹窗
├── services/
│   ├── __init__.py
│   ├── book_service.py        # 书籍加载服务 (线程化)
│   └── export_service.py       # 导出服务 (线程化+进度)
├── utils/
│   ├── __init__.py
│   └── threading_helper.py    # 线程工具类
└── requirements.txt            # (如需额外依赖)
```

### 4.2 实现步骤

#### Phase 1: 项目初始化
```bash
cd ~/books-exporter
mkdir -p gui services utils
touch gui/__init__.py services/__init__.py utils/__init__.py
```

#### Phase 2: 核心服务层 (不修改现有代码)
```python
# services/book_service.py
class BookService:
    def load_books_async(self, callback):
        """异步加载书籍列表"""
        # 调用现有 get_books_with_notes()
        # 通过 callback 返回结果

    def get_book_detail(self, asset_id):
        """获取单本书详情(含笔记分类统计)"""
        # 调用现有 get_annotations_for_book()
        # 分类统计返回
```

#### Phase 3: 主窗口框架
```python
# gui/main_window.py
import PySimpleGUI as sg

class MainWindow:
    def __init__(self):
        self.window = sg.Window("Apple Books 笔记导出工具", layout)
    
    def run(self):
        # 事件循环
        pass
```

#### Phase 4: 书籍列表 (分页Table)
```python
# gui/book_list_panel.py
class BookListPanel:
    PAGE_SIZE = 20
    
    def __init__(self):
        self.current_page = 1
        self.total_pages = 1
        self.books = []
    
    def load_page(self, page_num):
        """加载指定页"""
        start = (page_num - 1) * self.PAGE_SIZE
        end = start + self.PAGE_SIZE
        return self.books[start:end]
```

#### Phase 5: 预览窗口
```python
# gui/preview_window.py
class PreviewWindow:
    def show(self, book, annotations):
        """显示笔记预览"""
        # 分组显示: 高亮/笔记/书签
        # 每条显示前100字符
        # 支持滚动
```

#### Phase 6: 导出进度
```python
# gui/export_dialog.py
class ExportDialog:
    def run(self, book, output_dir, progress_callback):
        """带进度的导出"""
        # 1. 创建子窗口
        # 2. 启动导出线程
        # 3. progress_callback 更新进度条
        # 4. 完成后显示文件路径
```

#### Phase 7: 主程序入口
```python
# gui/main.py
from gui.main_window import MainWindow

if __name__ == '__main__':
    app = MainWindow()
    app.run()
```

### 4.3 进度回调机制

```python
def export_with_progress(book, output_dir, progress_callback):
    """
    progress_callback(status, current, total)
    status: "loading" | "exporting" | "done" | "error"
    current: 当前处理数
    total: 总数
    """
    annotations = get_annotations_for_book(book['asset_id'])
    total = len(annotations)
    
    for i, ann in enumerate(annotations):
        # 处理单条笔记
        progress_callback("exporting", i + 1, total)
    
    filepath = export_book_to_markdown(book, annotations, output_dir)
    progress_callback("done", total, total)
    return filepath
```

---

## 五、代码模块详细说明

### 5.1 gui/main_window.py

```python
import PySimpleGUI as sg
from gui.book_list_panel import BookListPanel
from gui.detail_panel import DetailPanel
from services.book_service import BookService
from services.export_service import ExportService

class MainWindow:
    TITLE = "Apple Books 笔记导出工具"
    
    def __init__(self):
        self.book_service = BookService()
        self.export_service = ExportService()
        self.selected_book = None
        
        # 布局
        layout = [
            [sg.Text(self.TITLE, font=("Helvetica", 16))],
            [sg.HorizontalSeparator()],
            [
                BookListPanel(),
                DetailPanel(on_export=self._on_export)
            ],
            [sg.StatusBar("就绪")],
        ]
        
        self.window = sg.Window(self.TITLE, layout, size=(800, 600))
    
    def run(self):
        # 加载书籍
        self._load_books()
        
        while True:
            event, values = self.window.read()
            if event in (None, 'Exit'):
                break
            self._handle_event(event, values)
        
        self.window.close()
    
    def _load_books(self):
        # TODO: 显示加载中
        # TODO: 后台线程加载
        pass
    
    def _handle_event(self, event, values):
        pass
    
    def _on_export(self, book):
        # TODO: 调用导出服务
        pass
```

### 5.2 services/export_service.py

```python
import threading
from pathlib import Path
from books_exporter import (
    get_annotations_for_book,
    export_book_to_markdown
)

class ExportService:
    def __init__(self):
        self.current_thread = None
    
    def export_async(self, book, output_dir, progress_callback):
        """异步导出"""
        def worker():
            try:
                filepath = self._export_with_progress(
                    book, output_dir, progress_callback
                )
                sg.popup(f"导出成功!\n{filepath}")
            except Exception as e:
                sg.popup_error(f"导出失败: {e}")
        
        self.current_thread = threading.Thread(target=worker)
        self.current_thread.start()
    
    def _export_with_progress(self, book, output_dir, progress_callback):
        annotations = get_annotations_for_book(book['asset_id'])
        total = len(annotations)
        
        progress_callback("loading", 0, total)
        
        # 模拟分批处理进度
        for i in range(0, total, 10):
            batch_size = min(10, total - i)
            progress_callback("exporting", i + batch_size, total)
        
        output_path = Path(output_dir)
        output_path.mkdir(parents=True, exist_ok=True)
        
        filepath = export_book_to_markdown(book, annotations, output_path)
        progress_callback("done", total, total)
        
        return filepath
```

---

## 六、工作量预估

### 6.1 按模块分解

| 模块 | 功能 | 预估行数 | 难度 |
|------|------|----------|------|
| services/book_service.py | 书籍异步加载 | ~80 | ★★ |
| services/export_service.py | 导出+进度回调 | ~100 | ★★ |
| gui/book_list_panel.py | 分页列表 | ~120 | ★★★ |
| gui/detail_panel.py | 详情面板 | ~80 | ★★ |
| gui/preview_window.py | 预览弹窗 | ~100 | ★★ |
| gui/export_dialog.py | 进度弹窗 | ~60 | ★★ |
| gui/main_window.py | 主窗口+事件 | ~150 | ★★★ |
| 集成测试+调试 | - | ~50 | ★★ |

### 6.2 总工作量

| 阶段 | 内容 | 预估工时 |
|------|------|----------|
| Phase 1 | 项目初始化 | 0.5h |
| Phase 2 | 核心服务层 | 2h |
| Phase 3 | 主窗口框架 | 1h |
| Phase 4 | 书籍列表(分页) | 2h |
| Phase 5 | 预览窗口 | 1.5h |
| Phase 6 | 导出进度 | 1.5h |
| Phase 7 | 集成测试 | 1h |
| **总计** | | **9.5h** |

### 6.3 风险点

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 数据库访问并发 | 多线程同时访问 SQLite | 使用单例模式共享连接, 或每次操作新建连接 |
| UI 响应性 | 大量书籍(256+)加载慢 | 后台线程+加载指示器 |
| macOS 权限 | 无法访问 iBooks 数据 | 添加错误提示和权限说明 |
| Tkinter 样式 | 界面较丑 | 使用 PySimpleGUI 的 Theme 优化 |

---

## 七、后续优化建议

1. **笔记预览增强**: 支持搜索笔记内容
2. **批量导出**: 支持选中多本书籍批量导出
3. **导出格式扩展**: 支持 JSON、HTML、PDF
4. **数据统计**: 添加书籍阅读/笔记统计图表
5. **界面美化**: 考虑迁移至 PyQt/PySide 获得更现代的外观

---

*文档版本: 1.0*
*创建日期: 2026-05-11*
