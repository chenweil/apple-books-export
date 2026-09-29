# Knowledge Module 设计方案

**日期**: 2026-05-12
**状态**: 已评估，待实现
**版本**: v1.3

---

## 1. 核心需求

**问题**: Apple Books 高亮只存储选中文本，没有上下文，缺乏上下文的笔记难以回忆和理解。

**目标**: 为高亮笔记补充上下文 + LLM 摘要/标签/问题，输出到 Obsidian 或纯 Markdown 进行知识管理。

**使用场景**:
- 自己回顾：打开笔记，秒回当时心境
- 知识管理：自动标签 + LLM 解释 + 复习问题
- 增量更新：只处理新增高亮，避免重复处理

---

## 2. 技术验证结论

已有验证（见 `CONTEXT_EXTRACTION_TEST.md`）：

| 项目 | 结论 |
|------|------|
| CFI 解析 | 可行，通过 `selected_text` 全文搜索比解析 CFI 更可靠 |
| 上下文提取 | 303 条高亮，100% 定位成功 |
| EPUB 依赖 | 需要用户提供未加密 EPUB |
| DRM | Apple Books Store 购买的有 FairPlay DRM，需用户自行解决 |

---

## 3. 模块定位

**功能独立，代码复用**。knowledge 模块依赖现有 `services/` 层，不绕开已有基础设施。

### 3.1 复用关系

| 已有代码 | 位置 | knowledge 模块如何复用 |
|----------|------|------------------------|
| `get_books_with_notes()` | `books_exporter.py:139-170` | 通过 `book_service` 获取书籍列表 |
| `get_annotations_for_book()` | `books_exporter.py:172-227` | 通过 `book_service` 获取高亮数据 |
| `book_service.py` | `services/book_service.py` | 统一的数据访问层（cache + 分类） |
| 章节标题提取（CFI 解析） | `books_exporter.py:38-115` | 抽取到 `cfi_utils.py` 共用 |
| item ID 提取 | `test_context_extraction.py:41-53` | 抽取到 `cfi_utils.py` 共用 |
| `extract_context()` | `test_context_extraction.py` | 直接搬到 `knowledge/context.py` |
| `format_context_card()` | `test_context_extraction.py` | 直接搬到 `knowledge/exporter.py` |

### 3.2 目录结构

```
books-exporter/
├── books_exporter.py       ← 已有，不修改
├── knowledge.py            ← CLI 入口（根目录，和 books_exporter.py 同级）
├── services/
│   ├── book_service.py     ← 已有，knowledge 模块的数据入口
│   └── cfi_utils.py        ← 新抽离：统一 CFI 解析（章节 + item ID）
├── gui/                    ← 不动
└── knowledge/              ← 新模块（依赖 services/）
    ├── __init__.py
    ├── config.py           ← 配置读写
    ├── provider/
    │   ├── __init__.py
    │   ├── base.py         ← LLMProvider 基类
    │   └── openai_compat.py← OpenAI 兼容接口
    ├── context.py          ← EPUB 上下文提取 + 元数据提取
    ├── enricher.py         ← LLM 标签/摘要/问题
    ├── cache.py            ← llm_cache.json 读写
    ├── exporter.py         ← 输出格式化（Obsidian / Markdown）
    ├── card.py             ← 图片卡片生成（Pillow）
    └── styles/             ← 卡片样式模板
        ├── dark.json
        ├── light.json
        └── minimal.json
```

### 3.3 小重构：抽取 cfi_utils.py

将两套 CFI 解析逻辑统一到 `services/cfi_utils.py`：

```python
# services/cfi_utils.py

def extract_item_id(cfi: str) -> str | None:
    """从 CFI 提取 manifest item ID（用于 EPUB 定位）
    来源: test_context_extraction.py:41-53
    """
    ...

def extract_chapter_title(cfi: str, epub) -> str | None:
    """从 CFI 提取章节标题（中文/章节号/语义过滤）
    来源: books_exporter.py:38-115
    """
    ...
```

- `books_exporter.py` 改为调用 `cfi_utils`（不改逻辑，只改导入）
- `knowledge/context.py` 直接调用 `cfi_utils`
- 零重复代码

---

## 4. 功能设计

### 4.1 三种模式

