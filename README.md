# Apple Books 笔记导出工具

Rust 版本 Apple Books 笔记导出工具，支持 CLI 和 GUI，可导出笔记、高亮、书签为 Markdown 文件，并支持 AI 增强和图片卡片生成。

## 功能

- 📚 列出 Apple Books 书库中所有做过笔记的书籍
- 📝 导出笔记、高亮、书签为 Markdown
- 🔍 按书名搜索导出（模糊匹配）
- 🤖 AI 增强：为笔记添加解释、标签、复习问题
- 🎴 图片卡片：生成精美的知识卡片
- 🖥️ GUI 支持：Tauri 跨平台桌面应用
- ⌨️ TUI 支持：OpenTUI 只读搜索和浏览书籍
- 🤖 AI Agent Skill：支持 AI 助手直接调用

## 系统要求

- macOS（Apple Books 数据仅存在于 macOS）
- Full Disk Access 权限

## 快速开始

### 方式一：使用 Skill（推荐）

适合 AI Agent 用户，安装后可直接被 Claude Code、Gemini CLI 等调用。

```bash
# 1. 克隆项目
git clone https://github.com/chenweil/apple-books-export.git
cd apple-books-export

# 2. 编译二进制（首次需要）
cargo build --release
cp target/release/apple-books-exporter skills/apple-books-export-rust/scripts/

# 3. 安装 skill
cd skills/apple-books-export-rust/scripts
./install.sh

# 4. 验证安装
~/.agents/skills/apple-books-export-rust/scripts/apple-books-exporter list
```

### 方式二：直接使用二进制

```bash
# 编译
cargo build --release

# 运行
./target/release/apple-books-exporter list
./target/release/apple-books-exporter export 1 -o ~/Desktop
```

### 方式三：GUI 应用

```bash
# 开发模式
npm install
npm run tauri dev

# 构建 macOS 应用
npm run tauri build
```

### 方式四：TUI（只读）

当前 TUI 第一阶段支持搜索、浏览书籍和查看笔记数量，不执行导出、AI、卡片或配置写操作。

```bash
# 首次使用
cargo build
cd tui && bun install && cd ..

# 启动
npm run tui

# 验证
npm run test:tui
npm run typecheck:tui
```

默认依次查找 `target/release/apple-books-exporter`、`target/debug/apple-books-exporter`
和 `PATH`。也可通过 `APPLE_BOOKS_EXPORTER_BIN` 指定后端二进制。

## CLI 命令

### 列出书籍

```bash
apple-books-exporter list

# 稳定的机器可读协议（供 TUI 等客户端使用）
apple-books-exporter list --json
```

### 导出笔记

```bash
# 按序号导出
apple-books-exporter export 1

# 按书名搜索导出
apple-books-exporter export -t "纳瓦尔"

# 指定输出目录
apple-books-exporter export 1 -o ~/Desktop

# 指定格式
apple-books-exporter export 1 --format obsidian
```

### AI 增强笔记

需要先配置 LLM API：

```bash
# 配置
apple-books-exporter config --api-key "sk-xxx" --model "gpt-4o-mini"

# 处理单条笔记
apple-books-exporter enrich 1 --index 42

# 处理整本书
apple-books-exporter enrich 1 --all

# 强制重新生成
apple-books-exporter enrich 1 --all --force
```

### 生成图片卡片

```bash
# 批量生成
apple-books-exporter card 1 --all

# 指定样式
apple-books-exporter card 1 --all --style dark  # dark/light/minimal
```

## AI Agent Skill

本项目提供 skill，支持 AI 助手直接调用。

### Skill 文件

```
skills/apple-books-export-rust/
├── SKILL.md                              # Skill 文档
└── scripts/
    ├── apple-books-exporter              # 默认二进制
    ├── apple-books-exporter-aarch64-apple-darwin  # macOS ARM
    ├── build.sh                          # 编译脚本
    └── install.sh                        # 安装脚本
```

### 编译二进制（必需）

Skill 需要二进制文件才能读取 Apple Books 数据。首次使用前必须编译：

```bash
# 方式一：使用编译脚本
cd skills/apple-books-export-rust/scripts
./build.sh

# 方式二：手动编译
cargo build --release
cp target/release/apple-books-exporter skills/apple-books-export-rust/scripts/
```

### 安装 Skill

```bash
cd skills/apple-books-export-rust/scripts
./install.sh
```

安装脚本会：
1. 检测当前系统平台
2. 复制二进制文件到 `~/.agents/skills/apple-books-export-rust/scripts/`
3. 复制 SKILL.md 文档

### 跨平台编译

```bash
# macOS ARM (M1/M2/M3)
cargo build --release --target aarch64-apple-darwin

# macOS Intel
cargo build --release --target x86_64-apple-darwin

# Linux x86_64
cargo build --release --target x86_64-unknown-linux-gnu
```

编译后复制到 scripts 目录并重命名：
```bash
cp target/<target>/release/apple-books-exporter \
   skills/apple-books-export-rust/scripts/apple-books-exporter-<target>
```

### 使用示例

安装 skill 后，AI 助手可以直接调用：

```
用户: 导出纳瓦尔宝典的笔记
AI: 正在搜索...
    找到：纳瓦尔宝典 - 218 条笔记
    已导出到 ~/Desktop/纳瓦尔宝典_xxxxx.md
```

## LLM 配置

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
  "output_format": "obsidian"
}
```

支持的 API：
- OpenAI (gpt-4o-mini, gpt-4o)
- DeepSeek (deepseek-chat)
- 通义千问 (qwen-turbo)
- MiMo (mimo-v2.5-pro)
- Ollama (本地)

## 项目结构

```
apple-books-export/
├── src/                           # Rust 核心库
│   ├── main.rs                    # CLI 入口
│   ├── db.rs                      # SQLite 数据访问
│   ├── models.rs                  # 数据结构
│   ├── exporter.rs                # Markdown 导出
│   ├── provider.rs                # LLM API 调用
│   ├── cache.rs                   # LLM 结果缓存
│   ├── card.rs                    # 图片卡片生成
│   └── ...
├── src-tauri/                     # Tauri GUI
│   ├── src/main.rs                # Tauri 入口
│   └── tauri.conf.json            # Tauri 配置
├── tui/                           # OpenTUI 只读终端界面
│   ├── src/                       # Core API 应用、后端协议与测试
│   └── package.json               # Bun 脚本与 OpenTUI 依赖
├── src/lib/                       # Svelte 前端
│   ├── pages/                     # 页面组件
│   └── components/                # UI 组件
├── skills/                        # AI Agent Skill
│   └── apple-books-export-rust/
│       ├── SKILL.md               # Skill 文档
│       └── scripts/               # 二进制和脚本
└── knowledge_config.json          # LLM 配置
```

## 数据来源

Apple Books 的笔记数据存储在：
```
~/Library/Containers/com.apple.iBooksX/Data/Documents/
├── BKLibrary/BKLibrary-*.sqlite      # 书籍元数据
└── AEAnnotation/AEAnnotation_*.sqlite # 笔记/标注数据
```

## macOS 权限

如果遇到"无法读取数据"提示，确保在 **系统设置 → 隐私与安全性 → 完全磁盘访问权限** 中给予终端或二进制文件完全磁盘访问权限。

## 技术栈

- **后端**: Rust + Tauri 2.0
- **前端**: Svelte 5 + TypeScript
- **数据库**: rusqlite (bundled)
- **HTTP**: reqwest + tokio
- **图片**: image + rusttype

## Python 版本

Python 版本已归档到 `python-legacy` 分支，如需使用：

```bash
git checkout python-legacy
```

## License

MIT
