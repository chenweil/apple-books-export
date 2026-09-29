# #15 只读 OpenTUI 浏览器扩充 — 验收记录

日期：2026-09-29
分支：`codex/issue-15-tui-browser`（基于 main `d6ac75b`）
后端：`/tmp/w15/target/release/apple-books-exporter`（main 线构建）
环境：macOS 15，终端已授予 Full Disk Access，Bun + tmux

## 结论

#15 的三条验收标准全部达成。TUI 的读路径（书籍列表 → 搜索 → 选择 → 标注详情 →
返回 → 退出）在真实 Apple Books 库上跑通，**全程无 GUI 窗口、无任何写入**。

本次交付的产品代码改动为**零**：`tui/src/app.ts` 与 main 完全一致。新增的是一个
可重复的真机冒烟脚本和两条回归测试（原因见第 4 节）。

## 1. Bun 测试覆盖验收标准

`bun --cwd tui test`：**18 pass / 0 fail**，46 个断言。

| 验收项 | 覆盖测试 |
| --- | --- |
| DTO 解析 | `backend.test.ts` 解析版本化 book-list 协议、含 nullable 字段的标注详情 |
| schema 拒绝 | `rejects an unknown protocol version with a stable error`、`rejects malformed annotation payloads` |
| 搜索 | `filters books from the search input`、`keeps the search filter after Esc and clears it when the query is edited` |
| 选择 | `loads annotation details while navigating books`、`ignores stale annotation responses after rapid navigation` |
| 详情加载 | `loads and renders the selected book annotation details` |
| 紧凑布局 | `switches compact mode between list and loaded detail`、`reflows when the terminal becomes compact`、`layout.test.ts` 三档宽度 |
| 错误状态 | `shows stable backend errors and remediation in the detail`、`parses stable machine errors` |
| 干净退出 | `destroys the renderer when q is pressed outside search` |
| 只读保证 | 真机冒烟中的进程树断言（见第 3 节） |

## 2. typecheck

`bun run --cwd tui typecheck`：`bunx tsc --noEmit` 通过。

> 环境提示：worktree 里没有 `node_modules`，首次跑 `bun --cwd tui test` 会因缺少
> `@opentui/core/testing` 失败。先 `bun install --cwd tui` 即可，这与代码无关。

## 3. 真机冒烟（新增 `scripts/tui-smoke.sh`）

脚本用**独立 tmux socket** 起一个真实 pty，跑真实 Bun/OpenTUI 前端接真实 Rust
后端，然后对渲染出的终端帧逐项断言。基线数据取自 `list --json`，所以断言的是
用户真实书库里的真实书名/作者，不是写死的字符串。

结果：**28 / 28 通过**，`TUI_EXIT=0`。

覆盖的断言分组：

- **宽屏 140×45**：载入 70 本真实书籍、显示真实书名与作者、显示真实标注文本
  （正文或笔记正文，占位符 `（无正文）` 不算）、真实 EPUB CFI 位置、真实标注时间。
  断言用书名/作者的前 12 个字符：列表面板只有 36 列、详情面板会换行，真实书名
  过长时会被截断，拿整串去匹配会让健康的应用误判为失败。
- **只读保证**：脚本把 `APPLE_BOOKS_EXPORTER_BIN` 指向一个记录 argv 的 shim，
  再断言后端被调用的子命令集合**恰好**是 `list` 与 `annotations`。空日志、
  出现任何写子命令，都会判失败——这比「grep 一下 stderr」强得多，原因见第 6 节。
- **无 GUI 证明**：枚举 tmux pane 的整棵进程树，断言其中没有 `.app/`、
  `osascript`、`Automator`、`/usr/bin/open`。在启动后和退出前各取一次快照，
  覆盖整段交互过程。实测进程树只有 bun 相关进程。
- **选择切换**：`↓` 之后详情切换到第二本书的作者与书名。
- **焦点**：`Enter` 页脚切到详情滚动模式，`Esc` 回到列表浏览模式。
- **紧凑 50 列**：列表屏隐藏详情面板、`Enter` 切详情屏并隐藏列表面板、`Esc` 回列表屏。
- **搜索**：`/` + 查询词把 70 本缩小到 1 本；`Esc` 保留查询词与过滤结果；
  `Esc` 确实把焦点交回列表（随后按 `/` 是**重新聚焦搜索框**而不是往查询词里
  插入字面量斜杠）；退格清空查询后恢复 70 本。
- **干净退出**：`q` 使 TUI 进程以退出码 0 结束。

## 4. 验证过程中的一个真实发现：Esc 契约与测试替身

排查搜索交互时，`Esc` 相关的单测一度红灯，现象是「`Esc` 离开不了搜索框」。
逐层埋点后拿到的证据链是：

1. 在 `tui/src/app.ts` 的 renderer 级 `keypress` handler 入口打印每个按键，
   发现搜索框聚焦时 **`escape` 键根本没到 handler**；