| | 单条 | 批量 | 增量（默认） |
|------|------|------|------|
| 触发 | `enrich --book 1 --index 42` | `enrich --book 1 --all` | `enrich --book 1` |
| 上下文 | 有 EPUB 时提取 | 同左 | 同左 |
| LLM | 单条调用 | 批量打包（10 条/次） | 只处理未缓存的 |
| 缓存 | 写入 cache.json | 同左 | 同左 |
| 输出 | 1 个 LLM 笔记 + 更新主笔记 | N 个笔记 + 整本主笔记 | 只输出新增笔记 |
| 场景 | 读到某条想深挖 | 首次导入 | 定期整理新增 |

**默认行为**: `enrich --book 1` 默认增量模式，只处理未缓存的高亮。加 `--all` 强制全量重新处理。

**手动刷新**: `enrich --book 1 --index 42 --force` 强制重新生成某条（清除缓存后重新调 LLM）。

### 4.2 数据流

```
输入参数 (--book 1 --index 42)
    │
    ▼
┌──────────────────┐
│ 1. 读取高亮数据   │  ← book_service.get_annotations_for_book()
│    (复用已有)      │     复用 books_exporter.py:172-227
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 2. 上下文提取     │  ← context.py（可选，需 EPUB）
│    EPUB + CFI     │     使用 cfi_utils.extract_item_id()
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 3. 查缓存         │  ← cache.py
│    cache.json     │
└────────┬─────────┘
         │
    ┌────┴────┐
    ▼         ▼
  命中       未命中
  复用       调 LLM
              │
              ▼
┌──────────────────┐
│ 4. LLM 处理      │  ← enricher.py
│    标签+摘要+问题 │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 5. 输出           │  ← exporter.py
│    Obsidian 笔记  │
│    + 更新缓存     │
└──────────────────┘
```

---

## 5. LLM Provider 设计

### 5.1 接口

```python
class LLMProvider:
    def complete(self, prompt: str, system: str = "") -> str:
        raise NotImplementedError

class OpenAICompatible(LLMProvider):
    """OpenAI 兼容接口（覆盖大多数第三方 API）"""
    def __init__(self, base_url, api_key, model):
        self.base_url = base_url
        self.api_key = api_key
        self.model = model
```

OpenAI 兼容接口覆盖：OpenAI、Claude（第三方转发）、DeepSeek、通义千问、Ollama（本地）、各种中转站。

**错误重试机制**: 指数退避重试 3 次（1s, 2s, 4s），失败记录到 `errors.json`。

### 5.2 Prompt 设计

**单条（精确）**:
```
你是读书笔记助手。根据以下高亮内容，输出 JSON：

{
  "explanation": "一句话解释（30字以内）",
  "tags": ["标签1", "标签2"],
  "question": "一个可以用这段话回答的复习问题"
}

---
书名: {book_name}
章节: {chapter}
上下文: {context_before} **{highlight}** {context_after}
```

**批量（保留上下文结构）**:
```
你是读书笔记助手。处理以下 N 条高亮，每条输出 JSON。

--- 第1条 ---
{book_name} | {chapter}
上下文: {context_before} **{highlight}** {context_after}

--- 第2条 ---
{book_name} | {chapter}
上下文: {context_before} **{highlight}** {context_after}

请以 JSON 数组返回，每条包含 explanation, tags, question。
```

> 注：批量模式保留完整上下文结构（context_before + highlight + context_after），不省略上下文。token 消耗略高，但 LLM 解释质量不打折。

### 5.3 成本估算

```
单条: ~1000 token（800 prompt + 200 response）
批量打包（10条/次）: ~5000 token = 500 token/条

300 条 → 30 次调用 → ~15 万 token
```

---

## 6. 输出设计

### 6.0 输出格式选项

| 格式 | 说明 | 适用场景 |
|------|------|----------|
| `obsidian` | 带 `[[wikilink]]` 外链、frontmatter | Obsidian vault |
| `markdown` | 纯 Markdown，无 Obsidian 特有语法 | 通用笔记工具 |

通过 `--format obsidian/markdown` 指定，默认 `obsidian`。

### 6.1 目录结构

```
vault/
├── books/
│   └── 巨婴国/
│       ├── 巨婴国.md                    ← 主笔记
│       ├── 婴儿是没法面对失控的.md       ← LLM 解释笔记
│       ├── 每个巨婴内心深处都住着这样一个魔鬼.md
│       └── ...
├── llm/
│   └── cache.json                       ← LLM 缓存索引
```

### 6.2 主笔记格式

