//! Apple Books Exporter - Markdown Exporter

use crate::models::{Annotation, Book, LLMResult};
use crate::utils::sanitize_filename;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
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
    fs::create_dir_all(&book_dir).with_context(|| format!("无法创建输出目录：{:?}", book_dir))?;

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

    let chapters = crate::chapter::collect_chapters(annotations);
    let mut printed_chapters: BTreeSet<String> = BTreeSet::new();

    // 笔记列表
    for (i, (ann, llm_result)) in annotations.iter().zip(llm_results.iter()).enumerate() {
        let selected_text = ann
            .selected_text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let note = ann
            .note
            .as_deref()
            .map(str::trim)
            .filter(|note| !note.is_empty());

        if selected_text.is_none() && note.is_none() {
            continue;
        }

        let chapter_key = crate::chapter::chapter_key(ann, i + 1);
        if printed_chapters.insert(chapter_key.clone()) {
            if let Some(chapter) = chapters.iter().find(|chapter| chapter.key == chapter_key) {
                content.push_str(&format!("## {}\n\n", chapter.display_title));
            }
        }

        // 处理有选中文字的高亮/笔记
        if let Some(selected_text) = selected_text {
            // 高亮
            content.push_str(&format!("> {}\n\n", selected_text));

            if let Some(note) = note {
                content.push_str(&format!("**笔记**: {}\n\n", note));
            }

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
        // 处理只有笔记、没有选中文字的记录；纯位置记录不导出。
        else if let Some(note) = note {
            content.push_str(&format!("**笔记**: {}\n\n", note));
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
    let chapter = crate::chapter::chapter_title(ann, 1);
    content.push_str(&format!("chapter: {}\n", chapter));
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

    #[test]
    fn test_main_note_skips_location_only_annotations() {
        let book = Book {
            asset_id: "book".to_string(),
            title: "测试书".to_string(),
            author: "作者".to_string(),
            note_count: 2,
        };
        let annotations = vec![
            Annotation {
                asset_id: "book".to_string(),
                selected_text: None,
                note: None,
                location: Some("epubcfi(/6/24[id16]!/4/222/1:129)".to_string()),
                annotation_type: 0,
                creation_date: None,
            },
            Annotation {
                asset_id: "book".to_string(),
                selected_text: Some("真正的高亮".to_string()),
                note: None,
                location: Some("epubcfi(/6/24[id16]!/4/224/1:0)".to_string()),
                annotation_type: 2,
                creation_date: None,
            },
        ];
        let llm_results = vec![None, None];

        let content =
            generate_main_note(&book, &annotations, &llm_results, ExportFormat::Markdown).unwrap();

        assert!(!content.contains("高亮位置"));
        assert!(content.contains("> 真正的高亮"));
    }
}
