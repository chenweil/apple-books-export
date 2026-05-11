# Apple Books 笔记导出工具 (SwiftUI 版本)

macOS 原生应用，导出 Apple Books 中的笔记、标注、书签为 Markdown 文件。

## 系统要求

- macOS 14.0+
- Xcode 15.0+

## 构建

```bash
cd books-exporter-swift
xcodebuild -project books-exporter.xcodeproj -scheme BooksExporter -configuration Release
```

## 使用

1. 双击运行 `BooksExporter.app`
2. 如果需要完全磁盘访问权限，跟随提示在系统设置中添加
3. 应用会自动列出所有有笔记的书籍
4. 点击书籍查看详情
5. 预览或导出笔记

## 与原 Python 版本的对比

| 项目 | Python 版本 | SwiftUI 版本 |
|------|------------|-------------|
| 应用体积 | ~50-100 MB | ~5-15 MB |
| 启动时间 | 2-3 秒 | <1 秒 |
| 依赖 | Python 3.14, PySimpleGUI | 无 |
| 原生体验 | 一般 | 优秀 |
| 可上架 App Store | 否 | 是 |

## 开发

### 项目结构

```
books-exporter-swift/
├── BooksExporter/
│   ├── Models/           # 数据模型
│   ├── Services/         # 数据库 + 业务逻辑
│   ├── ViewModels/       # MVVM 状态管理
│   ├── Views/            # SwiftUI 视图
│   └── Utilities/        # 工具类
└── books-exporter.xcodeproj
```

### macOS 权限

需要在`Info.plist`中配置：

```xml
<key>NSAppleEventsUsageDescription</key>
<string>需要访问 Apple Books 数据</string>
```

用户需要在`系统设置 → 隐私与安全性 → 完全磁盘访问权限`中添加应用。

## 导出格式

与 Python 版本保持一致的 Markdown 格式：

```markdown
# 书名

**作者**: 作者名

**笔记数量**: N

---

## 高亮与标注

### 1. 位置: epubcfi(...)
*2025-01-15 10:30*

> 高亮文字

**笔记**: 我的笔记内容

---
```
