# tests/test_cfi_utils.py
from services.cfi_utils import extract_item_id, extract_chapter_title, format_chapter_display


class TestExtractItemId:
    def test_standard_cfi(self):
        cfi = 'epubcfi(/6/10[item4]!/4/82/1,:0,:44)'
        assert extract_item_id(cfi) == 'item4'

    def test_cfi_with_xhtml(self):
        cfi = 'epubcfi(/6/6[Section0001.xhtml]!/4/2,/2[sigil_toc_id_1]/1:0,/1716/2)'
        assert extract_item_id(cfi) == 'Section0001.xhtml'

    def test_empty_cfi(self):
        assert extract_item_id('') is None

    def test_none_cfi(self):
        assert extract_item_id(None) is None

    def test_no_brackets(self):
        assert extract_item_id('epubcfi(/6/10!/4/82)') is None

    def test_non_epubcfi(self):
        assert extract_item_id('not-a-cfi') is None


class TestExtractChapterTitle:
    def test_chinese_title(self):
        cfi = 'epubcfi(/6/6[15-面向并发的内存模型.xhtml]!/4/2)'
        result = extract_chapter_title(cfi)
        assert result == '面向并发的内存模型'

    def test_section_number(self):
        cfi = 'epubcfi(/6/6[Section0003.xhtml]!/4/2)'
        result = extract_chapter_title(cfi)
        assert result == '第3章'

    def test_chapter_format(self):
        cfi = 'epubcfi(/6/6[chapter5.xhtml]!/4/2)'
        result = extract_chapter_title(cfi)
        assert result == '第5章'

    def test_chinese_no_suffix(self):
        cfi = 'epubcfi(/6/6[巨婴的全能自恋]!/4/2)'
        result = extract_chapter_title(cfi)
        assert result == '巨婴的全能自恋'

    def test_uuid_filtered(self):
        cfi = 'epubcfi(/6/6[a1b2c3d4e5f6a1b2c3d4e5f6]!/4/2)'
        result = extract_chapter_title(cfi)
        assert result is None

    def test_empty(self):
        assert extract_chapter_title('') is None

    def test_none(self):
        assert extract_chapter_title(None) is None


class TestFormatChapterDisplay:
    def test_with_chapter(self):
        assert format_chapter_display('第3章', 1) == '第3章'

    def test_id_format(self):
        assert format_chapter_display('id45', 1) == '位置 45'

    def test_no_chapter(self):
        assert format_chapter_display(None, 7) == '位置 7'

    def test_empty_string(self):
        assert format_chapter_display('', 3) == '位置 3'
