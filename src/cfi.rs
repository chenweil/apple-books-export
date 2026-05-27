//! Apple Books Exporter - EPUB CFI Parser

use regex::Regex;

/// 从 EPUB CFI 字符串中提取 manifest item ID
///
/// 示例: epubcfi(/6/10[item4]!/4/82/1,:0,:44) -> "item4"
pub fn extract_item_id(cfi: &str) -> Option<String> {
    if cfi.is_empty() || !cfi.starts_with("epubcfi(") {
        return None;
    }

    let re = Regex::new(r"\[([^\]]+)\]").ok()?;
    let captures = re.captures(cfi)?;
    let item_id = captures.get(1)?.as_str();

    Some(item_id.to_string())
}

/// 从 EPUB CFI 字符串中提取章节标题
///
/// 处理中文标题、章节编号 (Section0003 -> 第 3 章)、
/// 章节格式 (chapter5 -> 第 5 章)，并过滤掉 UUID。
pub fn extract_chapter_title(cfi: &str) -> Option<String> {
    if cfi.is_empty() || !cfi.starts_with("epubcfi(") {
        return None;
    }

    let re = Regex::new(r"\[([^\]]+)\]").ok()?;
    let matches: Vec<&str> = re
        .captures_iter(cfi)
        .filter_map(|c| c.get(1))
        .map(|m| m.as_str())
        .collect();

    if matches.is_empty() {
        return None;
    }

    /// 判断是否为无意义的 ID（UUID 或 id 后跟大量数字）
    fn is_meaningless_id(s: &str) -> bool {
        if s.len() > 20 {
            let re_uuid = Regex::new(r"[0-9a-f]{8,}").ok()?;
            if re_uuid.is_match(s) {
                return true;
            }
        }
        let re_id = Regex::new(r"^id\d{3,}$").ok()?;
        re_id.is_match(s)
    }

    /// 判断是否为有效的章节标识
    fn is_valid_chapter(s: &str) -> bool {
        if s.ends_with('-') || s.ends_with('_') {
            return false;
        }

        // 包含中文字符
        let re_chinese = Regex::new(r"[一-鿿]").ok()?;
        if re_chinese.is_match(s) {
            return true;
        }

        // chapter/ch/section 后跟数字
        let re_chapter = Regex::new(r"(chapter|ch|section)\d+").ok()?;
        if re_chapter.is_match_ignore_case(s) {
            return true;
        }

        // 字母开头的文件名
        if s.len() > 3 {
            let re_filename = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$").ok()?;
            if re_filename.is_match(s) {
                return true;
            }
        }

        false
    }

    let mut chapter_candidates: Vec<String> = Vec::new();

    // 从后往前遍历匹配（最后一个有意义的通常是章节）
    for &match_str in matches.iter().rev() {
        // 去除文件扩展名
        let mut base = match_str;
        let re_ext = Regex::new(r"\.(xhtml|html)$").ok()?;
        if re_ext.is_match_ignore_case(base) {
            base = re_ext.replace(base, "").as_ref();
        }

        // 中文标题 - 提取分隔符后的部分
        let re_chinese = Regex::new(r"[一-鿿]").ok()?;
        if re_chinese.is_match(base) {
            if base.contains('-') || base.contains('_') {
                let re_split = Regex::new(r"[-_]").ok()?;
                if let Some(caps) = re_split.captures(base) {
                    if let Some(part) = caps.get(1) {
                        let part_str = part.as_str().trim();
                        if part_str.len() > 2 {
                            return Some(part_str.to_string());
                        }
                    }
                }
            }
            return Some(base.to_string());
        }

        // 文件名带扩展名 - 尝试 section/chapter 模式
        if match_str.ends_with(".xhtml") || match_str.ends_with(".html") {
            let re_section = Regex::new(r"Section(\d+)").ok()?;
            if let Some(caps) = re_section.captures_ignore_case(base) {
                if let Some(num) = caps.get(1) {
                    if let Ok(n) = num.as_str().parse::<u32>() {
                        return Some(format!("第{}章", n));
                    }
                }
            }

            let re_ch = Regex::new(r"(ch|chapter)(\d+)").ok()?;
            if let Some(caps) = re_ch.captures_ignore_case(base) {
                if let Some(num) = caps.get(2) {
                    if let Ok(n) = num.as_str().parse::<u32>() {
                        return Some(format!("第{}章", n));
                    }
                }
            }

            if base.len() > 3 && !is_meaningless_id(base) {
                chapter_candidates.push(base.to_string());
            }
            continue;
        }

        if base.len() > 3 && !is_meaningless_id(base) && is_valid_chapter(base) {
            chapter_candidates.push(base.to_string());
        }
    }

    chapter_candidates.into_iter().next()
}

/// 格式化章节显示
/// 如果没有章节标题，使用位置索引作为回退
pub fn format_chapter_display(chapter: Option<&str>, index: usize) -> String {
    if let Some(chapter) = chapter {
        let re_id = Regex::new(r"^id(\d+)$").ok()?;
        if let Some(caps) = re_id.captures(chapter) {
            if let Some(num) = caps.get(1) {
                return format!("位置 {}", num.as_str());
            }
        }
        return chapter.to_string();
    }
    format!("位置 {}", index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_item_id() {
        let cfi = "epubcfi(/6/10[item4]!/4/82/1,:0,:44)";
        assert_eq!(extract_item_id(cfi), Some("item4".to_string()));

        let cfi = "epubcfi(/6/20[item20]!/4/2/1,:0,:10)";
        assert_eq!(extract_item_id(cfi), Some("item20".to_string()));

        // 无效 CFI
        assert_eq!(extract_item_id(""), None);
        assert_eq!(extract_item_id("invalid"), None);
    }

    #[test]
    fn test_extract_chapter_title_section() {
        let cfi = "epubcfi(/6/10[Section0003.xhtml]!/4/82/1,:0,:44)";
        assert_eq!(extract_chapter_title(cfi), Some("第 3 章".to_string()));
    }

    #[test]
    fn test_extract_chapter_title_chapter() {
        let cfi = "epubcfi(/6/10[chapter5.xhtml]!/4/82/1,:0,:44)";
        assert_eq!(extract_chapter_title(cfi), Some("第 5 章".to_string()));
    }

    #[test]
    fn test_extract_chapter_title_chinese() {
        let cfi = "epubcfi(/6/10[15-面向并发的内存模型.xhtml]!/4/82/1,:0,:44)";
        let result = extract_chapter_title(cfi);
        assert!(result.is_some());
        assert!(result.unwrap().contains("面向并发的内存模型"));
    }

    #[test]
    fn test_format_chapter_display() {
        assert_eq!(format_chapter_display(Some("第 3 章"), 1), "第 3 章");
        assert_eq!(format_chapter_display(Some("id42"), 1), "位置 42");
        assert_eq!(format_chapter_display(None, 5), "位置 5");
    }
}