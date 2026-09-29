//! Apple Books Exporter - 工具函数

use std::path::PathBuf;

/// 获取用户主目录
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// 判断目录当前是否可写。
///
/// 真正的可写性取决于文件系统权限位与挂载选项（例如只读卷），所以用一次
/// 真实的创建-删除探测，而不是只看权限位。探测文件在返回前一定被删除。
pub fn dir_is_writable(dir: &std::path::Path) -> bool {
    // `File::create` 本身就会因权限或只读挂载失败，这已经足够判定；
    // 写 0 字节和 `sync_all()` 不增加任何证明力，却让每次 doctor 都做一次
    // 同步落盘。探针文件名带上时间戳，降低进程被 SIGKILL 后残留重名的可能。
    let probe = dir.join(format!(
        ".exporter-write-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let created = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    created
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
            '.' | ',' | '!' | ';' | '\'' | '(' | ')' | '[' | ']' => result.push(ch),
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
