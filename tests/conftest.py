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
