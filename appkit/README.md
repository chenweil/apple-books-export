# AppKit 子项目

AppKit GUI 是本仓库的图形界面实现。它通过 Rust CLI 的 **Machine JSON 协议**读取
Apple Books 数据、显示书籍与标注详情、执行整本书 Markdown 导出，并提供 Share Card
编辑器。`DatabaseService` 仅保留作迁移期兼容和历史测试，不再是 AppKit 默认数据源。

AppKit 不实现自己的数据库规则：书籍归一化、标注身份与导出结果全部来自
Canonical Rust Data Core。

## 与主线的关系

`main` 是 **Headless Mainline**：默认产品入口是 Rust CLI、Read-only TUI 和
Agent Data Skill。Tauri Legacy GUI 已在 `main` 标记废弃，源码保留用于回滚，回滚
锚点是标签 `legacy/tauri-gui-mainline`。

AppKit 已经在 `main` 上（源码树由 #35 引入，与本分支逐字相同），是本仓库的正式
GUI。它通过机器协议消费与 Headless Mainline 完全相同的 Rust 契约，不维护第二套
数据规则。

Tauri Legacy GUI 已在 `main` 标记废弃，源码暂留用于回滚，回滚锚点是标签
`legacy/tauri-gui-mainline`。删除 Tauri 是 Cutover Gate（issue #19）的独立步骤，
尚未执行；在那之前两者并存，但 AppKit 是唯一受 CI 门禁覆盖的图形界面。

## 系统要求

- macOS 14.0+
- Swift 5.9+
- Rust stable toolchain（用于构建随 AppKit 打包的 canonical CLI binary）
- Full Disk Access 权限（读取 Apple Books 数据库）

## 构建与运行

```bash
# 在 Rust mainline checkout 中构建 canonical Rust CLI
cd /path/to/rust-mainline
cargo build --release
cd /path/to/books-exporter/appkit
swift build               # 编译 AppKit
swift test                # 协议桥接、Share Card 与回归测试
APPLE_BOOKS_EXPORTER_BIN="/path/to/rust-mainline/target/release/apple-books-exporter" \
  swift run BooksExporter # 运行（会自动激活窗口）
./Scripts/verify-ui.sh    # UI 回归验证
```

本地运行时可以把 `APPLE_BOOKS_EXPORTER_BIN` 指向 `target/debug/apple-books-exporter`。
应用不会自动下载或执行未知 binary。

解析顺序见 `RustCLIClient.makeForCurrentApp`：`APPLE_BOOKS_EXPORTER_BIN` **优先**，
其次是打包进 `Contents/Resources/apple-books-exporter`，再次是 App 可执行文件旁的同名
文件，最后是 `PATH`。也就是说环境变量是一个显式的开发期覆盖项——能向已安装 App 的
环境注入该变量，就能把它指向任意可执行文件；正常安装路径下走的始终是随包分发的那份。

或用 Xcode 打开 `appkit/Package.swift` 后按 Run。

## 打包 unsigned DMG

当前项目**没有** Developer ID 签名和 notarization。本机安装验证可用 unsigned DMG：

```bash
cd appkit
chmod +x Scripts/package-dmg.sh
./Scripts/package-dmg.sh
```

脚本默认读取仓库的 `target/release/apple-books-exporter`；binary 在另一个
Rust mainline checkout 时显式传入：

```bash
RUST_CLI_BIN=/path/to/rust-mainline/target/release/apple-books-exporter \
  ./Scripts/package-dmg.sh
```

可以覆盖版本号与发布说明（默认 `0.1.8` / build `9`）：

```bash
APP_VERSION=0.3.4 BUILD_VERSION=14 RELEASE_NOTES='修复版本检查' ./Scripts/package-dmg.sh
```

默认产物是仓库根目录的 `dist/Books-Exporter-<版本>-unsigned.dmg`，同时生成供
Version Discovery 使用的 `dist/latest.json`；发布 stable 版本时把两者一起上传到
对应 GitHub Release。

安装时把 DMG 里的 `Books Exporter.app` 拖到 `/Applications`。首次打开若被
Gatekeeper 拦截，优先在 Finder 中右键应用并选择“打开”；必要时：

```bash
xattr -dr com.apple.quarantine "/Applications/Books Exporter.app"
open "/Applications/Books Exporter.app"
```

应用读取 Apple Books 数据仍需要在“系统设置 → 隐私与安全性 → 完全磁盘访问权限”
中添加**安装后的** `Books Exporter.app`——不是终端，也不是构建目录里的那份。

## 项目结构

