# #17 AppKit 接入 bundled Rust CLI — 验收记录

日期：2026-09-29
分支：`codex/issue-17-appkit-bridge`（基于 PR #35 合并后的 main `8734de3`）
AppKit 版本：0.3.4（build 14），unsigned DMG

## 结论

#17 的四条验收标准全部达成。AppKit GUI 通过 `RustCLIClient` 调用随包分发的
Rust 二进制，本次验证确认打包产物里的那份确实能读取真实 Apple Books 数据。

## 1. 公开 AppKit 测试覆盖

`appkit/Tests/BooksExporterCoreTests/RustCLIServiceTests.swift`，8 个测试：

- `testProcessRunnerKeepsSuccessfulJSONOnStdoutSeparateFromStderr` — 子进程解析
- `testListBooksUsesMachineJSONAndMapsStableAssetIdentity` — JSON 解析与稳定身份
- `testAnnotationsUseAssetIDAndPreserveNullableMachineFields`
- `testExportReturnsReceiptAndUsesExplicitOutputDirectory` — 导出收据
- `testUnsupportedSchemaVersionFailsExplicitly` — schema 版本
- `testNonzeroExitPreservesStableErrorAndRemediationFromStderr` — 稳定错误
- `testNonzeroExitWithoutStructuredErrorIsNotSilentlyAccepted`
- `testBookServiceUsesRustClientForListAndFullExport`

Full Disk Access 失败的可重试路径由
`BookListViewController` 依据 `FULL_DISK_ACCESS_REQUIRED` 稳定错误码触发，
并用 `hasShownPermissionAlert` 防止重复弹窗。

`swift test` 全量：**47 tests, 0 failures**。

## 2. 打包 AppKit 冒烟（真实数据）

DMG 构建成功，��载后直接使用包内二进制：

```
$ hdiutil attach Books-Exporter-0.3.4-unsigned.dmg
$ "/Volumes/Books Exporter/Books Exporter.app/Contents/Resources/apple-books-exporter" list --json
✅ 读到 70 本书，首本: 100 Go Mistakes and How to Avoid Them
```

标注读取：

```
$ .../apple-books-exporter annotations --asset-id 706DB5A… --json
✅ 读到 1 条标注
```

稳定错误路径：

```
$ .../apple-books-exporter annotations --asset-id NOT_A_REAL_ID --json
退出码: 1
错误码: INVALID_ASSET_ID
remediation: Run `apple-books-exporter list --json` and use an asset_id f…
```

包内 CLI 暴露完整 `speech` 命令家族（voices / profile / generate / play /
export / cache / history）——这是旧 appkit 分支上那份 2026-09-11 构建
无法提供的。

## 3. 修好一个打包脚本缺陷

`package-dmg.sh` 已经算出 `APPKIT_DIR`，但两处 `swift build` 仍依赖调用者的
当前工作目录。从仓库根执行会直接失败：

```
error: Could not find Package.swift in this directory or any of its parent directories.
```

脚本现已显式 `cd "$APPKIT_DIR"`，可从任意目录调用。修复后从仓库根重跑，
成功产出同样的 DMG。

## 4. 构建与验证状态

| 检查 | 结果 |
| --- | --- |
| `swift build` | ✅ |
| `swift test` | ✅ 47 tests, 0 failures |
| Release 构建 | ✅ |
| unsigned DMG 打包 | ✅ `Books-Exporter-0.3.4-unsigned.dmg` |
| DMG 内 CLI 读真实书库 | ✅ 70 本书 / 1 条标注 |
| `latest.json` 更新清单 | ✅ |

## 未执行 / 限制

- **未做 Developer ID 签名与 notarization**，产物为 unsigned，仅适合本机安装验证。
- **未在图形界面中人工点击验证**（未做安装后的人工 UI 走查）；`verify-ui.swift`
  探针脚本存在但本次未执行。
- 未验证真实 Full Disk Access 被拒绝时的界面表现（本机已授权）。
- 未验证语音功能的 GUI 入口——**#17 本身不包含语音界面**，那属于后续 cutover 单。
