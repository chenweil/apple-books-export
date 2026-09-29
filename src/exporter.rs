//! Apple Books Exporter - Markdown Exporter

use crate::models::{Annotation, Book, LLMResult};
use crate::speech::export::{resolve_export_links, ExportLinkReport, ResolvedAudioLink};
use crate::speech::{SpeechContentKind, SpeechWarning};
use crate::utils::sanitize_filename;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

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
#[derive(Debug, thiserror::Error)]
pub enum ExportWriteError {
    #[error("output file already exists: {0}")]
    OutputFileExists(PathBuf),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// 导出书籍笔记。保留旧入口的覆盖行为，供现有人类 CLI/GUI 调用。
pub fn export_book(
    book: &Book,
    annotations: &[Annotation],
    llm_results: &[Option<LLMResult>],
    output_dir: &Path,
    format: ExportFormat,
) -> Result<()> {
    export_book_checked(book, annotations, llm_results, output_dir, format, true)
        .map(|_| ())
        .map_err(anyhow::Error::from)
}

/// 一次书籍导出额外返回的 Speech 信息。
#[derive(Debug, Clone, Default)]
pub struct SpeechExportLinks {
    /// 实际写入的相对音频链接。
    pub written: Vec<String>,
    /// 被省略的链接与原因。**不会**让整本阅读笔记导出失败。
    pub warnings: Vec<SpeechWarning>,
}

impl SpeechExportLinks {
    fn from_report(report: ExportLinkReport, written: Vec<String>) -> Self {
        Self {
            written,
            warnings: report.warnings,
        }
    }
}

/// 机器入口使用的受保护导出：默认由调用方禁止覆盖，并返回所有生成文件。
pub fn export_book_checked(
    book: &Book,
    annotations: &[Annotation],
    llm_results: &[Option<LLMResult>],
    output_dir: &Path,
    format: ExportFormat,
    overwrite: bool,
) -> std::result::Result<Vec<PathBuf>, ExportWriteError> {
    export_book_checked_with_speech(book, annotations, llm_results, output_dir, format, overwrite)
        .map(|outcome| outcome.files)
}

/// 与 [`export_book_checked`] 相同，另外报告 Speech 音频链接的处理结果。
///
/// 常规 Markdown/Obsidian 导出**只读取** Speech Export Manifest 并渲染已存在且校验通过的
/// active 变体：它不复制仅由应用管理的缓存，不为补齐链接调用 Speech Provider，也不在
/// 没有导出语音时写占位链接。缺失、格式不符、checksum 不匹配、路径逃出导出根或 manifest
/// 损坏时省略对应链接并返回结构化 warning——音频是可选的，主体阅读笔记导出照常完成。
pub fn export_book_checked_with_speech(
    book: &Book,
    annotations: &[Annotation],
    llm_results: &[Option<LLMResult>],
    output_dir: &Path,
    format: ExportFormat,
    overwrite: bool,
) -> std::result::Result<BookExportOutcome, ExportWriteError> {
    let book_dir = output_dir.join(safe_path_component(&book.title));
    let main_file = book_dir.join(format!("{}.md", sanitize_filename(&book.title)));
    let mut generated_files = vec![main_file.clone()];

    for (ann, llm_result) in annotations.iter().zip(llm_results.iter()) {
        if llm_result.is_some() {
            if let Some(selected_text) = &ann.selected_text {
                generated_files
                    .push(book_dir.join(format!("{}.md", sanitize_filename(selected_text))));
            }
        }
    }

    if !overwrite {
        if let Some(existing) = generated_files.iter().find(|path| path.exists()) {
            return Err(ExportWriteError::OutputFileExists(existing.clone()));
        }
    }

    // Speech 音频链接来自用户此前显式执行 `speech export` 留下的 manifest。
    // 解析失败只会产生 warning，不会让主体导出失败。
    let link_report = resolve_export_links(&book_dir, &book.asset_id);
    let mut written_links = Vec::new();

    fs::create_dir_all(&book_dir).with_context(|| format!("无法创建输出目录：{:?}", book_dir))?;
    let main_content = generate_main_note(
        book,
        annotations,
        llm_results,
        format,
        &link_report,
        &mut written_links,
    )?;
    fs::write(&main_file, main_content)
        .with_context(|| format!("无法写入主笔记文件：{:?}", main_file))?;

    for (ann, llm_result) in annotations.iter().zip(llm_results.iter()) {
        if let (Some(result), Some(selected_text)) = (llm_result, &ann.selected_text) {
            let file_path = book_dir.join(format!("{}.md", sanitize_filename(selected_text)));
            let content = generate_llm_note(book, ann, result, format)?;
            fs::write(&file_path, content)
                .with_context(|| format!("无法写入笔记文件：{:?}", file_path))?;
        }
    }

    Ok(BookExportOutcome {
        files: generated_files,
        speech: SpeechExportLinks::from_report(link_report, written_links),
    })
}

/// 书籍导出的完整结果：生成的文件 + Speech 链接处理情况。
#[derive(Debug, Clone)]
pub struct BookExportOutcome {
    /// 本次生成/允许覆盖的所有 Markdown 文件。
    pub files: Vec<PathBuf>,
    /// Speech 音频链接与 warning。
    pub speech: SpeechExportLinks,
}

fn safe_path_component(value: &str) -> String {
    let sanitized = sanitize_filename(value);
    match sanitized.as_str() {
        "" | "." | ".." => "_".to_string(),
        _ => sanitized,
    }
}

/// 生成主笔记内容
///
/// 音频链接紧跟它自己的内容部分：高亮链接跟在 `> 高亮` 之后，笔记链接跟在
/// `**笔记**` 之后；两者不会共享一个链接（实施 spec §10）。
fn generate_main_note(
    book: &Book,
    annotations: &[Annotation],
    llm_results: &[Option<LLMResult>],
    format: ExportFormat,
    links: &ExportLinkReport,
    written_links: &mut Vec<String>,
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

            // 高亮的语音链接紧跟高亮本身。
            push_audio_link(
                &mut content,
                links,
                &ann.id,
                SpeechContentKind::Highlight,
                format,
                written_links,
            );

            if let Some(note) = note {
                content.push_str(&format!("**笔记**: {}\n\n", note));

                // 笔记的语音链接紧跟笔记本身。
                push_audio_link(
                    &mut content,
                    links,
                    &ann.id,
                    SpeechContentKind::Note,
                    format,
                    written_links,
                );
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
            push_audio_link(
                &mut content,
                links,
                &ann.id,
                SpeechContentKind::Note,
                format,
                written_links,
            );
            content.push_str("---\n\n");
        }
    }

