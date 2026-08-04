# Apple Books Exporter — AppKit GUI(实验)

AppKit 版本的 Apple Books 笔记导出工具,把高亮和笔记导出为 Markdown 文件,也支持把单条标注生成可保存和分享的 Share Card。

> **状态**: 实验性研究项目,可能归档。当前聚焦 AppKit GUI 栈。

当前 AppKit 版本: `v0.1.6` (`build 7`, unsigned)。应用重新激活时会重新读取 Apple Books 数据，也可以在“设置…”中配置低频自动刷新，变更记录见
[`CHANGELOG.md`](CHANGELOG.md)。

## 系统要求

- macOS 14.0+
- Swift 5.5+(用于 `import SQLite3` 模块化)
- Full Disk Access 权限(读取 Apple Books 数据库)

## 构建与运行

```bash
cd appkit
swift build               # 编译
swift test                # Share Card、数据库 WAL 与设置偏好回归测试
swift run BooksExporter   # 运行(会自动激活窗口)
./Scripts/verify-ui.sh    # UI 回归验证
```

或用 Xcode 打开 `appkit/Package.swift` 后按 Run。

## 打包 unsigned DMG

当前项目没有 Developer ID 签名和 notarization。可以先生成本机安装用的 unsigned DMG：

```bash
cd appkit
chmod +x Scripts/package-dmg.sh
./Scripts/package-dmg.sh
```

产物位于仓库根目录的 `dist/Books-Exporter-0.1.6-unsigned.dmg`。也可以覆盖版本号：

```bash
APP_VERSION=0.1.7 BUILD_VERSION=8 ./Scripts/package-dmg.sh
```

安装时将 DMG 中的 `Books Exporter.app` 拖到 `/Applications`。首次打开如果被 Gatekeeper 拦截，优先在 Finder 中右键应用并选择“打开”；必要时可执行：

```bash
xattr -dr com.apple.quarantine "/Applications/Books Exporter.app"
open "/Applications/Books Exporter.app"
```

应用读取 Apple Books 数据仍需要在“系统设置 → 隐私与安全性 → 完全磁盘访问权限”中添加安装后的 `Books Exporter.app`。

## 项目结构

```
appkit/
├── Package.swift              # SPM 清单
├── Scripts/
│   ├── verify-ui.sh           # 布局与可访问性回归验证
│   └── verify-ui.swift        # 探针,与真实源码一起编译
├── Tests/BooksExporterCoreTests/ # 公共行为与偏好测试
├── Sources/BooksExporterApp/
│   └── main.swift             # 瘦 executable 入口
└── Sources/BooksExporter/     # BooksExporterCore library
    ├── AppEntry.swift         # AppKit 启动入口
    ├── AppDelegate.swift      # 窗口与主菜单装配
    ├── MainMenu.swift         # 主菜单(⌘C/⌘V/⌘Q 等靠它派发)
    ├── MainViewController.swift        # 分栏容器
    ├── SettingsViewController.swift    # 设置页
    ├── SettingsWindowController.swift  # 设置窗口
    ├── BookListView(Controller).swift  # 左栏:书单
    ├── BookDetailView(Controller).swift# 右栏:笔记详情
    ├── AnnotationCellView.swift        # 笔记行(自适应高度)
    ├── ShareCardEditorViewController.swift # Share Card 编辑器
    ├── AnnotationClassifier.swift      # 标注归类规则(唯一来源)
    ├── AnnotationFilter.swift          # 类型筛选(纯函数)
    ├── BookListSorter.swift            # 排序(纯函数)
    ├── BookColumn.swift                # 列标识
    ├── Models/                # Book / Annotation / AnnotationType
    ├── Services/              # AppSettings / DatabaseService / BookService / ShareCardService
    │   └── ShareCardService.swift # 生成、分页、PNG 导出的公共 seam
    ├── Resources/share-card-backgrounds/ # 六个本地主题背景
    ├── Resources/fonts/       # Share Card 字体与对应许可证
    └── Utilities/             # PermissionHelper
```

## 标注归类

**不要用 `ZANNOTATIONTYPE` 判断类型。** 该字段与内容不对应:实测本地库
type 1 有 105 条、type 3 有 379 条,三个文本字段全为空。它们不是书签
(全部带高亮样式、近半数带选区范围),而是取词失败的高亮。

Apple Books 不把「笔记」存成独立对象 —— 笔记就是给高亮加的批注,原文和批注同在一行。
因此按**内容**归类,规则收在 `AnnotationClassifier` 里,书籍计数与逐条读取共用:

| 条件 | 含义 |
|---|---|
| `ZANNOTATIONNOTE` 非空 | 笔记 |
| 仅 `ZANNOTATIONSELECTEDTEXT` 非空 | 高亮 |
| 两者皆空 | 空壳,导出与计数都跳过 |

书签类目已移除:`ZAEANNOTATION` 是唯一的标注表,而 type 0 本地库一条都没有。

## 当前进度

