# Headless Mainline + AppKit Cutover Spec

- 状态：需求已确认，待拆分 tickets
- 日期：2026-08-07
- 关联 ADR：[`ADR 0005`](../adr/0005-headless-mainline-appkit-cutover.md)

## 目标

把仓库的产品边界调整为：

- `main` 提供 Rust CLI、Read-only TUI 和 Agent Data Skill，不再把 Tauri GUI 作为默认产品入口。
- `appkit` 继续独立开发，最终成为正式 macOS GUI，并在接入 Rust 数据核心后合并到 `main`。
- Apple Books 数据读取、规范化、稳定身份、权限错误和 Markdown 导出由 Rust 作为唯一事实源。
- 用户不打开 GUI 时，仍可通过终端 TUI 或 AI agent 获取本地 Apple Books 数据。

## 范围

### 首期必须交付

| Surface | 能力 | 写入边界 |
| --- | --- | --- |
| Rust CLI | 书籍列表、标注详情、Markdown 导出、环境诊断 | 只写用户指定的导出目录 |
| Read-only TUI | 搜索书籍、浏览书籍摘要、浏览标注详情 | 不写 Apple Books，不导出 |
| Agent Data Skill | 刷新列表、筛选、读取标注、导出 Markdown、验证文件 | 不写 Apple Books，不调用 AI |
| AppKit | 通过 Rust CLI 获取书籍/标注/导出结果；保留现有 Share Card | 不直接维护长期 Apple Books 查询规则 |

### 暂不迁移到 AppKit 的能力

Rust CLI 继续保留 `enrich`、Rust `card`、`cache`、`config`。这些能力不因 Tauri GUI 下线而删除，也不阻塞 AppKit 首期合并；是否接入 AppKit 另立后续需求。

## 机器协议

### 命令形态

保持现有人类 CLI 兼容，同时增加机器入口：

```bash
apple-books-exporter list
apple-books-exporter export 3

apple-books-exporter list --json
apple-books-exporter annotations --asset-id <asset-id> --json
apple-books-exporter export --asset-id <asset-id> --json
apple-books-exporter doctor --json
```

机器入口不得要求消费者解析人类表格或日志。`asset_id` 是机器身份；人类序号只是刷新列表后的显示选择。

### 成功响应

- stdout 只输出 JSON。
- 每个响应包含整数 `schema_version`。
- 书籍 DTO 至少包含 `asset_id`、`title`、`author`、`note_count`。
- 标注 DTO 至少包含 `id`、`type`、`content_text`、`note_text`、`chapter_title`、`location`、`created_at`；可缺省字段使用 `null`。
- 导出成功返回 receipt，至少包含 `asset_id`、书名、标注数、格式、输出目录、生成文件路径。

### 失败响应

- stderr 输出结构化错误 JSON。
- 退出码非零。
- 错误对象至少包含稳定 `code` 和人类可读 `message`；需要行动时包含 `remediation`。
- 首期至少定义 `FULL_DISK_ACCESS_REQUIRED`、数据库缺失/不可读、无效 `asset_id`、不支持的 schema、输出文件已存在和 binary 不兼容错误。

### 兼容性

- 新增可选字段不提升 `schema_version`。
- 删除字段、改变类型或改变语义必须提升版本。
- TUI、Skill、AppKit 遇到不支持的版本必须明确失败，不能静默降级。

## 导出契约

- 默认目录：`~/books-exported`。
- 默认格式：`obsidian`；可覆盖为 `markdown`。
- 默认不覆盖已有文件；覆盖必须显式传入 `--overwrite`。
- 只写用户选择的输出目录，不修改 Apple Books 数据库。
- Skill 在报告成功前，必须检查目标目录中存在非空 Markdown 文件。

## TUI 契约

- 运行时：Bun + OpenTUI。
- 后端：调用 Rust `list --json` 和标注详情机器接口。
- 交互：搜索、上下选择、打开详情、返回、退出。
- 详情至少显示书名、作者、标注数量、正文、笔记、位置和时间。
- 第一阶段不提供导出、AI、卡片、缓存和配置写操作。
- TUI 不解析人类 CLI 输出，不依赖 AppKit GUI。

## Agent Data Skill 契约

- Skill 作为 `main` 的仓库级能力维护，暂保留 `apple-books-export-rust` 名称。
- 执行前验证 binary 存在、架构兼容、`--help` 包含所需命令。
- binary 解析顺序：`APPLE_BOOKS_EXPORTER_BIN`、仓库 release binary、仓库 debug binary、PATH。
- 执行前刷新列表；机器流程使用 `asset_id`。
- Skill 可列出、筛选、读取、导出，并验证输出；不自动下载 binary、不上传笔记、不自动调用远程 AI。
- 缺少 Full Disk Access 时，原样保留稳定错误码和可执行设置提示。

## AppKit 接入契约

- AppKit 通过 bundled Rust CLI 子进程消费 JSON；首期不引入 FFI 或常驻 daemon。
- AppKit 必须分离 stdout、stderr 和退出码，并将稳定错误映射到 GUI 状态/权限引导。
- AppKit 不直接把 Swift `DatabaseService` 作为长期事实源；迁移期可保留，但新功能不继续扩展第二套查询规则。
- AppKit 继续提供当前 Share Card 能力；Rust 高级 CLI 能力不因本次接入被删除。

## 分支、发布与迁移

1. 在 `main` 实现并验证 Rust JSON 协议、CLI 兼容入口、`doctor`、TUI 和 Skill。
2. 从 `main` 的默认 README、CI、构建和发布路径移除 Tauri GUI，但保留源码。
3. 在 `appkit` 接入 bundled Rust CLI，并完成真实权限、数据、导出和错误 smoke。
4. 完成 arm64/x86_64 原生构建、binary 校验、AppKit 签名/notarization 规划和安装验证。
5. 评估高级能力缺口并得到明确接受。
6. 满足 Cutover Gate 后把 AppKit 合并到 `main`，统一版本和发布源。
7. 用独立提交删除 Tauri 源码和依赖。

过渡期 AppKit 与 Rust 主线保持独立版本；正式合并后才统一版本。开发阶段允许 unsigned/local binary，正式发布不以 unsigned DMG 作为完成证据。

## 验收证据

- Rust：contract tests 覆盖 JSON schema、`asset_id`、错误码、receipt、覆盖保护和权限预检。
- TUI：Bun test/typecheck、fixture 解析、搜索/详情交互和真实 macOS 启动 smoke。
- Skill：Skill 校验、真实 binary `list`/`export`、非空 Markdown 验证。
- AppKit：bundled Rust binary 启动、stdout/stderr/退出码、真实数据和 Full Disk Access smoke。
- 发布：arm64/x86_64 构建、版本/架构声明、安装和签名/notarization 证据分别记录。
- 所有证据区分 fixture、真实 Apple Books 数据和未验证的生产环境能力。

## 非目标

- 不在首期重写 Rust TUI 为 Rust 框架。
- 不把 TUI 变成第二个 GUI。
- 不让 Skill 修改 Apple Books、自动下载 binary 或自动调用 AI。
- 不立即删除 Tauri、不立即合并 AppKit、不在未通过 Cutover Gate 前统一版本。
