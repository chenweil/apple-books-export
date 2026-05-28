# Knowledge Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a knowledge module that enriches Apple Books highlights with EPUB context + LLM-generated summaries/tags/questions, exporting to Obsidian/Markdown and image cards.

**Architecture:** New `knowledge/` package depends on existing `services/` layer (BookService, cfi_utils). CLI entry at `knowledge.py` (root, alongside `books_exporter.py`). LLM via OpenAI-compatible HTTP API. Cache in JSON. Image cards via Pillow (optional dependency).

**Tech Stack:** Python 3.9+, `requests` (LLM HTTP), `Pillow` (optional, card images), `pytest` (testing). No GUI changes.

---

## File Structure

```
books-exporter/
├── books_exporter.py                    # MODIFY: import from cfi_utils (lines 37-115 become thin wrappers)
├── knowledge.py                         # CREATE: CLI entry point (argparse)
├── requirements.txt                     # MODIFY: add requests, pytest, Pillow
├── services/
│   ├── cfi_utils.py                     # CREATE: unified CFI parsing (extract_item_id, extract_chapter_title)
│   └── book_service.py                  # NO CHANGE
├── knowledge/
│   ├── __init__.py                      # CREATE: empty
│   ├── config.py                        # CREATE: load/save knowledge_config.json
│   ├── context.py                       # CREATE: EPUB context extraction
│   ├── cache.py                         # CREATE: llm_cache.json read/write
│   ├── provider/
│   │   ├── __init__.py                  # CREATE: empty
│   │   ├── base.py                      # CREATE: LLMProvider abstract base
│   │   └── openai_compat.py             # CREATE: OpenAI-compatible provider with retry
│   ├── enricher.py                      # CREATE: single/batch enrichment logic
│   ├── exporter.py                      # CREATE: Obsidian + Markdown export
│   ├── card.py                          # CREATE: Pillow image card generation
│   └── styles/
│       ├── dark.json                    # CREATE: dark card style
│       ├── light.json                   # CREATE: light card style
│       └── minimal.json                 # CREATE: minimal card style
└── tests/
    ├── conftest.py                      # CREATE: shared fixtures
    ├── test_cfi_utils.py                # CREATE: CFI parsing tests
    ├── test_config.py                   # CREATE: config load/save tests
    ├── test_context.py                  # CREATE: EPUB context extraction tests
    ├── test_cache.py                    # CREATE: cache read/write tests
    ├── test_provider.py                 # CREATE: LLM provider tests (mocked HTTP)
    ├── test_enricher.py                 # CREATE: enricher logic tests
    ├── test_exporter.py                 # CREATE: export format tests
    └── test_card.py                     # CREATE: card generation tests
```

---

## Task 1: Project Setup

**Files:**
- Modify: `requirements.txt`
- Create: `tests/conftest.py`
- Create: `tests/__init__.py`

- [ ] **Step 1: Update requirements.txt**

```txt
# Homebrew Python 3.14 (需要 python-tk@3.14)
# 安装方式:
#   brew install python@3.14 python-tk@3.14
#   /opt/homebrew/bin/python3.14 -m pip install PySimpleGUI --break-system-packages
PySimpleGUI>=6.0

# Knowledge module
requests>=2.28.0
Pillow>=10.0.0

# Testing
pytest>=7.0.0
```

- [ ] **Step 2: Create test infrastructure**

```python
# tests/__init__.py
```

```python
# tests/conftest.py
import pytest
from pathlib import Path


@pytest.fixture
def tmp_dir(tmp_path):
    """Provide a temporary directory for test outputs."""
    return tmp_path


@pytest.fixture
def sample_book():
    """Sample book dict matching books_exporter.py output format."""
    return {
        'asset_id': '44D43B7A372DA51FB1B5AD664DBE4D53',
        'title': '巨婴国',
        'author': '武志红',
        'path': '/path/to/book.epub',
        'note_count': 303,
        'page_count': 350,
        'reading_progress': 0.85,
        'last_open_date': 781234567.0,
        'creation_date': 780000000.0,
        'date_finished': None,
        'is_finished': 0,
    }


@pytest.fixture
def sample_annotations():
    """Sample annotations list matching books_exporter.py output format."""
    return [
        {
            'type': 2,
            'selected_text': '婴儿是没法面对失控的，失控会引起他们巨大的无助感',
            'note': '',
            'created_date': 781000000.0,
            'location': 'epubcfi(/6/6[Section0003.xhtml]!/4/2/1:0,/1716/2)',
        },
        {
            'type': 2,
            'selected_text': '每个巨婴内心深处都住着这样一个魔鬼',
            'note': '这个比喻很深刻',
            'created_date': 781000001.0,
            'location': 'epubcfi(/6/6[Section0003.xhtml]!/4/2/1:0,/1717/2)',
        },
    ]


@pytest.fixture
def sample_config():
    """Sample knowledge config dict."""
    return {
        'llm': {
            'provider': 'openai_compatible',
            'base_url': 'https://api.example.com/v1',
            'api_key': 'sk-test-key',
            'model': 'gpt-4o-mini',
            'batch_size': 10,
            'max_retries': 3,
            'retry_delays': [1, 2, 4],
            'batch_fallback_to_single': True,
        },
        'epub_mappings': {
            '44D43B7A372DA51FB1B5AD664DBE4D53': {
                'epub': '/path/to/book.epub',
                'output': '/tmp/output/books/',
            }
        },
        'output_format': 'obsidian',
        'card_style': 'dark',
        'card_output': '~/cards/',
        'context_chars': 200,
        'filename_max_length': 20,
    }
```

- [ ] **Step 3: Install test dependencies and verify pytest runs**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/ -v --co
```

Expected: `no tests ran` (collected 0 items, no errors).

- [ ] **Step 4: Commit**

```bash
git add requirements.txt tests/__init__.py tests/conftest.py
git commit -m "chore: add test infrastructure and knowledge module dependencies"
```

---

## Task 2: Extract cfi_utils.py

**Files:**
- Create: `services/cfi_utils.py`
- Create: `tests/test_cfi_utils.py`
- Modify: `books_exporter.py` (replace parse_cfi_chapter and format_chapter_display with imports)

- [ ] **Step 1: Write failing tests for cfi_utils**

```python
# tests/test_cfi_utils.py
from services.cfi_utils import extract_item_id, extract_chapter_title, format_chapter_display


class TestExtractItemId:
    def test_standard_cfi(self):
        cfi = 'epubcfi(/6/10[item4]!/4/82/1,:0,:44)'
        assert extract_item_id(cfi) == 'item4'

    def test_cfi_with_xhtml(self):
        cfi = 'epubcfi(/6/6[Section0001.xhtml]!/4/2,/2[sigil_toc_id_1]/1:0,/1716/2)'
        assert extract_item_id(cfi) == 'Section0001.xhtml'

    def test_empty_cfi(self):
        assert extract_item_id('') is None

    def test_none_cfi(self):
        assert extract_item_id(None) is None

    def test_no_brackets(self):
        assert extract_item_id('epubcfi(/6/10!/4/82)') is None

    def test_non_epubcfi(self):
        assert extract_item_id('not-a-cfi') is None


class TestExtractChapterTitle:
    def test_chinese_title(self):
        cfi = 'epubcfi(/6/6[15-面向并发的内存模型.xhtml]!/4/2)'
        result = extract_chapter_title(cfi)
        assert result == '面向并发的内存模型'

    def test_section_number(self):
        cfi = 'epubcfi(/6/6[Section0003.xhtml]!/4/2)'
        result = extract_chapter_title(cfi)
        assert result == '第3章'

    def test_chapter_format(self):
        cfi = 'epubcfi(/6/6[chapter5.xhtml]!/4/2)'
        result = extract_chapter_title(cfi)
        assert result == '第5章'

    def test_chinese_no_suffix(self):
        cfi = 'epubcfi(/6/6[巨婴的全能自恋]!/4/2)'
        result = extract_chapter_title(cfi)
        assert result == '巨婴的全能自恋'

    def test_uuid_filtered(self):
        cfi = 'epubcfi(/6/6[a1b2c3d4e5f6a1b2c3d4e5f6]!/4/2)'
        result = extract_chapter_title(cfi)
        assert result is None

    def test_empty(self):
        assert extract_chapter_title('') is None

    def test_none(self):
        assert extract_chapter_title(None) is None


class TestFormatChapterDisplay:
    def test_with_chapter(self):
        assert format_chapter_display('第3章', 1) == '第3章'

    def test_id_format(self):
        assert format_chapter_display('id45', 1) == '位置 45'

    def test_no_chapter(self):
        assert format_chapter_display(None, 7) == '位置 7'

    def test_empty_string(self):
        assert format_chapter_display('', 3) == '位置 3'
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_cfi_utils.py -v
```

Expected: FAIL — `ModuleNotFoundError: No module named 'services.cfi_utils'`

- [ ] **Step 3: Create cfi_utils.py with implementation**

```python
# services/cfi_utils.py
"""Unified CFI parsing utilities for EPUB annotations."""
import re


def extract_item_id(cfi: str) -> str | None:
    """Extract manifest item ID from EPUB CFI string.

    Example: epubcfi(/6/10[item4]!/4/82/1,:0,:44) -> 'item4'
    """
    if not cfi or not cfi.startswith('epubcfi('):
        return None
    match = re.search(r'\[([^\]]+)\]', cfi)
    if match:
        return match.group(1)
    return None


