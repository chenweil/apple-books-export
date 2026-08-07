# ADR 0004: AppKit 采用稳定版本发现和手动更新

- 状态: 已接受
- 日期: 2026-08-07

## 背景

AppKit 应用通过独立的 unsigned DMG 发布链交付。应用需要让读者知道有新版本，但当前发布物没有 Developer ID 签名或 notarization，因此自动下载、替换和安装会引入超出本功能范围的信任、回滚和失败恢复问题。

同时，Rust/Tauri 主线拥有独立的版本和发布链。本决策只约束当前 AppKit 应用，不把两条发布线合并成一个更新器。

## 决策

### 产品边界

- 本功能只实现 AppKit 的 Version Discovery。
- 发现更新后，读者通过 Manual Update 打开官方 GitHub Release 页面，自行下载和安装。
- 不自动下载、执行、替换或安装 DMG。
- 只检查 Stable Channel；beta、nightly 和其他 prerelease 不参与比较。

### 发布元数据

- 客户端从固定 HTTPS 地址读取 `latest.json`。
- `latest.json` 作为 GitHub Release 资产随版本发布，由打包/发布流程生成并上传。
- 首期 manifest 的最小字段为 `schema_version`、`channel`、`version`、`minimum_macos`、`architectures`、`release_url` 和简短 `notes`。
- `release_url` 必须指向固定的 GitHub 仓库和 HTTPS Release 页面；客户端拒绝其他主机或非 HTTPS 地址。
- 每个发布版本如实声明自己的最低 macOS 版本和 CPU 架构。客户端只把 Compatible Release 呈现为可用更新。

### 版本语义

- 使用 `CFBundleShortVersionString` 作为用户可见的 SemVer 版本进行比较。
- `CFBundleVersion` 只作为构建诊断信息，不单独触发更新提示。
- 只有远程 stable 版本严格高于当前版本时，才算发现新版本。

### 检查和提示

- 正式打包的 App 在主窗口显示后异步检查；后台检查最多每 24 小时一次。
- App 菜单提供“检查更新…”入口；手动检查可以立即刷新，不受后台检查间隔限制。
- 每个远程版本最多主动提示一次；“稍后”不会在同一版本上重复打断读者。
- 后台发现更新使用一次原生 `NSAlert`，提供“查看更新”和“稍后”。
- 手动检查时，如果已经是最新版本，明确显示结果；后台检查保持安静。
- `swift run`、测试和没有有效正式 Bundle 版本的开发运行不执行网络检查。

### 失败和隐私

- 后台检查超时、离线、manifest 缺失、JSON 损坏或 schema 不支持时静默忽略。
- 手动检查失败时显示可操作的“暂时无法检查”结果，但不使用无效或不完整数据。
- 请求使用短超时；后台不连续重试，手动操作由读者决定是否再次检查。
- 不收集版本检查遥测，只请求固定的 HTTPS manifest。

## 后果

- 更新发现不会阻塞 Apple Books 浏览、导出或 Share Card 工作流。
- 发布流程必须在每个 stable Release 中同时提供合法的 `latest.json` 和 DMG，并保持版本、最低系统和架构元数据一致。
- 需要测试 SemVer 比较、manifest 解码、schema 兼容性、最低系统和架构过滤、离线行为以及同一版本的重复提示抑制。
- 需要至少用打包后的 App 做一次真实手动检查；只运行 `swift run` 或单元测试不能证明正式 Bundle 的版本读取和发布链可用。
- 自动更新、签名校验、notarization、强制更新和 beta 通道属于后续独立决策，不由本 ADR 隐式开启。