    Ok(content)
}

/// 把某个 Annotation 内容部分的 active 音频链接紧跟在它后面写进笔记。
///
/// 没有可用链接时**不写任何占位**：ADR 0007「没有导出语音时不写占位链接」。
fn push_audio_link(
    content: &mut String,
    links: &ExportLinkReport,
    annotation_id: &str,
    content_kind: SpeechContentKind,
    format: ExportFormat,
    written_links: &mut Vec<String>,
) {
    let Some(link) = links
        .links
        .get(&(annotation_id.to_string(), content_kind))
    else {
        return;
    };
    let rendered = render_audio_link(link, content_kind, format);
    written_links.push(rendered.clone());
    content.push_str(&rendered);
    content.push_str("\n\n");
}

/// 渲染一条音频链接。普通 Markdown 写相对播放链接，Obsidian 在同一位置写相对音频嵌入。
fn render_audio_link(
    link: &ResolvedAudioLink,
    content_kind: SpeechContentKind,
    format: ExportFormat,
) -> String {
    let label = match content_kind {
        SpeechContentKind::Highlight => "播放高亮语音",
        SpeechContentKind::Note => "播放笔记语音",
    };
    match format {
        ExportFormat::Obsidian => format!("![[{}]]", link.relative_path),
        ExportFormat::Markdown => format!("[▶ {label}]({})", link.relative_path),
    }
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
                id: "annotation-test".to_string(),
                asset_id: "book".to_string(),
                selected_text: None,
                note: None,
                location: Some("epubcfi(/6/24[id16]!/4/222/1:129)".to_string()),
                annotation_type: 0,
                creation_date: None,
            },
            Annotation {
                id: "annotation-test".to_string(),
                asset_id: "book".to_string(),
                selected_text: Some("真正的高亮".to_string()),
                note: None,
                location: Some("epubcfi(/6/24[id16]!/4/224/1:0)".to_string()),
                annotation_type: 2,
                creation_date: None,
            },
        ];
        let llm_results = vec![None, None];

        let content = generate_main_note(
            &book,
            &annotations,
            &llm_results,
            ExportFormat::Markdown,
            &ExportLinkReport::default(),
            &mut Vec::new(),
        )
        .unwrap();

        assert!(!content.contains("高亮位置"));
        assert!(content.contains("> 真正的高亮"));
    }
}
