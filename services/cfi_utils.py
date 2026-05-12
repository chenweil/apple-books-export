# services/cfi_utils.py
"""Unified CFI parsing utilities for EPUB annotations."""
import re
from typing import Optional


def extract_item_id(cfi: str) -> Optional[str]:
    """Extract manifest item ID from EPUB CFI string.

    Example: epubcfi(/6/10[item4]!/4/82/1,:0,:44) -> 'item4'
    """
    if not cfi or not cfi.startswith('epubcfi('):
        return None
    match = re.search(r'\[([^\]]+)\]', cfi)
    if match:
        return match.group(1)
    return None


def extract_chapter_title(cfi: str) -> Optional[str]:
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
        # Strip file extension before further processing
        base = match
        if match.endswith('.xhtml') or match.endswith('.html'):
            base = re.sub(r'\.(xhtml|html)$', '', match, flags=re.IGNORECASE)

        # Chinese title — extract part after separator
        if re.search(r'[一-鿿]', base):
            if '-' in base or '_' in base:
                parts = re.split(r'[-_]', base, 1)
                if len(parts) > 1 and len(parts[1].strip()) > 2:
                    return parts[1].strip()
            return base

        # File names with extension — try section/chapter patterns
        if match.endswith('.xhtml') or match.endswith('.html'):
            section_match = re.match(r'Section(\d+)', base, re.IGNORECASE)
            if section_match:
                return f"第{int(section_match.group(1))}章"
            ch_match = re.match(r'(ch|chapter)(\d+)', base, re.IGNORECASE)
            if ch_match:
                return f"第{int(ch_match.group(2))}章"
            if len(base) > 3 and not is_meaningless_id(base):
                chapter_candidates.append(base)
            continue

        if len(match) > 3 and not is_meaningless_id(match) and is_valid_chapter(match):
            chapter_candidates.append(match)

    return chapter_candidates[0] if chapter_candidates else None


def format_chapter_display(chapter: Optional[str], index: int) -> str:
    """Format chapter for display. Falls back to position index."""
    if chapter:
        id_match = re.match(r'^id(\d+)$', chapter, re.IGNORECASE)
        if id_match:
            return f"位置 {id_match.group(1)}"
        return chapter
    return f"位置 {index}"
