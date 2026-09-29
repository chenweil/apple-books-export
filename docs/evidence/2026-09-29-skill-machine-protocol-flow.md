# #16 Agent Data Skill 对接机器协议 — 验收记录

日期：2026-09-29
分支：`codex/issue-15-tui-browser`（与 #15 同分支，基于 main `d6ac75b`）
二进制：`/tmp/w15/target/release/apple-books-exporter`（main 线构建）
环境：macOS 15，终端已授予 Full Disk Access

## 结论

#16 的四条验收标准全部达成。真机跑通了「列书 → 选 asset → 导出 Markdown →
验证非空输出」的完整链路。

过程中发现并修复了 SKILL.md 第 5 步的一个**真实措辞漏洞**：导出器为每本书建一个
子目录，而原文让人去「选定的输出目录里找非空 Markdown 文件」。按字面执行
（对输出目录做扁平的 `*.md` 匹配）会一个文件都找不到，从而把一次完全成功的导出
误报成空。详见第 5 节。

## 1. Skill 触发条件与命令和 Rust 源码、`--help` 一致

逐条核对 `skills/apple-books-export-rust/SKILL.md`：

| SKILL.md 声明 | 源码 / `--help` 依据 |
| --- | --- |
| `list --json` | `src/main.rs:42` `List { json: bool }`；`--help` 首行 `list 列出所有有笔记的书籍` |
| `annotations --asset-id <id> --json` | `src/main.rs:49` `Annotations { asset_id, json }`；`--help` `annotations 获取一本书的标注详情` |
| `export --asset-id <id> --json --output <dir> --format obsidian` | `src/main.rs:60` `Export { index, asset_id, json, overwrite, output, format }`，`format` 默认值就是 `obsidian`（`src/main.rs:81`） |
| `--overwrite` 为显式授权，默认拒绝 | `src/main.rs:74` `overwrite: bool`，默认 `false` |
| `doctor --json` | `src/main.rs:86` `Doctor { json: bool }`；`--help` `doctor 诊断 binary 和 Apple Books 数据库可用性` |
| 收据含 `asset_id` / title / `annotation_count` / `format` / output directory / `generated_files` | `src/machine.rs:278` `ExportReceipt` |
| 每个标注暴露 `id` / `type` / `content_text` / `note_text` / `chapter_title` / `location` / `created_at`，可空值保持 `null` | 真机 `annotations --json` 响应字段完全一致，`note_text` 实测为 `null` |
| 二进制解析顺序：env → 仓库 release → 仓库 debug → PATH → Skill 内置 | `scripts/validate.sh` 的 `resolve_binary()` |

`--help` 实际输出的命令列表确认包含 `list`、`annotations`、`export`、`doctor`
（另外还有 `enrich`、`card`、`cache`、`speech`、`config`，Skill 明确不使用它们）。

## 2. Skill 校验通过

`bash skills/apple-books-export-rust/tests/contract.sh` → `Skill contract and
runtime validation passed`。CI 的两条相关 job 也复跑通过：

- `bash tests/headless_mainline.sh` → `headless mainline contract passed`
- 随包分发的两个 arm64 二进制都能通过 `validate.sh --print-path`

本次给 `contract.sh` 的必备文本加了 4 个新护栏：`per-book subdirectory`、
`descendant`、`audio_links`、`warnings`，防止第 5 节的措辞被静默回退。

变异验证：把 SKILL.md 里全部 `per-book subdirectory` 改成
`per-book directory` 后，`contract.sh` 报 `missing Skill contract text:
per-book subdirectory` 并以 1 退出；还原后通过。

## 3. 真机全流程

用独立输出目录（`/tmp/skill-export.*`），避免往用户真实 Obsidian 书库里写文件。

**① 预检**

```
$ skills/apple-books-export-rust/scripts/validate.sh --print-path
/tmp/w15/target/release/apple-books-exporter
```

**② 列书并按稳定身份选择**

