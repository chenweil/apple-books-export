//! Apple Books Exporter - EPUB CFI Parser

use regex::Regex;

/// 从 EPUB CFI 字符串中提取 manifest item ID
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
            if let Ok(re) = Regex::new(r"[0-9a-f]{8,}") {
                if re.is_match(s) {
                    return true;
                }
            }
        }
        if let Ok(re) = Regex::new(r"^id\d{3,}$") {
            if re.is_match(s) {
                return true;
            }
        }
        false
    }

    /// 判断是否为有效的章节标识
    fn is_valid_chapter(s: &str) -> bool {
        if s.ends_with('-') || s.ends_with('_') {
            return false;
        }

        // 包含中文字符
        if let Ok(re) = Regex::new(r"[一 - 鿿]") {
            if re.is_match(s) {
                return true;
            }
        }

        // chapter/ch/section 后跟数字
        if let Ok(re) = Regex::new(r"(?i)(chapter|ch|section)\d+") {
            if re.is_match(s) {
                return true;
            }
        }

        // 字母开头的文件名
        if s.len() > 3 {
            if let Ok(re) = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$") {
                if re.is_match(s) {
                    return true;
                }
            }
        }

        false
    }

    let mut chapter_candidates: Vec<String> = Vec::new();

    // 从后往前遍历匹配（最后一个有意义的通常是章节）
    for &match_str in matches.iter().rev() {
        // 去除文件扩展名
        let base = if let Ok(re) = Regex::new(r"(?i)\.(xhtml|html)$") {
            if re.is_match(match_str) {
                re.replace(match_str, "").to_string()
            } else {
                match_str.to_string()
            }
        } else {
            match_str.to_string()
        };

        // 中文标题 - 提取分隔符后的部分
        if let Ok(re) = Regex::new(r"[一 - 鿿]") {
            if re.is_match(&base) {
                if base.contains('-') || base.contains('_') {
                    if let Ok(re) = Regex::new(r"[-_]") {
                        if let Some(caps) = re.captures(&base) {
                            if let Some(part) = caps.get(1) {
                                let part_str = part.as_str().trim();
                                if part_str.len() > 2 {
                                    return Some(part_str.to_string());
                                }
                            }
                        }
                    }
                }
                return Some(base);
            }
        }

        // 文件名带扩展名 - 尝试 section/chapter 模式
        if let Ok(re) = Regex::new(r"(?i)\.(xhtml|html)$") {
            if re.is_match(match_str) {
                if let Ok(re) = Regex::new(r"(?i)Section(\d+)") {
                    if let Some(caps) = re.captures(&base) {
                        if let Some(num) = caps.get(1) {
                            if let Ok(n) = num.as_str().parse::<u32>() {
                                return Some(format!("第{}章", n));
                            }
                        }
                    }
                }

                if let Ok(re) = Regex::new(r"(?i)(ch|chapter)(\d+)") {
                    if let Some(caps) = re.captures(&base) {
                        if let Some(num) = caps.get(2) {
                            if let Ok(n) = num.as_str().parse::<u32>() {
                                return Some(format!("第{}章", n));
                            }
                        }
                    }
                }

                if base.len() > 3 && !is_meaningless_id(&base) {
                    chapter_candidates.push(base);
                }
                continue;
            }
        }

        if base.len() > 3 && !is_meaningless_id(&base) && is_valid_chapter(&base) {
            chapter_candidates.push(base);
        }
    }

    chapter_candidates.into_iter().next()
}

/// 格式化章节显示
pub fn format_chapter_display(chapter: Option<&str>, index: usize) -> String {
    if let Some(chapter) = chapter {
        if let Ok(re) = Regex::new(r"^id(\d+)$") {
            if let Some(caps) = re.captures(chapter) {
                if let Some(num) = caps.get(1) {
                    return format!("位置 {}", num.as_str());
                }
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
    }

    #[test]
    fn test_extract_chapter_title_section() {
        let cfi = "epubcfi(/6/10[Section0003.xhtml]!/4/82/1,:0,:44)";
        assert_eq!(extract_chapter_title(cfi), Some("第3章".to_string()));
    }

    #[test]
    fn test_extract_chapter_title_chapter() {
        let cfi = "epubcfi(/6/10[chapter5.xhtml]!/4/82/1,:0,:44)";
        assert_eq!(extract_chapter_title(cfi), Some("第5章".to_string()));
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