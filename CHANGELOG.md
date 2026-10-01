# Changelog

本文件记录可见变更。`v0.1.8` 及之前只发布 AppKit，且 AppKit 有自己的版本线；
自 `v0.3.4` 起，Rust CLI 与 AppKit GUI 在同一个 Release 中发布，共用
`Cargo.toml` 的版本号。

## Unreleased

### 语音 Cache 契约：每条 entry 带上归属与时长

`speech cache status` 的 `CacheStatusEntry` 新增 `asset_id`、`annotation_id`、
`content_kind` 和 `duration_ms` 四个字段。纯增量，不新增命令，不改任何已有字段。

原因是**归属查询答不了**。`clip_id` 是「内容 + Voice Profile」的指纹，算法归 Rust 核心
所有（ADR 0008 明令 AppKit 不得重实现语音语义），所以图形界面此前无法回答
「这条高亮之前生成过吗」，只能靠用户再生成一次——参数没动时命中缓存不计费，但动过
任一滑块就是新 clip、真计费，为了听一条旧音频而意外付费是真实存在的风险。

- 人类与 `--json` 输出都带归属：`- <clip_id> status=ready …` 下一行是
  `书=… 标注=… 内容=highlight 时长=12528 ms`；
- 音频没通过校验时四个字段都是 `null`，**不从 state 回退读取**。一个 `corrupt` entry
  的身份不可信，报出来会和同一行的 `status=corrupt` 自相矛盾；
- 同一条标注可以有多条 clip（改了音色或任一音调参数就是另一条、另一次计费），
  所以 `annotation_id` 相同、`clip_id` 不同的 entry 各自独立列出。

### AppKit 语音生成入口

AppKit 现在有了语音入口。它是语音的**第二个用户入口，不是第二个实现**：全部语音
语义由 Rust 核心通过 machine JSON 拥有，图形界面只消费 receipt 与稳定错误码。

- 选中一条标注后，行尾出现「生成语音」入口。面板展示将要发送的内容、音色、语速、
  音量、声调和本地估算的计费字符数。**按下「生成」即为授权**，没有第二个确认弹窗；
- **API Key 在「设置 › 语音」里填写**，存本机钥匙串，不写入配置文件，也不出现在
  命令行参数里。「验证」按钮跑一次音色目录（免费）确认凭据可用；
- **「生成语音」是界面上唯一联网且产生费用的操作**。费用提示**只显示估算的计费字符
  数，不显示金额** —— 契约里没有货币、没有单价、没有总额，显示任何金额都是编造；
- **改动音色、语速、音量或声调中的任何一个都会生成新音频并单独计费**，原因见
  `clip_id` 的指纹语义（上文 v0.3.4）；
- 音色选择器为两级分组，**不可用的组合显式置灰并附原因，不静默替换为近似音色**；
  存储的音色不在当前目录中时停在第一个可用项，而不是退回列表第一条；
- **生成后可以「导出音频」**，把缓存里的音频写进这本书的导出目录并更新 Speech Export
  Manifest。导出目标**不是任意目录**：面板常驻显示「导出目录：<path>」，应用导出过这本书
  就直接写进那一层；没有记录、或记录的那一层已经被移动删除时，提示先在书籍详情「导出」
  本书的 Markdown。手动选的目录里若没有任何 Markdown，写入前会说明「这份音频不会被任何
  笔记引用」再由用户确认（先导音频、后导 Markdown 是合法顺序，所以不拦）；
- **面板把「准备 / 生成 / 播放 / 导出」四类结果分开显示，互不覆盖**。此前共用一行，
  后一次操作会盖掉前一次：按「播放」把「已生成（clip …）」换成「正在播放。」，导出
  再把两者都盖掉；路径解析失败时更糟——刚付过费的那条生成结果正好被挤掉。现在每类
  结果各占一处、只有一个写者。重新生成会清掉上一条音频的播放与导出结果，那两条描述
  的是旧 clip，属于过期而不是覆盖；
- 目前**只呈现 SenseAudio 这一个已测试的渠道**。目录里来自未验证供应商的条目被丢弃
  而非置灰——置灰仍然是在承诺一个存在但没走过的路径。

边界与取舍见 [ADR 0008](docs/adr/0008-appkit-speech-entry.md)，导出目标的规则见
[ADR 0009](docs/adr/0009-speech-export-target.md)，供应商侧规则见
[ADR 0007](docs/adr/0007-annotation-speech-generation-boundary.md)。

