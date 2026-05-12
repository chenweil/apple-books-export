# knowledge/card.py
"""Image card generation for book highlights using Pillow."""
from __future__ import annotations

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


def _find_font(font_family: str, font_size: int):
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
):
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
