---
name: apple-books-export-rust
description: Use when user wants to access Apple Books notes/highlights on macOS using the Rust binary, export book annotations, AI-enrich notes, generate cards, or mentions "Books app", "Apple Books", "Rust版", "cargo build"
---

# Apple Books Export (Rust)

## Overview

Rust 版本的 Apple Books 笔记导出工具。编译后生成单文件二进制，无需 Python 环境。

**核心优势**：单文件二进制 (10MB)，无运行时依赖，启动快 (~10ms)。

## When to Use

User mentions:
- "Apple Books" / "Books app" / "iBooks"
- "导出书籍笔记" / "books notes" / "highlights"
- "AI 增强笔记" / "enrich" / "知识卡片"
- "Rust 版本" / "cargo build" / "编译"
- "生成卡片" / "card"

## Prerequisites

**系统要求**:
- macOS only（Apple Books 数据仅存在于 macOS）
- Rust 1.85+（`rustup update stable`）
- Full Disk Access 权限

**权限设置**:
```
System Settings → Privacy & Security → Full Disk Access → Add Terminal
```

## Build

```bash
cd ~/books-exporter

# 开发构建
cargo build

# Release 构建（推荐）
cargo build --release

# 二进制位置
ls -lh target/release/apple-books-exporter
```

## Quick Reference

| 任务 | 命令 |
|------|------|
| 列出书籍 | `target/release/apple-books-exporter list` |
| 导出笔记 | `target/release/apple-books-exporter export <序号>` |
| 指定输出目录 | `target/release/apple-books-exporter export <序号> -o ~/Desktop` |
| AI 增强（单条） | `target/release/apple-books-exporter enrich <书序号> --index <笔记序号>` |
| AI 增强（全量） | `target/release/apple-books-exporter enrich <书序号> --all` |
| AI 增强（强制刷新） | `target/release/apple-books-exporter enrich <书序号> --all --force` |
| 生成卡片 | `target/release/apple-books-exporter card <书序号> --all` |
| 查看缓存 | `target/release/apple-books-exporter cache <书序号>` |
| 配置 LLM | `target/release/apple-books-exporter config --api-key <key>` |

## Commands

### list - 列出书籍

```bash
apple-books-exporter list
```

输出：序号、书名、作者、笔记数。序号用于后续命令。

### export - 导出笔记

```bash
# 导出第 1 本书
apple-books-exporter export 1

# 指定输出目录
apple-books-exporter export 1 -o ~/Desktop/books

# 指定格式（obsidian 或 markdown）
apple-books-exporter export 1 --format markdown
```

### enrich - AI 增强笔记

需要配置 LLM API（见下方配置章节）。

```bash
# 处理单条笔记
apple-books-exporter enrich 145 --index 1

# 处理整本书（默认前 5 条）
apple-books-exporter enrich 145

# 处理整本书所有笔记
apple-books-exporter enrich 145 --all

# 强制重新生成（跳过缓存）
apple-books-exporter enrich 145 --all --force

# 指定输出目录
apple-books-exporter enrich 145 --all -o ~/obsidian/books
```

### card - 生成图片卡片

```bash
# 生成单条卡片
apple-books-exporter card 145 --index 1

# 批量生成
apple-books-exporter card 145 --all

# 指定样式（dark/light/minimal）
apple-books-exporter card 145 --all --style dark

# 指定输出目录
apple-books-exporter card 145 --all -o ~/cards
```

### cache - 查看缓存

```bash
apple-books-exporter cache 145
```

### config - 配置 LLM

```bash
# 查看当前配置
apple-books-exporter config

# 设置 API Key
apple-books-exporter config --api-key "your-api-key"

# 设置完整配置
apple-books-exporter config \
  --base-url "https://api.openai.com/v1" \
  --api-key "sk-xxx" \
  --model "gpt-4o-mini"
```

## LLM Configuration

AI 增强功能需要配置 LLM API。配置文件：`knowledge_config.json`

```json
{
  "llm": {
    "provider": "openai_compatible",
    "base_url": "https://api.openai.com/v1",
    "api_key": "sk-xxx",
    "model": "gpt-4o-mini",
    "batch_size": 10,
    "max_retries": 3,
    "retry_delays": [1, 2, 4]
  },
  "output_format": "obsidian",
  "context_chars": 200
}
```

支持的 API（OpenAI 兼容）:
- OpenAI (gpt-4o-mini, gpt-4o)
- DeepSeek (deepseek-chat)
- 通义千问
- MiMo (mimo-v2.5-pro)
- Ollama (本地)

## Title Search Workflow

当用户提到书名时：

1. 运行 `list` 命令找到书的序号
2. 用序号执行 export/enrich/card 命令
3. 如果找不到，提示用户检查书名

**示例**:
```
用户: "导出宝典的笔记"
Agent:
  1. apple-books-exporter list | grep "宝典"
  2. 找到: 195. 纳瓦尔宝典 - 218 条笔记
  3. apple-books-exporter export 195 -o ~/Desktop
```

## Data Location

Apple Books 数据存储在：
```
~/Library/Containers/com.apple.iBooksX/Data/Documents/
├── BKLibrary/BKLibrary-*.sqlite      # 书籍元数据
└── AEAnnotation/AEAnnotation_*.sqlite # 笔记/标注数据
```

## Common Issues

| 问题 | 解决方案 |
|------|----------|
| Permission denied | 终端添加 Full Disk Access 权限 |
| No books found | 用户需要在 Apple Books 中添加笔记 |
| LLM 401 错误 | 检查 API Key 配置 |
| LLM 超时 | 检查网络连接，或增加 retry_delays |
| 编译失败 | 运行 `rustup update stable` 升级 Rust |

## Architecture

```
src/
├── main.rs           # CLI 入口 (clap)
├── models.rs         # 数据结构
├── db.rs             # SQLite 数据访问
├── cfi.rs            # EPUB CFI 解析
├── config.rs         # 配置管理
├── cache.rs          # LLM 结果缓存
├── provider.rs       # LLM API 调用
├── exporter.rs       # Markdown 导出
├── card.rs           # 图片卡片生成
├── prompt.rs         # LLM 提示词
└── utils.rs          # 工具函数
```

## Python vs Rust 版本对比

| 特性 | Python | Rust |
|------|--------|------|
| 环境依赖 | Python 3.14 + tkinter | 无（编译后单文件） |
| 启动速度 | ~500ms | ~10ms |
| 二进制大小 | N/A（需要 Python） | 10MB |
| AI 增强 | ✅ | ✅ |
| 图片卡片 | ✅ (Pillow) | ✅ (image crate) |
| GUI | ✅ (PyQt6) | ❌（CLI only） |

## Red Flags

- 用户提到 Apple Books → **先调用此 skill**
- 用户提到书名 → **先 list 找序号，再操作**
- 需要 GUI → **引导到 Python 版本（main 分支）**
- 需要 AI 功能 → **检查 knowledge_config.json 是否配置**