```markdown
---
book: 巨婴国
author: 武志红
isbn: 978-7-5502-6367-8
publisher: 浙江文艺出版社
publish_date: "2016"
tags: [心理学, 武志红]
---

# 巨婴国

## 第3章 · 巨婴的全能自恋

> 婴儿是没法面对失控的，失控会引起他们巨大的无助感...

[[婴儿是没法面对失控的]]

---

> 每个巨婴内心深处都住着这样一个魔鬼...

[[每个巨婴内心深处都住着这样一个魔鬼]]
```

### 6.3 LLM 解释笔记格式

```markdown
---
type: llm-note
book: 巨婴国
chapter: 第3章
highlight: "婴儿是没法面对失控的，失控会引起他们巨大的无助感..."
tags: [心理防御, 失控, 巨婴]
created: 2026-05-12
---

## 解释

巨婴面对失控时，通过心理切割将失控归因于外部敌对力量

## 复习问题

巨婴如何处理内心的失控感？

## 上下文

> 从2012年至今，我不断地在认识自己内心深处的这个魔鬼，**每个巨婴内心深处都住着这样一个魔鬼。** 随着认识越来越深越来越全面，我也越来越爱这个魔鬼。
```

### 6.4 图片卡片导出

支持将高亮导出为可分享的图片卡片（适合朋友圈、小红书等）。

**触发方式**:
```bash
# 单条导出图片
python3 knowledge.py card --book 1 --index 42

# 批量导出图片
python3 knowledge.py card --book 1 --all

# 指定样式模板
python3 knowledge.py card --book 1 --all --style dark

# 指定输出目录
python3 knowledge.py card --book 1 --all --output ~/cards/
```

**样式自定义**:

样式配置存放在 `knowledge/styles/` 目录下，JSON 格式：

```json
// knowledge/styles/dark.json
{
  "name": "dark",
  "width": 800,
  "padding": 60,
  "background": "#1a1a2e",
  "text_color": "#e0e0e0",
  "highlight_color": "#ffd700",
  "accent_color": "#0f3460",
  "font_family": "Noto Sans SC",
  "font_size": 18,
  "border_radius": 16,
  "show_book_info": true,
  "show_tags": true,
  "show_question": false
}
```

```json
// knowledge/styles/light.json
{
  "name": "light",
  "width": 800,
  "padding": 60,
  "background": "#ffffff",
  "text_color": "#333333",
  "highlight_color": "#e74c3c",
  "accent_color": "#f5f5f5",
  "font_family": "Noto Sans SC",
  "font_size": 18,
  "border_radius": 16,
  "show_book_info": true,
  "show_tags": true,
  "show_question": true
}
```

**卡片布局**:
```
┌────────────────────────────────────┐
│  📖 巨婴国 · 武志红               │
│  第3章 · 巨婴的全能自恋            │
├────────────────────────────────────┤
│                                    │
│  从2012年至今，我不断地在认识      │
│  自己内心深处的这个魔鬼，          │
│                                    │
│  ▶ 每个巨婴内心深处都住着这样      │
│    一个魔鬼。                      │
│                                    │
│  随着认识越来越深越来越全面，      │
│  我也越来越爱这个魔鬼。            │
│                                    │
├────────────────────────────────────┤
│  💡 巨婴把失控感投射为外部敌意     │
│                                    │
│  🏷 #巨婴 #心理投射 #失控          │
└────────────────────────────────────┘
```

**图片生成**: 使用 Python Pillow 库，支持自定义字体、颜色、布局。

**依赖说明**: Pillow 为可选依赖，未安装时 `card` 命令提示安装（`pip install Pillow`），不阻塞核心的文本导出功能。

### 6.5 文件名规则

- 高亮前 20 字作为文件名
- 保留中文、英文、数字、逗号、句号
- 特殊字符（/ \ : * ? " < > |）替换为下划线
- 同名冲突：加章节前缀 `第3章-婴儿是没法面对失控的.md`

---

## 7. 缓存设计

```json
// llm/cache.json
{
  "44D43B7A_a1b2c3d4": {
    "highlight": "婴儿是没法面对失控的，失控会引起他们巨大的无助感...",
    "file": "婴儿是没法面对失控的.md",
    "book_id": "44D43B7A372DA51FB1B5AD664DBE4D53",
    "book": "巨婴国",
    "explanation": "巨婴面对失控时，通过心理切割将失控归因于外部敌对力量",
    "tags": ["心理防御", "失控", "巨婴"],
    "question": "巨婴如何处理内心的失控感？",
    "updated": "2026-05-12"
  }
}
```

