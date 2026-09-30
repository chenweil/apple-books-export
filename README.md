# Apple Books 笔记导出工具

Rust 版本 Apple Books 笔记导出工具，提供 CLI、只读 TUI 和 Agent Data Skill，可导出笔记、高亮、书签为 Markdown 文件，并支持 AI 增强和图片卡片生成。

当前 `main` 是 Headless Mainline：默认入口是 Rust CLI、Read-only TUI 和
Agent Data Skill，图形界面是 AppKit。Tauri Legacy GUI 源码已删除。

## 功能

- 📚 列出 Apple Books 书库中所有做过笔记的书籍
- 📝 导出笔记、高亮、书签为 Markdown
- 🔍 按书名搜索导出（模糊匹配）
- 🤖 AI 增强：为笔记添加解释、标签、复习问题
- 🎴 图片卡片：生成精美的知识卡片
- 🧭 Headless Mainline：CLI、只读 TUI 和 Agent Data Skill
- ⌨️ TUI 支持：OpenTUI 只读搜索和浏览书籍
- 🤖 AI Agent Skill：支持 AI 助手直接调用

## 系统要求

- macOS（Apple Books 数据仅存在于 macOS）
- Full Disk Access 权限

## 快速开始

### 方式一：Rust CLI

```bash
# 编译
cargo build --release

# 读取书籍列表
./target/release/apple-books-exporter list
./target/release/apple-books-exporter doctor
```

### Machine JSON 协议（供 TUI / Agent Skill / GUI 消费）

GUI 与自动化入口应消费 `--json` 输出，不要解析人类可读表格。

```bash
# 一次性环境预检：binary、两个 Apple Books 数据库、HOME、输出目录、可用空间
./target/release/apple-books-exporter doctor --json

# 用稳定 asset_id 查询，而不是显示序号
./target/release/apple-books-exporter list --json
./target/release/apple-books-exporter annotations --asset-id <asset_id> --json
./target/release/apple-books-exporter export --asset-id <asset_id> --format markdown --json
```

约定：

- 成功 JSON 走 **stdout** 并以 0 退出；结构化错误走 **stderr** 并以非 0 退出。
- 每个响应都带 `schema_version`。
- 身份用 `asset_id`，不要从人类输出里取序号。
- `list`、`annotations`、`export` **不做任何网络请求**（Local Data Boundary），
  由 `tests/machine_cli.rs::read_only_machine_commands_make_no_network_requests`
  在运行时用连接计数证明。
- `doctor --json` 的 `environment` 块用于一次性发现环境问题：

```json
"environment": {
  "home": { "status": "ok", "path": "/Users/you" },
  "default_output_dir": { "status": "missing", "path": "/Users/you/books-exported", "writable": true },
  "free_bytes": 72806043648
}
```

`default_output_dir.status` 为 `missing` 表示尚未创建（全新机器的正常状态，
此时 `writable` 描述的是**将要接收它的父目录**），为 `unwritable` 才需要
用户处理。`doctor` 只报告，不创建也不修复任何东西。

真机冒烟记录见 [`docs/evidence/2026-09-29-machine-protocol-smoke.md`](docs/evidence/2026-09-29-machine-protocol-smoke.md)。

### 方式二：Read-only TUI

```bash
cargo build
bun install --cwd tui
bun run --cwd tui start
```

TUI 支持搜索书籍、打开详情，并查看高亮正文、笔记、章节、位置和时间；
不执行导出、AI、卡片或配置写操作。验证命令：

```bash
bun run --cwd tui test
bun run --cwd tui typecheck
```

### 方式三：Agent Data Skill

```bash
# 编译并安装 Skill
cargo build --release
cd skills/apple-books-export-rust/scripts
./install.sh

# 验证安装并读取机器 JSON
~/.agents/skills/apple-books-export-rust/scripts/validate.sh --print-path
~/.agents/skills/apple-books-export-rust/scripts/apple-books-exporter list --json
```

Skill 会刷新 `list --json`，通过 `asset_id` 读取标注或导出 Markdown，并
验证生成的非空文件。它不解析人类表格、不修改 Apple Books、不自动下载
binary，也不调用 AI。

### 方式四：AppKit GUI（从 Release 安装）

