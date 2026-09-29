# Apple Books Exporter

从 macOS Apple Books 导出笔记/标注为 Markdown 的 Headless Mainline，默认提供 Rust CLI、Read-only TUI 和 Agent Data Skill。

Tauri Legacy GUI 源码已删除（issue #43）。需要取回时用标签
`legacy/tauri-gui-mainline`，详见根 `README.md` 的回滚锚点一节。

## 技术栈

- **语言**: Rust (edition 2021)
- **CLI**: 单文件二进制,依赖 `clap` + `rusqlite (bundled)` + `reqwest/rustls` + `chrono`
- **TUI**: Bun + OpenTUI (`tui/`)
- **Agent Data Skill**: `skills/apple-books-export-rust/`
- **AppKit GUI**: Swift + AppKit (`appkit/`),main 上的正式图形界面,受 `appkit` CI job 门禁覆盖
- **Tauri Legacy GUI**: Tauri 2 + Svelte,源码已删除(issue #43);回滚锚点 `legacy/tauri-gui-mainline`
- **AI 增强**: 支持多 LLM provider (OpenAI 兼容),通过 `provider.rs` 调用
- **图片卡片**: `card.rs` 生成 Markdown/图片卡片
- **数据源**: Apple Books SQLite 数据库 (`~/Library/Containers/com.apple.iBooksX/`)

## 项目结构

```
src/                    # CLI 核心
├── main.rs             # CLI 入口
├── db.rs               # SQLite 数据访问
├── cfi.rs              # EPUB CFI 解析
├── exporter.rs         # Markdown 导出
├── card.rs             # 卡片生成
├── chapter.rs          # 章节解析
├── config.rs           # 配置管理
├── cache.rs            # LLM 缓存
├── provider.rs         # LLM API 调用
├── models.rs           # 数据结构
└── utils.rs            # 工具函数

src-tauri/ 与 Svelte 前端  # Tauri Legacy GUI,已删除(issue #43)
                              # 回滚锚点:标签 legacy/tauri-gui-mainline -> 6bac3e5

appkit/                   # AppKit GUI (Swift, main 上的正式图形界面)
├── Sources/BooksExporter/
├── Tests/BooksExporterCoreTests/
└── Scripts/verify-ui.sh  # UI 回归探针

skills/
└── apple-books-export-rust/   # 可安装的 Skill (Claude/Gemini CLI)
    ├── SKILL.md
    └── scripts/
        ├── apple-books-exporter        # 平台无关默认二进制
        ├── apple-books-exporter-aarch64-apple-darwin
        ├── apple-books-exporter-x86_64-apple-darwin
        ├── validate.sh                  # binary/架构/协议能力预检
        └── install.sh / build.sh
```

## 开发命令

```bash
# CLI 构建
cargo build --release
./target/release/apple-books-exporter list

# Read-only TUI
bun install --cwd tui
bun run --cwd tui start

# Agent Data Skill
./skills/apple-books-export-rust/scripts/install.sh

# 跨平台编译(本地)
./skills/apple-books-export-rust/scripts/build.sh

# AppKit GUI 构建
cd appkit && swift build && swift test && ./Scripts/verify-ui.sh

```

## 系统要求

- macOS only(Apple Books 数据仅存在于 macOS)
- Full Disk Access 权限(系统设置 → 隐私与安全性)
- Rust stable toolchain
- Bun(仅 Read-only TUI 开发需要)
- Swift 5.9+(仅 AppKit GUI 开发需要)

仓库根已无 `package.json`:Node.js 不再是任何入口的前置依赖。

## 笔记分类

**不要用 `ZANNOTATIONTYPE` 判断笔记类型。** 该字段与内容并不对应:实测本地库
type 1 有 105 条、type 3 有 379 条,`ZANNOTATIONSELECTEDTEXT` /
`ZANNOTATIONNOTE` / `ZANNOTATIONREPRESENTATIVETEXT` 三个字段全为空。它们不是
书签(全部带高亮样式、近半数带选区范围),而是取词失败的高亮。真正的书签
(type 0)本地库一条都没有,且 `ZAEANNOTATION` 是唯一的标注表。

按**内容**分类:

| 条件 | 含义 |
|------|------|
| `ZANNOTATIONNOTE` 非空 | 笔记(即给高亮加的批注,原文在 `ZANNOTATIONSELECTEDTEXT`) |
| 仅 `ZANNOTATIONSELECTEDTEXT` 非空 | 高亮 |
| 两者皆空 | 空壳,导出与计数都应跳过 |

Apple Books 不把「笔记」存成独立对象 —— 笔记就是给高亮加批注,两者同在一行。

计数口径必须与导出口径一致,否则列表显示 307 条、导出只有 302 条
(见 `db::tests::count_excludes_annotations_without_text_or_note`)。

## 时间戳

Apple CoreData 时间戳从 **2001-01-01 UTC** 开始(`APPLE_EPOCH`),转换时需要偏移。

## 发布

推送 `v*` tag 时,GitHub Actions (`.github/workflows/release.yml`) 自动:
- **先校验 tag 与 `Cargo.toml` 的 `version` 一致**,不一致直接失败(在任何构建之前)
- 并行构建 macOS arm64 / Intel CLI binary
- 生成 SHA256SUMS
- 创建/更新 GitHub Release

```bash
# tag 版本号必须等于 Cargo.toml 的 version
git tag v0.3.3 && git push --tags
```

校验逻辑在 `scripts/check-release-tag.sh`,发布流水线与 `tests/headless_mainline.sh`
共用同一份实现。历史上 tag 曾只用于命名 Release 而不与 `Cargo.toml` 比对,导致可以
发布出一个页面写着 v0.4.0、而二进制 `--version` 报 0.3.3 的产物且无人报错。

## 分支约定

- `main` — 当前 Rust 版本,稳定
- `rust-version` — 下一个 Rust 版本开发
- `appkit` — AppKit GUI 的历史分支线;其源码已合入 `main`(见 #35、#42),保留用于追溯;早先的 `swiftui` 分支已删除,其全部历史都在这条线上
- `python-legacy` — 已废弃的 Python 实现(归档)

## 约束

- **不使用虚拟环境**(Python 时代已废止)
- **CLI 二进制静态链接 SQLite**(`rusqlite` bundled feature)
- **发布二进制不要 commit 单独的平台副本到仓库根** — 走 GitHub Release
- **`*.md` 在 .gitignore**(除 README.md / CLAUDE.md / AGENTS.md / .gitignore / docs/**)

## 常见问题

- **Full Disk Access 缺失** → 数据库读不到,提示权限错误
- **AppKit 启动失败** → 检查 Full Disk Access,并确认随包 Rust binary 可解析
- **章节显示为原始 CFI** → 确认用 `cfi.rs` 里的解析函数,不要直接读 `ZANNOTATIONLOCATION`

## Agent skills

### Issue tracker

GitHub Issues via `gh` CLI (PRs are not a triage surface). See `docs/agents/issue-tracker.md`.

### Triage labels

Five canonical English strings (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: repo-root `CONTEXT.md` + `docs/adr/`. See `docs/agents/domain.md`.
