# Cutover Gate 验收清单

- 日期：2026-08-11（**2026-09-30 刷新**）
- 关联：[`CONTEXT.md`](../../CONTEXT.md)、[`ADR 0005`](../adr/0005-headless-mainline-appkit-cutover.md)、[`ADR 0006`](../adr/0006-appkit-initial-capability-boundary.md)、[`Headless Mainline + AppKit Cutover Spec`](2026-08-07-headless-mainline-appkit-cutover-spec.md)
- `main` Headless Mainline 基线：`662e1bb`（#40，AppKit CI 门禁）
- `appkit`：`2bf3ff1`（#41，AppKit 首次获得 CI 门禁覆盖）
- 本记录范围：本机 arm64 macOS（Swift 6.3.3）、当前 Apple Books 数据源、fixture/contract、unsigned 本地 AppKit 包、本机对 x86_64 的交叉编译验证，以及 CI runner 上的构建与测试。CI job 不固定 Swift 版本：首次运行时 `macos-14` 提供的工具链观测为 5.10，下文凡涉及该版本均指这次实际观测值。

## 结论

**Cutover Gate 的代码侧已通过；剩余工作全部是删除 Tauri、发布与签名、以及只能在真实 macOS 上完成的 smoke。**

与 2026-08-11 原始记录相比，两件关键事实已经改变：

1. **AppKit 源码早已在 `main` 上。** 73 个 AppKit 文件由 `8734de3`（#35）引入；`git diff main appkit -- appkit/Sources appkit/Tests appkit/Package.swift appkit/Scripts` 为空，两侧逐字相同。因此 #19 里的「把 `appkit` 合并到 `main`」这一步的实际内容只有 3 个文件（`.gitignore`、`CHANGELOG.md`、`appkit/README.md`），已由 #42 完成。
2. **AppKit 现在有 CI 门禁。** #40 给 main 加了 `appkit` job，#41 把同一 job 和一处 Swift 修复同步到了 `appkit` 分支。这是原始记录中不存在的一层保障——它已经抓到一个真实缺陷，见下。

#19 尚未关闭。剩余工作已拆分为：