### 未覆盖的范围

- **AppKit 语音路径的自动化覆盖仍然是 mock**：真实供应商验收是显式的本地 opt-in smoke，
  不在普通 CI 中运行，也不读取真实 Apple Books 内容（见 ADR 0007「验证边界」）。
  但**人工验收已经做过**——2026-10-01 在打包后的 app 里对本机真实供应商跑通三次，
  `~/Library/Application Support/books-exporter/speech/attempts/` 留下三条
  `status: "succeeded"` 的 Speech Attempt，均带供应商 `trace_id` 与回传字符数
  （73 / 34 / 34）。此前本文档写的「GUI 从未对真实供应商跑过」已过期；
- Read-only TUI 与 Agent Data Skill **仍无**语音入口。

## v0.3.5 - 2026-09-30

### 修复：点击「生成卡片」必崩

`v0.3.4` 的 AppKit 应用在**第一次渲染 Share Card 时必然崩溃**，用户点「生成卡片」即
触发。崩溃是 `EXC_BREAKPOINT (SIGTRAP)`，不是内存错误——栈底落在
`_assertionFailure`，位于 `static NSBundle.module` 的一次性初始化里，也就是
SwiftPM 生成的访问器中的 `fatalError`。

原因是资源 bundle 的位置。SwiftPM 为 `Bundle.module` 生成的访问器只试两个路径：

```
Bundle.main.bundleURL + "BooksExporter_BooksExporterCore.bundle"   ← .app 根目录
/Users/<构建者>/.../.build/.../BooksExporter_BooksExporterCore.bundle
```

`package-dmg.sh` 把它复制到了 `$APP_DIR/Contents/`，**两个都不是**。在构建机上第二个
候选总是命中，所以 app 在自己的 build 树里能跑；换到任何没有那份 `.build` 的机器上，
两个候选全部落空，于是第一张卡片就 trap。第二个路径是编译期写死的绝对路径，在任何
已发布的副本里都是死代码。

47 个 XCTest 和 106 条 UI 断言全程为绿：它们都在 build 树里跑，而 `Bundle.module`
在那里总能解析。**没有任何检查见过一个被真正打包过的 app。**

### 新的检查

- `appkit/Scripts/verify-resource-bundle-layout.sh` 检查一个真实的 `.app`。正确与错误
  的目的地只差同一条 `cp -R` 行里的一个路径片段，grep 分不开它们；而「grep 不到那个
  出错的字符串」恰好就是会通过的检查。
- `package-dmg.sh` 在 hdiutil 之前对暂存的 app 跑这个检查；`STAGE_ONLY=1` 可以在
  校验后停下，让 AppKit CI job 在每个 PR 上以秒级跑一遍真实的打包脚本。
- `release.yml` 对**挂载后的 DMG** 跑同一个检查，也就是检查真正会被下载的字节。
- CI 另外断言生成的访问器仍然从 `Bundle.main.bundleURL` 解析。app 根目录这条规则是
  对工具链行为的假设；万一 SwiftPM 变了，这个检查会报出来，而不是继续对着一个没人
  读的布局默默通过。

## v0.3.4 - 2026-09-30

> **这一版的 Share Card 不可用**：点「生成卡片」必然崩溃（`EXC_BREAKPOINT`），
> 资源 bundle 的位置错了。用 `v0.3.5` 或任何更高版本。其余部分（CLI、更新检查、
> 版本统一）不受影响。

### 首个 CLI 与 GUI 合并发布的版本

- 同一个 Release 同时提供 Rust CLI（arm64 / x86_64）和 arm64 AppKit GUI
  `Books-Exporter-0.3.4-unsigned.dmg`，`CFBundleShortVersionString` 与 CLI `--version`
  由同一个 tag 决定。
- 图形界面只剩 AppKit 一个：Tauri 源码已删除，仓库不再需要 Node.js。回滚锚点
  `legacy/tauri-gui-mainline` 仍保留 Tauri 时代完整状态。
- 流水线在任何构建之前校验 tag 与 `Cargo.toml` 的 `version` 一致，不一致直接失败。
  此前 tag 只用于命名 Release，不一致时能发布出「页面写着一个号、二进制自报另一个号」
  的产物且全流程无报错。
