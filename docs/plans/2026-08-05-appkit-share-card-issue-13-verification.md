# #13 Share Card 集成与发布验收记录

- 验收日期：2026-08-05
- 版本：`v0.1.8` / `build 9`
- 范围：AppKit Share Card 真实详情入口、排版、模板、多页预览、页面动作、笔记样式和发布收口

## 结果摘要

| 验收项 | 结果 | 证据或限制 |
| --- | --- | --- |
| 详情控制器入口进入编辑器并生成默认预览 | 通过 | `verify-ui.sh` 使用代表性 `Annotation` fixture 通过真实 `BookDetailViewController`、`BookDetailView` 和 `ShareCardEditorViewController` 链路；默认预览为 1200×1600、页码为第 1 / 1 页；live Apple Books 数据库未在探针中读取 |
| 排版、模板、背景、分页和当前页协同 | 通过 | UI 探针覆盖 12 个模板、12 个主题缩略图、字体/字号/水平/垂直对齐和 37 页固定字号长文 |
| 页面动作不混淆状态 | 通过 | 服务测试和 UI 探针分别覆盖当前页复制、全部页复制、全部页面导出和当前页 AirDrop 临时 PNG |
| 笔记视觉层级 | 通过 | 笔记沿用正文颜色，使用轻微斜体；正文与首段笔记之间有细分割线，续页不重复显示；31 个 XCTest 覆盖布局契约和渲染路径 |
| 短文、恰好一页、跨页、极长文和中英文混排 | 通过 | `ShareCardServiceTests` 覆盖短文、固定字号长文、自动字号分页、长笔记续页和混排完整性 |
| 目标窗口布局稳定 | 通过 | UI 探针覆盖主题/候选/缩略图内部滚动、预览比例、完成入口和长文下的页面钳制 |
| 服务、AppKit UI 和图片视觉验证 | 通过 | 见下方命令与视觉记录；编译警告为既有 `Selector` 建议，不影响结果 |
| 原始标注、尺寸、顺序和命名 | 通过 | 原始 `Annotation` 不变；服务测试断言 1200×1600、连续页顺序和单页/多页文件名 |
| 真实设备 AirDrop | 未执行 | 当前环境的 AirDrop 服务可用性由探针检查，但未在两台真实设备间发送 |
| README、CHANGELOG、发布说明和构建物 | 通过 | 已同步 `v0.1.8 build 9`，unsigned DMG 已生成并发布到 GitHub Release |
| 发布前工作区阻塞项 | 已声明 | 未跟踪 `.pi-glla/`、`.pi-subagents/` 为既有外部目录，未纳入本任务；签名、notarization 和真实 AirDrop 是已记录的环境限制 |

## 可复现验证

在 `appkit/` 目录执行：

```bash
swift test
./Scripts/verify-ui.sh
```

本次执行结果：

- `swift test`：31 个 XCTest 全部通过，0 失败，退出码 0。
- `./Scripts/verify-ui.sh`：全部 UI 检查通过，包含详情控制器入口打开编辑器、默认预览、主题/字体、多页缩略图、当前页/全部页复制、防抖和完成入口。
- UI 探针有一条既有编译警告：建议使用 `#selector` 替代显式 `Selector`，未导致失败。

## 图片视觉检查

使用服务测试生成默认卡片：

```bash
rm -rf /private/tmp/books-exporter-share-card-visual
mkdir -p /private/tmp/books-exporter-share-card-visual
SHARE_CARD_INSPECTION_DIR=/private/tmp/books-exporter-share-card-visual \
  swift test --filter ShareCardServiceTests.testHighlightCreatesReadableDefaultCardAndPNG
```

检查文件：

- `/private/tmp/books-exporter-share-card-visual/share-card.png`
- PNG 尺寸：`1200×1600`
- SHA-256：`2e4239828c42a7e74e8b8dfb265d2db1aa65e5fab1ae5326c5add92152f06ba1`
- 人工检查：默认短文正文、署名、3:4 留白和背景纹理均可读，未见裁切或溢出

笔记样式视觉检查：

- `/private/tmp/books-exporter-share-card-note.tY4LpZ/share-card-note.png`
- PNG 尺寸：`1200×1600`
- SHA-256：`c9ab0989c18ce5c5649aaed19ba2ae3b845fd74f7121485a2e9a1d3381bd2fa4`
- 人工检查：笔记与正文保持同色，斜体效果可见，细分割线位于正文和首段笔记之间，续页没有重复分割线。

模板素材来源、制作方式和 SHA-256 记录见
[`docs/assets/share-card-backgrounds/SOURCES.md`](../assets/share-card-backgrounds/SOURCES.md)。

## 发布物

```bash
cd appkit
./Scripts/package-dmg.sh
```

脚本默认使用 `APP_VERSION=0.1.8`、`BUILD_VERSION=9`，生成：
`dist/Books-Exporter-0.1.8-unsigned.dmg`。

本次生成物大小为 165999318 bytes，SHA-256 为
`8f50f7d3af4473e5e17e59b64ad947a83020c3c2b2f2045e4633328a981f4371`；挂载后
Info.plist 确认为 `CFBundleShortVersionString=0.1.8`、`CFBundleVersion=9`。

该 DMG 只证明本地构建、资源打包和 Info.plist 版本替换成功，不证明 Developer ID
签名、notarization、真实 Apple Books 数据读取或 AirDrop 设备间发送。发布地址：
[GitHub Release v0.1.8](https://github.com/chenweil/apple-books-export/releases/tag/v0.1.8)。
