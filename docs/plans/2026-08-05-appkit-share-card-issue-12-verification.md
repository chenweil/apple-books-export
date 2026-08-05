# #12 Share Card 输出操作验证记录

日期：2026-08-05

## 验证边界

本票的输出操作都消费编辑器同一组 `renderedPageResults`：

- 保存使用全部页面，并保留页面顺序、分页数量、1200 × 1600 尺寸和既有文件命名。
- 主复制操作使用当前选中页；复制菜单使用完整页面序列。
- AirDrop 使用当前选中页生成临时 PNG，不依赖先保存。
- AirDrop 不可用时保留原位置的按钮并禁用；泛用 `NSSharingServicePicker` 不属于该界面。

## 自动化证据

- `ShareCardServiceTests.testCurrentPageTemporaryPNGAndAllPageExportPreservePageScopeAndOrder` 已通过，使用渲染图片的可观察内容验证当前页临时 PNG与全部页面导出的范围差异、顺序、尺寸和文件名。
- `Scripts/verify-ui.swift` 已补充真实 AppKit 控件断言：主复制只传一张、复制菜单传完整序列，并比较当前选中页在完整序列中的内容位置；同时确认没有泛用分享入口，关闭按钮仍可见。
- 本次会话未能完成 `Scripts/verify-ui.sh` 的运行：直接编译先被用户级 Clang ModuleCache 权限拒绝；切换到临时 module cache 后，又被当前 Swift 6.2 直接编译路径中既有的 AppKit/Foundation 类型解析问题拦截（`UserDefaults`、`NotificationCenter`、`FileManager` 等），探针未启动。因此不把 UI 结果记为通过。
- 服务层验证 AirDrop 的临时 PNG 输入路径；它不把保存结果作为前置条件。

## 真实设备限制

本次自动化环境未执行物理设备 AirDrop 接收确认，因此不宣称发送成功。当前可确认的是控件位置、可用性禁用状态、当前页临时 PNG生成和发送调用边界；发布验收仍需在可用的 macOS 与已配对接收设备上手动选择非首页并确认：

1. AirDrop 按钮位于原分享位置且可用。
2. 接收端只收到当前选中页，而不是全部页面。
3. 接收 PNG 可打开且为 1200 × 1600；文件名遵循当前页编号约定。
