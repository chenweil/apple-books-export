# Apple Books Exporter - Rust 版本设计方案

**日期**: 2026-05-27
**状态**: 设计阶段
**分支**: rust-version
**目标**: 用 Rust 重写 Python 版本，生成单文件可执行二进制

---

## 1. 项目背景

### 当前 Python 版本问题
| 问题 | 描述 |
|------|------|
| 环境依赖 | 需要 Python 3.14 + python-tk@3.14 + PySimpleGUI |
| 安装复杂 | 需要 Homebrew 安装 Python，pip 安装依赖 |
| 启动慢 | Python 解释器启动 + 模块加载 |
| GUI 兼容 | macOS 15.x 与 Xcode Python 3.9 tkinter 不兼容 |
| 分发困难 | 需要用户安装 Python 环境 |

### Rust 版本优势
| 优势 | 描述 |
|------|------|
| 单文件二进制 | `cargo build --release` 生成一个可执行文件 |
| 无运行时依赖 | 不需要 Python、Java 等运行时 |
| 启动快 | 原生编译，毫秒级启动 |
| 跨平台 | macOS/Linux/Windows 均可编译 |
| 性能更好 | 内存安全 + 零成本抽象 |

---

## 2. 架构设计

### 2.1 模块划分

```
src/
├── main.rs           # CLI 入口，命令解析
├── db.rs             # SQLite 数据访问层
├── cfi.rs            # EPUB CFI 解析
├── config.rs         # 配置管理 (JSON)
├── cache.rs          # LLM 结果缓存
├── provider.rs       # LLM API 调用 (OpenAI 兼容)
├── exporter.rs       # Markdown/Obsidian 导出
├── models.rs         # 数据结构定义
└── utils.rs          # 工具函数

Cargo.toml            # 项目配置
config.json           # 用户配置 (运行时生成)
```

### 2.2 数据流

```
┌─────────────────────────────────────────────────────────────┐
│                        CLI (main.rs)                         │
│  Commands: list, export, enrich, card, cache, config         │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                      DB Layer (db.rs)                        │
│  - 连接 ~/Library/Containers/com.apple.iBooksX/.../*.sqlite  │
│  - 查询书籍列表、笔记数据                                    │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    CFI Parser (cfi.rs)                       │
│  - extract_item_id(cfi) -> "item4"                           │
│  - extract_chapter_title(cfi) -> "第3章"                     │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                   LLM Provider (provider.rs)                 │
│  - OpenAI Compatible API                                     │
│  - 重试机制 (指数退避)                                       │
│  - batch_size = 10                                           │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Cache (cache.rs)                          │
│  - JSON 文件持久化                                           │
│  - Key = {book_id[:8]}_{md5(highlight)}                      │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                   Exporter (exporter.rs)                     │
│  - Obsidian 格式 (带 wikilink)                               │
│  - Markdown 格式 (纯文本)                                    │
│  - 图片卡片 (可选，需要 image 库)                            │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. 数据结构设计

### 3.1 models.rs

```rust
// 书籍
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub asset_id: String,
    pub title: String,
    pub author: String,
    pub note_count: u32,
}

// 笔记/高亮
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub asset_id: String,
    pub selected_text: Option<String>,
    pub note: Option<String>,
    pub location: Option<String>,  // CFI
    pub annotation_type: u32,      // 0=bookmark, 1=note, 2=highlight, 3=annotation
    pub creation_date: Option<f64>,
}

// LLM 处理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResult {
    pub explanation: String,
    pub tags: Vec<String>,
    pub question: String,
}

// 缓存条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub highlight: String,
    pub file: String,
    pub book_id: String,
    pub book: String,
    pub explanation: String,
    pub tags: Vec<String>,
    pub question: String,
    pub updated: String,  // ISO date
}

// LLM 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub batch_size: u32,
    pub max_retries: u32,
    pub retry_delays: Vec<u64>,
}

