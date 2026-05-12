# knowledge/exporter.py
"""Export enriched notes to Obsidian/Markdown format."""
from __future__ import annotations

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