```
appkit/
├── Package.swift              # SPM 清单
├── Scripts/
│   ├── package-dmg.sh         # 打包 unsigned DMG（目录无关，可从任意目录调用）
│   ├── verify-ui.sh           # 布局与可访问性回归验证
│   └── verify-ui.swift        # 探针，与真实源码一起编译
├── Tests/BooksExporterCoreTests/ # 公共行为与偏好测试
├── Sources/BooksExporterApp/
│   └── main.swift             # 瘦 executable 入口
└── Sources/BooksExporter/     # BooksExporterCore library
    ├── AppEntry.swift         # AppKit 启动入口
    ├── AppDelegate.swift      # 窗口与主菜单装配
    ├── MainMenu.swift         # 主菜单（⌘C/⌘V/⌘Q 等靠它派发）
    ├── MainViewController.swift        # 分栏容器
    ├── SettingsViewController.swift    # 设置页
    ├── SettingsWindowController.swift  # 设置窗口
    ├── BookListView(Controller).swift  # 左栏：书单
    ├── BookDetailView(Controller).swift# 右栏：笔记详情
    ├── AnnotationCellView.swift        # 笔记行（自适应高度）
    ├── ShareCardEditorViewController.swift # Share Card 编辑器
    ├── AnnotationClassifier.swift      # 标注归类规则（唯一来源）
    ├── AnnotationFilter.swift          # 类型筛选（纯函数）
    ├── BookListSorter.swift            # 排序（纯函数）
    ├── BookColumn.swift                # 列标识
    ├── Models/                # Book / Annotation / AnnotationType
    ├── Services/              # AppSettings / DatabaseService（迁移期）/ RustCLIClient / BookService / ShareCardService / UpdateChecker
    │   └── ShareCardService.swift      # 生成、分页、PNG 导出的公共 seam
    ├── Resources/share-card-backgrounds/ # 十二个本地主题背景
    ├── Resources/fonts/       # Share Card 字体与对应许可证
    └── Utilities/             # PermissionHelper
```

## 标注归类

**不要用 `ZANNOTATIONTYPE` 判断类型。** 该字段与内容不对应：实测本地库 type 1 有
105 条、type 3 有 379 条，三个文本字段全为空。它们不是书签（全部带高亮样式、
近半数带选区范围），而是取词失败的高亮。

Apple Books 不把「笔记」存成独立对象——笔记就是给高亮加的批注，原文和批注同在
一行。因此按**内容**归类，规则收在 `AnnotationClassifier` 里，书籍计数与逐条读取共用：

| 条件 | 含义 |
| --- | --- |
| `ZANNOTATIONNOTE` 非空 | 笔记 |
| 仅 `ZANNOTATIONSELECTEDTEXT` 非空 | 高亮 |
| 两者皆空 | 空壳，导出与计数都跳过 |

书签类目已移除：`ZAEANNOTATION` 是唯一的标注表，而 type 0 本地库一条都没有。

## 架构选择

采用 **M1 纯 MVC**：

- NSViewController 是核心，直接持有 Model
- 自定义 NSView 子类作为 UI
- 不引入 ViewModel 层（SwiftUI MVVM 是 SwiftUI 没有 view controller 才逼出的妥协）

风格选择 **P1 纯代码 Programmatic**：不依赖 Storyboard / XIB，所有 UI 用 Swift
代码构建。

排序、筛选、归类等逻辑抽成纯函数（`BookListSorter` / `AnnotationFilter` /
`AnnotationClassifier`），可以脱离 UI 直接断言。Share Cards 通过 `ShareCardService`
暴露生成和导出 seam，视图层不直接处理分页、命名或绘图。

### 已知能力边界：筛选导出仍走本地路径

导出整本书时走 canonical Rust exporter（`BookService` 调 `rustCLIClient.export`）。
但**按当前筛选导出**时，`annotations.count != book.totalAnnotations`，会退回
AppKit 本地生成 Markdown 并直接 `write(to:)`——因为机器导出契约目前只接受整本书的
`asset_id`，刻意没有提供标注选择参数。

这条本地路径**绕过整个机器导出契约**：没有 `ExportReceipt`，不参与 `--overwrite`
语义，没有稳定错误码，也不携带 main 线新增的 `audio_links` / `warnings`。这是
Cutover 时需要显式处置的能力分歧，代码里的注释与本节说明保持一致。

## 测试现状

`BooksExporterCore` 是 library target，`BooksExporter` 只保留瘦 executable 入口，
因此 `Tests/BooksExporterCoreTests` 可以在公共 Share Card seam、数据库服务和设置
偏好边界上运行 XCTest。覆盖默认 Highlight、note-only、可选 note、作者缺失、临时文字
编辑、PNG 尺寸与命名、长文分页、字体资源渲染、目录偏好、十二个模板和四个
Alternative Cards，以及 Apple Books WAL 中最新标注的读取、共享数据库连接并发读取
串行化和刷新间隔偏好持久化。

`RustCLIClient` 的子进程解析、JSON 解析、稳定错误码、非零退出和导出收据由
`RustCLIServiceTests` 覆盖。

`Scripts/verify-ui.sh` 把探针和真实源码一起编译（排除 executable 入口），断言分栏
约束、内容列宽度、按钮排布、行高、排序可访问性、类型筛选、选中标注后才显示 Card
Entry，以及真实 `BookDetailViewController` 入口打开编辑器后的默认预览。它是 UI
回归探针，不是 XCTest。

仓库级的入口说明、Headless 能力矩阵与 Tauri 回滚方式见根
[`README.md`](../README.md)；可见变更记录见根 [`CHANGELOG.md`](../CHANGELOG.md)。
