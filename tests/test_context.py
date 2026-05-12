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