2. 在 `searchInput.onKeyDown` 里打印，发现 mock 传来的按键名是**空字符串**，
   而非 `escape`；
3. 真终端（tmux）复测：搜索框聚焦时按 `Esc`，随后按 `/` **没有**字面量斜杠进入
   查询词，说明焦点确实回到了列表；再按 `q` 会退出。

结论：**产品行为与 README 描述一致，`Esc` 本来就能离开搜索框**；问题出在测试
替身——`mockInput.pressEscape()` 只发一个裸 ESC 字节、没有后续字节，OpenTUI 无法
解析出键名。仓库里原有的紧凑布局测试本来就用 `kittyKeyboard: true` 来让 `Esc`
可判定，本次两条新测试沿用同一做法。

因此 `tui/src/app.ts` **不需要任何改动**。新增的两条测试锁住 `Esc` 契约：

- `Esc returns focus from the search box to the book list`：用「`Esc` 之后按 `q`
  能退出」证明焦点确实离开了搜索框（搜索框仍在焦点时 `q` 只会被当文本输入）。
- `keeps the search filter after Esc and clears it when the query is edited`：锁住
  「`Esc` 只交还焦点、不过滤词」这一设计。

变异验证：把 `tui/src/app.ts` 的 `key.name === "escape"` 改成
`"escapeMUTANT"`，上述两条测试与原有的紧凑布局测试**同时失败**（7 pass / 3 fail）；
还原后 10 pass / 0 fail。

## 5. 记录下来的既有设计：查询词跨 `Esc` 保留

`Esc` 离开搜索框时**不会清空查询词**，过滤结果继续生效。空结果详情页的提示
「按 / 修改搜索条件」与这个行为一致：改查询词才是恢复完整列表的途径。这是设计而非
缺陷，冒烟脚本与单元测试都按此断言。

## 6. 独立 code review 抓到的真问题

`/code-review` 由一个独立 verifier 执行，它无法读取本机 Apple Books 数据，因此没有
重跑真机冒烟，但把自动化部分全部重做了一遍。三条被采纳并修复：

1. **（中）原本的「只读保证」断言根本不可能失败。** 它 grep 的是 bun 进程自己的
   fd 2（`tui.err`），而 TUI 在 `tui/src/backend.ts:215` 用 `stdout: "pipe",
   stderr: "pipe"` 派生子进程，**后端的 argv 与 stderr 都被 Bun 吃掉，不会到达
   那个文件**。也就是说这个断言只能因为「bun/OpenTUI 恰好往 stderr 写了 config」
   而误报，永远抓不到真正的写操作。
   已改为：把后端包一层记录 argv 的 shim，断言子命令集合恰好是 `list` 与
   `annotations`。四种情形的判别力单独验过：只读用法 → PASS；出现 `export` →
   REJECT；出现 `speech` → REJECT；日志为空 → REJECT。
2. **（低）`grep -Fq "${TITLE}"` 缺少 `--`。** 真实书名/作者以 `-` 开头时，
   `grep` 会把模式当成选项、退出码 2，`if` 读成假，冒烟会在健康应用上**误报失败**。
   全部内联 grep 已补 `--`（`wait_for_pattern` 原本就有）。
3. **（低）「无 GUI 证明」只是单点快照。** 进程树只在启动后取一次，短命的 GUI
   进程可能已退出或被 reparent 而不可见。已在退出前再取一次，并把文档措辞与
   实际机制对齐。

另有一条（低）「整串书名匹配假设书名能完整渲染」已通过改用前缀断言处理，见第 3 节。

verifier 未能验证的部分：真机冒烟本身（无 Full Disk Access / 不读用户书库）、
`InputRenderable` 未聚焦时是否渲染 placeholder、`tmux` 是否逐字节送达中文查询词。
其中查询词送达已被 70→1 的过滤结果间接证实。

## 未执行 / 已知限制

- **没有人工 GUI 走查**：本 issue 的验收标准是「不开 GUI 的真机冒烟」，已达成；
  但没有人用肉眼在真实终端里长时间使用过这个 TUI。
- **未覆盖的终端**：只在 tmux 下验证。其它终端（尤其是不支持 kitty keyboard
  协议、且对裸 ESC 字节判定不同的终端）未测。`Esc` 依赖终端能无歧义地报告该键。
- **未测终端尺寸边界**：只测了 140×45 与 50×两档，未测 60/100 档位与极小高度。
- **未测真实 Apple Books 变更**：TUI 是只读的，本次也没有在 Apple Books 里增删
  标注来验证数据变化后的刷新。
- **冒烟脚本不进 CI**：它需要 macOS、Full Disk Access、真实书库数据和 tmux，
  与 `scripts/senseaudio-smoke.py` 一样属于本机脚本，不适合放进
  `.github/workflows/ci.yml` 的 ubuntu job。