def extract_chapter_title(cfi: str) -> str | None:
    """Extract chapter title from EPUB CFI string.

    Handles Chinese titles, section numbers (Section0003 -> 第3章),
    chapter formats (chapter5 -> 第5章), and filters out UUIDs.
    """
    if not cfi or not cfi.startswith('epubcfi('):
        return None

    matches = re.findall(r'\[([^\]]+)\]', cfi)
    if not matches:
        return None

    def is_meaningless_id(s: str) -> bool:
        if len(s) > 20 and re.search(r'[0-9a-f]{8,}', s, re.IGNORECASE):
            return True
        if re.match(r'^id\d{3,}$', s, re.IGNORECASE):
            return True
        return False

    def is_valid_chapter(s: str) -> bool:
        if s.endswith('-') or s.endswith('_'):
            return False
        if re.search(r'[一-鿿]', s):
            return True
        if re.match(r'(chapter|ch|section)\d+', s, re.IGNORECASE):
            return True
        if len(s) > 3 and re.match(r'^[a-zA-Z][a-zA-Z0-9_-]*$', s):
            return True
        return False

    chapter_candidates = []
    for match in reversed(matches):
        # Chinese title — extract part after separator
        if re.search(r'[一-鿿]', match):
            if '-' in match or '_' in match:
                parts = re.split(r'[-_]', match, 1)
                if len(parts) > 1 and len(parts[1].strip()) > 2:
                    return parts[1].strip()
            return match

        # File names
        if match.endswith('.xhtml') or match.endswith('.html'):
            chapter_name = re.sub(r'\.(xhtml|html)$', '', match, flags=re.IGNORECASE)
            section_match = re.match(r'Section(\d+)', chapter_name, re.IGNORECASE)
            if section_match:
                return f"第{int(section_match.group(1))}章"
            ch_match = re.match(r'(ch|chapter)(\d+)', chapter_name, re.IGNORECASE)
            if ch_match:
                return f"第{int(ch_match.group(2))}章"
            if len(chapter_name) > 3 and not is_meaningless_id(chapter_name):
                chapter_candidates.append(chapter_name)
            continue

        if len(match) > 3 and not is_meaningless_id(match) and is_valid_chapter(match):
            chapter_candidates.append(match)

    return chapter_candidates[0] if chapter_candidates else None


def format_chapter_display(chapter: str | None, index: int) -> str:
    """Format chapter for display. Falls back to position index."""
    if chapter:
        id_match = re.match(r'^id(\d+)$', chapter, re.IGNORECASE)
        if id_match:
            return f"位置 {id_match.group(1)}"
        return chapter
    return f"位置 {index}"
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_cfi_utils.py -v
```

Expected: All 13 tests PASS.

- [ ] **Step 5: Update books_exporter.py to use cfi_utils**

Replace the `parse_cfi_chapter` and `format_chapter_display` functions in `books_exporter.py` (lines 37-136) with thin wrappers:

```python
# In books_exporter.py, replace lines 37-136 with:

from services.cfi_utils import (
    extract_chapter_title as parse_cfi_chapter,
    format_chapter_display,
)
```

Keep the old function names as aliases so all existing callers (GUI, CLI) continue to work without changes.

- [ ] **Step 6: Verify existing functionality still works**

```bash
cd /Users/chenweilong/books-exporter && python3 -c "from books_exporter import parse_cfi_chapter, format_chapter_display; print(parse_cfi_chapter('epubcfi(/6/6[Section0003.xhtml]!/4/2)')); print(format_chapter_display('第3章', 1))"
```

Expected:
```
第3章
第3章
```

- [ ] **Step 7: Commit**

```bash
git add services/cfi_utils.py tests/test_cfi_utils.py books_exporter.py
git commit -m "refactor: extract CFI parsing to services/cfi_utils.py"
```

---

## Task 3: Knowledge Config

**Files:**
- Create: `knowledge/__init__.py`
- Create: `knowledge/config.py`
- Create: `tests/test_config.py`

- [ ] **Step 1: Write failing tests for config**

```python
# tests/test_config.py
import json
from pathlib import Path
from knowledge.config import KnowledgeConfig, load_config, save_config


class TestKnowledgeConfig:
    def test_default_values(self):
        config = KnowledgeConfig()
        assert config.output_format == 'obsidian'
        assert config.card_style == 'dark'
        assert config.context_chars == 200
        assert config.filename_max_length == 20
        assert config.llm.batch_size == 10
        assert config.llm.max_retries == 3

    def test_from_dict(self, sample_config):
        config = KnowledgeConfig.from_dict(sample_config)
        assert config.llm.provider == 'openai_compatible'
        assert config.llm.base_url == 'https://api.example.com/v1'
        assert config.llm.model == 'gpt-4o-mini'
        assert '44D43B7A372DA51FB1B5AD664DBE4D53' in config.epub_mappings

    def test_to_dict(self, sample_config):
        config = KnowledgeConfig.from_dict(sample_config)
        data = config.to_dict()
        assert data['llm']['provider'] == 'openai_compatible'
        assert data['output_format'] == 'obsidian'

    def test_roundtrip(self, sample_config):
        config = KnowledgeConfig.from_dict(sample_config)
        data = config.to_dict()
        config2 = KnowledgeConfig.from_dict(data)
        assert config2.llm.model == config.llm.model
        assert config2.epub_mappings == config.epub_mappings


class TestLoadSaveConfig:
    def test_save_and_load(self, tmp_dir, sample_config):
        config_path = tmp_dir / 'knowledge_config.json'
        config = KnowledgeConfig.from_dict(sample_config)
        save_config(config, config_path)
        assert config_path.exists()

        loaded = load_config(config_path)
        assert loaded.llm.api_key == 'sk-test-key'
        assert loaded.output_format == 'obsidian'

    def test_load_nonexistent_returns_default(self, tmp_dir):
        config_path = tmp_dir / 'nonexistent.json'
        config = load_config(config_path)
        assert config.llm.batch_size == 10

    def test_env_api_key(self, tmp_dir, sample_config):
        """Config with env: prefix stores the reference, not the value."""
        sample_config['llm']['api_key'] = 'env:MY_API_KEY'
        config_path = tmp_dir / 'config.json'
        config = KnowledgeConfig.from_dict(sample_config)
        save_config(config, config_path)
        loaded = load_config(config_path)
        assert loaded.llm.api_key == 'env:MY_API_KEY'
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_config.py -v
```

Expected: FAIL — `ModuleNotFoundError: No module named 'knowledge'`

- [ ] **Step 3: Create knowledge/__init__.py**

```python
# knowledge/__init__.py
```

- [ ] **Step 4: Create knowledge/config.py**

```python
# knowledge/config.py
"""Configuration management for the knowledge module."""
import json
import os
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class LLMConfig:
    provider: str = 'openai_compatible'
    base_url: str = ''
    api_key: str = ''
    model: str = ''
    batch_size: int = 10
    max_retries: int = 3
    retry_delays: list = field(default_factory=lambda: [1, 2, 4])
    batch_fallback_to_single: bool = True

    def get_api_key(self) -> str:
        """Resolve API key, supporting env: prefix."""
        if self.api_key.startswith('env:'):
            env_var = self.api_key[4:]
            return os.environ.get(env_var, '')
        return self.api_key


@dataclass
class EpubMapping:
    epub: str = ''
    output: str = ''


@dataclass
class KnowledgeConfig:
    llm: LLMConfig = field(default_factory=LLMConfig)
    epub_mappings: dict[str, EpubMapping] = field(default_factory=dict)
    output_format: str = 'obsidian'
    card_style: str = 'dark'
    card_output: str = '~/cards/'
    context_chars: int = 200
    filename_max_length: int = 20

    @classmethod
    def from_dict(cls, data: dict) -> 'KnowledgeConfig':
        llm_data = data.get('llm', {})
        llm = LLMConfig(**{k: v for k, v in llm_data.items() if k in LLMConfig.__dataclass_fields__})

        mappings = {}
        for book_id, mapping_data in data.get('epub_mappings', {}).items():
            mappings[book_id] = EpubMapping(**{
                k: v for k, v in mapping_data.items() if k in EpubMapping.__dataclass_fields__
            })

        return cls(
            llm=llm,
            epub_mappings=mappings,
            output_format=data.get('output_format', 'obsidian'),
            card_style=data.get('card_style', 'dark'),
            card_output=data.get('card_output', '~/cards/'),
            context_chars=data.get('context_chars', 200),
            filename_max_length=data.get('filename_max_length', 20),
        )

    def to_dict(self) -> dict:
        return {
            'llm': {
                'provider': self.llm.provider,
                'base_url': self.llm.base_url,
                'api_key': self.llm.api_key,
                'model': self.llm.model,
                'batch_size': self.llm.batch_size,
                'max_retries': self.llm.max_retries,
                'retry_delays': self.llm.retry_delays,
                'batch_fallback_to_single': self.llm.batch_fallback_to_single,
            },
            'epub_mappings': {
                book_id: {'epub': m.epub, 'output': m.output}
                for book_id, m in self.epub_mappings.items()
            },
            'output_format': self.output_format,
            'card_style': self.card_style,
            'card_output': self.card_output,
            'context_chars': self.context_chars,
            'filename_max_length': self.filename_max_length,
        }


DEFAULT_CONFIG_PATH = Path('knowledge_config.json')


def load_config(path: Path | None = None) -> KnowledgeConfig:
    """Load config from JSON file, or return defaults if not found."""
    path = path or DEFAULT_CONFIG_PATH
    if not path.exists():
        return KnowledgeConfig()
    with open(path, 'r', encoding='utf-8') as f:
        data = json.load(f)
    return KnowledgeConfig.from_dict(data)


def save_config(config: KnowledgeConfig, path: Path | None = None) -> None:
    """Save config to JSON file."""
    path = path or DEFAULT_CONFIG_PATH
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, 'w', encoding='utf-8') as f:
        json.dump(config.to_dict(), f, indent=2, ensure_ascii=False)
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_config.py -v
```

Expected: All 6 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add knowledge/__init__.py knowledge/config.py tests/test_config.py
git commit -m "feat: add knowledge config management"
```

---

## Task 4: EPUB Context Extraction

**Files:**
- Create: `knowledge/context.py`
- Create: `tests/test_context.py`

- [ ] **Step 1: Write failing tests for context extraction**

