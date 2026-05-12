# knowledge/enricher.py
"""LLM-powered enrichment of book highlights."""
from __future__ import annotations

import json
import re
from typing import Any

from .provider.base import LLMProvider
from .cache import LLMCache

SYSTEM_PROMPT = "你是读书笔记助手。根据高亮内容，输出结构化的解释、标签和复习问题。"

SINGLE_PROMPT_TEMPLATE = """根据以下高亮内容，输出 JSON：

{{
  "explanation": "一句话解释（30字以内）",
  "tags": ["标签1", "标签2"],
  "question": "一个可以用这段话回答的复习问题"
}}

---
书名: {book_name}
章节: {chapter}
上下文: {context_before} **{highlight}** {context_after}"""

BATCH_PROMPT_TEMPLATE = """处理以下 {count} 条高亮，每条输出 JSON。

{items}

请以 JSON 数组返回，每条包含 explanation, tags, question。"""

BATCH_ITEM_TEMPLATE = """--- 第{n}条 ---
{book_name} | {chapter}
上下文: {context_before} **{highlight}** {context_after}"""


def build_single_prompt(
    book_name: str,
    chapter: str,
    highlight: str,
    context_before: str = '',
    context_after: str = '',
) -> str:
    return SINGLE_PROMPT_TEMPLATE.format(
        book_name=book_name,
        chapter=chapter,
        highlight=highlight,
        context_before=context_before or '(无上下文)',
        context_after=context_after or '(无上下文)',
    )


def build_batch_prompt(items: list[dict]) -> str:
    parts = []
    for i, item in enumerate(items, 1):
        parts.append(BATCH_ITEM_TEMPLATE.format(
            n=i,
            book_name=item.get('book_name', ''),
            chapter=item.get('chapter', ''),
            highlight=item['highlight'],
            context_before=item.get('context_before', '') or '(无上下文)',
            context_after=item.get('context_after', '') or '(无上下文)',
        ))
    return BATCH_PROMPT_TEMPLATE.format(count=len(items), items='\n\n'.join(parts))


def _clean_json_text(text: str) -> str:
    """Strip markdown code fences and leading/trailing junk."""
    text = text.strip()
    # Remove ```json ... ``` wrapper
    m = re.match(r'```(?:json)?\s*\n?(.*?)\n?\s*```', text, re.DOTALL)
    if m:
        text = m.group(1).strip()
    return text


def parse_llm_response(response: str) -> dict[str, Any] | None:
    """Parse a single LLM response into explanation/tags/question dict."""
    text = _clean_json_text(response)
    try:
        data = json.loads(text)
        if isinstance(data, dict) and 'explanation' in data:
            return data
    except (json.JSONDecodeError, TypeError):
        pass
    return None


def parse_batch_response(response: str, expected_count: int) -> list[dict] | None:
    """Parse a batch LLM response. Returns None if count mismatch (signals fallback)."""
    text = _clean_json_text(response)
    try:
        data = json.loads(text)
        if isinstance(data, list) and len(data) == expected_count:
            return data
    except (json.JSONDecodeError, TypeError):
        pass
    return None


class Enricher:
    """Orchestrates LLM enrichment of highlights with caching."""

    def __init__(self, provider: LLMProvider, cache: LLMCache):
        self._provider = provider
        self._cache = cache

    def enrich_single(
        self,
        book_id: str,
        highlight: str,
        book_name: str,
        chapter: str,
        context_before: str = '',
        context_after: str = '',
        force: bool = False,
    ) -> dict[str, Any]:
        """Enrich a single highlight. Returns cached result if available."""
        # Check cache first
        if not force and self._cache.is_cached(book_id, highlight):
            return self._cache.get(book_id, highlight)

        # Skip very short highlights
        if len(highlight.strip()) < 5:
            return {
                'explanation': '',
                'tags': [],
                'question': '',
                'highlight': highlight,
            }

        # Truncate very long highlights
        if len(highlight) > 500:
            highlight = highlight[:500]

        prompt = build_single_prompt(book_name, chapter, highlight,
                                     context_before, context_after)
        response = self._provider.complete(prompt, system=SYSTEM_PROMPT)
        result = parse_llm_response(response)

        if result is None:
            return {
                'explanation': '(LLM 返回格式异常)',
                'tags': [],
                'question': '',
                'highlight': highlight,
            }

        result['highlight'] = highlight
        # Write to cache
        self._cache.put(
            book_id=book_id,
            highlight=highlight,
            file='',
            book_name=book_name,
            explanation=result.get('explanation', ''),
            tags=result.get('tags', []),
            question=result.get('question', ''),
        )
        return result

    def enrich_batch(
        self,
        items: list[dict],
        force: bool = False,
    ) -> list[dict[str, Any]]:
        """Enrich a batch of highlights. Falls back to single on failure."""
        # Filter out cached items
        to_process = []
        results = [None] * len(items)

        for i, item in enumerate(items):
            highlight = item['highlight']
            if not force and self._cache.is_cached(item.get('book_id', ''), highlight):
                results[i] = self._cache.get(item['book_id'], highlight)
            elif len(highlight.strip()) < 5:
                results[i] = {'explanation': '', 'tags': [], 'question': '', 'highlight': highlight}
            else:
                to_process.append(i)

        if not to_process:
            return results

        # Try batch
        batch_items = [items[i] for i in to_process]
        prompt = build_batch_prompt(batch_items)
        response = self._provider.complete(prompt, system=SYSTEM_PROMPT)
        batch_results = parse_batch_response(response, expected_count=len(batch_items))

        if batch_results is not None:
            # Batch succeeded
            for j, idx in enumerate(to_process):
                batch_results[j]['highlight'] = items[idx]['highlight']
                results[idx] = batch_results[j]
        else:
            # Batch failed -- fallback to single calls
            for idx in to_process:
                item = items[idx]
                result = self.enrich_single(
                    book_id=item.get('book_id', ''),
                    highlight=item['highlight'],
                    book_name=item.get('book_name', ''),
                    chapter=item.get('chapter', ''),
                    context_before=item.get('context_before', ''),
                    context_after=item.get('context_after', ''),
                    force=force,
                )
                results[idx] = result

        return results
