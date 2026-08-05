# Changelog

本文件记录 AppKit 版本的可见变更。

## v0.1.8 - 2026-08-05

### Share Card 编辑器

- 从选中的 Apple Books 标注进入 Share Card 编辑器，默认直接生成可预览的 1200×1600 卡片。
- 支持自动/固定字号、11 款随应用分发字体、水平/垂直对齐、12 个绑定背景与配色的模板。
- 长正文和长笔记形成连续页面；大预览、页码和有边界缩略图带共享同一组渲染页面。
- 保存全部页面，复制默认当前页并可复制全部页面；AirDrop 发送当前页临时 PNG，不要求先保存。
- 移除泛用 macOS 分享面板；卡片编辑只影响导出内容，不修改原始 Apple Books 标注。
- 补充服务层测试、AppKit UI 探针和生成图片视觉检查记录。

### 发布准备

- 发布候选版本为 `0.1.8`、构建号 `9`，默认产物为 `Books-Exporter-0.1.8-unsigned.dmg`。
- 当前 DMG 未进行 Developer ID 签名或 notarization；真实设备 AirDrop、GitHub Release 和推送需在发布环境单独完成。
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
