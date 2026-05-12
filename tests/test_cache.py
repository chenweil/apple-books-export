# tests/test_cache.py
from knowledge.cache import LLMCache


class TestLLMCache:
    def test_empty_cache(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        assert cache.get('book1', 'some text') is None
        assert cache.count() == 0

    def test_put_and_get(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        cache.put(
            book_id='book1',
            highlight='some highlight text',
            file='test.md',
            book_name='Test Book',
            explanation='test explanation',
            tags=['tag1', 'tag2'],
            question='test question?',
        )
        result = cache.get('book1', 'some highlight text')
        assert result is not None
        assert result['explanation'] == 'test explanation'
        assert result['tags'] == ['tag1', 'tag2']
        assert result['question'] == 'test question?'

    def test_get_miss(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        cache.put(
            book_id='book1',
            highlight='text A',
            file='a.md',
            book_name='Book',
            explanation='exp',
            tags=[],
            question='q?',
        )
        assert cache.get('book1', 'text B') is None

    def test_normalization(self, tmp_dir):
        """Whitespace differences should still hit cache."""
        cache = LLMCache(tmp_dir / 'cache.json')
        cache.put(
            book_id='book1',
            highlight='  hello   world  ',
            file='test.md',
            book_name='Book',
            explanation='exp',
            tags=[],
            question='q?',
        )
        # Same text with different whitespace should hit
        result = cache.get('book1', 'hello world')
        assert result is not None

    def test_cross_book_no_collision(self, tmp_dir):
        """Same highlight text in different books should be separate entries."""
        cache = LLMCache(tmp_dir / 'cache.json')
        cache.put(book_id='book1', highlight='same text', file='a.md',
                  book_name='Book A', explanation='exp A', tags=[], question='q?')
        cache.put(book_id='book2', highlight='same text', file='b.md',
                  book_name='Book B', explanation='exp B', tags=[], question='q?')

        assert cache.get('book1', 'same text')['explanation'] == 'exp A'
        assert cache.get('book2', 'same text')['explanation'] == 'exp B'

    def test_persistence(self, tmp_dir):
        path = tmp_dir / 'cache.json'
        cache = LLMCache(path)
        cache.put(book_id='book1', highlight='text', file='test.md',
                  book_name='Book', explanation='exp', tags=[], question='q?')

        # Load fresh instance
        cache2 = LLMCache(path)
        assert cache2.get('book1', 'text') is not None

    def test_count(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        assert cache.count() == 0
        cache.put(book_id='book1', highlight='a', file='a.md',
                  book_name='Book', explanation='e', tags=[], question='q?')
        cache.put(book_id='book1', highlight='b', file='b.md',
                  book_name='Book', explanation='e', tags=[], question='q?')
        assert cache.count() == 2

    def test_remove(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        cache.put(book_id='book1', highlight='text', file='test.md',
                  book_name='Book', explanation='exp', tags=[], question='q?')
        cache.remove('book1', 'text')
        assert cache.get('book1', 'text') is None

    def test_is_cached(self, tmp_dir):
        cache = LLMCache(tmp_dir / 'cache.json')
        assert not cache.is_cached('book1', 'text')
        cache.put(book_id='book1', highlight='text', file='test.md',
                  book_name='Book', explanation='exp', tags=[], question='q?')
        assert cache.is_cached('book1', 'text')
