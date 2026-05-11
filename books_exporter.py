#!/usr/bin/env python3
"""
Apple Books 笔记导出工具
用于导出 Apple Books 中的笔记/标注为 Markdown 文件
"""

import sys
import os
import sqlite3
import argparse
import re
from datetime import datetime, timezone, timedelta
from pathlib import Path

# Apple CoreData 时间戳起点（2001-01-01 00:00:00 UTC）
APPLE_EPOCH = datetime(2001, 1, 1, 0, 0, 0, tzinfo=timezone.utc)

# Apple Books 数据路径
IBOOKS_PATH = Path.home() / "Library/Containers/com.apple.iBooksX/Data/Documents"
BK_LIBRARY_DB = IBOOKS_PATH / "BKLibrary/BKLibrary-1-091020131601.sqlite"
AE_ANNOTATION_DB = IBOOKS_PATH / "AEAnnotation/AEAnnotation_v10312011_1727_local.sqlite"

ANNOTATION_TYPE_MAP = {
    0: "书签",
    1: "笔记",
    2: "高亮",
    3: "标注"
}


def apple_timestamp_to_datetime(ts):
    """将 Apple CoreData 时间戳（2001-01-01 起秒数）转换为 datetime"""
    if ts is None:
        return None
    return APPLE_EPOCH + timedelta(seconds=float(ts))


def parse_cfi_chapter(cfi):
    """
    从 EPUB CFI 中提取章节信息
    
    Args:
        cfi: EPUB CFI 字符串，如 "epubcfi(/6/6[Section0001.xhtml]!/4/2,/2[sigil_toc_id_1]/1:0,/1716/2)"
    
    Returns:
        章节标识符字符串，如 "Section0001" 或 "15-面向并发的内存模型"
    """
    if not cfi or not cfi.startswith('epubcfi('):
        return None
    
    # 提取所有方括号内的标识符
    matches = re.findall(r'\[([^\]]+)\]', cfi)
    if not matches:
        return None
    
    # 判断是否是 UUID 或类似的无意义 ID
    def is_meaningless_id(s):
        # UUID 格式（包含长串十六进制，长度 > 20）
        if len(s) > 20 and re.search(r'[0-9a-f]{8,}', s, re.IGNORECASE):
            return True
        # 纯数字 ID 格式如 id123（但不包括 id45 这种短 ID）
        if re.match(r'^id\d{3,}$', s, re.IGNORECASE):
            return True
        return False
    
    # 判断是否是有效的章节标题
    def is_valid_chapter(s):
        # 排除以连字符或下划线结尾的不完整标识符
        if s.endswith('-') or s.endswith('_'):
            return False
        # 包含中文
        if re.search(r'[\u4e00-\u9fff]', s):
            return True
        # 是章节格式（chapter, ch, section 等）
        if re.match(r'(chapter|ch|section)\d+', s, re.IGNORECASE):
            return True
        # 是有意义的英文单词（长度 > 3）
        if len(s) > 3 and re.match(r'^[a-zA-Z][a-zA-Z0-9_-]*$', s):
            return True
        return False
    
    # 优先选择看起来像章节标题的标识符（从后往前查找，因为后面的通常更具体）
    chapter_candidates = []
    for match in reversed(matches):
        # 如果标识符包含中文，很可能是章节标题
        if re.search(r'[\u4e00-\u9fff]', match):
            # 如果是 "15-面向并发的内存模型" 这样的格式，提取标题部分
            if '-' in match or '_' in match:
                parts = re.split(r'[-_]', match, 1)
                if len(parts) > 1 and len(parts[1].strip()) > 2:
                    return parts[1].strip()
            return match
        
        # 处理文件名
        if match.endswith('.xhtml') or match.endswith('.html'):
            chapter_name = re.sub(r'\.(xhtml|html)$', '', match, flags=re.IGNORECASE)
            # 如果是 Section0001 这样的格式，尝试提取数字
            section_match = re.match(r'Section(\d+)', chapter_name, re.IGNORECASE)
            if section_match:
                return f"第{int(section_match.group(1))}章"
            # 如果是 ch1, chapter1 等格式
            ch_match = re.match(r'(ch|chapter)(\d+)', chapter_name, re.IGNORECASE)
            if ch_match:
                return f"第{int(ch_match.group(2))}章"
            # 其他文件名
            if len(chapter_name) > 3 and not is_meaningless_id(chapter_name):
                chapter_candidates.append(chapter_name)
            continue
        
        # 其他标识符，长度 > 3 且不是无意义的 ID
        if len(match) > 3 and not is_meaningless_id(match) and is_valid_chapter(match):
            chapter_candidates.append(match)
    
    # 返回第一个候选（从后往前找到的第一个）
    return chapter_candidates[0] if chapter_candidates else None