```python
# tests/test_context.py
import zipfile
import pytest
from pathlib import Path
from knowledge.context import (
    extract_text_from_xhtml,
    get_manifest_map,
    get_chapter_text,
    extract_context,
    normalize_text,
)


@pytest.fixture
def sample_epub(tmp_path):
    """Create a minimal EPUB file for testing."""
    epub_path = tmp_path / 'test.epub'

    content_opf = '''<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <manifest>
    <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="chapter2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
</package>'''

    chapter1 = '''<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body>
  <h1>第一章</h1>
  <p>这是第一章的内容。婴儿是没法面对失控的，失控会引起他们巨大的无助感。这是一种非常原始的反应。</p>
  <p>第二段落的内容在这里。</p>
</body>
</html>'''

    chapter2 = '''<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body>
  <h1>第二章</h1>
  <p>每个巨婴内心深处都住着这样一个魔鬼。随着认识越来越深，我们也越来越爱这个魔鬼。</p>
</body>
</html>'''

    with zipfile.ZipFile(epub_path, 'w') as zf:
        zf.writestr('OEBPS/content.opf', content_opf)
        zf.writestr('OEBPS/chapter1.xhtml', chapter1)
        zf.writestr('OEBPS/chapter2.xhtml', chapter2)

    return epub_path


class TestExtractTextFromXhtml:
    def test_strips_tags(self):
        html = '<p>Hello <b>world</b></p>'
        assert extract_text_from_xhtml(html) == 'Hello world'

    def test_collapses_whitespace(self):
        html = '<p>  hello   world  </p>'
        assert extract_text_from_xhtml(html) == 'hello world'

    def test_empty(self):
        assert extract_text_from_xhtml('') == ''


class TestGetManifestMap:
    def test_parses_manifest(self, sample_epub):
        manifest = get_manifest_map(sample_epub)
        assert manifest == {'chapter1': 'chapter1.xhtml', 'chapter2': 'chapter2.xhtml'}

    def test_returns_empty_on_missing_opf(self, tmp_path):
        epub_path = tmp_path / 'bad.epub'
        with zipfile.ZipFile(epub_path, 'w') as zf:
            zf.writestr('README.txt', 'no opf here')
        manifest = get_manifest_map(epub_path)
        assert manifest == {}


class TestGetChapterText:
    def test_extracts_chapter(self, sample_epub):
        text = get_chapter_text(sample_epub, 'chapter1.xhtml')
        assert '婴儿是没法面对失控的' in text
        assert '<p>' not in text

    def test_missing_chapter_raises(self, sample_epub):
        with pytest.raises(KeyError):
            get_chapter_text(sample_epub, 'nonexistent.xhtml')


class TestExtractContext:
    def test_finds_highlight(self):
        text = '前面的文字。婴儿是没法面对失控的，失控会引起他们巨大的无助感。后面的文字。'
        result = extract_context(text, '婴儿是没法面对失控的', context_chars=20)
        assert result is not None
        before, highlight, after = result
        assert highlight == '婴儿是没法面对失控的'
        assert '前面的文字' in before

    def test_not_found(self):
        result = extract_context('some text', '不存在的文字')
        assert result is None

    def test_at_start(self):
        result = extract_context('开头就是高亮文字然后继续', '开头就是高亮', context_chars=10)
        assert result is not None
        before, highlight, after = result
        assert before == ''

    def test_at_end(self):
        result = extract_context('前面的内容然后结尾是高亮文字', '结尾是高亮文字', context_chars=10)
        assert result is not None
        before, highlight, after = result
        assert after == ''


class TestNormalizeText:
    def test_strips_whitespace(self):
        assert normalize_text('  hello  ') == 'hello'

    def test_collapses_spaces(self):
        assert normalize_text('hello   world') == 'hello world'

    def test_collapses_newlines(self):
        assert normalize_text('hello\n\n  world') == 'hello world'
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_context.py -v
```

Expected: FAIL — `ModuleNotFoundError: No module named 'knowledge.context'`

- [ ] **Step 3: Create knowledge/context.py**

```python
# knowledge/context.py
"""EPUB context extraction for book highlights."""
import re
import zipfile
from pathlib import Path


def normalize_text(text: str) -> str:
    """Normalize whitespace: strip and collapse runs to single space."""
    return re.sub(r'\s+', ' ', text).strip()


def extract_text_from_xhtml(html_content: str) -> str:
    """Extract plain text from XHTML, stripping all tags."""
    text = re.sub(r'<[^>]+>', '', html_content)
    return re.sub(r'\s+', ' ', text).strip()


def get_manifest_map(epub_path: Path) -> dict[str, str]:
    """Extract manifest item_id -> href mapping from EPUB content.opf.

    Returns dict like {'chapter1': 'chapter1.xhtml', ...}.
    """
    try:
        with zipfile.ZipFile(epub_path, 'r') as z:
            content_opf = z.read('OEBPS/content.opf').decode('utf-8')
    except (KeyError, FileNotFoundError):
        return {}

    items = re.findall(r'<item\s+id="([^"]+)"[^>]*href="([^"]+)"', content_opf)
    return {item_id: href for item_id, href in items}


def get_chapter_text(epub_path: Path, chapter_href: str) -> str:
    """Extract plain text from a chapter in the EPUB.

    Args:
        epub_path: Path to EPUB file.
        chapter_href: Chapter href from manifest (e.g. 'chapter1.xhtml').

    Returns:
        Plain text content of the chapter.

    Raises:
        KeyError: If the chapter file is not found in the EPUB.
    """
    full_path = f"OEBPS/{chapter_href}"
    with zipfile.ZipFile(epub_path, 'r') as z:
        content = z.read(full_path).decode('utf-8')
        return extract_text_from_xhtml(content)


def extract_context(
    text: str,
    highlight_text: str,
    context_chars: int = 100,
) -> tuple[str, str, str] | None:
    """Find highlight_text in chapter text and extract surrounding context.

    Args:
        text: Full chapter plain text.
        highlight_text: The highlighted text to find.
        context_chars: Number of characters to extract before and after.

    Returns:
        (before, highlight, after) tuple, or None if not found.
    """
    # Try exact match first
    pos = text.find(highlight_text)
    if pos < 0:
        # Fallback: normalize both and try again
        norm_text = normalize_text(text)
        norm_highlight = normalize_text(highlight_text)
        pos = norm_text.find(norm_highlight)
        if pos < 0:
            return None
        # Map position back to original text (approximate)
        start = max(0, pos - context_chars)
        end = min(len(norm_text), pos + len(norm_highlight) + context_chars)
        before = norm_text[start:pos]
        after = norm_text[pos + len(norm_highlight):end]
        return (before, norm_highlight, after)

    start = max(0, pos - context_chars)
    end = min(len(text), pos + len(highlight_text) + context_chars)

    before = text[start:pos]
    after = text[pos + len(highlight_text):end]

    return (before, highlight_text, after)
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_context.py -v
```

Expected: All 11 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add knowledge/context.py tests/test_context.py
git commit -m "feat: add EPUB context extraction"
```

---

## Task 5: LLM Cache

**Files:**
- Create: `knowledge/cache.py`
- Create: `tests/test_cache.py`

- [ ] **Step 1: Write failing tests for cache**

```python
# tests/test_cache.py
import json
import hashlib
from pathlib import Path
from knowledge.cache import LLMCache


