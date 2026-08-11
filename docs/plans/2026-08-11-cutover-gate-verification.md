# Cutover Gate 验收清单

- 日期：2026-08-11
- 关联：[`CONTEXT.md`](../../CONTEXT.md)、[`ADR 0005`](../adr/0005-headless-mainline-appkit-cutover.md)、[`Headless Mainline + AppKit Cutover Spec`](2026-08-07-headless-mainline-appkit-cutover-spec.md)
- `main`：`a811b29`（与 `origin/main` 同步）
- `appkit`：`9623496`（与 `origin/appkit` 同步）
- 本记录范围：本机 arm64 macOS、当前 Apple Books 数据源、fixture/contract、unsigned 本地 AppKit 包

## 结论

**Cutover Gate 当前为条件通过，尚不能执行最终合并和删除 Tauri。**

Headless Mainline、Rust machine protocol、TUI、Agent Data Skill 和 AppKit bundled
Rust CLI bridge 的本机证据已经完成。仍有三类不能由本次本机自动验证替代的事项：

1. 实际撤销 Full Disk Access 后的 AppKit 负向 smoke；
2. x86_64 AppKit/Rust 产物以及正式签名/notarization；
3. 对 AppKit 当前筛选导出 fallback 和高级 Rust 能力缺口的最终发布决策。

在这些事项关闭前，保留 `src-tauri/`，不要合并 `appkit`，不要删除 Tauri。

## 验收结果

| Gate 项 | 状态 | 证据与边界 |
| --- | --- | --- |
| Rust Machine JSON contract | ✅ 通过 | `cargo test`：43 项通过（27 library + 1 main + 15 integration）；`cargo build --release`、`cargo check --all-targets` 通过；machine CLI contract tests 覆盖 schema、`asset_id`、错误码、receipt、覆盖保护和权限预检。 |
| Headless mainline 默认路径 | ✅ 通过 | `bash tests/headless_mainline.sh` 通过；`main` README、release workflow 和默认构建路径不再把 Tauri GUI 作为入口，Tauri 源码仍保留。 |
| 真实 Rust 数据路径 | ✅ 通过（本机正向） | 不输出书名、asset ID 或正文，仅记录：`list --json` schema=1、70 本；首项 `annotations --asset-id` schema=1、1 条且 identity 匹配；临时目录 `export --asset-id` schema=1、receipt identity 匹配、1 个非空 Markdown 且路径在目标目录内；`doctor --json` schema=1、status=ok、两个数据库 readable。 |
| Read-only TUI | ✅ 通过 | `bun test`：16 pass、39 assertions；`bun run --cwd tui typecheck` 通过；PTY 真实启动后发送 `q`，exit=0，渲染输出全部丢弃。TUI 不提供导出、AI、卡片、缓存或配置写操作。 |
| Agent Data Skill | ✅ 通过（仓库副本） | `skills/apple-books-export-rust/tests/contract.sh` 通过；validator 解析当前 release binary 并通过 Mach-O/arm64/`--help` 校验；真实 list/annotations/export 走相同 machine protocol 并完成非空 Markdown 检查。未把安装到用户目录的副本当作额外生产证据。 |
| AppKit Rust bridge | ✅ 通过（本机 arm64） | 对 `origin/appkit@9623496` 建临时 worktree：`swift test` 47 项通过、release build、`verify-ui.sh`、DMG 打包和包内 CLI `--help` 均通过；从挂载的 DMG 直接启动 AppKit，3 秒后以 SIGTERM 结束，证明 bundled resource 可解析。AppKit 的 stdout/stderr/exit code、schema、stable error 和权限提示有 seam tests。 |
| AppKit 真实数据正向 smoke | ✅ 通过（间接） | Rust canonical binary 已通过真实 list/annotations/export/doctor；挂载 DMG 的 AppKit 启动未输出用户数据。未把用户书名、正文或路径写入日志。 |
| Full Disk Access 负向 smoke | ⏳ 待人工 | Rust fixture/integration、TUI 和 AppKit stable-error tests 已覆盖 `FULL_DISK_ACCESS_REQUIRED`；本次没有自动修改系统隐私权限，因此尚未证明“真实 AppKit 在被拒权时弹出引导并可重试”。 |
| arm64 packaging | ✅ 通过（unsigned/local） | unsigned DMG 已生成、挂载成功，`Contents/Resources/apple-books-exporter` 存在且可执行；包内 binary `--help` exit=0。 |
| x86_64 packaging | ⏳ 待 CI/人工 | 当前本机为 arm64，仅确认 release workflow 声明 `aarch64-apple-darwin` 与 `x86_64-apple-darwin` CLI 构建矩阵；没有本机 x86_64 AppKit + bundled binary 产物证据。 |
| 签名与 notarization | ⏳ 待发布流程 | 当前验证的是 unsigned/local DMG；没有 Developer ID、notarization、干净机器安装和 Gatekeeper 证据。 |
| capability-gap decision | ⚠️ 已记录，需最终确认 | AppKit 首期保留 Share Card；`enrich`、Rust `card`、`cache`、`config` 继续由 Rust CLI/Skill 提供，不阻塞首期 GUI；当前筛选导出因 Rust export contract 只接受整本 `asset_id`，仍使用 AppKit 本地 Markdown fallback，这个边界需在正式合并前明确接受或另立 ticket。 |

## 关闭条件

### 1. Full Disk Access 负向 smoke

在可恢复的测试环境中让 AppKit/Rust CLI 无法读取 Apple Books 数据，确认：

- stderr 是 schema=1 的结构化 JSON；
- code 为 `FULL_DISK_ACCESS_REQUIRED`；
- AppKit 状态显示 code、message 和 Full Disk Access remediation；
- 恢复权限后重试可以重新读取列表。

不要为了本清单修改当前用户生产环境的隐私权限；应记录为真实 macOS smoke，而不是把 fixture 当作等价证据。

### 2. Release architecture and trust

- 在 CI 或对应 Intel Mac 构建并验证 x86_64 Rust binary；
- 对 AppKit 和 bundled binary 分别执行目标架构检查；
- 使用 Developer ID 签名、notarize、安装并验证 Gatekeeper；
- 将产物、架构、签名和 notarization 结果写入发布证据。

### 3. Capability sign-off

在合并前明确接受以下首期边界：

- AppKit 的 canonical 数据读取和整本 Markdown export 走 Rust CLI；
- Share Card 继续是 AppKit 本地能力；
- AI enrich、Rust card、cache、config 不复制到 AppKit；
- 筛选导出 fallback 是暂时兼容路径，后续是否扩展 Rust selection export 另立需求。

## 允许的下一步

关闭上述人工/发布项后，按 ADR 0005 执行：

1. 将 `origin/appkit@9623496` 合并到 `main`；
2. 在独立提交中删除 Tauri Legacy GUI 源码和依赖；
3. 重新统一版本、发布源、架构产物和安装证据。

在此之前，本清单的状态保持为 **条件通过**，不能把 unsigned DMG 或本机 arm64 smoke 描述为正式发布完成。
