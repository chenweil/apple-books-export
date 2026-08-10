# AppKit 子项目

详细说明、构建命令、架构选择见仓库根 `README.md`。

AppKit 通过 Rust CLI 的 machine JSON 协议读取书籍、标注并执行整本书导出。
`DatabaseService` 仅保留作迁移期兼容和历史测试，不再是 AppKit 默认数据源。

## 快速运行

```bash
# 在 Rust mainline checkout 中构建 canonical Rust CLI
cd /path/to/rust-mainline
cargo build --release
cd /path/to/books-exporter/appkit
APPLE_BOOKS_EXPORTER_BIN="/path/to/rust-mainline/target/release/apple-books-exporter" \
  swift run BooksExporter
```

打包应用会把 Rust binary 放在
`Books Exporter.app/Contents/Resources/apple-books-exporter`。本地运行时也可以把
`APPLE_BOOKS_EXPORTER_BIN` 指向 `target/debug/apple-books-exporter`；应用不会自动下载
或执行未知 binary。

unsigned DMG 打包时，脚本默认读取 `../target/release/apple-books-exporter`；如果 Rust
binary 在其他 checkout，请传入 `RUST_CLI_BIN=/path/to/apple-books-exporter`。