// 主配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub llm: LLMConfig,
    pub epub_mappings: std::collections::HashMap<String, EpubMapping>,
    pub output_format: String,
    pub card_style: String,
    pub card_output: String,
    pub context_chars: u32,
    pub filename_max_length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpubMapping {
    pub epub: String,
    pub output: String,
}
```

---

## 4. 依赖选择

### 4.1 Cargo.toml

```toml
[package]
name = "apple-books-exporter"
version = "0.1.0"
edition = "2021"
description = "Export Apple Books notes and highlights to Markdown"

[dependencies]
# SQLite
rusqlite = { version = "0.31", features = ["bundled"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# HTTP Client
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
tokio = { version = "1", features = ["full"] }

# CLI
clap = { version = "4.5", features = ["derive"] }

# Utilities
chrono = { version = "0.4", features = ["serde"] }
md5 = "0.7"
regex = "1.10"
thiserror = "1.0"
anyhow = "1.0"
home = "0.5"

# Optional: Image cards
# image = "0.25"  # 可选，默认不启用

[dev-dependencies]
tempfile = "3.10"
```

### 4.2 依赖说明

| 依赖 | 用途 | 备注 |
|------|------|------|
| rusqlite + bundled | SQLite 访问 | bundled 特性包含 libsqlite3，无需系统依赖 |
| serde + serde_json | JSON 序列化 | derive 特性自动生成序列化代码 |
| reqwest + rustls-tls | HTTP 客户端 | rustls-tls 避免 OpenSSL 依赖 |
| tokio | 异步运行时 | reqwest 需要 |
| clap + derive | CLI 参数解析 | derive 特性生成命令代码 |
| chrono | 时间处理 | Apple 时间戳转换 |
| md5 | 缓存 key | 替代 Python hashlib |
| regex | CFI 解析 | 替代 Python re |
| thiserror + anyhow | 错误处理 | thiserror 定义错误，anyhow 传递 |
| home | 路径查找 | 替代 Python pathlib |

---

## 5. CLI 命令设计

### 5.1 命令结构

```
apple-books-exporter <COMMAND>

Commands:
  list       列出所有有笔记的书籍
  export     导出笔记为 Markdown
  enrich     AI 增强笔记（调用 LLM）
  card       导出图片卡片
  cache      查看缓存状态
  config     配置 LLM 和输出选项
  help       打印帮助信息
```

### 5.2 命令详情

```bash
# 列出书籍
apple-books-exporter list

# 导出笔记
apple-books-exporter export <INDEX> [OPTIONS]
  -o, --output <DIR>      输出目录
  -f, --format <FORMAT>   格式 (obsidian|markdown)

# AI 增强
apple-books-exporter enrich <INDEX> [OPTIONS]
  --all                   处理整本书
  --index <N>             处理单条
  --force                 强制重新生成
  --output <DIR>          输出目录
  --format <FORMAT>       格式

# 图片卡片
apple-books-exporter card <INDEX> [OPTIONS]
  --all                   批量导出
  --index <N>             单条
  --style <STYLE>         样式 (dark|light|minimal)
  --output <DIR>          输出目录

# 查看缓存
apple-books-exporter cache <INDEX>

# 配置
apple-books-exporter config [OPTIONS]
  --base-url <URL>
  --api-key <KEY>
  --model <MODEL>
```

---

## 6. 实现计划

### 阶段 1: 项目骨架 (本周)
- [ ] 创建 Cargo.toml
- [ ] 创建 src/models.rs (数据结构)
- [ ] 创建 src/main.rs (CLI 骨架)
- [ ] 创建 src/config.rs (配置加载)
- [ ] 创建 src/db.rs (SQLite 读取)
- [ ] 创建 src/cfi.rs (CFI 解析)
- [ ] 验证：`cargo run -- list` 能列出书籍

### 阶段 2: 基础功能 (下周)
- [ ] 实现 export 命令
- [ ] 实现 cache 模块
- [ ] 实现 provider 模块 (LLM API)
- [ ] 验证：`cargo run -- export 1` 能导出笔记

### 阶段 3: AI 增强 (第三周)
- [ ] 实现 enrich 命令
- [ ] 实现批量处理
- [ ] 实现增量处理（跳过缓存）
- [ ] 验证：`cargo run -- enrich 1 --all` 能处理整本书

### 阶段 4: 图片卡片 (可选)
- [ ] 添加 image 依赖
- [ ] 实现 card 命令
- [ ] 支持样式模板
- [ ] 验证：`cargo run -- card 1 --all` 能导出图片

### 阶段 5: GUI (可选，后续)
- [ ] 使用 egui 或 iced 创建 GUI
- [ ] 或使用 TUI (ratatui) 创建终端界面

---

## 7. 与 Python 版本对比

| 功能 | Python 版本 | Rust 版本 |
|------|-------------|-----------|
| 列出书籍 | ✅ | ✅ |
| 导出 Markdown | ✅ | ✅ |
| AI 增强 | ✅ | ✅ |
| 图片卡片 | ✅ (Pillow) | 可选 (image) |
| GUI | ✅ (PySimpleGUI/PyQt) | 可选 (egui/iced) |
| 增量处理 | ✅ | ✅ |
| 缓存 | ✅ | ✅ |
| 配置管理 | ✅ | ✅ |

---

## 8. 性能预期

| 指标 | Python | Rust (预期) |
|------|--------|-------------|
| 启动时间 | ~500ms | ~10ms |
| 内存占用 | ~50MB | ~5MB |
| SQLite 查询 | ~10ms | ~1ms |
| 单条 LLM 调用 | ~2s | ~2s (网络瓶颈) |
| 100 条笔记处理 | ~3-5min | ~3-5min (LLM 瓶颈) |

---

## 9. 已知挑战和解决方案

### 9.1 SQLite 路径
**问题**: Apple Books 数据库路径复杂，有多个版本文件
**方案**: 使用 glob 匹配 `BKLibrary-*.sqlite`，取最新的

### 9.2 Apple 时间戳
**问题**: Apple CoreData 时间戳从 2001-01-01 UTC 开始
**方案**: `chrono` 计算偏移：`DateTime::UNIX_EPOCH + (timestamp - 978307200) * 秒`

### 9.3 CFI 解析
**问题**: CFI 格式复杂，包含特殊字符
**方案**: 复用 Python 版本的正则表达式逻辑

### 9.4 异步 vs 同步
**问题**: reqwest 是异步的，但 CLI 通常是同步的
**方案**: 使用 `tokio::runtime::Runtime` 在 main 中运行异步代码

### 9.5 错误处理
**问题**: Rust 错误处理与 Python 不同
**方案**: 
- 使用 `thiserror` 定义自定义错误类型
- 使用 `anyhow` 在业务逻辑中传递错误
- CLI 层使用 `Result` 返回，由 main 打印

---

## 10. 测试策略

### 10.1 单元测试
```rust
// tests/db_test.rs
#[test]
fn test_list_books() { ... }

// tests/cfi_test.rs
#[test]
fn test_extract_item_id() { ... }

// tests/config_test.rs
#[test]
fn test_load_config() { ... }
```

### 10.2 集成测试
```bash
# 测试完整流程
cargo run -- list
cargo run -- export 1 --output /tmp/test
cargo run -- enrich 1 --index 1 --force
```

---

## 11. 发布计划

### 11.1 开发版本
```bash
cargo build --release
./target/release/apple-books-exporter list
```

### 11.2 发布版本
```bash
# macOS
cargo build --release --target aarch64-apple-darwin  # Apple Silicon
cargo build --release --target x86_64-apple-darwin   # Intel

# 打包
tar -czf apple-books-exporter-macos.tar.gz \
    target/release/apple-books-exporter \
    README.md \
    config.json.example
```

### 11.3 GitHub Release
- 提供 macOS ARM/Intel 二进制
- 提供 Linux x86_64 二进制
- 附带 README 和配置示例

---

## 12. 后续扩展

### 12.1 GUI 界面
- **egui**: 轻量级，Rust 原生
- **iced**: Elm 风格，函数式
- **ratatui**: TUI，终端界面

### 12.2 新功能
- 笔记搜索和过滤
- 批量标签管理
- 笔记合并和去重
- EPUB 自动下载（无 DRM）

### 12.3 集成
- Obsidian 插件
- Logseq 支持
- Notion 导出

---

**下一步**: 确认设计方案后，开始实现阶段 1（项目骨架）。