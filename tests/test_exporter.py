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
        note = build_llm_note(
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
