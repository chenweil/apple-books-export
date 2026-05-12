# tests/test_card.py
import json
import pytest
from pathlib import Path

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
