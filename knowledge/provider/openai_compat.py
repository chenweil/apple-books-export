# knowledge/provider/openai_compat.py
"""OpenAI-compatible LLM provider with retry logic."""
from __future__ import annotations

import time
import requests
from .base import LLMProvider


class OpenAICompatible(LLMProvider):
    """Works with OpenAI, DeepSeek, Ollama, Claude (via proxy), etc."""

    def __init__(self, base_url: str, api_key: str, model: str,
                 max_retries: int = 3, retry_delays: list[int] | None = None):
        self.base_url = base_url.rstrip('/')
        self.api_key = api_key
        self.model = model
        self._max_retries = max_retries
        self._retry_delays = retry_delays or [1, 2, 4]

    def complete(self, prompt: str, system: str = "") -> str:
        """Send a single prompt with exponential backoff retry."""
        messages = []
        if system:
            messages.append({'role': 'system', 'content': system})
        messages.append({'role': 'user', 'content': prompt})

        payload = {
            'model': self.model,
            'messages': messages,
            'temperature': 0.3,
        }
        headers = {
            'Authorization': f'Bearer {self.api_key}',
            'Content-Type': 'application/json',
        }

        last_error = None
        for attempt in range(self._max_retries):
            try:
                resp = requests.post(
                    f"{self.base_url}/chat/completions",
                    json=payload,
                    headers=headers,
                    timeout=60,
                )
                resp.raise_for_status()
                return resp.json()['choices'][0]['message']['content']
            except Exception as e:
                last_error = e
                if attempt < self._max_retries - 1:
                    delay = self._retry_delays[min(attempt, len(self._retry_delays) - 1)]
                    time.sleep(delay)

        raise last_error

    def batch_complete(self, prompts: list[str], system: str = "") -> list[str]:
        """Send prompts sequentially (OpenAI API doesn't support true batch)."""
        return [self.complete(p, system) for p in prompts]