`list --json` → `schema_version: 1`，70 本书。选标注最多的一本：
《巨婴国: 国内心理学家系统透视中国国民性》/ 武志红 / 302 条，
`asset_id=44D43B7A372DA51FB1B5AD664DBE4D53`。

**③ 读标注**

`annotations --asset-id 44D4… --json` → `schema_version: 1`，响应 `asset_id` 与
请求一致，`annotation_count: 302` 且数组长度 302，首条标注七个字段齐全，
`note_text` 为 `null`（证明可空字段在协议里保持 `null`，不是被省略或填空串）。

**④ 导出 Markdown**

`export --asset-id 44D4… --json --output /tmp/skill-export.bwESqo --format obsidian`
退出码 0，stderr 为空，收据字段与请求一致（`asset_id` 匹配、`format: obsidian`、
`output_directory` 为所选目录）。本次收据**不含** `audio_links` 与 `warnings`——
这两个字段带 `skip_serializing_if = "Vec::is_empty"`，为空时不会出现
（`src/machine.rs:286`、`:289`）。

**⑤ 验证非空输出**

```
size=  30350  inside_output_dir=True
  /tmp/skill-export.bwESqo/巨婴国_ 国内心理学家系统透视中国国民性/巨婴国_ 国内心理学家系统透视中国国民性.md
```

生成的是真实 30 KB Markdown，YAML front matter 里有 `book` 与 `author`，正文含
`## 第 1 章 · item4` 与 `> 孝道是中国文化的核心…` 高亮块，路径在所选输出目录内。

同一目录下做**扁平** `*.md` 匹配的结果是 `[]` ——正是第 5 节要修的那个陷阱。

## 4. Full Disk Access 补救说明准确

SKILL.md 要求保留稳定错误码 `FULL_DISK_ACCESS_REQUIRED` 并引导用户到
**System Settings → Privacy & Security → Full Disk Access**。

Rust 侧 `src/machine.rs:124` 确实发出该码，remediation 为
`Grant Full Disk Access to this terminal or application in System Settings >
Privacy & Security > Full Disk Access, then retry.` 系统设置路径、动作主体
（终端或宿主应用）、以及「之后重试」三点都与 SKILL.md 的说法一致，没有夸大也没有
遗漏。

覆盖保护路径实测：

```
$ export … --output <同一目录>            # 不带 --overwrite
exit=1
{"schema_version":1,"error":{"code":"OUTPUT_FILE_EXISTS",
 "message":"Output file already exists: …/巨婴国_ ….md",
 "remediation":"Choose another output directory or pass --overwrite to replace existing files."}}
$ export … --output <同一目录> --overwrite
exit=0
```

默认拒绝、码与补救文案准确、显式 `--overwrite` 才替换，与 SKILL.md 第 4 步一致。

`doctor --json` 的 `environment` 块实测：

```json
{ "home": { "status": "ok", "path": "/Users/chenweilong" },
  "default_output_dir": { "status": "ok", "path": "/Users/chenweilong/books-exported", "writable": true },
  "free_bytes": 64920535040 }
```

与 SKILL.md 里逐字段的说明一致。

## 5. 修复的缺陷：第 5 步会误报空导出

**问题**：原文第 5 步要求「至少一个 Markdown 文件在选定输出目录里非空」，
「文件留在用户选定的输出目录内」。但 `src/exporter.rs:109` 固定为每本书建一个
子目录：

```rust
let book_dir = output_dir.join(safe_path_component(&book.title));
```

所以 Markdown 文件从不在输出目录顶层。任何按字面执行这一步的 agent——例如对
`~/books-exported/*.md` 做匹配——都会得到空结果，进而把一次成功的导出报成失败。
本次验证自己就踩中了这个坑（首次用非递归 `os.listdir` 校验时报
`VERIFY_FAILED`）。

**修复**（`SKILL.md` 第 4、5 步）：

- 明确写出导出器为每本书建子目录，`generated_files` 是「`--output` 的值 + 每本书
  子目录 + 文件名」拼出来的；并要求传**绝对路径**的 `--output`（`~/books-exported`
  这个默认值本来就是），这样返回的路径才是绝对路径；
