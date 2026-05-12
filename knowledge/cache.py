# knowledge/cache.py
"""LLM result caching with JSON persistence."""
from __future__ import annotations

import hashlib
import json
import re
from datetime import date
from pathlib import Path


def _normalize(text: str) -> str:
    """Normalize text for cache key: strip + collapse whitespace."""
    return re.sub(r'\s+', ' ', text).strip()


def _make_key(book_id: str, highlight: str) -> str:
    """Generate cache key: {first8_of_book_id}_{md5(normalized_highlight)}."""
    norm = _normalize(highlight)
    md5 = hashlib.md5(norm.encode('utf-8')).hexdigest()
    return f"{book_id[:8]}_{md5}"


class LLMCache:
    """JSON-backed cache for LLM enrichment results."""

    def __init__(self, path: Path):
        self._path = path
        self._data: dict = {}
        self._load()

    def _load(self) -> None:
        if self._path.exists():
            with open(self._path, 'r', encoding='utf-8') as f:
                self._data = json.load(f)

    def _save(self) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        with open(self._path, 'w', encoding='utf-8') as f:
            json.dump(self._data, f, indent=2, ensure_ascii=False)

    def get(self, book_id: str, highlight: str) -> dict | None:
        """Get cached result for a highlight. Returns None if not found."""
        key = _make_key(book_id, highlight)
        return self._data.get(key)

    def is_cached(self, book_id: str, highlight: str) -> bool:
        """Check if a highlight has been cached."""
        return _make_key(book_id, highlight) in self._data

    def put(
        self,
        book_id: str,
        highlight: str,
        file: str,
        book_name: str,
        explanation: str,
        tags: list[str],
        question: str,
    ) -> None:
        """Store an LLM enrichment result in the cache."""
        key = _make_key(book_id, highlight)
        self._data[key] = {
            'highlight': _normalize(highlight),
            'file': file,
            'book_id': book_id,
            'book': book_name,
            'explanation': explanation,
            'tags': tags,
            'question': question,
            'updated': date.today().isoformat(),
        }
        self._save()

    def remove(self, book_id: str, highlight: str) -> None:
        """Remove a cached entry."""
        key = _make_key(book_id, highlight)
        if key in self._data:
            del self._data[key]
            self._save()

    def count(self) -> int:
        """Return total number of cached entries."""
        return len(self._data)

    def get_all_for_book(self, book_id: str) -> dict[str, dict]:
        """Get all cached entries for a specific book."""
        prefix = book_id[:8]
        return {
            k: v for k, v in self._data.items()
            if k.startswith(prefix + '_')
        }