Key = `{book_id}_{md5(normalize(highlight_text))}`，避免跨书冲突。

**文本标准化**: `normalize()` = 去首尾空白 + 合并连续空白为单空格。避免用户高亮时多选空格导致缓存未命中。

---

## 8. CLI 接口

```bash
# 配置 LLM（首次）
python3 knowledge.py config \
  --provider openai_compatible \
  --base-url https://api.example.com/v1 \
  --api-key sk-xxx \
  --model claude-sonnet-4-20250514

# 配置 EPUB 映射
python3 knowledge.py map-book \
  --book-id 44D43B7A... \
  --epub path/to/book.epub

# 指定书籍（两种方式）
python3 knowledge.py enrich --book 1              # 序号
python3 knowledge.py enrich --book-id 44D43B7A... # asset_id

# 增量处理（默认，只处理未缓存的）
python3 knowledge.py enrich --book 1

# 整本批量（强制全量）
python3 knowledge.py enrich --book 1 --all

# 单条处理
python3 knowledge.py enrich --book 1 --index 42

# 强制刷新某条（清除缓存重新生成）
python3 knowledge.py enrich --book 1 --index 42 --force

# 重试之前失败的条目
python3 knowledge.py enrich --book 1 --retry-errors

# 指定输出目录
python3 knowledge.py enrich --book 1 --output ~/obsidian/books/

# 指定输出格式
python3 knowledge.py enrich --book 1 --format markdown

# 导出图片卡片（纯上下文，不依赖 LLM）
python3 knowledge.py card --book 1 --index 42

# 导出图片卡片（带 LLM 解释）
python3 knowledge.py card --book 1 --index 42 --with-llm

# 批量导出图片
python3 knowledge.py card --book 1 --all

# 指定卡片样式
python3 knowledge.py card --book 1 --all --style dark

# 查看缓存状态
python3 knowledge.py cache --book 1
```

---

## 9. 边界处理

| 情况 | 处理 |
|------|------|
| 无 EPUB | 只用高亮文本，不提供上下文 |
| EPUB 路径差异 | v1 使用 `OEBPS/` 前缀（已验证），后续版本升级为 `container.xml` 动态解析 |
| 高亮与 EPUB 文本有空白差异 | 去空白后匹配作为 fallback |
| 高亮在章节中多次出现 | 利用 CFI 偏移信息辅助定位，无法定位时取第一个 |
| LLM 返回异常 | 指数退避重试 3 次（1s, 2s, 4s），失败记录到 `errors.json` |
| 批量 LLM 解析失败 | 自动降级为逐条调用（`batch_fallback_to_single`） |
| 批量返回格式不标准 | 尝试清理 markdown 代码块包裹、多余逗号后重新解析 |
| 高亮太短（<5字） | 跳过 LLM，直接导出 |
| 高亮太长（>500字） | 截断到 500 字 |
| 缓存命中 | 跳过，不调 LLM |
| API 限流 | 批量时加延迟（1-2 秒/次） |
| 重复运行 | 默认增量跳过已处理的 |
| 失败条目重试 | `--retry-errors` 重新处理 `errors.json` 中的条目 |
| 字体缺失 | fallback 链：Noto Sans SC → PingFang SC → STHeiti → 提示安装 |
| 主笔记更新 | 全量重写，用户不应在主笔记中写自定义内容 |

---

## 10. 配置文件

```json
// knowledge_config.json
{
  "llm": {
    "provider": "openai_compatible",
    "base_url": "https://api.example.com/v1",
    "api_key": "env:KNOWLEDGE_API_KEY",
    "model": "claude-sonnet-4-20250514",
    "batch_size": 10,
    "max_retries": 3,
    "retry_delays": [1, 2, 4],
    "batch_fallback_to_single": true
  },
  "epub_mappings": {
    "44D43B7A372DA51FB1B5AD664DBE4D53": {
      "epub": "/path/to/巨婴国.epub",
      "output": "/path/to/obsidian-vault/books/巨婴国/"
    }
  },
  "output_format": "obsidian",
  "card_style": "dark",
  "card_output": "~/cards/",
  "context_chars": 200,
  "filename_max_length": 20
}
```

