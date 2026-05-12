# knowledge/cli.py
"""CLI entry point for the knowledge module."""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from services.book_service import BookService
from knowledge.config import KnowledgeConfig, load_config, save_config
from knowledge.cache import LLMCache
from knowledge.context import get_manifest_map, get_chapter_text, extract_context
from knowledge.provider.openai_compat import OpenAICompatible
from knowledge.enricher import Enricher
from knowledge.exporter import export_book, sanitize_filename
from services.cfi_utils import extract_item_id, extract_chapter_title, format_chapter_display


def cmd_config(args):
    """Configure LLM provider settings."""
    config = load_config(Path(args.config))
    config.llm.provider = args.provider or config.llm.provider
    config.llm.base_url = args.base_url or config.llm.base_url
    config.llm.api_key = args.api_key or config.llm.api_key
    config.llm.model = args.model or config.llm.model
    save_config(config, Path(args.config))
    print(f"配置已保存到 {args.config}")


def cmd_map_book(args):
    """Map a book ID to its EPUB file."""
    config = load_config(Path(args.config))
    from knowledge.config import EpubMapping
    config.epub_mappings[args.book_id] = EpubMapping(
        epub=args.epub,
        output=args.output or '',
    )
    save_config(config, Path(args.config))
    print(f"已映射 {args.book_id[:8]}... -> {args.epub}")


def cmd_enrich(args):
    """Enrich book highlights with LLM."""
    config = load_config(Path(args.config))

    # Validate LLM config
    if not config.llm.base_url or not config.llm.model:
        print("错误: 请先运行 config 子命令配置 LLM")
        sys.exit(1)

    # Resolve book
    service = BookService()
    books = service.load_books()

    if args.book_id:
        book = next((b for b in books if b['asset_id'] == args.book_id), None)
    elif args.book and 1 <= args.book <= len(books):
        book = books[args.book - 1]
    else:
        print("错误: 找不到指定书籍")
        sys.exit(1)

    if not book:
        print("错误: 找不到指定书籍")
        sys.exit(1)

    asset_id = book['asset_id']
    print(f"处理: {book['title']} ({book['note_count']} 条笔记)")

    # Setup
    mapping = config.epub_mappings.get(asset_id)
    epub_path = Path(mapping.epub) if mapping and mapping.epub else None
    output_dir = Path(mapping.output if mapping and mapping.output else args.output or './output')
    cache_path = output_dir / 'llm' / 'cache.json'

    cache = LLMCache(cache_path)
    annotations = service.get_annotations(asset_id)

    # Filter annotations with text
    annotations = [a for a in annotations if a.get('selected_text')]
    print(f"有效高亮: {len(annotations)} 条")

    # Determine which to process
    if args.index is not None:
        if 1 <= args.index <= len(annotations):
            annotations = [annotations[args.index - 1]]
        else:
            print(f"错误: 索引 {args.index} 超出范围 (1-{len(annotations)})")
            sys.exit(1)
    elif not args.all:
        # Incremental mode: skip cached
        annotations = [
            a for a in annotations
            if not cache.is_cached(asset_id, a['selected_text'])
        ]
        print(f"增量模式: 需处理 {len(annotations)} 条新高亮")

    if not annotations:
        print("没有需要处理的高亮")
        return

    # Load EPUB manifest if available
    epub_text_cache = {}
    if epub_path and epub_path.exists():
        manifest = get_manifest_map(epub_path)
        print(f"EPUB 已加载: {len(manifest)} 个文档项")
    else:
        manifest = {}

    # Build enricher
    provider = OpenAICompatible(
        base_url=config.llm.base_url,
        api_key=config.llm.get_api_key(),
        model=config.llm.model,
        max_retries=config.llm.max_retries,
        retry_delays=config.llm.retry_delays,
    )
    enricher = Enricher(provider=provider, cache=cache)

    # Prepare items with context
    def get_context_for_annotation(ann):
        ctx_before, ctx_after = '', ''
        if manifest and epub_path:
            cfi = ann.get('location', '')
            item_id = extract_item_id(cfi)
            if item_id and item_id in manifest:
                href = manifest[item_id]
                if href not in epub_text_cache:
                    try:
                        epub_text_cache[href] = get_chapter_text(epub_path, href)
                    except Exception:
                        epub_text_cache[href] = ''
                chapter_text = epub_text_cache[href]
                if chapter_text:
                    ctx = extract_context(chapter_text, ann['selected_text'],
                                         config.context_chars)
                    if ctx:
                        ctx_before, _, ctx_after = ctx
        return ctx_before, ctx_after

    # Process
    llm_results = []
    for i, ann in enumerate(annotations):
        selected = ann['selected_text']
        cfi = ann.get('location', '')
        chapter = extract_chapter_title(cfi) if cfi else None
        chapter_display = format_chapter_display(chapter, i + 1)

        ctx_before, ctx_after = get_context_for_annotation(ann)

        result = enricher.enrich_single(
            book_id=asset_id,
            highlight=selected,
            book_name=book['title'],
            chapter=chapter_display,
            context_before=ctx_before,
            context_after=ctx_after,
            force=args.force,
        )
        llm_results.append(result)
        print(f"  [{i+1}/{len(annotations)}] {selected[:30]}...")

    # Export
    export_format = args.format or config.output_format
    export_book(book, annotations, llm_results, output_dir, format=export_format)
    print(f"导出完成: {output_dir}")


