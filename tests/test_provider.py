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