- AppKit 打包的默认版本改为读 `Cargo.toml`，此前硬编码为一个早已停更的旧号。
- 更新清单 `latest.json` 的 `channel` 由版本号推导，不再硬编码 `stable`，预发布版本
  不会出现在 stable 通道用户的更新列表里。

### 语音生成

这一版首次交付整套标注语音生成能力，此前从未有任何版本包含它。

```bash
speech voices     [--refresh]      # 音色目录；--refresh 是唯一会联网的免费命令
speech profile show|set|reset      # Voice Profile
speech generate --asset-id … --content highlight|note
speech play --clip-id              # 人类入口：用系统播放器发声
speech export --clip-id --output   # 写进书籍导出目录并更新导出清单
speech cache status|clear
speech history clear
```

- 供应商为 **SenseAudio**（`sensenova-tts-2.0`）。API Key 只从环境变量
  `SENSEAUDIO_API_KEY` 读取：不写入配置文件，也不接受命令行传入，变量名本身由
  `speech profile set` 决定而非硬编码；
- **`speech generate` 是唯一计费的操作**。`speech voices --refresh` 虽联网但免费；
- **`clip_id` 是「内容 + Voice Profile」的指纹**。改动音色、语速、音量或声调中的任何
  一个，得到的是另一个 clip 和**另一次计费**，而不是对同一段音频的重新调音。参数与
  已缓存音频一致时才会命中缓存；
- 单一标注的文本上限 10,000 字符，超出即拒绝而不是截断；
- 本地状态（Profile、目录缓存、音频、导出清单）位于
  `~/Library/Application Support/books-exporter/speech/`，**不跟随**当前工作目录、
  `--config` 或 Markdown 导出目录 —— 所以 `speech export` 是把音频复制进导出目录的
  独立一步，而不是把状态放在那里；
- 人类 `speech play` 会发声；machine JSON 的播放入口**只返回状态和路径，不产生声音**。
  两者是不同入口，不是同一个命令的两种输出格式。

### 未覆盖的范围

- **CLI 路径曾成功调用过真实供应商，但从未做过正式验收**。本机 2026-09-29 留下两条
  `status: "succeeded"` 的 Speech Attempt：含供应商 `trace_id`、供应商回传的实际用量
  （`provider_usage_characters`），产物是符合声明规格的真实 MP3（32 kHz / 128 kbps /
  双声道），并已通过 `speech export` 写进《100 Go Mistakes and How to Avoid Them》的
  导出目录。**那次生成用的文本只有 2 个 unicode 字符**（`annotation-688` 的一条高亮），
  是最小可用性探针，不是真实长标注的验收。记录未保存请求端点，因此无法从记录本身
  断定当时用的是默认端点而非 `SENSEAUDIO_API_BASE_URL` 覆盖。SenseAudio 的供应商
  **合同**另于 2026-09-11 用固定文本 tracer 验证过（含 live 技术证据与人工试听，见
  [`docs/evidence/2026-09-11-senseaudio-contract-smoke.md`](docs/evidence/2026-09-11-senseaudio-contract-smoke.md)）。
  真实供应商验收按 ADR 0007 属显式 opt-in，不在普通 CI 中运行，需要使用者自备 API
  Key。与本版本开头的 Share Card 崩溃无关；
- 这一版的语音能力**只有 CLI 入口**，AppKit 图形界面尚无语音（见下方 Unreleased）；
- Agent Data Skill 的 `contract.sh` 当时**未覆盖** speech 子命令，语音命令的契约回归
  当时不在 CI 门禁内；
- DMG 未做 Developer ID 签名与 notarization，文件名带 `-unsigned`；首次安装需手动
  移除 quarantine 属性。