def cmd_card(args):
    """Generate image cards for highlights."""
    try:
        from knowledge.card import generate_cards
    except ImportError:
        print("错误: 需要安装 Pillow。运行: pip install Pillow")
        sys.exit(1)

    config = load_config(Path(args.config))
    service = BookService()
    books = service.load_books()

    if args.book_id:
        book = next((b for b in books if b['asset_id'] == args.book_id), None)
    elif args.book and 1 <= args.book <= len(books):
        book = books[args.book - 1]
    else:
        print("错误: 找不到指定书籍")
        sys.exit(1)

    if not book:
        print("错误: 找不到指定书籍")
        sys.exit(1)

    asset_id = book['asset_id']
    annotations = service.get_annotations(asset_id)
    annotations = [a for a in annotations if a.get('selected_text')]

    if args.index is not None:
        if 1 <= args.index <= len(annotations):
            annotations = [annotations[args.index - 1]]
        else:
            print(f"错误: 索引 {args.index} 超出范围")
            sys.exit(1)

    output_dir = Path(args.output or config.card_output).expanduser()
    style_name = args.style or config.card_style

    # Load LLM cache if --with-llm
    llm_cache = None
    if args.with_llm:
        cache_path = Path(args.cache or 'llm/cache.json')
        if cache_path.exists():
            llm_cache = LLMCache(cache_path)

    generate_cards(book, annotations, output_dir, style_name, llm_cache)
    print(f"卡片已导出: {output_dir}")


def cmd_cache(args):
    """Show cache status."""
    cache_path = Path(args.cache or 'llm/cache.json')
    if not cache_path.exists():
        print("缓存文件不存在")
        return

    cache = LLMCache(cache_path)
    print(f"缓存条目: {cache.count()}")

    if args.book_id:
        entries = cache.get_all_for_book(args.book_id)
        print(f"该书籍条目: {len(entries)}")


def _add_config_arg(parser):
    """Add --config argument to a subparser."""
    parser.add_argument('--config', default='knowledge_config.json', help='配置文件路径')


def main():
    parser = argparse.ArgumentParser(description='Apple Books 知识管理工具')
    subparsers = parser.add_subparsers(dest='command', help='子命令')

    # config
    p_config = subparsers.add_parser('config', help='配置 LLM')
    _add_config_arg(p_config)
    p_config.add_argument('--provider', default='openai_compatible')
    p_config.add_argument('--base-url')
    p_config.add_argument('--api-key')
    p_config.add_argument('--model')

    # map-book
    p_map = subparsers.add_parser('map-book', help='映射书籍到 EPUB')
    _add_config_arg(p_map)
    p_map.add_argument('--book-id', required=True)
    p_map.add_argument('--epub', required=True)
    p_map.add_argument('--output')

    # enrich
    p_enrich = subparsers.add_parser('enrich', help='处理高亮笔记')
    _add_config_arg(p_enrich)
    p_enrich.add_argument('--book', type=int, help='书籍序号')
    p_enrich.add_argument('--book-id', help='书籍 asset_id')
    p_enrich.add_argument('--index', type=int, help='单条索引')
    p_enrich.add_argument('--all', action='store_true', help='全量处理')
    p_enrich.add_argument('--force', action='store_true', help='强制重新生成')
    p_enrich.add_argument('--retry-errors', action='store_true', help='重试失败条目')
    p_enrich.add_argument('--output', default='./output')
    p_enrich.add_argument('--format', choices=['obsidian', 'markdown'])

    # card
    p_card = subparsers.add_parser('card', help='导出图片卡片')
    _add_config_arg(p_card)
    p_card.add_argument('--book', type=int)
    p_card.add_argument('--book-id')
    p_card.add_argument('--index', type=int)
    p_card.add_argument('--all', action='store_true')
    p_card.add_argument('--style', choices=['dark', 'light', 'minimal'])
    p_card.add_argument('--output')
    p_card.add_argument('--with-llm', action='store_true')
    p_card.add_argument('--cache')

    # cache
    p_cache = subparsers.add_parser('cache', help='查看缓存状态')
    _add_config_arg(p_cache)
    p_cache.add_argument('--book', type=int)
    p_cache.add_argument('--book-id')
    p_cache.add_argument('--cache')

    args = parser.parse_args()

    if args.command == 'config':
        cmd_config(args)
    elif args.command == 'map-book':
        cmd_map_book(args)
    elif args.command == 'enrich':
        cmd_enrich(args)
    elif args.command == 'card':
        cmd_card(args)
    elif args.command == 'cache':
        cmd_cache(args)
    else:
        parser.print_help()


if __name__ == '__main__':
    main()
