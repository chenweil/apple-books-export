# #14 真机 macOS 冒烟记录

日期：2026-09-29
二进制：`target/release/apple-books-exporter`（v0.3.3，aarch64-apple-darwin）
分支：`codex/issue-14-machine-protocol`

本记录只写**实际观察到**的内容。它证明的是本机运行时行为，
不是任何 provider 行为，也不代表其它机器的结果。

## 环境

真实的 Apple Books 数据库，Full Disk Access 已授予（`doctor` 能打开两个
sqlite 文件即为证据）：

- `AEAnnotation_v10312011_1727_local.sqlite` → `databases.annotation.status = readable`
- `BKLibrary-1-091020131601.sqlite` → `databases.library.status = readable`

## 1. `doctor --json`

```json
{
  "schema_version": 1,
  "status": "ok",
  "binary": { "version": "0.3.3", "os": "macos", "architecture": "aarch64" },
  "databases": {
    "annotation": { "status": "readable", "path": "…/AEAnnotation_v10312011_1727_local.sqlite" },
    "library":   { "status": "readable", "path": "…/BKLibrary-1-091020131601.sqlite" }
  },
  "environment": {
    "home": { "status": "ok", "path": "/Users/chenweilong" },
    "default_output_dir": { "status": "ok", "path": "/Users/chenweilong/books-exported", "writable": true },
    "free_bytes": 69784743936
  }
}
```

`free_bytes` 约 65 GB，是一个真实容量值。这条同时是对 review 发现缺陷的
回归证明：修复前 `statvfs` 在 `~/books-exported` 不存在时返回 `ENOENT`，
经 `unwrap_or(0)` 变成 `0`，而每个首次运行的用户都会看到「磁盘已满」。

## 2. `list --json`

- `schema_version: 1`
- 读取到 **70 本书**
- DTO 字段：`asset_id`、`title`、`author`、`note_count`
- 首条：`706DB5A46682C0CA482434189BBACE24` / *100 Go Mistakes and How to Avoid Them* / Teiva Harsanyi

## 3. `annotations --asset-id <id> --json`

- `schema_version: 1`，`annotation_count: 1`
- DTO 字段：`id`、`type`、`content_text`、`note_text`、`chapter_title`、`location`、`created_at`
- 首条 `annotation-688`，`type: highlight`

## 4. `export --asset-id <id> --format markdown --json`

成功返回结构化收据：`asset_id`、`title`、`annotation_count`、`format`、
`output_directory`、`generated_files`。实际产出一个 Markdown 文件。

## 5. 覆盖保护

对同一输出目录再次执行相同导出：

- 退出码 **1**
- 错误码 **`OUTPUT_FILE_EXISTS`**

符合 #14「existing output files」稳定错误码要求。

## 6. 错误流

成功 JSON 走 stdout；结构化错误走 stderr 并以非零码退出（本条即为证据）。

## 未覆盖

- 真实 Full Disk Access **被拒绝**时的路径：本机已授权，无法在不撤销授权的情况下
  观察到 `FULL_DISK_ACCESS_REQUIRED`。该路径由 `tests/machine_cli.rs` 的
  `chmod 000` 夹具覆盖。
- 不兼容二进制（`BINARY_INCOMPATIBLE`）：本机 aarch64/macOS，无法构造。
- 其它架构与其它 Apple Books 数据形态。
