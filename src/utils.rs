//! Apple Books Exporter - 工具函数

use std::path::PathBuf;

/// 获取用户主目录
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
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

#[cfg(test)]
mod tests {
    use super::*;

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