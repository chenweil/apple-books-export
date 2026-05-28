# 高亮上下文提取功能 - 技术验证报告

**日期**: 2026-05-12
**状态**: ✅ 验证通过

---

## 1. 背景

Apple Books 的笔记数据中，高亮文字 (`ZANNOTATIONSELECTEDTEXT`) 只存储了被选中的文本，但**不包含原文的前后上下文**。

Apple Books 存储的是 CFI (EPUB Canonical Fragment Identifier)：

```
epubcfi(/6/10[item4]!/4/82/1,:0,:44)
```

这个 CFI 指向 EPUB 文件的某个位置，但实际文本内容在 EPUB 文件里，不在 Apple Books 数据库。

**结论**: 需要用户提供对应的 EPUB 文件才能提取上下文。

---

## 2. 测试数据

| 项目 | 值 |
|------|-----|
| 书名 | 巨婴国 |
| 作者 | 武志红 |
| Asset ID | `44D43B7A372DA51FB1B5AD664DBE4D53` |
| EPUB 路径 | `books/巨婴国 (武志红) (Z-Library).epub` |
| 高亮总数 | 303 条 |
| DRM 状态 | 无（未加密） |

---

## 3. 技术方案

### 3.1 CFI 格式

```
epubcfi(/6/10[item4]!/4/82/1,:0,:44)
         │  │   │     │  │   └─ 结束字符位置
         │  │   │     │  └─ 开始字符位置
         │  │   │     └─ 段落/元素索引
         │  │   └─ manifest item ID（如 item4）
         │  └─ spine 索引
         └─ EPUB 根路径
```

**关键发现**: 直接用 `selected_text` 在章节文本中搜索，比解析 CFI 坐标更简单可靠。

### 3.2 提取流程

```
EPUB 文件
    │
    ▼
┌─────────────────────┐
│ 解析 content.opf    │  manifest id -> href 映射
│ (item4 -> part0003) │
└─────────────────────┘
    │
    ▼
┌─────────────────────┐
│ 提取章节纯文本      │  解压 XHTML，移除 HTML 标签
│ (part0003.xhtml)    │
└─────────────────────┘
    │
    ▼
┌─────────────────────┐
│ 搜索高亮文字        │  text.find(selected_text)
│ ("婴儿是没法...")  │
└─────────────────────┘
    │
    ▼
┌─────────────────────┐
│ 提取上下文          │  前后各取 N 字符，句子级别切分
│                     │
└─────────────────────┘
    │
    ▼
卡片样式输出
```

---

## 4. 测试结果

### 4.1 提取效果

**示例 1**:
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  
  ▶ 婴儿是没法面对失控的，失控会引起他们巨大的无助感，他们需要将失控这件事从自己身上切割出去
  。他们会认为，既然失控意味着"我"控制不了，那必然意味着，是有一个"我"之外的力量在控制这件事，并且，因为这件事是伤害性的，所以必然是敌对力量在控制着这件事。成年婴儿，即巨婴，和婴儿的心理逻辑是一样的
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**示例 2**:
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  从2012年至今，我不断地在认识自己内心深处的这个魔鬼，
  ▶ 每个巨婴内心深处都住着这样一个魔鬼。
  随着认识越来越深越来越全面，我也越来越爱这个魔鬼。
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### 4.2 数据统计

| 指标 | 值 |
|------|-----|
| 测试高亮数 | 303 条 |
| 定位成功 | 303 条 (100%) |
| 上下文完整 | 303 条 (100%) |

---

## 5. 关键代码

### 5.1 CFI 解析

```python
def parse_cfi(cfi):
    """解析 CFI，提取 manifest item ID"""
    if not cfi or not cfi.startswith('epubcfi('):
        return None
    match = re.search(r'\[([^\]]+)\]', cfi)
    return match.group(1) if match else None
```

### 5.2 上下文提取

```python
def extract_context(text, highlight_text, context_chars=100):
    """提取高亮文字的上下文"""
    pos = text.find(highlight_text)
    if pos < 0:
        return None
    start = max(0, pos - context_chars)
    end = min(len(text), pos + len(highlight_text) + context_chars)
    before = text[start:pos]
    after = text[pos + len(highlight_text):end]
    return (before, highlight_text, after)
```

### 5.3 句子级别切分

```python
def format_context_card(before, highlight, after):
    """格式化卡片，智能切分句子"""
    lines = ["━" * 50]
    if before:
        # 找前一个句子的开头
        sentence_start = before.rfind('。')
        if sentence_start < 0:
            sentence_start = before.rfind('，')
        if sentence_start >= 0 and len(before) - sentence_start < 50:
            before = before[sentence_start+1:]
        lines.append(f"  {before}")
    lines.append(f"  ▶ {highlight}")
    if after:
        # 找下一个句子的结尾
        sentence_end = after.find('。')
        if sentence_end < 0:
            sentence_end = after.find('，')
        if 0 < sentence_end < 50:
            after = after[:sentence_end+1]
        lines.append(f"  {after}")
    lines.append("━" * 50)
    return '\n'.join(lines)
```

---

## 6. 已知限制

### 6.1 DRM 问题

Apple Books Store 购买的 EPUB 通常有 FairPlay DRM 加密。

**解决方案**: 用户需要提供未加密的 EPUB（自己购买的或从其他渠道获取的）。

### 6.2 CFI 格式差异

不同出版社/工具生成的 EPUB，CFI 格式可能不同。

**处理方式**: 使用全文搜索，对 CFI 格式差异有鲁棒性。

### 6.3 编码问题

测试的 EPUB 是 UTF-8 编码。其他编码可能需要额外处理。

### 6.4 书籍匹配

需要用户手动指定 EPUB 文件，通过书名/作者自动匹配功能暂未实现。

---

## 7. 后续集成建议

### 7.1 CLI 集成

```bash
# 导出时指定 EPUB
python3 books_exporter.py export 1 --epub "path/to/book.epub"
```

### 7.2 GUI 集成

在详情面板增加"上下文预览"标签页，展示卡片样式。

### 7.3 配置持久化

```json
// epub_mappings.json
{
  "44D43B7A372DA51FB1B5AD664DBE4D53": "/path/to/巨婴国.epub",
  "ANOTHER_ASSET_ID": "/path/to/another.epub"
}
```

---

## 8. 文件清单

| 文件 | 说明 |
|------|------|
| `test_context_extraction.py` | 技术验证脚本 |
| `docs/CONTEXT_EXTRACTION_TEST.md` | 本文档 |

---

## 9. 下一步行动

- [ ] 确认是否集成到主项目
- [ ] 设计用户交互流程（EPUB 指定方式）
- [ ] 设计 CLI 参数 (`--with-context`, `--epub`)
- [ ] 设计 GUI 上下文预览组件
- [ ] 实现 EPUB 路径持久化配置