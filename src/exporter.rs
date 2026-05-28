//! Apple Books Exporter - Markdown Exporter

use crate::models::{Annotation, Book, LLMResult};
use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::path::Path;

/// 导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Obsidian,
    Markdown,
}

impl From<&str> for ExportFormat {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "obsidian" => ExportFormat::Obsidian,
            "markdown" => ExportFormat::Markdown,
            _ => ExportFormat::Obsidian,
        }
    }
}

/// 导出书籍笔记
pub fn export_book(
    book: &Book,
    annotations: &[Annotation],
    llm_results: &[Option<LLMResult>],
    output_dir: &Path,
    format: ExportFormat,
) -> Result<()> {
    // 创建输出目录
    let book_dir = output_dir.join(&book.title);
    fs::create_dir_all(&book_dir)
        .with_context(|| format!("无法创建输出目录：{:?}", book_dir))?;

    // 生成主笔记文件
    let main_file = book_dir.join(format!("{}.md", sanitize_filename(&book.title)));

    let main_content = generate_main_note(book, annotations, llm_results, format)?;
    fs::write(&main_file, main_content)
        .with_context(|| format!("无法写入主笔记文件：{:?}", main_file))?;

    // 生成单独的 LLM 笔记文件
    for (_i, (ann, llm_result)) in annotations.iter().zip(llm_results.iter()).enumerate() {
        if let Some(result) = llm_result {
            if let Some(selected_text) = &ann.selected_text {
                let file_name = format!("{}.md", sanitize_filename(selected_text));
                let file_path = book_dir.join(file_name);

                let content = generate_llm_note(book, ann, result, format)?;
                fs::write(&file_path, content)
                    .with_context(|| format!("无法写入笔记文件：{:?}", file_path))?;
            }
        }
    }

    Ok(())
}

/// 生成主笔记内容
fn generate_main_note(
    book: &Book,
    annotations: &[Annotation],
    llm_results: &[Option<LLMResult>],
    format: ExportFormat,
) -> Result<String> {
    let mut content = String::new();

    // Frontmatter
    if format == ExportFormat::Obsidian {
        content.push_str(&format!(
            "---\nbook: \"{}\"\nauthor: \"{}\"\n---\n\n",
            book.title, book.author
        ));
    }

    // 书名
    content.push_str(&format!("# {}\n\n", book.title));

    // 笔记列表
    for (_i, (ann, llm_result)) in annotations.iter().zip(llm_results.iter()).enumerate() {
        // 处理有选中文字的高亮/笔记
        if let Some(selected_text) = &ann.selected_text {
            // 章节信息
            if let Some(location) = &ann.location {
                if let Some(chapter) = crate::cfi::extract_chapter_title(location) {
                    content.push_str(&format!("## {}\n\n", chapter));
                }
            }

            // 高亮
            content.push_str(&format!("> {}\n\n", selected_text));

            // LLM 笔记链接
            if llm_result.is_some() {
                let file_name = sanitize_filename(selected_text);
                if format == ExportFormat::Obsidian {
                    content.push_str(&format!("[[{}]]\n\n", file_name));
                } else {
                    content.push_str(&format!("[{}]({}.md)\n\n", file_name, file_name));
                }
            }

            content.push_str("---\n\n");
        }
        // 处理只有位置的高亮（无选中文字）
        else if let Some(location) = &ann.location {
            if let Some(chapter) = crate::cfi::extract_chapter_title(location) {
                content.push_str(&format!("## {}\n\n", chapter));
            }
            content.push_str(&format!("*高亮位置：{}*\n\n", location));
            content.push_str("---\n\n");
        }
    }

    Ok(content)
}

/// 生成 LLM 笔记内容
fn generate_llm_note(
    book: &Book,
    ann: &Annotation,
    result: &LLMResult,
    _format: ExportFormat,
) -> Result<String> {
    let mut content = String::new();

    // Frontmatter
    content.push_str("---\n");
    content.push_str("type: llm-note\n");
    content.push_str(&format!("book: {}\n", book.title));
    content.push_str(&format!(
        "chapter: {}\n",
        ann.location.as_deref().unwrap_or("unknown")
    ));
    if let Some(highlight) = &ann.selected_text {
        content.push_str(&format!("highlight: \"{}\"\n", highlight));
    }
    content.push_str(&format!("tags: [{}]\n", result.tags.join(", ")));
    content.push_str(&format!(
        "created: {}\n",
        chrono::Utc::now().format("%Y-%m-%d")
    ));
    content.push_str("---\n\n");

    // 解释
    content.push_str("## 解释\n\n");
    content.push_str(&format!("{}\n\n", result.explanation));

    // 复习问题
    content.push_str("## 复习问题\n\n");
    content.push_str(&format!("{}\n\n", result.question));

    // 上下文
    if let Some(highlight) = &ann.selected_text {
        content.push_str("## 上下文\n\n");
        content.push_str(&format!("> {}\n", highlight));
    }

    Ok(content)
}

/// 清理文件名（移除非法字符）
pub fn sanitize_filename(name: &str) -> String {
    // 移除或替换非法字符
    let re = Regex::new(r#"/\\:*?"<>|"#).unwrap();
    let cleaned = re.replace_all(name, "_");

    // 限制长度
    let max_len = 100;
    if cleaned.len() > max_len {
        cleaned.chars().take(max_len).collect()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("hello/world"), "hello_world");
        assert_eq!(sanitize_filename("test:file"), "test_file");
        assert_eq!(sanitize_filename("normal_name"), "normal_name");
    }

    #[test]
    fn test_export_format_from_str() {
        assert_eq!(ExportFormat::from("obsidian"), ExportFormat::Obsidian);
        assert_eq!(ExportFormat::from("markdown"), ExportFormat::Markdown);
        assert_eq!(ExportFormat::from("unknown"), ExportFormat::Obsidian);
    }
}