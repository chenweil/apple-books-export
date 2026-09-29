# ADR 0001: AppKit 实验分支采用 SPM 纯代码骨架

- 状态: 已接受
- 日期: 2026-03-12

## 背景

`swiftui` 分支已经验证了 Apple Books 数据读取和 SwiftUI 界面，但需要单独验证 AppKit 的窗口、列表和控制器模型。实验不应继续依赖 Xcode project 或 SwiftUI 生命周期。

## 决策

创建 `appkit` 分支，基于 `swiftui` 的 Models、Services 和 Utilities，删除 SwiftUI App、ViewModel、View 以及 Xcode project，改用 Swift Package Manager 管理一个 AppKit executable target。

UI 采用纯 Swift 代码构建：`NSApplication` + `AppDelegate` 负责应用和窗口生命周期，`NSViewController` 负责页面，后续列表使用 AppKit 的 `NSTableView`。

## 后果

- 核心数据访问代码可以继续复用，实验提交保持可独立回退。
- 不再需要 Storyboard、XIB 或 SwiftUI 依赖。
- SPM executable 不会自动生成应用图标、签名和沙盒配置；这些不属于本次 GUI 骨架实验。
- 读取 Apple Books 数据库仍需要用户授予完全磁盘访问权限。