正式图形界面只发布 **arm64** 的 DMG，文件名为 `Books-Exporter-<version>-unsigned.dmg`，
见 [Releases](https://github.com/chenweil/apple-books-export/releases)：

```bash
VERSION=0.3.4   # 换成 Release 里的实际版本号
DMG=~/Downloads/Books-Exporter-${VERSION}-unsigned.dmg

# 1. 挂载并拷入 Applications
hdiutil attach "$DMG"
cp -R "/Volumes/Books Exporter/Books Exporter.app" /Applications/
hdiutil detach "/Volumes/Books Exporter"

# 2. 未签名，Gatekeeper 会拦：手动移除 quarantine
xattr -dr com.apple.quarantine "/Applications/Books Exporter.app"

# 3. 授予 Full Disk Access
#    系统设置 → 隐私与安全性 → Full Disk Access → 加入 Books Exporter.app
#    之后完全退出并重开应用
```

第 2 步是**既定代价，不是变通方案**。本仓库没有 Apple Developer Program 身份，
拿不到 Developer ID 证书也没有 notary team，因此不签名；相应地，本仓库也不把
移除 quarantine 当作「已签名」的发布证据——`tests/headless_mainline.sh` 会拒绝任何
声称已签名但实际无凭据的步骤。跟踪 issue [#44](https://github.com/chenweil/apple-books-export/issues/44)。

App 内置的 Rust CLI 随包分发在 `Contents/Resources`，不需要另外编译；排查问题时
可以用 `APPLE_BOOKS_EXPORTER_BIN` 指向别的二进制。

Intel Mac 目前没有可安装的 GUI 产物，只有 CLI 的 x86_64 版本，见
[#45](https://github.com/chenweil/apple-books-export/issues/45)。

## Tauri Legacy GUI（已移除）

Tauri GUI 的源码（`src-tauri/`、Svelte 前端、root `package.json` 及其构建配置）已从
`main` 删除。AppKit 取代它成为本仓库唯一的图形界面，并受 `appkit` CI job 门禁覆盖
（见 [`appkit/README.md`](appkit/README.md)）。删除动作见 issue #43。

仓库不再需要 Node.js：`package.json` 已随前端一起移除，Headless Mainline 的三条入口
都是原生工具链——`cargo`（CLI）、`bun`（TUI）、`swift`（AppKit）。

**回滚锚点**：Tauri GUI 仍是默认入口的最后一个 `main` 提交是 `6bac3e5`
（`feat: harden Apple Books agent data skill`），已打标签
`legacy/tauri-gui-mainline`。需要取回当时的完整状态时：

```bash
git checkout legacy/tauri-gui-mainline
```

该标签下 `package.json` 仍有 `dev` / `build` / `preview` / `tauri` 四个默认脚本，
`release.yml` 仍会发布 GUI 产物。要在当前代码上取回**源码**（而非整体回退），用：

```bash
git checkout legacy/tauri-gui-mainline -- src-tauri package.json package-lock.json \
  svelte.config.js vite.config.ts tsconfig.json
git checkout legacy/tauri-gui-mainline -- 'src/*.svelte' 'src/lib' 'src/main.ts' \
  'src/app.css' 'src/index.html'
```

`tests/headless_mainline.sh` 会持续验证该标签仍能解析到 `6bac3e5`、是 `HEAD` 的
祖先，且仍携带上面这些路径——因此回滚能力不会在无人察觉的情况下失效。

## AppKit GUI

正式图形界面是同一仓库内的 AppKit 应用，位于 [`appkit/`](appkit/README.md)。它通过
Rust CLI 的 machine JSON 协议读取 Apple Books 数据，不实现自己的数据库规则。

```bash
cd appkit
swift build               # 编译
swift test                # 47 项 XCTest
./Scripts/verify-ui.sh    # 106 条 UI 断言回归探针
```

`appkit` CI job（`macos-14`）在每次 push 到 `main` 以及每个 PR 上执行上面三条命令。
该 job 不固定 Swift 版本，而是打印 runner 自带的 `swift --version`；源码须同时兼容
`appkit/Package.swift` 声明的 5.9 与 runner 当时提供的工具链。

运行需要先构建 canonical Rust CLI，并用 `APPLE_BOOKS_EXPORTER_BIN` 指向它
（解析优先级：`APPLE_BOOKS_EXPORTER_BIN` 环境变量 → App 包内
`Contents/Resources` → 可执行文件同级 → `PATH`）。二进制解析、能力边界和已知
限制详见 [`appkit/README.md`](appkit/README.md)。

## 发布

版本号只有一个来源：`Cargo.toml` 的 `version`。推送 `v*` tag 会触发
[`.github/workflows/release.yml`](.github/workflows/release.yml)：

```bash
# tag 版本号必须与 Cargo.toml 的 version 完全一致
git tag v0.3.4 && git push --tags
```

流水线**在任何构建之前**先校验 tag 与 `Cargo.toml` 是否一致，不一致直接失败。
这条检查是补上的：此前 tag 只用于命名 GitHub Release，从不与 `Cargo.toml` 比对，
因此 `git tag v0.4.0` 会发布出一个页面写着 v0.4.0、而二进制 `--version` 报
`Cargo.toml` 里那个旧版本号的产物，且全流程无任何报错。校验逻辑在
`scripts/check-release-tag.sh`，由发布流水线和 `tests/headless_mainline.sh`
共用，避免两处实现漂移。

### 发布产物

| 产物 | 架构 | 说明 |
|---|---|---|
| `apple-books-exporter-<arch>` | arm64 / x86_64 | Rust CLI，驱动 TUI、Skill 和 AppKit |
| `Books-Exporter-<version>-unsigned.dmg` | arm64 | AppKit GUI，内含对应架构的 Rust CLI |

AppKit 的 `CFBundleShortVersionString` 从同一个 tag 注入，`CFBundleVersion` 取
CI run number（bundle build 号需要随每次构建递增，不再手工维护）。流水线会挂载产出的
DMG，回读 `Info.plist` 确认版本与 tag 一致后才继续——不信任打包脚本的退出码。

**AppKit DMG 未签名、未 notarization。** 本仓库没有 Developer ID 身份，
`xattr -d com.apple.quarantine` 之类的用户侧绕过手段不构成发布证据。`tests/headless_mainline.sh`
会拒绝任何声称已签名但实际无凭据的步骤。跟踪 issue #44。

**x86_64 的 AppKit DMG 尚未产出**——目前只有 CLI 的 x86_64 版本。AppKit 需要 Intel
runner 或交叉编译，且产物未经运行验证，归入 issue #45。

## Headless 能力矩阵

**没有任何功能只存在于已废弃的 Tauri GUI 里**：下面每一项都能在 Headless Mainline
上用 Rust CLI 完成。下表说明每个命令在无 GUI 环境下的边界。

| 命令 | Headless 可用 | 联网 | 产生费用 | 写入用户文件 |
| --- | --- | --- | --- | --- |
| `list`、`annotations` | 是 | 否 | 否 | 否 |
| `doctor` | 是 | 否 | 否 | 否（只报告） |
| `export` | 是 | 否 | 否 | 是（你选择的输出目录） |
| `enrich` | 是 | 是（你配置的 LLM provider） | **是** | 是（`llm_cache.json` + 输出目录） |
| `card` | 是 | 否 | 否 | 是（图片写入输出目录） |
| `config` | 是 | 否 | 否 | 是（配置文件） |
| `cache <book>` | 是 | 否 | 否 | 否（只读状态） |
| `speech profile show` | 是 | 否 | 否 | 否 |
| `speech profile set` / `reset` | 是 | 否 | 否 | 是（speech 配置） |
| `speech voices` | 是 | 是（仅音色目录） | 否 | 是（本地音色目录缓存） |
| `speech generate` | 是 | 是 | **是** | 是（仅应用缓存目录） |
| `speech play`（默认人类模式） | 是 | 否 | 否 | 是（仅应用缓存目录：clip 使用时间与播放锁标记） |
| `speech play --json` | 是 | 否 | 否 | 否（只返回已校验路径与来源） |
| `speech export` | 是 | 否 | 否 | 是（你选择的导出目录） |
| `speech cache status` | 是 | 否 | 否 | 否（只读） |
| `speech cache clear` | 是 | 否 | 否 | 是（应用缓存目录） |
| `speech history clear` | 是 | 否 | 否 | 是（attempt 元数据） |

「写入用户文件」指写入你可见的文件系统位置，**包含应用自己的状态目录**
（`~/Library/Application Support/books-exporter/`）。几个容易被误判为只读的命令：

- `speech play` 默认（人类）模式会刷新该 clip 的使用时间并落一个播放锁标记，
  这是刻意保留的行为；只有 `--json` 形式完全不留痕迹。
- `speech voices` 会把音色目录缓存到本地磁盘，即使你只"看一眼"。
- `speech profile show` 与 `speech cache status` 才是真正不写任何文件的形式。

`list`、`annotations`、`export` 的「不联网」由
`tests/machine_cli.rs::read_only_machine_commands_make_no_network_requests` 在运行时
用连接计数证明（该测试只覆盖这三个命令）。`doctor` 不联网是因为它只读数据库和
文件系统、代码路径里没有 HTTP 客户端，但没有同样的连接计数测试。语音命令族的
联网/计费边界见下文「四类语音操作的边界」。

## CLI 命令

### 列出书籍

```bash
apple-books-exporter list

# 稳定的机器可读协议（供 TUI 等客户端使用）
apple-books-exporter list --json
```

### 导出笔记

```bash
# 按序号导出
apple-books-exporter export 1

# 按书名搜索导出
apple-books-exporter export -t "纳瓦尔"

# 指定输出目录
apple-books-exporter export 1 -o ~/Desktop

# 指定格式
apple-books-exporter export 1 --format obsidian
```

### AI 增强笔记

需要先配置 LLM API：

```bash
# 配置
apple-books-exporter config --api-key "sk-xxx" --model "gpt-4o-mini"

# 处理单条笔记
apple-books-exporter enrich 1 --index 42

# 处理整本书
apple-books-exporter enrich 1 --all

# 强制重新生成
apple-books-exporter enrich 1 --all --force
```

### 生成图片卡片

```bash
# 批量生成
apple-books-exporter card 1 --all

# 指定样式
apple-books-exporter card 1 --all --style dark  # dark/light/minimal
```

### 语音 Voice Profile

管理全局 Voice Profile。所有命令都是纯本地操作：不联网，不读写 Apple Books 数据。

```bash
# 查看当前 Voice Profile
apple-books-exporter speech profile show

# 设置（--voice-id 必填，精确匹配音色 ID）
apple-books-exporter speech profile set --voice-id male_0004_a --speed 1.0 --volume 1.0 --pitch 0

# 恢复默认值（默认音色 male_0004_a，speed 1.0，volume 1.0，pitch 0，MP3/32000Hz）
apple-books-exporter speech profile reset

# 机器可读 JSON（成功只写 stdout，失败只写 stderr）
apple-books-exporter speech profile show --json
```

- 配置文件位于 `~/Library/Application Support/books-exporter/speech/config.json`，与当前
  工作目录、`--config` 和导出目录无关；
- 该文件**只保存非秘密配置**和 API Key 的**环境变量名**（默认 `SENSEAUDIO_API_KEY`）。
  密钥值只从环境变量读取，永远不落盘；
- `speed` 取值 `0.5`–`2.0`，`volume` 取值 `0.01`–`10.0`，两者最多两位小数且不做四舍五入；
  `pitch` 为 `-12`–`12` 的整数。非法、非有限、需要舍入或越界的值都返回稳定错误
  `SPEECH_PROFILE_INVALID`（JSON 模式下带 `details.field` / `details.reason`）；
- 无法核对当前 Voice Catalog 时，会保存为 Unverified Voice Profile
  （`verification_status: "unverified"`）并返回结构化 warning
  （`no_catalog` / `stale_catalog` / `other_provider`）。Unverified 只表示本地配置合法，
  **不代表**当前账号可用，也不能授权语音生成。

### 语音 Voice Catalog

列出当前账号可用的 SenseAudio 音色。这是 `speech` 命令族里唯一会联网的浏览命令，
`profile` 命令仍然是纯本地操作。

```bash
# 默认复用 24 小时缓存；缓存过期时自动刷新
apple-books-exporter speech voices

# 强制刷新，绕过缓存
apple-books-exporter speech voices --refresh

# 机器可读 JSON（成功只写 stdout，失败只写 stderr）
apple-books-exporter speech voices --json
```

- 目录缓存在 `~/Library/Application Support/books-exporter/speech/voices/senseaudio.json`；
  24 小时内直接复用，`--refresh` 强制绕过缓存；
- 刷新失败且存在旧目录时，人类输出和 JSON 都会标记 `stale=true`、保留 `fetched_at`，
  并返回 `stale_catalog` warning。旧目录**不是**当前账号权限的保证，不能授权语音生成；
- 音色 ID 与情感/风格标签都来自供应商响应，不从 `voice_id` 后缀推断，也不生成目录里
  不存在的标签组合；
- 本地缓存文件损坏或 schema 不匹配不会阻断 `--refresh`：刷新成功后原子替换掉损坏文档。

### 语音生成（唯一会产生费用的操作）

把一条 Annotation 的高亮或个人笔记合成为一个 Speech Clip。**这是整个 `speech` 命令族里
唯一会向外部供应商发送内容并产生费用的动作**，只有你显式执行它才会发生。

```bash
# 人类形式：用书籍和 Annotation 的显示序号
apple-books-exporter speech generate 1 --annotation 1 --content highlight

# 机器形式：用稳定 ID（可脚本化）
apple-books-exporter speech generate \
  --asset-id BOOK_ID --annotation-id ANNOTATION_ID --content note --json
```

- 机器形式必须显式给出 `asset_id`、`annotation_id` 和 `content_kind`
  （`highlight` 或 `note`），且不接受显示序号；人类形式反过来不接受 `--asset-id`。
  两组参数混用返回稳定错误 `INVALID_ARGUMENT`，不会忽略其中一组；
- 一条 Speech Clip 只朗读**一个**内容部分：同一条 Annotation 的高亮与笔记是两个独立
  clip，需要分别生成、分别计费；
- 顺序固定为：命中有效缓存 → 尝试从你已导出的音频恢复（0 次 provider 调用）→ 才会
  请求供应商。缓存命中与恢复都不产生费用；
- `--regenerate` 是唯一可以越过 unknown 结果、替换已有有效音频的入口。普通 `generate`
  在结果不确定时**不会**自动重放，因为重复请求可能重复计费；
- 人类输出只显示内容类型、音色与字符估算，不回显完整原文。

### 语音播放（纯本地）

播放一个已经通过校验的 Speech Clip。**播放永远不联系 Speech Provider，也不会重新生成**
——播放失败不代表重新生成。

```bash
apple-books-exporter speech play --clip-id CLIP_ID
apple-books-exporter speech play --clip-id CLIP_ID --json
```

- 查找顺序：本地缓存 → 显式 `--export-root` → 已验证的导出定位记录；
- `--json` 只返回已校验的路径与来源，**不启动播放器**、不产生声音；
- 缓存与导出音频都找不到时返回 `SPEECH_CLIP_NOT_FOUND`；
- 你自己改过导出音频导致校验不匹配时，它不再作为该 clip 的播放回退，但仍然可以自己用
  系统播放器打开这个普通文件。

### 语音导出（把缓存里的音频交给用户）

把一个已校验的 Cached Speech Clip 原子复制到这本书的导出目录。**不联网，也不修改已有
Markdown**——音频链接由下一次常规导出统一写入。

```bash
apple-books-exporter speech export \
  --clip-id CLIP_ID --output BOOK_EXPORT_DIRECTORY [--overwrite] [--json]
```

- `--output` 指向该书的导出根目录，不是 `assets/audio/` 本身；
- 导出后音频归你所有，不再受缓存 LRU 淘汰影响，`speech cache clear` 不会删除它；
- 同一路径内容相同时直接复用；内容不同默认返回 `SPEECH_OUTPUT_FILE_EXISTS`，
  只有显式 `--overwrite` 才替换；
- 同一 Annotation 的内容部分可以保留多个导出变体，但只有一个是 active；新导出的成为
  active，旧文件仍然保留；
- 导出清单损坏时返回 `SPEECH_EXPORT_MANIFEST_INVALID`，既不覆盖也不按文件名猜测重建。

### 四类语音操作的边界

| 操作 | 是否联网 | 是否产生费用 | 是否可能修改用户文件 |
| --- | --- | --- | --- |
| `profile show`、`cache status` | 否 | 否 | 否（只读） |
| `profile set/reset`、`cache clear`、`history clear` | 否 | 否 | 只改本地应用状态 |
| `voices` / `voices --refresh` | 是（仅音色目录） | 否 | 写入本地音色目录缓存 |
| `generate`（缓存未命中且无可恢复导出时） | 是 | **是** | 只写应用缓存目录 |
| `play`（默认人类模式） | 否 | 否 | 只改本地应用状态（clip 使用时间、播放锁标记） |
| `play --json`、`export` | 否 | 否 | `play --json` 否；`export` 写入你选择的导出目录 |

本表与上文「Headless 能力矩阵」对同一批命令的结论必须一致；如出现分歧，以能力
矩阵为准。

浏览、读取标注、播放、常规 Markdown/Obsidian 导出和 Agent Data Skill **都不会**触发
远程生成。

### 语音 Cache 与 Attempt History

维护本地 Cached Speech Clip 与 Speech Attempt History。两者都是纯本地操作：不联网，
不调用 Speech Provider。

```bash
# 只读视图：预算、占用、已接受/无音频/阻塞/损坏/占用中 entry 与可回收孤立 version
apple-books-exporter speech cache status
apple-books-exporter speech cache status --json

# 删除可淘汰 Cached Speech Clip，并显式清除没有有效音频的阻塞门
apple-books-exporter speech cache clear
apple-books-exporter speech cache clear --json

# 只删除 attempt history
apple-books-exporter speech history clear
apple-books-exporter speech history clear --json
```

- 默认预算 1 GiB（`config.json` 的非秘密字段 `cache_budget_bytes` 可调），其中必须保留
  128 MiB 安全余量，因此真正可给缓存内容使用的是 `budget - 128 MiB`；
- `cache status` 不删除、不改写任何东西，也不调用 provider：损坏 entry 与未被 current
  pointer 引用的孤立 version 只被报告；
- LRU 与 `cache clear` 都不淘汰当前 clip，也不淘汰正在生成（跨进程 writer 锁）、播放或
  导出（usage marker）的 entry；`cache clear` 的 receipt 用 `skipped_reasons` 说明每个
  跳过 entry 的占用原因；
- `history clear` 只删 attempt metadata：不动缓存、不动 unknown gate、不动导出状态，也不删
  用户自己的音频；超过 90 天的 attempt metadata 由正常维护自动删除，与音频 LRU 相互独立；
- 根目录不可写、可用空间保不住安全余量或没有可淘汰空间时，`generate` 在 provider 调用
  **之前**返回 `SPEECH_STORAGE_UNAVAILABLE`。运维可以用
  `APPLE_BOOKS_SPEECH_MIN_FREE_BYTES` 调高要求的空闲空间（只能调高，不能削弱保护）。

## AI Agent Skill

本项目提供 skill，支持 AI 助手直接调用。

### Skill 文件

```
skills/apple-books-export-rust/
├── SKILL.md                              # Skill 文档
└── scripts/
    ├── apple-books-exporter              # 默认二进制
    ├── apple-books-exporter-aarch64-apple-darwin  # macOS ARM
    ├── apple-books-exporter-x86_64-apple-darwin   # macOS Intel
    ├── validate.sh                        # binary/架构/协议能力校验
    ├── build.sh                          # 编译脚本
    └── install.sh                        # 安装脚本
```

Skill 使用 Rust Machine JSON Protocol：执行前校验 binary 存在、macOS CPU
架构和 `--help` 能力；刷新 `list --json` 后通过 `asset_id` 调用
`annotations --asset-id ... --json` 与 `export --asset-id ... --json`，并在报告成功前验证非空 Markdown 文件。不会解析人类表格、自动下载 binary、修改 Apple Books 或调用 AI。

### 编译二进制（必需）

Skill 需要二进制文件才能读取 Apple Books 数据。首次使用前必须编译：

```bash
# 方式一：使用编译脚本
cd skills/apple-books-export-rust/scripts
./build.sh

# 方式二：手动编译
cargo build --release
cp target/release/apple-books-exporter skills/apple-books-export-rust/scripts/
```

### 安装 Skill

```bash
cd skills/apple-books-export-rust/scripts
./install.sh
```

安装脚本会：
1. 检测当前系统平台
2. 复制二进制文件到 `~/.agents/skills/apple-books-export-rust/scripts/`
3. 复制 SKILL.md 和 validator
4. 验证 binary 架构及 `list`、`annotations`、`export`、`doctor` 命令

### 跨平台编译

```bash
# macOS ARM (M1/M2/M3)
cargo build --release --target aarch64-apple-darwin

# macOS Intel
cargo build --release --target x86_64-apple-darwin

# Linux x86_64
cargo build --release --target x86_64-unknown-linux-gnu
```

编译后复制到 scripts 目录并重命名：
```bash
cp target/<target>/release/apple-books-exporter \
   skills/apple-books-export-rust/scripts/apple-books-exporter-<target>
```

### 使用示例

安装 skill 后，AI 助手可以直接调用：

```
用户: 导出纳瓦尔宝典的笔记
AI: 正在搜索...
    找到：纳瓦尔宝典 - 218 条笔记
    已导出到 ~/Desktop/纳瓦尔宝典_xxxxx.md
```

## LLM 配置

AI 增强功能需要配置 LLM API。配置文件：`knowledge_config.json`

```json
{
  "llm": {
    "provider": "openai_compatible",
    "base_url": "https://api.openai.com/v1",
    "api_key": "sk-xxx",
    "model": "gpt-4o-mini",
    "batch_size": 10,
    "max_retries": 3,
    "retry_delays": [1, 2, 4]
  },
  "output_format": "obsidian"
}
```

支持的 API：
- OpenAI (gpt-4o-mini, gpt-4o)
- DeepSeek (deepseek-chat)
- 通义千问 (qwen-turbo)
- MiMo (mimo-v2.5-pro)
- Ollama (本地)

## 项目结构

```
apple-books-export/
├── src/                           # Rust 核心库
│   ├── main.rs                    # CLI 入口
│   ├── db.rs                      # SQLite 数据访问
│   ├── models.rs                  # 数据结构
│   ├── exporter.rs                # Markdown 导出
│   ├── provider.rs                # LLM API 调用
│   ├── cache.rs                   # LLM 结果缓存
│   ├── card.rs                    # 图片卡片生成
│   ├── speech/                    # Speech Clip 生成、缓存、播放与导出
│   └── ...
├── tui/                           # OpenTUI 只读终端界面
│   ├── src/                       # Core API 应用、后端协议与测试
│   └── package.json               # Bun 脚本与 OpenTUI 依赖
├── appkit/                        # AppKit GUI (Swift)
│   ├── Sources/BooksExporter/     # 应用主体
│   └── Scripts/verify-ui.sh       # UI 回归探针
├── skills/                        # AI Agent Skill
│   └── apple-books-export-rust/
│       ├── SKILL.md               # Skill 文档
│       └── scripts/               # 二进制和脚本
└── knowledge_config.json          # LLM 配置
```

## 数据来源

Apple Books 的笔记数据存储在：
```
~/Library/Containers/com.apple.iBooksX/Data/Documents/
├── BKLibrary/BKLibrary-*.sqlite      # 书籍元数据
└── AEAnnotation/AEAnnotation_*.sqlite # 笔记/标注数据
```

## macOS 权限

如果遇到"无法读取数据"提示，确保在 **系统设置 → 隐私与安全性 → 完全磁盘访问权限** 中给予终端或二进制文件完全磁盘访问权限。

## 技术栈

- **后端**: Rust CLI + rusqlite (bundled)
- **TUI**: Bun + OpenTUI
- **Agent Data Skill**: 仓库内 `apple-books-export-rust` Skill
- **图形界面**: AppKit (Swift)，随包分发 canonical Rust CLI
- **Legacy GUI**: Tauri 2 + Svelte 5（源码已删除，回滚锚点 `legacy/tauri-gui-mainline`）
- **数据库**: rusqlite (bundled)
- **HTTP**: reqwest + tokio
- **图片**: image + rusttype

## Python 版本

Python 版本已归档到 `python-legacy` 分支，如需使用：

```bash
git checkout python-legacy
```

## License

MIT
