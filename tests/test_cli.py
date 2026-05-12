# tests/test_cli.py
import pytest
from unittest.mock import patch, MagicMock
from knowledge import cli


class TestCLI:
    def test_config_command(self, tmp_dir):
        """config subcommand should save config file."""
        config_path = tmp_dir / 'config.json'
        with patch('sys.argv', [
            'knowledge.py', 'config',
            '--provider', 'openai_compatible',
            '--base-url', 'https://api.test.com/v1',
            '--api-key', 'sk-test',
            '--model', 'gpt-4o-mini',
            '--config', str(config_path),
        ]):
            cli.main()
        assert config_path.exists()

    def test_cache_command(self, tmp_dir):
        """cache subcommand should show cache status."""
        cache_path = tmp_dir / 'cache.json'
        with patch('sys.argv', [
            'knowledge.py', 'cache',
            '--cache', str(cache_path),
        ]):
            cli.main()

    def test_enrich_no_config(self, tmp_dir):
        """enrich without config should show error."""
        config_path = tmp_dir / 'config.json'
        with patch('sys.argv', [
            'knowledge.py', 'enrich', '--book', '1',
            '--config', str(config_path),
        ]):
            with pytest.raises(SystemExit):
                cli.main()