- 只有 arm64 的 AppKit DMG。x86_64 的 GUI 产物仍未产出，见 [#45](https://github.com/chenweil/apple-books-export/issues/45)。

## v0.1.8 - 2026-08-05

### Share Card 编辑器

- 从选中的 Apple Books 标注进入 Share Card 编辑器，默认直接生成可预览的 1200×1600 卡片。
- 支持自动/固定字号、11 款随应用分发字体、水平/垂直对齐、12 个绑定背景与配色的模板。
- 长正文和长笔记形成连续页面；笔记沿用正文颜色，以轻微斜体和细分割线与正文区分，续页不重复分割线；大预览、页码和有边界缩略图带共享同一组渲染页面。
- 保存全部页面，复制默认当前页并可复制全部页面；AirDrop 发送当前页临时 PNG，不要求先保存。
- 移除泛用 macOS 分享面板；卡片编辑只影响导出内容，不修改原始 Apple Books 标注。
- 补充服务层测试、AppKit UI 探针和生成图片视觉检查记录。

### 发布

- 发布版本为 `0.1.8`、构建号 `9`，产物为 `Books-Exporter-0.1.8-unsigned.dmg`，见 [GitHub Release v0.1.8](https://github.com/chenweil/apple-books-export/releases/tag/v0.1.8)。
- DMG 未进行 Developer ID 签名或 notarization；真实设备 AirDrop 未执行，限制已记录在验收文档中。
- 完整验收证据与未执行项见 [`docs/plans/2026-08-05-appkit-share-card-issue-13-verification.md`](docs/plans/2026-08-05-appkit-share-card-issue-13-verification.md)。

## v0.1.7 - 2026-08-04

### 修复

- 修复自动刷新、切回应用和首次加载同时发生时，SQLite 连接并发访问导致应用闪退的问题。
- 为共享数据库连接增加串行访问保护，并加入并发读取回归测试。
- 默认 unsigned DMG 版本更新为 `Books-Exporter-0.1.7-unsigned.dmg`。

### 发布

- 发布 `Books-Exporter-0.1.7-unsigned.dmg`，应用版本为 `0.1.7`、构建号为 `8`。
- DMG 未进行 Developer ID 签名或 notarization，仅适合本机安装验证；安装和权限说明见 README。

## v0.1.6 - 2026-08-04

### 修复

- 修复 unsigned DMG 打包脚本在已有 release 可执行文件时跳过重新编译，导致版本号更新但应用代码未更新的问题。
- 重新生成包含设置页和自动刷新的 unsigned DMG。

## v0.1.5 - 2026-08-04

### 新增

- 新增设置窗口，可通过“设置…”或 `⌘,` 打开。
- 新增 Apple Books 自动刷新间隔设置，支持关闭、1 分钟、5 分钟、15 分钟、30 分钟和每小时。
- 应用按设置的低频周期读取 Mac 本地 Apple Books 数据，设置变更立即生效。
- 默认 unsigned DMG 版本更新为 `Books-Exporter-0.1.5-unsigned.dmg`。

## v0.1.4 - 2026-08-04

### 修复

- 修复 Apple Books 使用 WAL 写入时，最新高亮和笔记未被读取的问题。
- 应用重新激活时自动刷新书单和当前选中书的标注。
- 默认 unsigned DMG 版本更新为 `Books-Exporter-0.1.4-unsigned.dmg`。

## v0.1.3 - 2026-08-04

### 变更

- 新增 `霞鹜文楷` Share Card 字体选项。
- 随应用资源附带霞鹜文楷完整 OFL 1.1 许可证和上游保留名称说明。
- 默认 unsigned DMG 版本更新为 `Books-Exporter-0.1.3-unsigned.dmg`。

## v0.1.2 - 2026-08-04

### 变更

- 移除 Share Card 字体选项“源界明朝体”，避免该字体字形覆盖不足导致卡片文字不完整。
- 新增汇文明朝体、汇文仿宋、汇文正楷和汇文港黑四个 Share Card 字体选项。
- 随应用资源打包汇文字体授权声明转录和来源说明。
- 默认 unsigned DMG 版本更新为 `Books-Exporter-0.1.2-unsigned.dmg`。

## v0.1.1 - 2026-08-04

### 新增

- Share Card 编辑器支持系统默认、思源黑体、思源宋体、源界明朝体、演示悠然小楷、演示佛系体、站酷文艺体和庞门正道粗书体。
- Share Card 支持复制图片、AirDrop 和 macOS 系统分享面板。
- Share Card 支持长正文分页、长归因文字换行和四个候选卡片。
- 新增字体来源、许可证和授权边界说明，随应用资源打包。
- 提供 `Books-Exporter-0.1.1-unsigned.dmg` 打包流程。

### 修复

- 固定标注行“生成卡片”入口在右侧并垂直居中。
- 修复长归因文字超出卡片底部的问题。
- 修复点击“换一换”后候选区可能挤出编辑器完成按钮的问题。
- 分享服务过滤微信，仅保留可用的其他系统服务。

### 发布说明

- 当前 DMG 未进行 Developer ID 签名或 notarization，仅适合本机安装验证。
- `手书体`和`锐字真言体`因授权条件尚未满足，未随应用分发。
