# ADR 0005: Headless Rust Mainline and AppKit GUI Cutover

- 状态: 已接受
- 日期: 2026-08-07

## 背景

仓库目前有两条产品线。`main` 承载 Rust CLI、Tauri/Svelte GUI、OpenTUI 原型和 Apple Books Skill；`appkit` 是独立的 AppKit GUI 实验分支。两条线分别拥有数据读取、模型和发布边界，不能把 AppKit 当前完成度直接当成 Rust 主线的替代品。

Tauri GUI 的当前效果不再满足目标，但删除它的同时必须保留终端和 AI agent 获取 Apple Books 数据的能力。AppKit 也不能继续永久维护一套与 Rust 数据规则平行的 SQLite 读取实现，否则书籍排序、标注归类、导出和权限错误会逐渐漂移。

## 决策

### 产品边界

- `main` 的目标产品入口变为 Headless Mainline：Rust CLI、Read-only TUI 和 Agent Data Skill。
- Tauri GUI 停止作为默认入口、默认发布产物、README 主路径和 CI 主路径，但暂时保留源码供回滚和历史比较。
- AppKit 成为未来正式的 AppKit GUI Surface。迁移完成前，AppKit 继续在独立分支开发；不在本决策中立即合并分支或删除 Tauri。
- AppKit 满足 Cutover Gate 后，才合并到 `main`，再以独立提交删除 Tauri Legacy GUI。

### 数据所有权与连接方式

- Canonical Rust Data Core 是 Apple Books 数据读取、规范化 DTO、稳定身份、导出结果和错误语义的唯一事实源。
- AppKit 通过随 App 打包的 Rust CLI 子进程消费 Machine JSON Protocol。首期不引入 Rust/Swift FFI，也不启动常驻 daemon；协议语义独立于未来可能的传输层替换。
- TUI 和 Agent Data Skill 直接调用同一 Rust CLI。AppKit、TUI、Skill 不各自维护 Apple Books 数据库查询规则。

### 首期机器能力

- `list --json` 返回书籍摘要。
- `annotations --asset-id <id> --json` 返回规范化标注详情。
- `export --asset-id <id> --json` 导出 Markdown 并返回结构化 receipt。
- `doctor --json` 提供 Full Disk Access、数据库、binary 架构和运行环境预检。
- 机器消费者使用 `asset_id`；人类 CLI 的序号入口继续兼容，但序号不是稳定身份。
- 成功结果写 stdout；结构化错误写 stderr 并返回非零退出码。响应携带 `schema_version`，新增可选字段可兼容演进，破坏性变化必须升级版本。
- DTO 隐藏 SQLite 原始行和表结构，只暴露书籍、标注和导出所需的规范化字段。

### TUI 与 Agent Data Skill

- TUI 第一阶段是 Bun + OpenTUI 的只读终端浏览器，支持搜索、书籍选择和标注详情；不执行导出、AI、卡片、缓存或配置写操作。
- Agent Data Skill 支持刷新列表、筛选、读取标注和导出 Markdown；不修改 Apple Books，不自动调用 AI，不上传笔记。
- Skill 继续作为 `main` 的仓库级能力，调用经过验证的 Rust binary。binary 解析优先使用 `APPLE_BOOKS_EXPORTER_BIN`，然后使用仓库 release/debug binary，最后才使用 PATH；缺失或架构不匹配时失败并给出明确提示。
- Skill 默认导出到 `~/books-exported` 的 Obsidian 格式；默认不覆盖已有文件，覆盖必须显式授权。

### 权限、隐私与发布

- Terminal、AI agent host、TUI、AppKit 等执行主体分别处理 Full Disk Access；程序不尝试自动修改系统权限。
- `FULL_DISK_ACCESS_REQUIRED` 等稳定错误码必须能被 TUI、Skill 和 AppKit 识别，并带有可执行的 macOS 设置提示。
- listing、reading、TUI 和 Markdown export 遵守 Local Data Boundary；网络 AI 能力属于未来单独决策。
- 正式发布支持 arm64 和 x86_64，按实际架构构建和声明 binary/DMG；不虚报 universal 能力。
- AppKit 正式发布需要代码签名和 notarization；CLI/TUI binary 提供校验信息。Skill 不自动下载和执行未知 binary。
- 过渡期 AppKit 与 Rust 主线保持独立版本；AppKit 合并到 `main` 后再统一版本和发布源。

### 迁移顺序与验收

1. 在 `main` 稳定 Rust Machine JSON Protocol 和 CLI 兼容入口。
2. 完善 TUI、Agent Data Skill、`doctor` 和契约测试。
3. 停止 Tauri 的默认文档、CI 和发布路径，但保留源码。
4. 让 AppKit 接入 bundled Rust CLI，并验证真实数据、权限、错误和导出。
5. 完成 AppKit 的打包、安装、签名/notarization 和真实 macOS smoke test。
6. 明确 AppKit 与 Rust 高级能力（enrich、Rust card、cache、config）的功能缺口；首期允许这些能力继续由 Rust CLI 提供。
7. 满足 Cutover Gate 后合并 AppKit，最后单独提交删除 Tauri Legacy GUI。

## 后果

- `main` 可以在不打开 GUI 的情况下通过 CLI、TUI 和 Agent Data Skill 读取和导出 Apple Books 数据。
- AppKit 的短期接入成本上升：需要管理 Rust 子进程、bundle 架构、Full Disk Access、stdout/stderr 和退出码。
- 首期保留 Tauri 源码和两条独立版本线，增加过渡期维护成本，但避免 GUI 空窗和不可逆删除。
- AppKit 当前 Swift `DatabaseService` 不能作为长期数据事实源；在接入 Rust 前，旧实现只能作为迁移中的临时路径。
- Share Card 可以继续作为 AppKit 本地能力；enrich、Rust card、cache、config 不因 GUI cutover 被静默删除。

## 非目标

- 本 ADR 不立即删除 Tauri、不立即合并 `appkit`、不切换默认分支。
- 本 ADR 不规定 Rust/Swift FFI 或 daemon 的未来实现。
- 本 ADR 不授权 Skill 自动下载 binary、自动调用远程 AI 或修改 Apple Books 数据。
- 本 ADR 不要求 AppKit 在第一阶段复制 Tauri 的全部高级功能。