- 把校验对象改成「收据里的 `generated_files` 路径」而不是「输出目录里的文件」，
  并逐条要求：路径存在且是普通文件、后缀为 `.md`、至少一个非空、每条路径是所选
  输出目录本身或其后代（`descendant`）；
- 明确禁止用扁平 `*.md` 匹配输出目录，并说明为什么；
- 补充两个可选收据字段 `audio_links` / `warnings`，并要求把 `warnings` 报给用户
  而不是丢弃。

> 关于「绝对路径」这一条的措辞：初版文档直接写「每个 `generated_files` 条目都是
> 绝对路径」，独立 code review 查出这是**错的**——`src/main.rs` 里没有
> `canonicalize()`，`cmd_export_json`（`src/main.rs:1436`）把 `--output` 的值原样
> 传下去，相对路径会原样出现在收据里。已改成上面这个更弱的、也才是真的说法。
> 没有改 Rust 行为：那超出本 issue 范围，而且 Skill 自身默认就用绝对路径。

**回归测试**（TDD，先写测试再改文档）：`tests/machine_cli.rs` 新增
`export_json_nests_generated_files_in_a_per_book_subdirectory`，断言每本书目录
直接位于所选输出目录之下、目录名与文件名都由书名派生、目录真实存在，并且输出
目录顶层的 `.md` 文件数为 0。

变异验证：把 `src/exporter.rs:109` 改成 `output_dir.to_path_buf()`（不再建子目录）
后该测试失败，报 `the per-book directory must sit directly inside the selected
output directory`；还原后通过。`src/exporter.rs` 与 main 完全一致，未改动。

## 全量门禁

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过 |
| `cargo test --all-targets --locked` | **364 passed / 0 failed**（比 main 多 1 条新测试） |
| `cargo check --all-targets --locked` | 通过 |
| `bash tests/headless_mainline.sh` | 通过 |
| `bash skills/apple-books-export-rust/tests/contract.sh` | 通过 |
| `bun --cwd tui test` | 18 passed / 0 failed |
| `bun run --cwd tui typecheck` | 通过 |

## 独立 code review

`/code-review` 由一个独立 verifier 执行。它在隔离副本里重跑了自动化部分，确认了
本文件里的每个数字与行号引用，并独立复现了三条变异结论（`app.ts` 的 escape 变异
3 条测试失败、`exporter.rs:109` 变异只有新测试失败、4 个 `contract.sh` 护栏在 HEAD
版本 SKILL.md 里都不存在因而非空断言）。

它给出 PARTIAL 结论，唯一原因是**无法自己执行真机冒烟**（无 Full Disk Access，
且拒绝在 code review 中读取用户 Apple Books 数据），这属于它的执行环境限制，不是
本次改动的问题。详见 #15 的证据文档第 6 节。

## 未执行 / 已知限制

- **没有用真实 Skill 宿主跑**：验证是按 SKILL.md 逐步手工执行命令，不是让某个
  agent 宿主加载 Skill 后自主决策。后者无法在本机无头复现。
- **未覆盖 FDA 被拒的真实路径**：本机终端已有 Full Disk Access，
  `FULL_DISK_ACCESS_REQUIRED` 的**文案**已与源码逐字比对，但「真的被拒之后 Skill
  会不会照第 4 步原样回报」没有在真实拒权场景下端到端跑过。#14 的
  `docs/evidence/2026-09-29-machine-protocol-smoke.md` 记录了错误码与退出码本身。
- **只验了 obsidian 格式的真实导出**：`--format markdown` 走同一个
  `book_dir`/`main_file` 代码路径，本次只按 SKILL.md 默认值跑了 obsidian。
- **未测 speech 相关的 `audio_links` / `warnings` 非空分支**：本次收据里这两个
  字段为空因而未出现。字段存在性与序列化规则已在 `src/machine.rs:286`、`:289`
  核对，文档按「可空且仅在非空时出现」描述。
- **未测多本书连续导出**：只导出了一本，未验证连续导出时不同 `asset_id` 各建各
  的子目录且互不覆盖。
