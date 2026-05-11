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

## 章节名映射方案

当前 Apple Books 注释数据库 `ZAEANNOTATION` 不直接保存章节标题，只保存位置字段 `ZANNOTATIONLOCATION`，格式通常类似：

```text
epubcfi(/6/10[item4]!/4/34/1,:64,:89)
```

其中方括号里的 `item4` 是 EPUB OPF manifest/spine 中的 item id。要把笔记位置显示为章节名，需要结合书籍文件本身解析：

| 步骤 | 说明 |
|------|------|
| 1 | 从 `ZBKLIBRARYASSET.ZPATH` 读取本地 EPUB 路径 |
| 2 | 兼容目录型 EPUB 和 zip 型 EPUB |
| 3 | 读取 `META-INF/container.xml`，找到 OPF 文件路径 |
| 4 | 解析 OPF manifest，建立 `item id -> href` 映射 |
| 5 | 解析 `toc.ncx` 或 EPUB3 `nav`，建立 `href -> 章节标题` 映射 |
| 6 | 从 `ZANNOTATIONLOCATION` 提取 `[item4]` |
| 7 | 用 `item4 -> href -> 章节标题` 得到笔记章节名 |
| 8 | 若任一环节失败，回退显示原始 `epubcfi` 位置 |

已用《巨婴国》验证过该映射链路：

| CFI item | OPF href | 章节名 |
|----------|----------|--------|
| `item4` | `Text/part0003.xhtml` | `自序` |
| `item7` | `Text/part0006.xhtml` | `能量的演变` |
| `item8` | `Text/part0007.xhtml` | `我们集体停留在婴儿期` |
| `item70` | `Text/part0069.xhtml` | `极致的控制欲=恋尸癖` |

实现建议：

| 模块 | 职责 |
|------|------|
| `EPUBChapterResolver` | 根据书籍路径解析 EPUB 目录，缓存 `item id -> chapter title` |
| `DatabaseService` | 读取 `ZPATH` 并在加载 annotations 时调用 resolver |
| `Annotation` | 保留 `chapterTitle` 和 `locationInfo`，显示时优先章节名，失败时使用原始位置 |

注意事项：

| 风险 | 处理方式 |
|------|----------|
| 书籍未下载到本地或 iCloud 文件不可读 | 保持当前 `epubcfi` fallback |
| EPUB 只有 `toc.ncx` 或只有 EPUB3 `nav` | 两种目录格式都支持 |
| 某个 spine 文件没有直接目录项 | 使用当前 spine 前面最近的目录项作为 fallback |
| DRM 或非 EPUB 内容不可解析 | 不阻塞预览和导出，只显示原始位置 |