- [#43](https://github.com/chenweil/apple-books-export/issues/43) 删除 Tauri Legacy GUI 源码（约 83 个文件）
- [#44](https://github.com/chenweil/apple-books-export/issues/44) 签名、notarization、双架构 AppKit 发布产物
- [#45](https://github.com/chenweil/apple-books-export/issues/45) Full Disk Access 真实负向 smoke 与 x86_64 实际运行

## 验收结果

| Gate 项 | 状态 | 证据与边界 |
| --- | --- | --- |
| Rust Machine JSON contract | ✅ 通过 | `cargo test --all-targets --locked`：**364 passed, 0 failed**（原始记录为 43 项）；`cargo fmt --all -- --check`、`cargo check --all-targets --locked` 通过。#14 增加了 `doctor environment` 预检与 `EXPORT_FILE_EXISTS` 覆盖保护；#29 锁定 speech 机器契约。 |
| Headless mainline 默认路径 | ✅ 通过 | `bash tests/headless_mainline.sh` 通过。#18 移除了 `package.json` 顶层 `dev`/`build`/`preview`/`tauri`，改为显式 `legacy-gui:*` 命名空间，并把该脚本从 76 行扩到 202 行 CI 门禁。回滚锚点 `legacy/tauri-gui-mainline` → `6bac3e5` 由 `git rev-parse` 真解析并断言为 HEAD 祖先。 |
| 真实 Rust 数据路径 | ✅ 通过（本机正向） | 不输出书名、asset ID 或正文：`list --json` schema=1、70 本；首项 `annotations --asset-id` schema=1、1 条且 identity 匹配；临时目录 `export --asset-id` schema=1、receipt identity 匹配；`doctor --json` schema=1、status=ok、两个数据库 readable。 |
| Read-only TUI | ✅ 通过 | `bun test`：**18 pass、46 assertions**（原始记录为 16 pass、39 assertions）；`bun run --cwd tui typecheck` 通过。#15 新增 `scripts/tui-smoke.sh`：独立 tmux socket 起真实 pty 驱动真实 Bun/OpenTUI 与真实 Rust 后端，**28/28 断言通过，`TUI_EXIT=0`**；把 `APPLE_BOOKS_EXPORTER_BIN` 指向记录 argv 的 shim，断言子命令集合恰好是 `{list, annotations}`；并枚举 tmux pane 进程树证明没有 `.app/`/`osascript`/Gatekeeper 启动。 |
| Agent Data Skill | ✅ 通过（仓库副本） | `skills/apple-books-export-rust/tests/contract.sh` 通过（含 runtime validation，需 debug + release 两个二进制）。#16 修正了 SKILL.md 第 4/5 步的真实漏洞：导出器固定为每本书建子目录（`src/exporter.rs:109`），原文的扁平 `*.md` 匹配会误报成功为失败；并补充可选收据字段 `audio_links` / `warnings`。 |
| AppKit Rust bridge | ✅ 通过（本机 arm64 + CI） | `swift build` / `swift test` **47 tests, 0 failures** / `./Scripts/verify-ui.sh` **106 条断言全部通过**。#17 修复了打包脚本的目录无关性。 |
| AppKit CI 门禁 | ✅ 通过 | #40 在 main、#41 在 appkit 分支各新增 `appkit` job（`macos-14`）：`swift build` → `swift test` → `./Scripts/verify-ui.sh`，全部为硬门禁。job 不固定 Swift 版本，只打印 `swift --version`；首次运行时该 runner 提供的是 5.10。**首次运行即抓到 main 上真实的编译断裂**：`BookDetailView.swift:174` 的 `allAnnotations.count { … }` 只在 Swift 6 成立，在该 runner 上报 `cannot call value of non-function type 'Int'`。本机 6.3.3 能编过，所以这个缺陷在零覆盖期间一直隐形。修正改用 `reduce(into:)`（Swift 4 起可用，两工具链均正确）。源码须同时兼容 `Package.swift` 声明的 5.9 与 runner 工具链，这是该 job 存在的理由。 |
| AppKit 真实数据正向 smoke | ✅ 通过（间接） | Rust canonical binary 已通过真实 list/annotations/export/doctor；打包 DMG 后用包内二进制读到 **70 本真实书**，asset_id 匹配，带全部 **7 个 speech 子命令**。未把用户书名、正文或路径写入日志。 |
| Full Disk Access 负向 smoke | ⏳ **待人工 → #45** | Rust fixture/integration、TUI 和 AppKit stable-error tests 已覆盖 `FULL_DISK_ACCESS_REQUIRED`；`sandbox-exec` 模拟 OS 级拒读，真实 CLI exit=1 且 stderr code 正确。**但这不等价于真实 TCC 拒权**：尚未证明 AppKit 会弹出引导并可重试。不得为取证而修改生产环境隐私权限。 |
| arm64 packaging | ✅ 通过（unsigned/local） | unsigned DMG 已生成、挂载成功，`Contents/Resources/apple-books-exporter` 存在且可执行；包内 binary `--help` exit=0。 |
| x86_64 packaging | ⚠️ 本机交叉打包通过，运行待人工 → #45 | `cargo build --release --target x86_64-apple-darwin` 通过并确认为 Mach-O x86_64；AppKit 以 `--triple x86_64-apple-macosx14.0` 构建并打包，DMG 内 AppKit executable 与 bundled Rust binary 均确认为 Mach-O x86_64。**当前验证机为 arm64，该产物从未被启动过。** |
| 签名与 notarization | ⏳ **待人工 → #44** | 当前只有 unsigned/local DMG；无 Developer ID、无 notarization、无干净机 Gatekeeper 证据。`release.yml` grep `appkit`/`swift` **0 命中**——发布流水线目前只产 Rust CLI，不产 AppKit。 |
| capability-gap decision | ✅ 已接受 | [`ADR 0006`](../adr/0006-appkit-initial-capability-boundary.md) 明确：AppKit 保留 Share Card；`enrich`、Rust `card`、`cache`、`config` 继续由 Rust CLI/Skill 提供（已在 `src/main.rs` 核实这些子命令存在于 CLI）。`appkit/README.md` 同时记录了一个必须公开的能力边界：`BookService.swift:42-62` 只在 `annotations.count == book.totalAnnotations` 时调用 Rust exporter，**任何筛选导出都在 Swift 侧生成 Markdown，绕过整个机器契约**（无 receipt、无 `--overwrite` 语义、无稳定错误码、不带 `audio_links`/`warnings`）。 |

## 关闭条件

### 1. 删除 Tauri（#43）

`src-tauri/` 与 Svelte 前端源码仍在 `main` 上，仅供回滚。AppKit 已成为唯一受 CI 门禁覆盖的图形界面后，删除它们是独立提交。

### 2. Release architecture and trust（#44）

在 CI 或对应 Intel Mac 构建、打包并运行 x86_64 AppKit；对 release 产物再次做架构检查；使用 Developer ID 签名、notarize、staple、安装并验证 Gatekeeper；将产物、架构、签名和 notarization 结果写入发布证据。同时统一 Rust CLI（当前 0.3.3）与 AppKit（当前 0.1.8 / build 9）的版本策略。

### 3. 真实 macOS smoke（#45）

Full Disk Access 真实 TCC 负向 smoke，以及 x86_64 产物的实际运行。

## 允许的下一步

1. 执行 #43 删除 Tauri，保留 `legacy/tauri-gui-mainline` 标签作为回滚锚点；
2. 执行 #44 统一版本与发布策略；
3. 在 #44、#45 关闭后关闭 #19。

在此之前，**不能把 unsigned DMG、本机 arm64 smoke 或 CI 通过描述为正式发布完成**。