| 阶段 | 状态 | 内容 |
|---|---|---|
| W1 | 完成 | 空窗口骨架(NSApplication + AppDelegate + 占位 ViewController) |
| W2 | 完成 | BookListView(NSTableView 显示 Apple Books 数据) |
| M3 | 完成 | 主从分栏:选择书籍并显示笔记详情 |
| M4 | 完成 | 选择目录并导出 Markdown,显示完成或失败状态 |
| M5a | 完成 | 顶部搜索框,按书名或作者实时过滤 |
| M5b | 完成 | 表头点击切换升降序 |
| M5c | 完成 | 无完全磁盘访问权限时弹出引导并支持重试 |
| M5d | 完成 | 详情页支持复制 Markdown 到剪贴板 |
| M5e | 完成 | 列表页支持批量导出当前结果为 Markdown |
| M5f | 完成 | 加载中状态会禁用搜索、表格和导出按钮 |
| M5g | 完成 | 表头排序偏好持久化到 UserDefaults |
| M5h | 完成 | 表头三态排序循环(升序 → 降序 → 无排序) |
| M5i | 完成 | 应用重新激活时重新读取 Apple Books 数据,同步当前选中书的标注 |
| M5j | 完成 | 设置页支持 Apple Books 低频自动刷新间隔,默认每 5 分钟 |
| M6a | 完成 | 界面评审:补主菜单、分栏最小宽度、笔记不再被截断、内容列宽度上限生效 |
| M6b | 完成 | 排序状态对 VoiceOver 可读(AXSortDirection + 变更播报) |
| M6c | 完成 | 详情页按类型筛选笔记(全部 / 高亮 / 笔记) |
| M6d | 完成 | 导出与复制区分「全书」和「当前筛选」 |
| M6e | 完成 | 标注按内容归类,丢弃空壳,移除书签类目 |
| S1 | 完成 | Share Card 选中态入口、默认卡片和 1200×1600 PNG 导出 |
| S2 | 完成 | 笔记-only 内容、可选 supplementary note、作者归因和 card-only 编辑 |
| S3 | 完成 | 42pt 最小字号、连续卡片分页、文件命名和打开目录偏好 |
| S4 | 完成 | 六个主题和按需四个完整 Alternative Cards |
| S5 | 完成 | 保存轻提示、保存后系统分享和 AppKit library/test target 拆分 |
| S6 | 完成 | Share Card 支持系统默认、思源黑体、思源宋体、演示悠然小楷、演示佛系体、站酷文艺体、庞门正道粗书体、四款汇文字体和霞鹜文楷切换,预览与导出共用字体 |

Share Card 编辑器当前提供“系统默认”、思源黑体、思源宋体、演示悠然小楷、演示佛系体、
站酷文艺体、庞门正道粗书体、汇文明朝体、汇文仿宋、汇文正楷、汇文港黑和霞鹜文楷共十二个选项。字体文件及来源/授权说明随应用资源
打包,详见 `appkit/Sources/BooksExporter/Resources/fonts/FONT-SOURCES.txt`。

## 架构选择

采用 **M1 纯 MVC**:

- NSViewController 是核心,直接持有 Model
- 自定义 NSView 子类作为 UI
- 不引入 ViewModel 层(SwiftUI MVVM 是 SwiftUI 没有 view controller 才逼出的妥协)

风格选择 **P1 纯代码 Programmatic**:不依赖 Storyboard / XIB,所有 UI 用 Swift 代码构建。

排序、筛选、归类等逻辑抽成纯函数(`BookListSorter` / `AnnotationFilter` /
`AnnotationClassifier`),可以脱离 UI 直接断言。Share Cards 通过
`ShareCardService` 暴露生成和导出 seam,视图层不直接处理分页、命名或绘图。

## 测试现状

`BooksExporterCore` 是 library target,`BooksExporter` 只保留瘦 executable 入口,
因此 `Tests/BooksExporterCoreTests` 可以在公共 Share Card seam、数据库服务和设置偏好边界上运行 XCTest。
测试覆盖默认 Highlight、note-only、可选 note、作者缺失、临时文字编辑、PNG
尺寸与命名、长文分页、字体资源渲染、目录偏好、四个 Alternative Cards,以及
Apple Books WAL 中最新标注的读取和刷新间隔偏好持久化。

`Scripts/verify-ui.sh` 仍把探针和真实源码一起编译(排除 executable 入口),
断言分栏约束、内容列宽度、按钮排布、行高、排序可访问性、类型筛选和选中标注后
才显示 Card Entry。它是 UI 回归探针,不是 XCTest。

## 跟其他分支的关系

- **平行于 `main`**:main 是 Rust + Tauri GUI 主线,跟本分支无关
- **历史**:本分支起源于已删除的 `swiftui` 分支,其全部提交都保留在本分支历史中
  (`cf1a291`、`9e2f013`)。Models / Services / Utilities 的目录结构沿用自那条线,
  但内容已随功能演进多次修改,不再是当初的复制品
- **目标**:验证 AppKit 作为另一条 GUI 路径的可行性,不替代 Tauri
