#!/usr/bin/env python3
"""
测试：根据 EPUB 和笔记 CFI 提取高亮文字的上下文
"""
import zipfile
import re
import sqlite3
from pathlib import Path

# ========== 配置 ==========
EPUB_PATH = Path("/Users/chenweilong/books-exporter/books/巨婴国 (武志红) (Z-Library).epub")
IBOOKS_PATH = Path.home() / "Library/Containers/com.apple.iBooksX/Data/Documents"
BK_LIBRARY_DB = IBOOKS_PATH / "BKLibrary/BKLibrary-1-091020131601.sqlite"
AE_ANNOTATION_DB = IBOOKS_PATH / "AEAnnotation/AEAnnotation_v10312011_1727_local.sqlite"

# ========== EPUB 解析 ==========

def get_manifest_map(epub_path):
    """从 content.opf 中提取 manifest id -> href 映射"""
    with zipfile.ZipFile(epub_path, 'r') as z:
        content_opf = z.read('OEBPS/content.opf').decode('utf-8')
    
    items = re.findall(r'<item\s+id="([^"]+)"[^>]*href="([^"]+)"', content_opf)
    return {item_id: href for item_id, href in items}

def extract_text_from_xhtml(html_content):
    """从 XHTML 中提取纯文本"""
    # 移除所有 HTML 标签
    text = re.sub(r'<[^>]+>', '', html_content)
    # 处理多余的空白
    text = re.sub(r'\s+', ' ', text).strip()
    return text

def get_chapter_text(epub_path, chapter_href):
    """从 EPUB 中提取指定章节的纯文本"""
    full_path = f"OEBPS/{chapter_href}"
    with zipfile.ZipFile(epub_path, 'r') as z:
        content = z.read(full_path).decode('utf-8')
        return extract_text_from_xhtml(content)

def parse_cfi(cfi):
    """
    解析 EPUB CFI，提取 manifest item ID
    例如: epubcfi(/6/10[item4]!/4/82/1,:0,:44) -> item4
    """
    if not cfi or not cfi.startswith('epubcfi('):
        return None
    
    # 提取 [itemX] 格式的 manifest ID
    match = re.search(r'\[([^\]]+)\]', cfi)
    if match:
        return match.group(1)
    return None

# ========== 上下文提取 ==========

def extract_context(text, highlight_text, context_chars=100):
    """
    在文本中查找高亮文字，提取上下文
    
    Args:
        text: 章节纯文本
        highlight_text: 高亮文字
        context_chars: 前后各提取的字符数
    
    Returns:
        (before, highlight, after) 元组
    """
    pos = text.find(highlight_text)
    if pos < 0:
        return None
    
    start = max(0, pos - context_chars)
    end = min(len(text), pos + len(highlight_text) + context_chars)
    
    before = text[start:pos]
    after = text[pos + len(highlight_text):end]
    
    return (before, highlight_text, after)

def format_context_card(before, highlight, after):
    """格式化输出卡片样式"""
    lines = []
    lines.append("━" * 50)
    
    if before:
        # 找前一个句子的开头
        sentence_start = before.rfind('。')
        if sentence_start < 0:
            sentence_start = before.rfind('，')
        if sentence_start >= 0 and len(before) - sentence_start < 50:
            before = before[sentence_start+1:]
        lines.append(f"  {before}")
    
    lines.append(f"  ▶ {highlight}")
    
    if after:
        # 找下一个句子的结尾
        sentence_end = after.find('。')
        if sentence_end < 0:
            sentence_end = after.find('，')
        if 0 < sentence_end < 50:
            after = after[:sentence_end+1]
        lines.append(f"  {after}")
    
    lines.append("━" * 50)
    return '\n'.join(lines)

# ========== 主程序 ==========

def main():
    # 1. 获取 manifest 映射
    manifest = get_manifest_map(EPUB_PATH)
    print(f"加载 EPUB: {EPUB_PATH.name}")
    print(f"共 {len(manifest)} 个文档项\n")
    
    # 2. 连接数据库，获取笔记
    conn = sqlite3.connect(str(AE_ANNOTATION_DB))
    cursor = conn.cursor()
    cursor.execute("""
        SELECT 
            ZANNOTATIONTYPE,
            ZANNOTATIONSELECTEDTEXT,
            ZANNOTATIONNOTE,
            ZANNOTATIONLOCATION
        FROM ZAEANNOTATION
        WHERE ZANNOTATIONASSETID = ?
        AND (ZANNOTATIONDELETED IS NULL OR ZANNOTATIONDELETED = 0)
        AND ZANNOTATIONTYPE IN (2, 3)  -- 只取高亮和标注
    """, ('44D43B7A372DA51FB1B5AD664DBE4D53',))
    
    annotations = cursor.fetchall()
    conn.close()
    
    print(f"找到 {len(annotations)} 条高亮/标注\n")
    
    # 3. 处理每条笔记
    for i, (ann_type, selected_text, note, location) in enumerate(annotations, 1):
        if not selected_text:
            continue
        
        print(f"\n[{i}] 类型: {'高亮' if ann_type == 2 else '标注'}")
        print(f"    CFI: {location}")
        
        # 解析 CFI 获取章节
        item_id = parse_cfi(location)
        if not item_id:
            print("    (无法解析 CFI)")
            continue
        
        chapter_href = manifest.get(item_id)
        if not chapter_href:
            print(f"    (找不到 item: {item_id})")
            continue
        
        # 提取章节文本
        try:
            chapter_text = get_chapter_text(EPUB_PATH, chapter_href)
        except Exception as e:
            print(f"    (读取章节失败: {e})")
            continue
        
        # 提取上下文
        ctx = extract_context(chapter_text, selected_text)
        if ctx:
            before, highlight, after = ctx
            card = format_context_card(before, highlight, after)
            print(card)
        else:
            print("    (未找到高亮文字)")
        
        if i >= 5:  # 只显示前5条
            print(f"\n... 还有 {len(annotations) - 5} 条")
            break

if __name__ == '__main__':
    main()