# Apple Books Exporter — AppKit GUI(实验)

基于 [swiftui 分支] 的 AppKit 实验。导出 macOS Apple Books 中的笔记、标注、书签为 Markdown 文件。

> **状态**: 实验性研究项目,可能归档。当前聚焦 AppKit GUI 栈,核心数据逻辑(SQLite 访问、模型)从 swiftui 分支复用。

## 系统要求

- macOS 14.0+
- Swift 5.5+(用于 `import SQLite3` 模块化)
- Full Disk Access 权限(读取 Apple Books 数据库)

## 构建与运行

```bash
cd appkit
swift build           # 编译
swift run BooksExporter   # 运行(会自动激活窗口)
```

或用 Xcode 打开 `appkit/Package.swift` 后按 Run。

## 项目结构

```
appkit/
├── Package.swift              # SPM 清单
├── Sources/BooksExporter/
│   ├── BooksExporter-Bridging-Header.h  # SQLite C API(暂时保留)
│   ├── Models/                # 从 swiftui 复用
│   │   ├── Book.swift
│   │   ├── Annotation.swift
│   │   └── AnnotationType.swift
│   ├── Services/              # 从 swiftui 复用
│   │   ├── DatabaseService.swift
│   │   └── BookService.swift
│   ├── Utilities/             # 从 swiftui 复用
│   │   └── PermissionHelper.swift
│   ├── Controllers/           # 新建,M1 架构核心
│   └── Views/                 # 新建,NSView 子类
└── README.md                  # 本文件
```

## 当前进度

| 阶段 | 状态 | 内容 |
|---|---|---|
| W1 | 完成 | 空窗口骨架(NSApplication + AppDelegate + 占位 ViewController) |
| W2 | 完成 | BookListView(NSTableView 显示 Apple Books 数据) |
| M3 | 完成 | 主从分栏：选择书籍并显示笔记详情 |
| M4 | 完成 | 选择目录并导出 Markdown，显示完成或失败状态 |
| M5a | 完成 | 顶部搜索框，按书名或作者实时过滤 |
| M5b | 完成 | 表头点击切换升降序 |
| M5c | 完成 | 无完全磁盘访问权限时弹出引导并支持重试 |
| M5d | 完成 | 详情页支持复制 Markdown 到剪贴板 |
| M5e | 完成 | 列表页支持批量导出当前结果为 Markdown |
| M5f | 完成 | 加载中状态会禁用搜索、表格和导出按钮 |
| M5g | 完成 | 表头排序偏好持久化到 UserDefaults |
| M5+ | 未来 | 其他 UI 调整 |

## 架构选择

采用 **M1 纯 MVC**:

- NSViewController 是核心,直接持有 Model
- 自定义 NSView 子类作为 UI
- 不引入 ViewModel 层(SwiftUI MVVM 是 SwiftUI 没有 view controller 才逼出的妥协)

风格选择 **P1 纯代码 Programmatic**:不依赖 Storyboard / XIB,所有 UI 用 Swift 代码构建。

## 跟其他分支的关系

- **基于 `swiftui`**:Models / Services / Utilities 全复用(目录结构保留,内容零修改)
- **平行于 `main`**:main 是 Rust + Tauri GUI 主线,跟本分支无关
- **目标**:验证 AppKit 作为另一条 GUI 路径的可行性,不替代 Tauri