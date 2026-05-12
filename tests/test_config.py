# tests/test_config.py
import json
from pathlib import Path
from knowledge.config import KnowledgeConfig, load_config, save_config


class TestKnowledgeConfig:
    def test_default_values(self):
        config = KnowledgeConfig()
        assert config.output_format == 'obsidian'
        assert config.card_style == 'dark'
        assert config.context_chars == 200
        assert config.filename_max_length == 20
        assert config.llm.batch_size == 10
        assert config.llm.max_retries == 3

    def test_from_dict(self, sample_config):
        config = KnowledgeConfig.from_dict(sample_config)
        assert config.llm.provider == 'openai_compatible'
        assert config.llm.base_url == 'https://api.example.com/v1'
        assert config.llm.model == 'gpt-4o-mini'
        assert '44D43B7A372DA51FB1B5AD664DBE4D53' in config.epub_mappings

    def test_to_dict(self, sample_config):
        config = KnowledgeConfig.from_dict(sample_config)
        data = config.to_dict()
        assert data['llm']['provider'] == 'openai_compatible'
        assert data['output_format'] == 'obsidian'

    def test_roundtrip(self, sample_config):
        config = KnowledgeConfig.from_dict(sample_config)
        data = config.to_dict()
        config2 = KnowledgeConfig.from_dict(data)
        assert config2.llm.model == config.llm.model
        assert config2.epub_mappings == config.epub_mappings


class TestLoadSaveConfig:
    def test_save_and_load(self, tmp_dir, sample_config):
        config_path = tmp_dir / 'knowledge_config.json'
        config = KnowledgeConfig.from_dict(sample_config)
        save_config(config, config_path)
        assert config_path.exists()

        loaded = load_config(config_path)
        assert loaded.llm.api_key == 'sk-test-key'
        assert loaded.output_format == 'obsidian'

    def test_load_nonexistent_returns_default(self, tmp_dir):
        config_path = tmp_dir / 'nonexistent.json'
        config = load_config(config_path)
        assert config.llm.batch_size == 10

    def test_env_api_key(self, tmp_dir, sample_config):
        """Config with env: prefix stores the reference, not the value."""
        sample_config['llm']['api_key'] = 'env:MY_API_KEY'
        config_path = tmp_dir / 'config.json'
        config = KnowledgeConfig.from_dict(sample_config)
        save_config(config, config_path)
        loaded = load_config(config_path)
        assert loaded.llm.api_key == 'env:MY_API_KEY'