def format_chapter_display(chapter, index):
    """
    格式化章节显示，当无法提取有意义的章节标题时，使用序号
    
    Args:
        chapter: 从 CFI 提取的章节标识符
        index: 笔记序号
    
    Returns:
        格式化的章节显示字符串
    """
    if chapter:
        # 如果是 id45 这种格式，转换为更友好的显示
        id_match = re.match(r'^id(\d+)$', chapter, re.IGNORECASE)
        if id_match:
            return f"位置 {id_match.group(1)}"
        return chapter
    else:
        return f"位置 {index}"


def get_books_with_notes():
    """获取所有书籍及其笔记数量"""
    if not BK_LIBRARY_DB.exists() or not AE_ANNOTATION_DB.exists():
        return []

    conn = sqlite3.connect(str(AE_ANNOTATION_DB))
    cursor = conn.cursor()

    # 获取每本书的笔记数量
    cursor.execute("""
        SELECT ZANNOTATIONASSETID, COUNT(*) as note_count
        FROM ZAEANNOTATION
        WHERE ZANNOTATIONDELETED IS NULL OR ZANNOTATIONDELETED = 0
        GROUP BY ZANNOTATIONASSETID
    """)
    annotation_counts = dict(cursor.fetchall())
    conn.close()

    if not annotation_counts:
        return []

    # 获取书籍信息
    conn = sqlite3.connect(str(BK_LIBRARY_DB))
    cursor = conn.cursor()

    books = []
    for asset_id, count in annotation_counts.items():
        cursor.execute("""
            SELECT ZASSETID, ZTITLE, ZAUTHOR, ZPATH
            FROM ZBKLIBRARYASSET
            WHERE ZASSETID = ?
        """, (asset_id,))
        row = cursor.fetchone()
        if row:
            books.append({
                'asset_id': row[0],
                'title': row[1] or '未知书名',
                'author': row[2] or '未知作者',
                'path': row[3] or '',
                'note_count': count
            })
    conn.close()

    # 按笔记数量降序排列
    books.sort(key=lambda x: x['note_count'], reverse=True)
    return books


def get_annotations_for_book(asset_id):
    """获取指定书籍的所有笔记"""
    conn = sqlite3.connect(str(AE_ANNOTATION_DB))
    cursor = conn.cursor()

    cursor.execute("""
        SELECT
            ZANNOTATIONTYPE,
            ZANNOTATIONSELECTEDTEXT,
            ZANNOTATIONNOTE,
            ZANNOTATIONCREATIONDATE,
            ZANNOTATIONLOCATION
        FROM ZAEANNOTATION
        WHERE ZANNOTATIONASSETID = ?
        AND (ZANNOTATIONDELETED IS NULL OR ZANNOTATIONDELETED = 0)
        ORDER BY ZANNOTATIONCREATIONDATE
    """, (asset_id,))

    annotations = []
    for row in cursor.fetchall():
        annotations.append({
            'type': row[0],
            'selected_text': row[1] or '',
            'note': row[2] or '',
            'created_date': row[3],
            'location': row[4] or ''
        })
    conn.close()
    return annotations


def export_book_to_markdown(book, annotations, output_dir):
    """将书籍笔记导出为 Markdown 文件"""
    # 创建安全的文件名
    safe_title = "".join(c if c.isalnum() or c in (' ', '-', '_') else '_' for c in book['title'])
    safe_title = safe_title[:50]  # 限制长度
    filename = f"{safe_title}_{book['asset_id'][:8]}.md"
    filepath = output_dir / filename

    with open(filepath, 'w', encoding='utf-8') as f:
        # 写入文件头
        f.write(f"# {book['title']}\n\n")
        f.write(f"**作者**: {book['author']}\n\n")
        f.write(f"**笔记数量**: {len(annotations)}\n\n")
        f.write("---\n\n")

        # 按类型分组
        highlights = []
        notes = []
        bookmarks = []

        for ann in annotations:
            ann_type = ann['type']
            if ann_type == 0:
                bookmarks.append(ann)
            elif ann_type == 1:
                notes.append(ann)
            elif ann_type == 2:
                highlights.append(ann)
            elif ann_type == 3:
                highlights.append(ann)

        # 写入高亮/标注
        if highlights:
            f.write("## 高亮与标注\n\n")
            for i, ann in enumerate(highlights, 1):
                chapter = parse_cfi_chapter(ann['location']) if ann['location'] else None
                chapter_display = format_chapter_display(chapter, i)
                f.write(f"### {i}. {chapter_display}\n\n")
                if ann['selected_text']:
                    f.write(f"> {ann['selected_text']}\n\n")
                if ann['note']:
                    f.write(f"**笔记**: {ann['note']}\n\n")
                if ann['created_date']:
                    date = apple_timestamp_to_datetime(ann['created_date'])
                    if date:
                        local = date.astimezone()  # 转为本机时区
                        f.write(f"*{local.strftime('%Y-%m-%d %H:%M %Z')}*\n\n")
                f.write("---\n\n")

        # 写入独立笔记
        if notes:
            f.write("## 独立笔记\n\n")
            for i, ann in enumerate(notes, 1):
                chapter = parse_cfi_chapter(ann['location']) if ann['location'] else None
                chapter_display = format_chapter_display(chapter, i)
                f.write(f"### {i}. {chapter_display}\n\n")
                if ann['note']:
                    f.write(f"{ann['note']}\n\n")
                if ann['created_date']:
                    date = apple_timestamp_to_datetime(ann['created_date'])
                    if date:
                        local = date.astimezone()  # 转为本机时区
                        f.write(f"*{local.strftime('%Y-%m-%d %H:%M %Z')}*\n\n")
                f.write("---\n\n")

        # 写入书签
        if bookmarks:
            f.write("## 书签\n\n")
            for i, ann in enumerate(bookmarks, 1):
                chapter = parse_cfi_chapter(ann['location']) if ann['location'] else None
                chapter_display = format_chapter_display(chapter, i)
                f.write(f"- {i}. {chapter_display}")
                if ann['note']:
                    f.write(f" - {ann['note']}")
                f.write("\n")

    return filepath


