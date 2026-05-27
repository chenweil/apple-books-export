# Apple Books Exporter - Rust 版本

## 项目概述
用 Rust 重写 Python 版本的 Apple Books 笔记导出工具，生成单文件可执行二进制。

## 架构
```
src/
├── main.rs           # CLI 入口
├── models.rs         # 数据结构
├── db.rs             # SQLite 数据访问
├── cfi.rs            # CFI 解析
├── config.rs         # 配置管理
├── cache.rs          # LLM 缓存
├── provider.rs       # LLM API 调用
├── exporter.rs       # Markdown 导出
└── utils.rs          # 工具函数
```

## 关键依赖
- rusqlite (bundled) - SQLite
- serde + serde_json - JSON
- reqwest + rustls-tls - HTTP
- clap (derive) - CLI
- chrono - 时间
- md5 + regex - 工具

## 实现阶段
1. 项目骨架 + list 命令
2. export + cache + provider
3. enrich 完整功能
4. 图片卡片（可选）
5. GUI（可选）

## 当前阶段
阶段 1：创建 Cargo.toml、models.rs、main.rs、config.rs、db.rs、cfi.rs

## 参考文档
- RUST_DESIGN.md - 完整设计方案
- docs/KNOWLEDGE_MODULE_DESIGN.md - Python 版本设计
- services/book_service.py - 数据访问参考
- knowledge/config.py - 配置参考