# ADR 0006: AppKit 首期能力边界

- 状态：已接受
- 日期：2026-08-11
- 关联：[`ADR 0005`](0005-headless-mainline-appkit-cutover.md)、[`Cutover Gate 验收清单`](../plans/2026-08-11-cutover-gate-verification.md)

## 背景

AppKit 已经通过 bundled Rust CLI 接入 Canonical Rust Data Core，但它不需要在
首期复制 Rust CLI 的全部高级能力。若没有明确边界，AppKit 合并后容易重新出现
两套数据查询、导出和 AI 规则，或者把 Tauri 已有功能缺口误认为本次迁移失败。

同时，当前 AppKit 的“当前筛选导出”动作早于 Machine JSON Protocol，Rust 的首期
`export --asset-id` 只表达整本书导出，不能表达一组 annotation selection。

## 决策

### AppKit 首期必须提供

- 书籍列表：通过 bundled `apple-books-exporter list --json`；
- 标注详情：通过 `annotations --asset-id <id> --json`；
- 整本 Markdown 导出：通过 `export --asset-id <id> --json`；
- Full Disk Access、稳定错误码、schema 不兼容和 binary 启动错误的 GUI 提示；
- 现有 Share Card 编辑、预览和导出能力。

AppKit 不再把 Swift `DatabaseService` 作为默认数据源，Rust 是书籍、标注和整本
导出结果的唯一事实源。

### 保留在 Rust CLI/Skill 的能力

以下能力继续由 Rust CLI 和 Agent Data Skill 提供，首期不在 AppKit 复制：

- `enrich` 与远程 LLM 调用；
- Rust `card`；
- LLM `cache`；
- `config` 与 provider 配置；
- TUI 的导出、AI、卡片和配置写操作。

这不是删除这些能力，也不阻塞 AppKit 首期 cutover。用户仍可通过 CLI 或 Skill
使用它们；网络 AI 仍受 Local Data Boundary 和后续独立决策约束。

### 筛选导出 fallback

AppKit 当前筛选导出暂时保留本地 Markdown fallback，以保持已有 UI 行为；它只写
用户选择的输出目录，不修改 Apple Books，也不代替 Rust 的全书导出事实源。

因此，首期合并可以接受这个迁移期边界，但发布说明不得声称“所有 AppKit 导出
模式都由 Rust CLI 实现”。后续如需统一导出路径，另立协议/功能 ticket，为 Rust
export 增加明确的 annotation selection contract，再删除 fallback。

## 后果

- AppKit 首期范围较小，避免重建 AI、缓存和配置系统；
- Share Card 继续作为 AppKit 本地差异化能力保留；
- 整本读取和导出不再继续扩展 Swift SQLite 查询规则；
- 筛选导出存在已知、文档化的临时双路径，后续需要单独收敛；
- Cutover Gate 的 capability-gap sign-off 已完成，但 Full Disk Access 负向 smoke、
  x86_64 构建和正式签名/notarization 仍是独立开放项。

## 非目标

- 本 ADR 不删除 Tauri；
- 本 ADR 不把 Rust 高级能力静默移除；
- 本 ADR 不授权 AppKit 自动调用远程 AI、上传笔记或修改 Apple Books；
- 本 ADR 不把筛选 fallback 描述为长期架构终态。