def list_books():
    """列出所有书籍"""
    books = get_books_with_notes()
    if not books:
        print("未找到任何书籍笔记数据")
        print("请确保 Apple Books 中有书籍并已经做了笔记/标注")
        return

    # 计算列宽
    title_width = 44
    author_width = 18
    num_width = 5

    print(f"\n Apple Books 书籍列表 (共 {len(books)} 本)\n")
    print(f"{'序号':^{num_width}}  {'书名':^{title_width}}  {'作者':^{author_width}}  {'笔记数':^{num_width}}")
    print()

    for i, book in enumerate(books, 1):
        title = book['title'][:title_width] + '…' if len(book['title']) > title_width else book['title']
        author = book['author'][:author_width] + '…' if len(book['author']) > author_width else book['author']
        print(f"{i:^{num_width}}  {title:<{title_width}}  {author:<{author_width}}  {book['note_count']:^{num_width}}")

    print()
    print(f"总计: {len(books)} 本书, {sum(b['note_count'] for b in books)} 条笔记")


def export_book(book_index, output_dir):
    """导出指定书籍的笔记"""
    books = get_books_with_notes()
    if not books:
        print("未找到任何书籍笔记数据")
        return None

    if book_index < 1 or book_index > len(books):
        print(f"无效的书籍序号，请选择 1-{len(books)} 之间的数字")
        return None

    book = books[book_index - 1]
    print(f"\n正在导出: {book['title']}")
    print(f"作者: {book['author']}")
    print(f"笔记数量: {book['note_count']}")

    annotations = get_annotations_for_book(book['asset_id'])
    output_path = Path(output_dir)
    output_path.mkdir(parents=True, exist_ok=True)

    filepath = export_book_to_markdown(book, annotations, output_path)
    print(f"\n导出成功: {filepath}")
    return filepath


def interactive_select_and_export(output_dir):
    """交互式选择书籍并导出"""
    books = get_books_with_notes()
    if not books:
        print("未找到任何书籍笔记数据")
        return

    # 计算列宽
    title_width = 44
    author_width = 18
    num_width = 5

    print(f"\n Apple Books 书籍列表 (共 {len(books)} 本)\n")
    print(f"{'序号':^{num_width}}  {'书名':^{title_width}}  {'作者':^{author_width}}  {'笔记数':^{num_width}}")
    print()

    for i, book in enumerate(books, 1):
        title = book['title'][:title_width] + '…' if len(book['title']) > title_width else book['title']
        author = book['author'][:author_width] + '…' if len(book['author']) > author_width else book['author']
        print(f"{i:^{num_width}}  {title:<{title_width}}  {author:<{author_width}}  {book['note_count']:^{num_width}}")

    print()
    print(f"总计: {len(books)} 本书, {sum(b['note_count'] for b in books)} 条笔记")

    while True:
        print("\n请输入要导出的书籍序号 (输入 q 退出): ", end="")
        choice = input().strip()
        if choice.lower() == 'q':
            print("已取消")
            return
        try:
            idx = int(choice)
            if 1 <= idx <= len(books):
                export_book(idx, output_dir)
                return
            print(f"请输入 1-{len(books)} 之间的数字")
        except ValueError:
            print("请输入有效的数字")


def main():
    parser = argparse.ArgumentParser(
        description="Apple Books 笔记导出工具",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
用法示例:
  %(prog)s list                    # 列出所有书籍
  %(prog)s export                   # 交互式选择书籍并导出
  %(prog)s export 3                # 导出第3本书的笔记
  %(prog)s export 3 -o ~/Desktop   # 导出到指定目录
        """
    )

    parser.add_argument('command', choices=['list', 'export'],
                        help='list: 列出书籍 | export: 导出笔记')
    parser.add_argument('book_index', nargs='?', type=int,
                        help='书籍序号 (从 list 命令获取)')
    parser.add_argument('-o', '--output', default='.',
                        help='导出目录 (默认: 当前目录)')

    args = parser.parse_args()

    if args.command == 'list':
        list_books()
    elif args.command == 'export':
        if args.book_index:
            export_book(args.book_index, args.output)
        else:
            interactive_select_and_export(args.output)


if __name__ == '__main__':
    main()