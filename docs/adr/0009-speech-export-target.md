# ADR 0009: 语音导出的目标必须是这本书的导出根

- 状态：已接受
- 日期：2026-10-01
- 关联：[`ADR 0007`](0007-annotation-speech-generation-boundary.md)（导出目录语义与 Speech Export Manifest）、[`ADR 0008`](0008-appkit-speech-entry.md)（AppKit 语音入口）

## 背景

`ADR 0007` 规定：与书籍 Markdown 一起导出的音频复制到**该书导出目录**的
`assets/audio/`，Markdown 用相对链接引用；Speech Export Manifest 保存在
`assets/audio/manifest.json`，记录 clip 身份、相对路径与校验信息。

`ADR 0008` 把 `speech export` 接进 AppKit 时，面板给的是一个普通目录选择器，
提示文字是「选择这本书的导出目录」。**没有任何地方告诉用户该选哪一层。**

契约侧不拦这个：`run_export` 只要求目标目录可写，于是 `create_dir_all` 建出
`assets/audio/`、提交 manifest、刷新 locator 投影，然后返回成功。2026-10-01
用户在面板里选了 `~/Downloads`，得到

```
~/Downloads/assets/audio/highlight-37470846a3db.mp3
~/Downloads/assets/audio/manifest.json
```

导出**成功**，`exports.json` 记下这本书的 export_root 是 `~/Downloads`，
回执没有任何 warning。但这份音频是孤儿：`exporter.rs` 在
`resolve_export_links(&book_dir, &book.asset_id)` 里只读**书籍导出时写入的那一层**，
`~/Downloads` 不是那一层，所以下次导出 Markdown 也不会生成链接。同一本书的
《100 Go Mistakes》既有正确目录，又有这条孤儿记录，两处并存。

这不是「校验漏了一条」，而是**界面上不存在正确答案**。目录选择器要求用户做一件
只有导出器内部才知道的事，而失败形态是静默的：音频在、manifest 在、回执是成功、
只有链接不会来。

## 决策

### 导出根由「Markdown 实际写到哪一层」定义，不由书名推导

Rust 导出器写的是 `output_dir.join(safe_path_component(&book.title))`，也就是
用户在 open panel 里选的那一层**下面**还有一层。正确的 speech 导出根是那一层，
不是用户选的那一层，也不是任何按书名拼出来的路径。

因此 AppKit 记的是**导出的真实落点**，不是推导值：`BookService.exportToMarkdown`
现在返回 Markdown 落地的目录 —— 整书路径取 CLI 回执 `generated_files` 首条的父目录，
筛选路径取自己写出的那个文件的父目录 —— 调用方按 `asset_id` 存进
`BookExportRootStore`。回执里没有文件时返回 nil，不猜。

在这里推导书名需要复制一份 Rust 的 `safe_path_component`，两份实现第一次遇到
标题里有不同处理字符时就会分叉，而分叉的后果正是静默孤儿。**不重实现 Rust 的语义**
这条 `ADR 0008` 已经定过，本 ADR 只是把它延伸到目录身份上。

### 面板显示目标，而不是让用户去猜

面板常驻一行「导出目录：<path>」。有记录时「导出音频」直接写进那一层，**不弹选择器**；
记录不存在、或记录的那一层已经被移动或删除时，这一行说明「这本书还没有导出目录」，
并提示先在书籍详情「导出」本书的 Markdown。

记录会失效：`existingExportRoot` 要求路径仍然是一个存在的目录。导出目录被搬走之后
继续提供一个死路径，和提供任意路径是同一类错误。

### 写入前对「不像导出根」的目录说明后果，但不拒绝

用户手动选的目录如果顶层没有任何 `.md`，面板在调用 CLI 之前说明：音频会写进
`<dir>/assets/audio/`，只有把本书 Markdown 导出到同一层才会生成链接，否则这份音频
不会被任何笔记引用。

**是警告而不是拒绝**，因为「先导音频、后导 Markdown」是合法顺序：manifest 先在，
下一次 Markdown 导出读它并生成链接。合法顺序不该被挡住。挡住的是**在不知情的
情况下**做出这个选择 —— 那个才需要用户先知情。不存在的目录、指向文件的路径都算
「不像导出根」，都走这条路。

### 契约侧不改

`speech export` 继续接受任意可写目录。这是 CLI 的正当用法，也是「先导音频后导
Markdown」得以成立的前提。把校验放进 Rust 会同时废掉这两件事，而且把「界面上没有
正确答案」这个真正的缺陷伪装成一条错误码。

## 后果

- 面板不再产生「选哪个目录」的疑问：有记录就一个目录，没有记录就一句可执行的话；
- 孤儿音频在界面上可见。仍然可以手动写进任意目录，但那是一个被说明过的决定；
- `BookService.exportToMarkdown` 的返回类型从 `Void` 变为 `URL?`，两个调用方
  （书籍详情、全部导出）负责记录。筛选导出与整书导出的落点不同这一点被显式记下来，
  而不是让调用方按书名猜；
- 记录存在本机 `UserDefaults`，随应用走，不进 Speech 状态目录 —— 它是图形界面的
  便利投影，和 `exports.json` 一样可丢弃：丢了只是回到「还没有导出目录」的提示；
- AppKit 仍然**不实现任何语音语义**。它记的是自己刚写完的目录，不是 clip 身份、
  不是指纹、不是 manifest 规则。

## 非目标

- 不改变 `speech export` 的参数、校验或回执；
- 不让 AppKit 读 `exports.json`。那是 Rust 的投影文件，AppKit 读它就会开始依赖
  locator 的语义，而 locator 明确是**非权威**的（`export.rs` 的注释就是这么写的）；
- 不追踪书籍导出目录被搬动后的迁移；
- 不为「选错目录」提供事后清理工具。