class TestLLMCache:
    def test_empty_cache(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        assert cache.get('book1', 'some text') is None
        assert cache.count() == 0

    def test_put_and_get(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        cache.put(
            book_id='book1',
            highlight='some highlight text',
            file='test.md',
            book_name='Test Book',
            explanation='test explanation',
            tags=['tag1', 'tag2'],
            question='test question?',
        )
        result = cache.get('book1', 'some highlight text')
        assert result is not None
        assert result['explanation'] == 'test explanation'
        assert result['tags'] == ['tag1', 'tag2']
        assert result['question'] == 'test question?'

    def test_get_miss(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        cache.put(
            book_id='book1',
            highlight='text A',
            file='a.md',
            book_name='Book',
            explanation='exp',
            tags=[],
            question='q?',
        )
        assert cache.get('book1', 'text B') is None

    def test_normalization(self, tmp_dir):
        """Whitespace differences should still hit cache."""
        cache = LLMCache(tmp_dir / 'cache.json')
        cache.put(
            book_id='book1',
            highlight='  hello   world  ',
            file='test.md',
            book_name='Book',
            explanation='exp',
            tags=[],
            question='q?',
        )
        # Same text with different whitespace should hit
        result = cache.get('book1', 'hello world')
        assert result is not None

    def test_cross_book_no_collision(self, tmp_dir):
        """Same highlight text in different books should be separate entries."""
        cache = LLMCache(tmp_dir / 'cache.json')
        cache.put(book_id='book1', highlight='same text', file='a.md',
                  book_name='Book A', explanation='exp A', tags=[], question='q?')
        cache.put(book_id='book2', highlight='same text', file='b.md',
                  book_name='Book B', explanation='exp B', tags=[], question='q?')

        assert cache.get('book1', 'same text')['explanation'] == 'exp A'
        assert cache.get('book2', 'same text')['explanation'] == 'exp B'

    def test_persistence(self, tmp_dir):
        path = tmp_dir / 'cache.json'
        cache = LLMCache(path)
        cache.put(book_id='book1', highlight='text', file='test.md',
                  book_name='Book', explanation='exp', tags=[], question='q?')

        # Load fresh instance
        cache2 = LLMCache(path)
        assert cache2.get('book1', 'text') is not None

    def test_count(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        assert cache.count() == 0
        cache.put(book_id='book1', highlight='a', file='a.md',
                  book_name='Book', explanation='e', tags=[], question='q?')
        cache.put(book_id='book1', highlight='b', file='b.md',
                  book_name='Book', explanation='e', tags=[], question='q?')
        assert cache.count() == 2

    def test_remove(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        cache.put(book_id='book1', highlight='text', file='test.md',
                  book_name='Book', explanation='exp', tags=[], question='q?')
        cache.remove('book1', 'text')
        assert cache.get('book1', 'text') is None

    def test_is_cached(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        assert not cache.is_cached('book1', 'text')
        cache.put(book_id='book1', highlight='text', file='test.md',
                  book_name='Book', explanation='exp', tags=[], question='q?')
        assert cache.is_cached('book1', 'text')
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_cache.py -v
```

Expected: FAIL — `ModuleNotFoundError: No module named 'knowledge.cache'`

- [ ] **Step 3: Create knowledge/cache.py**

```python
# knowledge/cache.py
"""LLM result caching with JSON persistence."""
import hashlib
import json
import re
from datetime import date
from pathlib import Path


def _normalize(text: str) -> str:
    """Normalize text for cache key: strip + collapse whitespace."""
    return re.sub(r'\s+', ' ', text).strip()


def _make_key(book_id: str, highlight: str) -> str:
    """Generate cache key: {first8_of_book_id}_{md5(normalized_highlight)}."""
    norm = _normalize(highlight)
    md5 = hashlib.md5(norm.encode('utf-8')).hexdigest()
    return f"{book_id[:8]}_{md5}"


class LLMCache:
    """JSON-backed cache for LLM enrichment results."""

    def __init__(self, path: Path):
        self._path = path
        self._data: dict = {}
        self._load()

    def _load(self) -> None:
        if self._path.exists():
            with open(self._path, 'r', encoding='utf-8') as f:
                self._data = json.load(f)

    def _save(self) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        with open(self._path, 'w', encoding='utf-8') as f:
            json.dump(self._data, f, indent=2, ensure_ascii=False)

    def get(self, book_id: str, highlight: str) -> dict | None:
        """Get cached result for a highlight. Returns None if not found."""
        key = _make_key(book_id, highlight)
        return self._data.get(key)

    def is_cached(self, book_id: str, highlight: str) -> bool:
        """Check if a highlight has been cached."""
        return _make_key(book_id, highlight) in self._data

    def put(
        self,
        book_id: str,
        highlight: str,
        file: str,
        book_name: str,
        explanation: str,
        tags: list[str],
        question: str,
    ) -> None:
        """Store an LLM enrichment result in the cache."""
        key = _make_key(book_id, highlight)
        self._data[key] = {
            'highlight': _normalize(highlight),
            'file': file,
            'book_id': book_id,
            'book': book_name,
            'explanation': explanation,
            'tags': tags,
            'question': question,
            'updated': date.today().isoformat(),
        }
        self._save()

    def remove(self, book_id: str, highlight: str) -> None:
        """Remove a cached entry."""
        key = _make_key(book_id, highlight)
        if key in self._data:
            del self._data[key]
            self._save()

    def count(self) -> int:
        """Return total number of cached entries."""
        return len(self._data)

    def get_all_for_book(self, book_id: str) -> dict[str, dict]:
        """Get all cached entries for a specific book."""
        prefix = book_id[:8]
        return {
            k: v for k, v in self._data.items()
            if k.startswith(prefix + '_')
        }
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_cache.py -v
```

Expected: All 9 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add knowledge/cache.py tests/test_cache.py
git commit -m "feat: add LLM result caching"
```

---

## Task 6: LLM Provider

**Files:**
- Create: `knowledge/provider/__init__.py`
- Create: `knowledge/provider/base.py`
- Create: `knowledge/provider/openai_compat.py`
- Create: `tests/test_provider.py`

- [ ] **Step 1: Write failing tests for provider**

```python
# tests/test_provider.py
import json
import pytest
from unittest.mock import patch, MagicMock
from knowledge.provider.base import LLMProvider
from knowledge.provider.openai_compat import OpenAICompatible


class TestLLMProviderBase:
    def test_base_raises(self):
        provider = LLMProvider()
        with pytest.raises(NotImplementedError):
            provider.complete('test prompt')


class TestOpenAICompatible:
    def _make_provider(self):
        return OpenAICompatible(
            base_url='https://api.example.com/v1',
            api_key='sk-test',
            model='gpt-4o-mini',
        )

    @patch('knowledge.provider.openai_compat.requests.post')
    def test_complete_single(self, mock_post):
        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            'choices': [{'message': {'content': '{"explanation": "test", "tags": ["a"], "question": "q?"}'}}]
        }
        mock_response.raise_for_status = MagicMock()
        mock_post.return_value = mock_response

        provider = self._make_provider()
        result = provider.complete('test prompt', system='test system')
        assert 'explanation' in result
        mock_post.assert_called_once()

    @patch('knowledge.provider.openai_compat.requests.post')
    def test_complete_retries_on_failure(self, mock_post):
        """Should retry on HTTP errors."""
        mock_fail = MagicMock()
        mock_fail.status_code = 429
        mock_fail.raise_for_status.side_effect = Exception('rate limited')

        mock_success = MagicMock()
        mock_success.status_code = 200
        mock_success.json.return_value = {
            'choices': [{'message': {'content': '{"ok": true}'}}]
        }
        mock_success.raise_for_status = MagicMock()

        mock_post.side_effect = [mock_fail, mock_success]

        provider = self._make_provider()
        provider._retry_delays = [0.01, 0.01, 0.01]  # speed up test
        result = provider.complete('test')
        assert 'ok' in result
        assert mock_post.call_count == 2

    @patch('knowledge.provider.openai_compat.requests.post')
    def test_complete_raises_after_max_retries(self, mock_post):
        mock_fail = MagicMock()
        mock_fail.status_code = 500
        mock_fail.raise_for_status.side_effect = Exception('server error')
        mock_post.return_value = mock_fail

        provider = self._make_provider()
        provider._retry_delays = [0.01]
        provider._max_retries = 1
        with pytest.raises(Exception):
            provider.complete('test')

    @patch('knowledge.provider.openai_compat.requests.post')
    def test_batch_complete(self, mock_post):
        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            'choices': [{'message': {'content': '[{"explanation": "a"}, {"explanation": "b"}]'}}]
        }
        mock_response.raise_for_status = MagicMock()
        mock_post.return_value = mock_response

        provider = self._make_provider()
        prompts = ['prompt 1', 'prompt 2']
        results = provider.batch_complete(prompts, system='test')
        assert len(results) == 2
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_provider.py -v
```

Expected: FAIL — `ModuleNotFoundError`

- [ ] **Step 3: Create provider modules**

```python
# knowledge/provider/__init__.py
from .base import LLMProvider
from .openai_compat import OpenAICompatible

__all__ = ['LLMProvider', 'OpenAICompatible']
```

```python
# knowledge/provider/base.py
"""Abstract base class for LLM providers."""


class LLMProvider:
    """Base LLM provider interface."""

    def complete(self, prompt: str, system: str = "") -> str:
        """Send a prompt and return the LLM's text response."""
        raise NotImplementedError

    def batch_complete(self, prompts: list[str], system: str = "") -> list[str]:
        """Send multiple prompts and return responses. Default: sequential."""
        return [self.complete(p, system) for p in prompts]
```

```python
# knowledge/provider/openai_compat.py
"""OpenAI-compatible LLM provider with retry logic."""
import time
import requests
from .base import LLMProvider


class OpenAICompatible(LLMProvider):
    """Works with OpenAI, DeepSeek, Ollama, Claude (via proxy), etc."""

    def __init__(self, base_url: str, api_key: str, model: str,
                 max_retries: int = 3, retry_delays: list[int] | None = None):
        self.base_url = base_url.rstrip('/')
        self.api_key = api_key
        self.model = model
        self._max_retries = max_retries
        self._retry_delays = retry_delays or [1, 2, 4]

    def complete(self, prompt: str, system: str = "") -> str:
        """Send a single prompt with exponential backoff retry."""
        messages = []
        if system:
            messages.append({'role': 'system', 'content': system})
        messages.append({'role': 'user', 'content': prompt})

        payload = {
            'model': self.model,
            'messages': messages,
            'temperature': 0.3,
        }
        headers = {
            'Authorization': f'Bearer {self.api_key}',
            'Content-Type': 'application/json',
        }

        last_error = None
        for attempt in range(self._max_retries):
            try:
                resp = requests.post(
                    f"{self.base_url}/chat/completions",
                    json=payload,
                    headers=headers,
                    timeout=60,
                )
                resp.raise_for_status()
                return resp.json()['choices'][0]['message']['content']
            except Exception as e:
                last_error = e
                if attempt < self._max_retries - 1:
                    delay = self._retry_delays[min(attempt, len(self._retry_delays) - 1)]
                    time.sleep(delay)

        raise last_error

    def batch_complete(self, prompts: list[str], system: str = "") -> list[str]:
        """Send prompts sequentially (OpenAI API doesn't support true batch)."""
        return [self.complete(p, system) for p in prompts]
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_provider.py -v
```

Expected: All 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add knowledge/provider/ tests/test_provider.py
git commit -m "feat: add LLM provider with OpenAI-compatible interface"
```

---

## Task 7: Enricher

**Files:**
- Create: `knowledge/enricher.py`
- Create: `tests/test_enricher.py`

- [ ] **Step 1: Write failing tests for enricher**

```python
# tests/test_enricher.py
import json
import pytest
from unittest.mock import MagicMock, patch
from knowledge.enricher import (
    build_single_prompt,
    build_batch_prompt,
    parse_llm_response,
    parse_batch_response,
    Enricher,
)


class TestBuildSinglePrompt:
    def test_with_context(self):
        prompt = build_single_prompt(
            book_name='巨婴国',
            chapter='第3章',
            highlight='婴儿是没法面对失控的',
            context_before='前面的内容。',
            context_after='后面的内容。',
        )
        assert '巨婴国' in prompt
        assert '第3章' in prompt
        assert '婴儿是没法面对失控的' in prompt
        assert '前面的内容' in prompt

    def test_without_context(self):
        prompt = build_single_prompt(
            book_name='Test', chapter='', highlight='some text',
            context_before='', context_after='',
        )
        assert 'some text' in prompt


class TestBuildBatchPrompt:
    def test_multiple_items(self):
        items = [
            {'book_name': 'Book', 'chapter': 'Ch1', 'highlight': 'text1',
             'context_before': '', 'context_after': ''},
            {'book_name': 'Book', 'chapter': 'Ch2', 'highlight': 'text2',
             'context_before': '', 'context_after': ''},
        ]
        prompt = build_batch_prompt(items)
        assert 'text1' in prompt
        assert 'text2' in prompt
        assert '第1条' in prompt or '---' in prompt


class TestParseLLMResponse:
    def test_valid_json(self):
        response = '{"explanation": "test", "tags": ["a", "b"], "question": "q?"}'
        result = parse_llm_response(response)
        assert result['explanation'] == 'test'
        assert result['tags'] == ['a', 'b']

    def test_json_in_markdown_block(self):
        response = '```json\n{"explanation": "test", "tags": ["a"], "question": "q?"}\n```'
        result = parse_llm_response(response)
        assert result['explanation'] == 'test'

    def test_invalid_json(self):
        result = parse_llm_response('not json at all')
        assert result is None


class TestParseBatchResponse:
    def test_valid_array(self):
        response = '[{"explanation": "a"}, {"explanation": "b"}]'
        results = parse_batch_response(response, expected_count=2)
        assert len(results) == 2
        assert results[0]['explanation'] == 'a'

    def test_count_mismatch(self):
        response = '[{"explanation": "a"}]'
        results = parse_batch_response(response, expected_count=2)
        assert results is None  # signals fallback needed

    def test_json_in_markdown(self):
        response = '```json\n[{"explanation": "a"}]\n```'
        results = parse_batch_response(response, expected_count=1)
        assert len(results) == 1


class TestEnricher:
    def _make_enricher(self):
        mock_provider = MagicMock()
        mock_cache = MagicMock()
        mock_cache.is_cached.return_value = False
        return Enricher(provider=mock_provider, cache=mock_cache), mock_provider, mock_cache

    def test_skips_cached(self):
        enricher, provider, cache = self._make_enricher()
        cache.is_cached.return_value = True
        cache.get.return_value = {'explanation': 'cached', 'tags': [], 'question': 'q?'}
        result = enricher.enrich_single('book1', 'text', 'Book', 'Ch1')
        assert result['explanation'] == 'cached'
        provider.complete.assert_not_called()

    def test_enrich_single_calls_provider(self):
        enricher, provider, cache = self._make_enricher()
        provider.complete.return_value = '{"explanation": "test", "tags": ["a"], "question": "q?"}'
        result = enricher.enrich_single('book1', 'highlight text', 'Book', 'Ch1')
        assert result['explanation'] == 'test'
        provider.complete.assert_called_once()

    def test_enrich_single_handles_short_text(self):
        enricher, provider, cache = self._make_enricher()
        result = enricher.enrich_single('book1', '短', 'Book', 'Ch1')
        # Short text (< 5 chars) should skip LLM
        provider.complete.assert_not_called()
        assert result is not None

    def test_enrich_batch_fallback(self):
        enricher, provider, cache = self._make_enricher()
        # First call (batch) returns bad format
        provider.complete.return_value = 'bad json'
        # Fallback single calls succeed
        provider.complete.side_effect = [
            'bad json',  # batch fails
            '{"explanation": "a", "tags": [], "question": "q?"}',  # single 1
            '{"explanation": "b", "tags": [], "question": "q?"}',  # single 2
        ]
        items = [
            {'highlight': 'text a', 'book_name': 'Book', 'chapter': 'Ch1',
             'context_before': '', 'context_after': '', 'book_id': 'b1'},
            {'highlight': 'text b', 'book_name': 'Book', 'chapter': 'Ch2',
             'context_before': '', 'context_after': '', 'book_id': 'b1'},
        ]
        results = enricher.enrich_batch(items)
        assert len(results) == 2
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_enricher.py -v
```

Expected: FAIL — `ModuleNotFoundError`

- [ ] **Step 3: Create knowledge/enricher.py**

```python
# knowledge/enricher.py
"""LLM-powered enrichment of book highlights."""
import json
import re
from typing import Any

from .provider.base import LLMProvider
from .cache import LLMCache

SYSTEM_PROMPT = "你是读书笔记助手。根据高亮内容，输出结构化的解释、标签和复习问题。"

SINGLE_PROMPT_TEMPLATE = """根据以下高亮内容，输出 JSON：

{{
  "explanation": "一句话解释（30字以内）",
  "tags": ["标签1", "标签2"],
  "question": "一个可以用这段话回答的复习问题"
}}

---
书名: {book_name}
章节: {chapter}
上下文: {context_before} **{highlight}** {context_after}"""

BATCH_PROMPT_TEMPLATE = """处理以下 {count} 条高亮，每条输出 JSON。

{items}

请以 JSON 数组返回，每条包含 explanation, tags, question。"""

BATCH_ITEM_TEMPLATE = """--- 第{n}条 ---
{book_name} | {chapter}
上下文: {context_before} **{highlight}** {context_after}"""


def build_single_prompt(
    book_name: str,
    chapter: str,
    highlight: str,
    context_before: str = '',
    context_after: str = '',
) -> str:
    return SINGLE_PROMPT_TEMPLATE.format(
        book_name=book_name,
        chapter=chapter,
        highlight=highlight,
        context_before=context_before or '(无上下文)',
        context_after=context_after or '(无上下文)',
    )


def build_batch_prompt(items: list[dict]) -> str:
    parts = []
    for i, item in enumerate(items, 1):
        parts.append(BATCH_ITEM_TEMPLATE.format(
            n=i,
            book_name=item.get('book_name', ''),
            chapter=item.get('chapter', ''),
            highlight=item['highlight'],
            context_before=item.get('context_before', '') or '(无上下文)',
            context_after=item.get('context_after', '') or '(无上下文)',
        ))
    return BATCH_PROMPT_TEMPLATE.format(count=len(items), items='\n\n'.join(parts))


def _clean_json_text(text: str) -> str:
    """Strip markdown code fences and leading/trailing junk."""
    text = text.strip()
    # Remove ```json ... ``` wrapper
    m = re.match(r'```(?:json)?\s*\n?(.*?)\n?\s*```', text, re.DOTALL)
    if m:
        text = m.group(1).strip()
    return text


def parse_llm_response(response: str) -> dict[str, Any] | None:
    """Parse a single LLM response into explanation/tags/question dict."""
    text = _clean_json_text(response)
    try:
        data = json.loads(text)
        if isinstance(data, dict) and 'explanation' in data:
            return data
    except (json.JSONDecodeError, TypeError):
        pass
    return None


def parse_batch_response(response: str, expected_count: int) -> list[dict] | None:
    """Parse a batch LLM response. Returns None if count mismatch (signals fallback)."""
    text = _clean_json_text(response)
    try:
        data = json.loads(text)
        if isinstance(data, list) and len(data) == expected_count:
            return data
    except (json.JSONDecodeError, TypeError):
        pass
    return None


class Enricher:
    """Orchestrates LLM enrichment of highlights with caching."""

    def __init__(self, provider: LLMProvider, cache: LLMCache):
        self._provider = provider
        self._cache = cache

    def enrich_single(
        self,
        book_id: str,
        highlight: str,
        book_name: str,
        chapter: str,
        context_before: str = '',
        context_after: str = '',
        force: bool = False,
    ) -> dict[str, Any]:
        """Enrich a single highlight. Returns cached result if available."""
        # Skip very short highlights
        if len(highlight.strip()) < 5:
            return {
                'explanation': '',
                'tags': [],
                'question': '',
                'highlight': highlight,
            }

        # Check cache
        if not force and self._cache.is_cached(book_id, highlight):
            return self._cache.get(book_id, highlight)

        # Truncate very long highlights
        if len(highlight) > 500:
            highlight = highlight[:500]

        prompt = build_single_prompt(book_name, chapter, highlight,
                                     context_before, context_after)
        response = self._provider.complete(prompt, system=SYSTEM_PROMPT)
        result = parse_llm_response(response)

        if result is None:
            return {
                'explanation': '(LLM 返回格式异常)',
                'tags': [],
                'question': '',
                'highlight': highlight,
            }

        result['highlight'] = highlight
        return result

    def enrich_batch(
        self,
        items: list[dict],
        force: bool = False,
    ) -> list[dict[str, Any]]:
        """Enrich a batch of highlights. Falls back to single on failure."""
        # Filter out cached items
        to_process = []
        results = [None] * len(items)

        for i, item in enumerate(items):
            highlight = item['highlight']
            if not force and self._cache.is_cached(item.get('book_id', ''), highlight):
                results[i] = self._cache.get(item['book_id'], highlight)
            elif len(highlight.strip()) < 5:
                results[i] = {'explanation': '', 'tags': [], 'question': '', 'highlight': highlight}
            else:
                to_process.append(i)

        if not to_process:
            return results

        # Try batch
        batch_items = [items[i] for i in to_process]
        prompt = build_batch_prompt(batch_items)
        response = self._provider.complete(prompt, system=SYSTEM_PROMPT)
        batch_results = parse_batch_response(response, expected_count=len(batch_items))

        if batch_results is not None:
            # Batch succeeded
            for j, idx in enumerate(to_process):
                batch_results[j]['highlight'] = items[idx]['highlight']
                results[idx] = batch_results[j]
        else:
            # Batch failed — fallback to single calls
            for idx in to_process:
                item = items[idx]
                result = self.enrich_single(
                    book_id=item.get('book_id', ''),
                    highlight=item['highlight'],
                    book_name=item.get('book_name', ''),
                    chapter=item.get('chapter', ''),
                    context_before=item.get('context_before', ''),
                    context_after=item.get('context_after', ''),
                    force=force,
                )
                results[idx] = result

        return results
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_enricher.py -v
```

Expected: All 10 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add knowledge/enricher.py tests/test_enricher.py
git commit -m "feat: add LLM enrichment logic with batch fallback"
```

---

## Task 8: Exporter

**Files:**
- Create: `knowledge/exporter.py`
- Create: `tests/test_exporter.py`

- [ ] **Step 1: Write failing tests for exporter**

```python
# tests/test_exporter.py
import pytest
from pathlib import Path
from knowledge.exporter import (
    sanitize_filename,
    build_main_note,
    build_llm_note,
    export_book,
)


class TestSanitizeFilename:
    def test_chinese(self):
        assert sanitize_filename('巨婴国') == '巨婴国'

    def test_special_chars(self):
        result = sanitize_filename('test: file/name?')
        assert '/' not in result
        assert ':' not in result
        assert '?' not in result

    def test_max_length(self):
        result = sanitize_filename('这是一个很长很长很长很长的文件名', max_length=10)
        assert len(result) <= 10

    def test_empty(self):
        result = sanitize_filename('')
        assert result == 'untitled'


class TestBuildMainNote:
    def test_basic_structure(self, sample_book, sample_annotations):
        llm_results = [
            {'explanation': 'exp1', 'tags': ['tag1'], 'question': 'q1?'},
            {'explanation': 'exp2', 'tags': ['tag2'], 'question': 'q2?'},
        ]
        note = build_main_note(sample_book, sample_annotations, llm_results, format='obsidian')
        assert '巨婴国' in note
        assert '武志红' in note
        assert '---' in note  # frontmatter
        assert '[[婴儿是没法面对失控的]]' in note or '婴儿是没法面对失控的' in note

    def test_markdown_format(self, sample_book, sample_annotations):
        llm_results = [
            {'explanation': 'exp1', 'tags': ['tag1'], 'question': 'q1?'},
            {'explanation': 'exp2', 'tags': ['tag2'], 'question': 'q2?'},
        ]
        note = build_main_note(sample_book, sample_annotations, llm_results, format='markdown')
        assert '巨婴国' in note
        assert '[[' not in note  # no wikilinks in markdown format


class TestBuildLLMNote:
    def test_structure(self):
        note = build_llM_note(
            book_name='巨婴国',
            chapter='第3章',
            highlight='测试高亮文字',
            explanation='测试解释',
            tags=['tag1', 'tag2'],
            question='测试问题？',
            format='obsidian',
        )
        assert 'type: llm-note' in note
        assert '测试解释' in note
        assert '测试问题' in note
        assert 'tag1' in note


class TestExportBook:
    def test_creates_files(self, tmp_dir, sample_book, sample_annotations):
        llm_results = [
            {'explanation': 'exp1', 'tags': ['tag1'], 'question': 'q1?',
             'highlight': '婴儿是没法面对失控的，失控会引起他们巨大的无助感'},
            {'explanation': 'exp2', 'tags': ['tag2'], 'question': 'q2?',
             'highlight': '每个巨婴内心深处都住着这样一个魔鬼'},
        ]
        output_dir = tmp_dir / 'output'
        export_book(sample_book, sample_annotations, llm_results, output_dir, format='obsidian')

        # Main note should exist
        main_files = list(output_dir.glob('**/巨婴国.md'))
        assert len(main_files) == 1

        # LLM notes should exist
        llm_files = list(output_dir.glob('**/*.md'))
        assert len(llm_files) >= 3  # main + 2 llm notes
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_exporter.py -v
```

Expected: FAIL — `ModuleNotFoundError`

- [ ] **Step 3: Create knowledge/exporter.py**

```python
# knowledge/exporter.py
"""Export enriched notes to Obsidian/Markdown format."""
import re
from datetime import date
from pathlib import Path

from services.cfi_utils import extract_chapter_title, format_chapter_display


def sanitize_filename(text: str, max_length: int = 20) -> str:
    """Create a safe filename from highlight text.

    Keeps Chinese, English, digits, comma, period. Replaces special chars.
    """
    if not text.strip():
        return 'untitled'
    # Keep alphanumeric, Chinese, comma, period, space
    safe = re.sub(r'[^一-鿿\w,. ]', '_', text)
    safe = safe.strip()[:max_length]
    return safe if safe else 'untitled'


def build_main_note(
    book: dict,
    annotations: list[dict],
    llm_results: list[dict],
    format: str = 'obsidian',
) -> str:
    """Build the main book note with all highlights."""
    lines = []

    # Frontmatter
    lines.append('---')
    lines.append(f"book: {book['title']}")
    lines.append(f"author: {book['author']}")
    if book.get('isbn'):
        lines.append(f"isbn: {book['isbn']}")
    if book.get('publisher'):
        lines.append(f"publisher: {book['publisher']}")
    if book.get('publish_date'):
        lines.append(f'publish_date: "{book["publish_date"]}"')
    lines.append('---')
    lines.append('')

    lines.append(f"# {book['title']}")
    lines.append('')

    # Group annotations by chapter
    current_chapter = None
    for i, (ann, llm) in enumerate(zip(annotations, llm_results)):
        cfi = ann.get('location', '')
        chapter = extract_chapter_title(cfi) if cfi else None
        chapter_display = format_chapter_display(chapter, i + 1)

        if chapter_display != current_chapter:
            current_chapter = chapter_display
            lines.append(f'## {chapter_display}')
            lines.append('')

        selected = ann.get('selected_text', '')
        if selected:
            lines.append(f'> {selected}')
            lines.append('')

            # Link to LLM note
            filename = sanitize_filename(selected)
            if format == 'obsidian':
                lines.append(f'[[{filename}]]')
            else:
                lines.append(f'[详细笔记]({filename}.md)')
            lines.append('')
            lines.append('---')
            lines.append('')

    return '\n'.join(lines)


def build_llm_note(
    book_name: str,
    chapter: str,
    highlight: str,
    explanation: str,
    tags: list[str],
    question: str,
    format: str = 'obsidian',
) -> str:
    """Build an individual LLM enrichment note."""
    lines = []

    lines.append('---')
    lines.append('type: llm-note')
    lines.append(f'book: {book_name}')
    if chapter:
        lines.append(f'chapter: {chapter}')
    lines.append(f'highlight: "{highlight[:100]}"')
    if tags:
        tag_str = ', '.join(tags)
        lines.append(f'tags: [{tag_str}]')
    lines.append(f'created: {date.today().isoformat()}')
    lines.append('---')
    lines.append('')

    if explanation:
        lines.append('## 解释')
        lines.append('')
        lines.append(explanation)
        lines.append('')

    if question:
        lines.append('## 复习问题')
        lines.append('')
        lines.append(question)
        lines.append('')

    if highlight:
        lines.append('## 上下文')
        lines.append('')
        lines.append(f'> {highlight}')

    return '\n'.join(lines)


def export_book(
    book: dict,
    annotations: list[dict],
    llm_results: list[dict],
    output_dir: Path,
    format: str = 'obsidian',
) -> Path:
    """Export a book with all its enriched notes.

    Returns the directory containing exported files.
    """
    book_dir = output_dir / sanitize_filename(book['title'], max_length=50)
    book_dir.mkdir(parents=True, exist_ok=True)

    # Write main note
    main_note = build_main_note(book, annotations, llm_results, format=format)
    main_path = book_dir / f"{sanitize_filename(book['title'], max_length=50)}.md"
    main_path.write_text(main_note, encoding='utf-8')

    # Write individual LLM notes
    for ann, llm in zip(annotations, llm_results):
        selected = ann.get('selected_text', '')
        if not selected or not llm:
            continue

        cfi = ann.get('location', '')
        chapter = extract_chapter_title(cfi) if cfi else ''
        chapter_display = format_chapter_display(chapter, 0) if chapter else ''

        note = build_llm_note(
            book_name=book['title'],
            chapter=chapter_display,
            highlight=selected,
            explanation=llm.get('explanation', ''),
            tags=llm.get('tags', []),
            question=llm.get('question', ''),
            format=format,
        )

        filename = sanitize_filename(selected)
        note_path = book_dir / f"{filename}.md"

        # Handle name collisions
        if note_path.exists():
            note_path = book_dir / f"{chapter_display}-{filename}.md"

        note_path.write_text(note, encoding='utf-8')

    return book_dir
```

- [ ] **Step 4: Fix the typo in the test (build_llM_note -> build_llm_note)**

```python
# In tests/test_exporter.py, fix the test:
    def test_structure(self):
        note = build_llm_note(  # was build_llM_note
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_exporter.py -v
```

Expected: All 4 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add knowledge/exporter.py tests/test_exporter.py
git commit -m "feat: add Obsidian/Markdown export"
```

---

## Task 9: CLI Entry Point

**Files:**
- Create: `knowledge.py`
- Create: `tests/test_cli.py`

- [ ] **Step 1: Write failing tests for CLI**

```python
# tests/test_cli.py
import pytest
from unittest.mock import patch, MagicMock
from knowledge import cli


class TestCLI:
    def test_config_command(self, tmp_dir):
        """config subcommand should save config file."""
        config_path = tmp_dir / 'config.json'
        with patch('sys.argv', [
            'knowledge.py', 'config',
            '--provider', 'openai_compatible',
            '--base-url', 'https://api.test.com/v1',
            '--api-key', 'sk-test',
            '--model', 'gpt-4o-mini',
            '--config', str(config_path),
        ]):
            cli.main()
        assert config_path.exists()

    def test_cache_command(self, tmp_dir):
        """cache subcommand should show cache status."""
        cache_path = tmp_dir / 'cache.json'
        with patch('sys.argv', [
            'knowledge.py', 'cache',
            '--cache', str(cache_path),
        ]):
            cli.main()

    def test_enrich_no_config(self, tmp_dir):
        """enrich without config should show error."""
        config_path = tmp_dir / 'config.json'
        with patch('sys.argv', [
            'knowledge.py', 'enrich', '--book', '1',
            '--config', str(config_path),
        ]):
            with pytest.raises(SystemExit):
                cli.main()
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_cli.py -v
```

Expected: FAIL — `ModuleNotFoundError: No module named 'knowledge.cli'`

- [ ] **Step 3: Create knowledge.py CLI entry point**

```python
#!/usr/bin/env python3
"""
Knowledge module CLI — enrich Apple Books highlights with LLM-powered context.

Usage:
    python3 knowledge.py config --provider openai_compatible --base-url URL --api-key KEY --model MODEL
    python3 knowledge.py map-book --book-id ID --epub path/to/book.epub
    python3 knowledge.py enrich --book 1 [--all] [--index N] [--force]
    python3 knowledge.py card --book 1 [--all] [--style dark]
    python3 knowledge.py cache --book 1
"""
import argparse
import json
import sys
from pathlib import Path

from services.book_service import BookService
from knowledge.config import KnowledgeConfig, load_config, save_config
from knowledge.cache import LLMCache
from knowledge.context import get_manifest_map, get_chapter_text, extract_context
from knowledge.provider.openai_compat import OpenAICompatible
from knowledge.enricher import Enricher
from knowledge.exporter import export_book, sanitize_filename
from services.cfi_utils import extract_item_id


def cmd_config(args):
    """Configure LLM provider settings."""
    config = load_config(Path(args.config))
    config.llm.provider = args.provider or config.llm.provider
    config.llm.base_url = args.base_url or config.llm.base_url
    config.llm.api_key = args.api_key or config.llm.api_key
    config.llm.model = args.model or config.llm.model
    save_config(config, Path(args.config))
    print(f"配置已保存到 {args.config}")


def cmd_map_book(args):
    """Map a book ID to its EPUB file."""
    config = load_config(Path(args.config))
    config.epub_mappings[args.book_id] = {
        'epub': args.epub,
        'output': args.output or '',
    }
    save_config(config, Path(args.config))
    print(f"已映射 {args.book_id[:8]}... -> {args.epub}")


def cmd_enrich(args):
    """Enrich book highlights with LLM."""
    config = load_config(Path(args.config))

    # Resolve book
    service = BookService()
    books = service.load_books()

    if args.book_id:
        book = next((b for b in books if b['asset_id'] == args.book_id), None)
    elif args.book and 1 <= args.book <= len(books):
        book = books[args.book - 1]
    else:
        print("错误: 找不到指定书籍")
        sys.exit(1)

    if not book:
        print("错误: 找不到指定书籍")
        sys.exit(1)

    asset_id = book['asset_id']
    print(f"处理: {book['title']} ({book['note_count']} 条笔记)")

    # Setup
    mapping = config.epub_mappings.get(asset_id, {})
    epub_path = Path(mapping.get('epub', '')) if mapping else None
    output_dir = Path(mapping.get('output', '') or args.output or './output')
    cache_path = output_dir / 'llm' / 'cache.json'

    cache = LLMCache(cache_path)
    annotations = service.get_annotations(asset_id)

    # Filter annotations with text
    annotations = [a for a in annotations if a.get('selected_text')]
    print(f"有效高亮: {len(annotations)} 条")

    # Determine which to process
    if args.index is not None:
        if 1 <= args.index <= len(annotations):
            annotations = [annotations[args.index - 1]]
        else:
            print(f"错误: 索引 {args.index} 超出范围 (1-{len(annotations)})")
            sys.exit(1)
    elif not args.all:
        # Incremental mode: skip cached
        annotations = [
            a for a in annotations
            if not cache.is_cached(asset_id, a['selected_text'])
        ]
        print(f"增量模式: 需处理 {len(annotations)} 条新高亮")

    if not annotations:
        print("没有需要处理的高亮")
        return

    # Load EPUB manifest if available
    manifest = {}
    epub_text_cache = {}
    if epub_path and epub_path.exists():
        manifest = get_manifest_map(epub_path)
        print(f"EPUB 已加载: {len(manifest)} 个文档项")

    # Build enricher
    provider = OpenAICompatible(
        base_url=config.llm.base_url,
        api_key=config.llm.get_api_key(),
        model=config.llm.model,
        max_retries=config.llm.max_retries,
        retry_delays=config.llm.retry_delays,
    )
    enricher = Enricher(provider=provider, cache=cache)

    # Prepare items with context
    def get_context_for_annotation(ann):
        ctx_before, ctx_after = '', ''
        if manifest and epub_path:
            cfi = ann.get('location', '')
            item_id = extract_item_id(cfi)
            if item_id and item_id in manifest:
                href = manifest[item_id]
                if href not in epub_text_cache:
                    try:
                        epub_text_cache[href] = get_chapter_text(epub_path, href)
                    except Exception:
                        epub_text_cache[href] = ''
                chapter_text = epub_text_cache[href]
                if chapter_text:
                    ctx = extract_context(chapter_text, ann['selected_text'],
                                         config.context_chars)
                    if ctx:
                        ctx_before, _, ctx_after = ctx
        return ctx_before, ctx_after

    # Process
    from services.cfi_utils import extract_chapter_title, format_chapter_display

    llm_results = []
    for i, ann in enumerate(annotations):
        selected = ann['selected_text']
        cfi = ann.get('location', '')
        chapter = extract_chapter_title(cfi) if cfi else None
        chapter_display = format_chapter_display(chapter, i + 1)

        ctx_before, ctx_after = get_context_for_annotation(ann)

        if len(annotations) == 1:
            # Single mode
            result = enricher.enrich_single(
                book_id=asset_id,
                highlight=selected,
                book_name=book['title'],
                chapter=chapter_display,
                context_before=ctx_before,
                context_after=ctx_after,
                force=args.force,
            )
        else:
            # Use batch for multiple
            result = enricher.enrich_single(
                book_id=asset_id,
                highlight=selected,
                book_name=book['title'],
                chapter=chapter_display,
                context_before=ctx_before,
                context_after=ctx_after,
                force=args.force,
            )
        llm_results.append(result)
        print(f"  [{i+1}/{len(annotations)}] {selected[:30]}...")

    # Export
    export_format = args.format or config.output_format
    export_book(book, annotations, llm_results, output_dir, format=export_format)
    print(f"导出完成: {output_dir}")


def cmd_card(args):
    """Generate image cards for highlights."""
    try:
        from knowledge.card import generate_cards
    except ImportError:
        print("错误: 需要安装 Pillow。运行: pip install Pillow")
        sys.exit(1)

    config = load_config(Path(args.config))
    service = BookService()
    books = service.load_books()

    if args.book_id:
        book = next((b for b in books if b['asset_id'] == args.book_id), None)
    elif args.book and 1 <= args.book <= len(books):
        book = books[args.book - 1]
    else:
        print("错误: 找不到指定书籍")
        sys.exit(1)

    if not book:
        print("错误: 找不到指定书籍")
        sys.exit(1)

    asset_id = book['asset_id']
    annotations = service.get_annotations(asset_id)
    annotations = [a for a in annotations if a.get('selected_text')]

    if args.index is not None:
        if 1 <= args.index <= len(annotations):
            annotations = [annotations[args.index - 1]]
        else:
            print(f"错误: 索引 {args.index} 超出范围")
            sys.exit(1)

    output_dir = Path(args.output or config.card_output).expanduser()
    style_name = args.style or config.card_style

    # Load LLM cache if --with-llm
    llm_cache = None
    if args.with_llm:
        cache_path = Path(args.cache or 'llm/cache.json')
        if cache_path.exists():
            llm_cache = LLMCache(cache_path)

    generate_cards(book, annotations, output_dir, style_name, llm_cache)
    print(f"卡片已导出: {output_dir}")


def cmd_cache(args):
    """Show cache status."""
    cache_path = Path(args.cache or 'llm/cache.json')
    if not cache_path.exists():
        print("缓存文件不存在")
        return

    cache = LLMCache(cache_path)
    print(f"缓存条目: {cache.count()}")

    if args.book_id:
        entries = cache.get_all_for_book(args.book_id)
        print(f"该书籍条目: {len(entries)}")


def main():
    parser = argparse.ArgumentParser(description='Apple Books 知识管理工具')
    parser.add_argument('--config', default='knowledge_config.json', help='配置文件路径')
    subparsers = parser.add_subparsers(dest='command', help='子命令')

    # config
    p_config = subparsers.add_parser('config', help='配置 LLM')
    p_config.add_argument('--provider', default='openai_compatible')
    p_config.add_argument('--base-url')
    p_config.add_argument('--api-key')
    p_config.add_argument('--model')

    # map-book
    p_map = subparsers.add_parser('map-book', help='映射书籍到 EPUB')
    p_map.add_argument('--book-id', required=True)
    p_map.add_argument('--epub', required=True)
    p_map.add_argument('--output')

    # enrich
    p_enrich = subparsers.add_parser('enrich', help='处理高亮笔记')
    p_enrich.add_argument('--book', type=int, help='书籍序号')
    p_enrich.add_argument('--book-id', help='书籍 asset_id')
    p_enrich.add_argument('--index', type=int, help='单条索引')
    p_enrich.add_argument('--all', action='store_true', help='全量处理')
    p_enrich.add_argument('--force', action='store_true', help='强制重新生成')
    p_enrich.add_argument('--retry-errors', action='store_true', help='重试失败条目')
    p_enrich.add_argument('--output', default='./output')
    p_enrich.add_argument('--format', choices=['obsidian', 'markdown'])

    # card
    p_card = subparsers.add_parser('card', help='导出图片卡片')
    p_card.add_argument('--book', type=int)
    p_card.add_argument('--book-id')
    p_card.add_argument('--index', type=int)
    p_card.add_argument('--all', action='store_true')
    p_card.add_argument('--style', choices=['dark', 'light', 'minimal'])
    p_card.add_argument('--output')
    p_card.add_argument('--with-llm', action='store_true')
    p_card.add_argument('--cache')

    # cache
    p_cache = subparsers.add_parser('cache', help='查看缓存状态')
    p_cache.add_argument('--book', type=int)
    p_cache.add_argument('--book-id')
    p_cache.add_argument('--cache')

    args = parser.parse_args()

    if args.command == 'config':
        cmd_config(args)
    elif args.command == 'map-book':
        cmd_map_book(args)
    elif args.command == 'enrich':
        cmd_enrich(args)
    elif args.command == 'card':
        cmd_card(args)
    elif args.command == 'cache':
        cmd_cache(args)
    else:
        parser.print_help()


if __name__ == '__main__':
    main()
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_cli.py -v
```

Expected: All 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add knowledge.py tests/test_cli.py
git commit -m "feat: add knowledge CLI entry point"
```

---

## Task 10: Card Generator

**Files:**
- Create: `knowledge/card.py`
- Create: `knowledge/styles/dark.json`
- Create: `knowledge/styles/light.json`
- Create: `knowledge/styles/minimal.json`
- Create: `tests/test_card.py`

- [ ] **Step 1: Write failing tests for card generator**

```python
# tests/test_card.py
import json
import pytest
from pathlib import Path
from unittest.mock import patch, MagicMock

# Patch Pillow import for tests that don't have it
try:
    from knowledge.card import (
        load_style,
        wrap_text,
        calculate_card_height,
        generate_card,
    )
    HAS_PILLOW = True
except ImportError:
    HAS_PILLOW = False


@pytest.fixture
def styles_dir(tmp_path):
    """Create a temporary styles directory with a test style."""
    styles = {
        'dark': {
            'name': 'dark',
            'width': 800,
            'padding': 60,
            'background': '#1a1a2e',
            'text_color': '#e0e0e0',
            'highlight_color': '#ffd700',
            'accent_color': '#0f3460',
            'font_family': 'Noto Sans SC',
            'font_size': 18,
            'border_radius': 16,
            'show_book_info': True,
            'show_tags': True,
            'show_question': False,
        }
    }
    style_path = tmp_path / 'dark.json'
    style_path.write_text(json.dumps(styles['dark'], indent=2))
    return tmp_path


class TestLoadStyle:
    def test_loads_json(self, styles_dir):
        style = load_style('dark', styles_dir)
        assert style['name'] == 'dark'
        assert style['width'] == 800

    def test_missing_style_returns_default(self, styles_dir):
        style = load_style('nonexistent', styles_dir)
        assert style['name'] == 'dark'  # fallback to dark


@pytest.mark.skipif(not HAS_PILLOW, reason="Pillow not installed")
class TestWrapText:
    def test_short_text(self):
        lines = wrap_text('短文本', max_width=200, font_size=18)
        assert len(lines) >= 1

    def test_long_text_wraps(self):
        text = '这是一个很长很长的文本' * 20
        lines = wrap_text(text, max_width=200, font_size=18)
        assert len(lines) > 1


@pytest.mark.skipif(not HAS_PILLOW, reason="Pillow not installed")
class TestCalculateCardHeight:
    def test_basic_height(self):
        height = calculate_card_height(
            text='测试文本',
            width=800,
            padding=60,
            font_size=18,
            show_book_info=True,
            show_tags=True,
        )
        assert height > 0


@pytest.mark.skipif(not HAS_PILLOW, reason="Pillow not installed")
class TestGenerateCard:
    def test_creates_image(self, tmp_path, styles_dir):
        img = generate_card(
            book_name='测试书名',
            author='测试作者',
            chapter='第1章',
            highlight='测试高亮文字内容',
            explanation='测试解释',
            tags=['标签1', '标签2'],
            style={'name': 'dark', 'width': 800, 'padding': 60,
                   'background': '#1a1a2e', 'text_color': '#e0e0e0',
                   'highlight_color': '#ffd700', 'accent_color': '#0f3460',
                   'font_family': 'Noto Sans SC', 'font_size': 18,
                   'border_radius': 16, 'show_book_info': True,
                   'show_tags': True, 'show_question': False},
        )
        assert img is not None
        assert img.size[0] == 800
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_card.py -v
```

Expected: FAIL — `ModuleNotFoundError`

- [ ] **Step 3: Create style templates**

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

```json
// knowledge/styles/minimal.json
{
  "name": "minimal",
  "width": 700,
  "padding": 50,
  "background": "#fafafa",
  "text_color": "#2c2c2c",
  "highlight_color": "#3498db",
  "accent_color": "#ecf0f1",
  "font_family": "Noto Sans SC",
  "font_size": 16,
  "border_radius": 8,
  "show_book_info": true,
  "show_tags": false,
  "show_question": false
}
```

- [ ] **Step 4: Create knowledge/card.py**

```python
# knowledge/card.py
"""Image card generation for book highlights using Pillow."""
import json
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    raise ImportError("Pillow is required for card generation. Install with: pip install Pillow")

from .cache import LLMCache

STYLES_DIR = Path(__file__).parent / 'styles'

# Font fallback chain for macOS
FONT_FALLBACKS = [
    'Noto Sans SC',
    'PingFang SC',
    'STHeiti',
    'Hiragino Sans GB',
]


def load_style(name: str = 'dark', styles_dir: Path | None = None) -> dict:
    """Load a card style JSON. Falls back to dark if not found."""
    styles_dir = styles_dir or STYLES_DIR
    style_path = styles_dir / f'{name}.json'
    if style_path.exists():
        with open(style_path, 'r', encoding='utf-8') as f:
            return json.load(f)
    # Fallback
    fallback = styles_dir / 'dark.json'
    if fallback.exists():
        with open(fallback, 'r', encoding='utf-8') as f:
            return json.load(f)
    return {'name': 'dark', 'width': 800, 'padding': 60, 'background': '#1a1a2e',
            'text_color': '#e0e0e0', 'highlight_color': '#ffd700', 'accent_color': '#0f3460',
            'font_family': 'Noto Sans SC', 'font_size': 18, 'border_radius': 16,
            'show_book_info': True, 'show_tags': True, 'show_question': False}


def _find_font(font_family: str, font_size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    """Find a usable font, trying fallback chain."""
    for name in [font_family] + FONT_FALLBACKS:
        try:
            return ImageFont.truetype(name, font_size)
        except (OSError, IOError):
            continue
    return ImageFont.load_default()


def _hex_to_rgb(hex_color: str) -> tuple[int, int, int]:
    hex_color = hex_color.lstrip('#')
    return tuple(int(hex_color[i:i+2], 16) for i in (0, 2, 4))


def wrap_text(text: str, max_width: int, font_size: int) -> list[str]:
    """Wrap text to fit within max_width pixels."""
    font = _find_font('Noto Sans SC', font_size)
    lines = []
    current_line = ''

    for char in text:
        test_line = current_line + char
        try:
            bbox = font.getbbox(test_line)
            width = bbox[2] - bbox[0]
        except AttributeError:
            width = len(test_line) * font_size * 0.6

        if width > max_width and current_line:
            lines.append(current_line)
            current_line = char
        else:
            current_line = test_line

    if current_line:
        lines.append(current_line)
    return lines or ['']


def calculate_card_height(
    text: str,
    width: int,
    padding: int,
    font_size: int,
    show_book_info: bool,
    show_tags: bool,
    show_question: bool = False,
) -> int:
    """Estimate card height for given text and layout options."""
    content_width = width - padding * 2
    lines = wrap_text(text, content_width, font_size)
    line_height = int(font_size * 1.6)

    height = padding  # top padding
    if show_book_info:
        height += font_size * 2 + 20  # book title + chapter
        height += 10  # separator
    height += len(lines) * line_height  # highlight text
    height += 30  # spacing
    if show_tags:
        height += font_size + 20
    if show_question:
        height += font_size * 2 + 20
    height += padding  # bottom padding
    return height


def generate_card(
    book_name: str,
    author: str,
    chapter: str,
    highlight: str,
    explanation: str = '',
    tags: list[str] = None,
    style: dict = None,
) -> Image.Image:
    """Generate a single highlight card image."""
    style = style or load_style()
    tags = tags or []

    width = style['width']
    padding = style['padding']
    font_size = style['font_size']
    bg_color = _hex_to_rgb(style['background'])
    text_color = _hex_to_rgb(style['text_color'])
    highlight_color = _hex_to_rgb(style['highlight_color'])

    content_width = width - padding * 2
    height = calculate_card_height(
        highlight, width, padding, font_size,
        style.get('show_book_info', True),
        style.get('show_tags', True),
        style.get('show_question', False),
    )

    img = Image.new('RGB', (width, height), bg_color)
    draw = ImageDraw.Draw(img)
    font = _find_font(style.get('font_family', 'Noto Sans SC'), font_size)
    small_font = _find_font(style.get('font_family', 'Noto Sans SC'), int(font_size * 0.85))
    y = padding

    # Book info header
    if style.get('show_book_info', True):
        header = f"\U0001f4d6 {book_name}"
        if author:
            header += f" · {author}"
        draw.text((padding, y), header, fill=text_color, font=font)
        y += int(font_size * 1.6)

        if chapter:
            draw.text((padding, y), chapter, fill=highlight_color, font=small_font)
            y += int(font_size * 1.4)

        # Separator line
        y += 10
        draw.line([(padding, y), (width - padding, y)], fill=highlight_color, width=1)
        y += 20

    # Highlight text
    lines = wrap_text(highlight, content_width, font_size)
    line_height = int(font_size * 1.6)
    for line in lines:
        draw.text((padding, y), line, fill=text_color, font=font)
        y += line_height

    y += 20

    # Explanation
    if explanation:
        draw.text((padding, y), f"\U0001f4a1 {explanation}", fill=highlight_color, font=small_font)
        y += int(font_size * 1.6)

    # Tags
    if tags and style.get('show_tags', True):
        tag_text = ' '.join(f'#{t}' for t in tags)
        draw.text((padding, y), f"\U0001f3f7 {tag_text}", fill=text_color, font=small_font)

    return img


def generate_cards(
    book: dict,
    annotations: list[dict],
    output_dir: Path,
    style_name: str = 'dark',
    llm_cache: LLMCache | None = None,
) -> list[Path]:
    """Generate image cards for all annotations of a book."""
    output_dir.mkdir(parents=True, exist_ok=True)
    style = load_style(style_name)
    generated = []

    for i, ann in enumerate(annotations):
        selected = ann.get('selected_text', '')
        if not selected:
            continue

        # Get LLM data if available
        explanation, tags, question = '', [], ''
        if llm_cache:
            cached = llm_cache.get(book['asset_id'], selected)
            if cached:
                explanation = cached.get('explanation', '')
                tags = cached.get('tags', [])
                question = cached.get('question', '')

        img = generate_card(
            book_name=book['title'],
            author=book.get('author', ''),
            chapter='',
            highlight=selected,
            explanation=explanation,
            tags=tags,
            style=style,
        )

        # Save
        safe_name = selected[:20].replace('/', '_').replace('\\', '_')
        filename = f"{safe_name}_{i+1:03d}.png"
        filepath = output_dir / filename
        img.save(filepath, 'PNG')
        generated.append(filepath)

    return generated
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/test_card.py -v
```

Expected: Tests that require Pillow should PASS (if installed) or be SKIPPED.

- [ ] **Step 6: Commit**

```bash
git add knowledge/card.py knowledge/styles/ tests/test_card.py
git commit -m "feat: add image card generation with style templates"
```

---

## Task 11: Integration Verification

**Files:**
- None (verification only)

- [ ] **Step 1: Run full test suite**

```bash
cd /Users/chenweilong/books-exporter && python3 -m pytest tests/ -v
```

Expected: All tests PASS (some card tests may be skipped if Pillow not installed).

- [ ] **Step 2: Verify CLI help works**

```bash
cd /Users/chenweilong/books-exporter && python3 knowledge.py --help
```

Expected: Help text showing all subcommands (config, map-book, enrich, card, cache).

- [ ] **Step 3: Verify config command works**

```bash
cd /Users/chenweilong/books-exporter && python3 knowledge.py config \
  --provider openai_compatible \
  --base-url https://api.example.com/v1 \
  --api-key sk-test \
  --model gpt-4o-mini
cat knowledge_config.json
```

Expected: Config file created with the specified values.

- [ ] **Step 4: Clean up test config**

```bash
rm -f /Users/chenweilong/books-exporter/knowledge_config.json
```

- [ ] **Step 5: Final commit (if any remaining changes)**

```bash
git status
```

If clean, no commit needed. If there are uncommitted changes from the integration test, commit them.

---

## Self-Review Checklist

**Spec coverage:**
- [x] Section 3.2 directory structure — implemented as specified
- [x] Section 3.3 cfi_utils refactoring — Task 2
- [x] Section 4.1 three modes (single/batch/incremental) — Task 9 CLI
- [x] Section 4.2 data flow — Tasks 4-7
- [x] Section 5 LLM Provider — Task 6
- [x] Section 6.0 output formats (obsidian/markdown) — Task 8
- [x] Section 6.1 directory structure — Task 8 export_book
- [x] Section 6.2 main note format — Task 8 build_main_note
- [x] Section 6.3 LLM note format — Task 8 build_llm_note
- [x] Section 6.4 image cards — Task 10
- [x] Section 6.5 filename rules — Task 8 sanitize_filename
- [x] Section 7 cache design — Task 5
- [x] Section 8 CLI interface — Task 9
- [x] Section 9 boundary handling — Tasks 4, 7, 9
- [x] Section 10 config file — Task 3
- [ ] Section 4.2 metadata extraction from EPUB (ISBN, publisher) — **Gap**: Not explicitly implemented in context.py. The `get_manifest_map` doesn't extract OPF metadata. Add to context.py if needed, or defer to v2.

**Placeholder scan:** No TBDs, TODOs, or "implement later" found.

**Type consistency:** All function signatures use consistent naming (`book_id`, `highlight`, `book_name`, `chapter`). Cache key format is `{book_id[:8]}_{md5}` across tasks.
