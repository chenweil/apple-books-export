//! Apple Books Exporter - 工具函数

use chrono::{DateTime, TimeZone, Utc};

/// Apple CoreData 时间戳基准（2001-01-01 UTC）
const APPLE_EPOCH: i64 = 978307200;

/// 将 Apple 时间戳转换为 DateTime
/// Apple 时间戳是从 2001-01-01 开始的秒数
pub fn apple_timestamp_to_datetime(timestamp: f64) -> Option<DateTime<Utc>> {
    let utc_timestamp = (timestamp + APPLE_EPOCH as f64) as i64;
    Utc.timestamp_opt(utc_timestamp, 0).single()
}

/// 格式化时间戳为人类可读的字符串
pub fn format_timestamp(timestamp: f64) -> String {
    match apple_timestamp_to_datetime(timestamp) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => "未知时间".to_string(),
    }
}

/// 安全文件名（支持中文 CJK 字符）
pub fn sanitize_filename(s: &str) -> String {
    let mut result = String::new();
    for ch in s.chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | ' ' => result.push(ch),
            // 保留中文和其他 Unicode 字符（CJK 范围）
            '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' | '\u{f900}'..='\u{faff}' => {
                result.push(ch)
            }
            // 保留常见标点（排除文件系统不安全的 : ? " < > | * \）
            '.' | ',' | '!' | ';' | '\'' | '(' | ')' | '[' | ']' => {
                result.push(ch)
            }
            _ => result.push('_'),
        }
    }
    // 按字符数截断，而不是字节数，避免多字节字符截断 panic
    let max_chars = 50;
    if result.chars().count() > max_chars {
        result = result.chars().take(max_chars).collect();
    }
    result
}

/// 计算缓存 key 的 MD5
pub fn compute_cache_key(book_id: &str, highlight: &str) -> String {
    let normalized = normalize_text(highlight);
    let hash = md5::compute(normalized.as_bytes());
    format!("{}_{:x}", &book_id[..book_id.len().min(8)], hash)
}

/// 文本标准化
pub fn normalize_text(text: &str) -> String {
    text.trim().split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apple_timestamp_conversion() {
        // 2024-01-01 00:00:00 UTC 的 Apple 时间戳
        let timestamp = 788918400.0; // 从 2001-01-01 到 2024-01-01
        let dt = apple_timestamp_to_datetime(timestamp);
        assert!(dt.is_some());
    }

    #[test]
    fn test_normalize_text() {
        assert_eq!(normalize_text("  hello   world  "), "hello world");
        assert_eq!(normalize_text("a\nb\nc"), "a b c");
    }

    #[test]
    fn test_compute_cache_key() {
        let key1 = compute_cache_key("book123", "test highlight");
        let key2 = compute_cache_key("book123", "test highlight");
        assert_eq!(key1, key2);

        let key3 = compute_cache_key("book123", "different text");
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("hello/world"), "hello_world");
        assert_eq!(sanitize_filename("test:file"), "test_file");
        assert_eq!(sanitize_filename("normal_name"), "normal_name");
        let name = sanitize_filename("测试：笔记内容");
        assert!(name.contains("测试"));
        assert!(name.contains("笔记"));
        assert!(!name.contains("："));
    }
}