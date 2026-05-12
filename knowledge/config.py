# knowledge/config.py
"""Configuration management for the knowledge module."""
from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class LLMConfig:
    provider: str = 'openai_compatible'
    base_url: str = ''
    api_key: str = ''
    model: str = ''
    batch_size: int = 10
    max_retries: int = 3
    retry_delays: list = field(default_factory=lambda: [1, 2, 4])
    batch_fallback_to_single: bool = True

    def get_api_key(self) -> str:
        """Resolve API key, supporting env: prefix."""
        if self.api_key.startswith('env:'):
            env_var = self.api_key[4:]
            return os.environ.get(env_var, '')
        return self.api_key


@dataclass
class EpubMapping:
    epub: str = ''
    output: str = ''


@dataclass
class KnowledgeConfig:
    llm: LLMConfig = field(default_factory=LLMConfig)
    epub_mappings: dict[str, EpubMapping] = field(default_factory=dict)
    output_format: str = 'obsidian'
    card_style: str = 'dark'
    card_output: str = '~/cards/'
    context_chars: int = 200
    filename_max_length: int = 20

    @classmethod
    def from_dict(cls, data: dict) -> 'KnowledgeConfig':
        llm_data = data.get('llm', {})
        llm = LLMConfig(**{k: v for k, v in llm_data.items() if k in LLMConfig.__dataclass_fields__})

        mappings = {}
        for book_id, mapping_data in data.get('epub_mappings', {}).items():
            mappings[book_id] = EpubMapping(**{
                k: v for k, v in mapping_data.items() if k in EpubMapping.__dataclass_fields__
            })

        return cls(
            llm=llm,
            epub_mappings=mappings,
            output_format=data.get('output_format', 'obsidian'),
            card_style=data.get('card_style', 'dark'),
            card_output=data.get('card_output', '~/cards/'),
            context_chars=data.get('context_chars', 200),
            filename_max_length=data.get('filename_max_length', 20),
        )

    def to_dict(self) -> dict:
        return {
            'llm': {
                'provider': self.llm.provider,
                'base_url': self.llm.base_url,
                'api_key': self.llm.api_key,
                'model': self.llm.model,
                'batch_size': self.llm.batch_size,
                'max_retries': self.llm.max_retries,
                'retry_delays': self.llm.retry_delays,
                'batch_fallback_to_single': self.llm.batch_fallback_to_single,
            },
            'epub_mappings': {
                book_id: {'epub': m.epub, 'output': m.output}
                for book_id, m in self.epub_mappings.items()
            },
            'output_format': self.output_format,
            'card_style': self.card_style,
            'card_output': self.card_output,
            'context_chars': self.context_chars,
            'filename_max_length': self.filename_max_length,
        }


DEFAULT_CONFIG_PATH = Path('knowledge_config.json')


def load_config(path: Path | None = None) -> KnowledgeConfig:
    """Load config from JSON file, or return defaults if not found."""
    path = path or DEFAULT_CONFIG_PATH
    if not path.exists():
        return KnowledgeConfig()
    try:
        with open(path, 'r', encoding='utf-8') as f:
            data = json.load(f)
    except (json.JSONDecodeError, OSError):
        return KnowledgeConfig()
    return KnowledgeConfig.from_dict(data)


def save_config(config: KnowledgeConfig, path: Path | None = None) -> None:
    """Save config to JSON file."""
    path = path or DEFAULT_CONFIG_PATH
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, 'w', encoding='utf-8') as f:
        json.dump(config.to_dict(), f, indent=2, ensure_ascii=False)
