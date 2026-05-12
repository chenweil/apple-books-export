# knowledge/context.py
"""EPUB context extraction for book highlights."""
from __future__ import annotations

import re
import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path


def normalize_text(text: str) -> str:
    """Normalize whitespace: strip, collapse runs, and remove spaces around punctuation."""
    text = re.sub(r'\s+', ' ', text).strip()
    # Remove spaces around CJK/full-width punctuation for fuzzy matching
    _CJK_PUNCT = '，。！？；：、（）《》【】“”‘’…—'
    text = re.sub(rf'\s*([{_CJK_PUNCT}])\s*', r'\1', text)
    return text


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

    try:
        root = ET.fromstring(content_opf)
    except ET.ParseError:
        return {}

    # Try namespaced search first, then non-namespaced fallback
    items = {}
    for item in root.findall('.//{http://www.idpf.org/2007/opf}manifest/{http://www.idpf.org/2007/opf}item'):
        item_id = item.get('id')
        href = item.get('href')
        if item_id and href:
            items[item_id] = href

    if not items:
        ns = {'opf': 'http://www.idpf.org/2007/opf'}
        for item in root.findall('.//opf:manifest/opf:item', ns):
            item_id = item.get('id')
            href = item.get('href')
            if item_id and href:
                items[item_id] = href

    return items


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