**API Key 安全**: `api_key` 支持两种格式：
- `"sk-xxx"` — 直接存储（不推荐，需确保 `.gitignore` 排除）
- `"env:KNOWLEDGE_API_KEY"` — 从环境变量读取（推荐）
```

---

## 11. 已知限制

| 限制 | 说明 |
|------|------|
| DRM | Apple Books Store 购买的 EPUB 有 FairPlay DRM，需用户自行解决 |
| EPUB 匹配 | 需要用户手动指定 EPUB 文件 |
| LLM 成本 | 批量处理 300 条约 15 万 token |
| 文件名冲突 | 同名高亮需加章节前缀区分 |
| 编码 | 目前只测试 UTF-8 EPUB |

---

## 12. 评估反馈采纳

### v1.1 反馈

| 来源建议 | 状态 | 说明 |
|----------|------|------|
| 智能缓存（模糊匹配） | P2 后续 | 当前 MD5 精确匹配够用，语义匹配需额外模型 |
| 成本优化（本地模型） | 已覆盖 | OpenAI 兼容接口已支持 Ollama |
| 错误重试机制 | ✅ 已采纳 | 指数退避 3 次（1s, 2s, 4s） |
| EPUB 自动发现 | P2 后续 | 手动配置一次即可，自动匹配复杂 |
| 输出灵活性 | ✅ 已采纳 | `--format obsidian/markdown` |
| 增量处理 | ✅ 已采纳 | 默认增量，加 `--all` 强制全量 |
| 元数据增强 | ✅ 已采纳 | 从 EPUB 提取 ISBN、出版社、出版日期 |
| 批量映射 | 已覆盖 | config 已是多书设计 |
| 图片卡片导出 | ✅ 已采纳 | `card.py` + 样式模板 |
| 手动刷新缓存 | ✅ 已采纳 | `--force` 参数 |

### v1.2 代码复用反馈

| 问题 | 调整 |
|------|------|
| DB 访问重复 | 复用 `book_service.py`，不重写 |
| CFI 解析两套 | 抽取 `services/cfi_utils.py` 共用 |
| 绕过服务层 | knowledge 模块依赖 `services/` 而非直接访问 DB |
| 测试代码可复用 | `extract_context()` / `format_context_card()` 直接搬到 knowledge |

### v1.3 鲁棒性反馈

| 问题 | 调整 |
|------|------|
| EPUB 路径硬编码 | 从 `META-INF/container.xml` 动态读取 `content.opf` |
| 上下文提取精确匹配 | 去空白后匹配作为 fallback，CFI 辅助定位多次出现 |
| 缓存 key 跨书冲突 | key = `{book_id}_{md5(highlight)}` |
| 批量 JSON 解析风险 | 清理格式 + 解析失败逐条 fallback + `batch_fallback_to_single` |
| 主笔记覆盖风险 | 全量重写，告知用户不要自定义主笔记 |
| 字体缺失 | fallback 链检测 + 安装提示 |
| API Key 明文 | 支持 `env:XXX` 格式读取环境变量 |
| 错误恢复 | `--retry-errors` 重试失败条目 |
| --book 语义 | 支持序号和 asset_id 两种方式 |
| 卡片依赖 LLM | `card` 命令支持纯上下文模式（不依赖 LLM 缓存） |

---

## 13. 设计决策记录

| # | 问题 | 决策 |
|---|------|------|
| 1 | 缓存 key 是否包含 chapter | 否，`book_id + hash` 足够，同一句话不同章节的情况极少 |
| 2 | 主笔记更新策略 | 全量重写，用户不应在主笔记中写自定义内容 |
| 3 | EPUB OEBPS 路径 | v1 使用 `OEBPS/` 前缀，后续升级 `container.xml` |
| 4 | 批量失败降级 | 自动降级为逐条调用 |
| 5 | --book 参数 | 支持序号和 asset_id 两种方式 |
| 6 | 卡片纯上下文模式 | 支持，`card` 命令不依赖 LLM 缓存 |
| 7 | CLI 入口 | 根目录 `knowledge.py`，和 `books_exporter.py` 同级 |
| 8 | 批量上下文 | 保留完整上下文结构，不省略 |
| 9 | Pillow 依赖 | 可选依赖，未安装时提示，不阻塞核心功能 |
| 10 | 缓存文本标准化 | 去首尾空白 + 合并连续空白，再 md5 |
