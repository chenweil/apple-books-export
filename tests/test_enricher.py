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
