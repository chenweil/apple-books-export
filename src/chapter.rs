//! Chapter indexing and coaching prompts.

use crate::cfi::{extract_chapter_title, extract_item_id, format_chapter_display};
use crate::models::{Annotation, Book};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChapterInfo {
    pub key: String,
    pub index: usize,
    pub title: String,
    pub display_title: String,
    pub highlight_count: usize,
    pub note_count: usize,
    pub sample_highlights: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoachStep {
    PreReading,
    RecallFeedback,
    ApplicationQuestion,
    ChapterCard,
}

impl CoachStep {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "pre_reading" => Some(Self::PreReading),
            "recall_feedback" => Some(Self::RecallFeedback),
            "application_question" => Some(Self::ApplicationQuestion),
            "chapter_card" => Some(Self::ChapterCard),
            _ => None,
        }
    }
}

pub fn chapter_key(annotation: &Annotation, fallback_index: usize) -> String {
    annotation
        .location
        .as_deref()
        .and_then(extract_item_id)
        .or_else(|| {
            annotation
                .location
                .as_deref()
                .and_then(extract_chapter_title)
        })
        .unwrap_or_else(|| format!("unknown-{}", fallback_index))
}

pub fn chapter_title(annotation: &Annotation, fallback_index: usize) -> String {
    let parsed = annotation
        .location
        .as_deref()
        .and_then(extract_chapter_title);
    format_chapter_display(parsed.as_deref(), fallback_index)
}

pub fn collect_chapters(annotations: &[Annotation]) -> Vec<ChapterInfo> {
    let mut chapters: BTreeMap<String, ChapterInfo> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    for (annotation_index, annotation) in annotations.iter().enumerate() {
        let key = chapter_key(annotation, annotation_index + 1);
        if !chapters.contains_key(&key) {
            let index = order.len() + 1;
            let title = normalize_chapter_title(&chapter_title(annotation, index), index);
            chapters.insert(
                key.clone(),
                ChapterInfo {
                    key: key.clone(),
                    index,
                    display_title: display_chapter_title(index, &title),
                    title,
                    highlight_count: 0,
                    note_count: 0,
                    sample_highlights: Vec::new(),
                },
            );
            order.push(key.clone());
        }

        if let Some(chapter) = chapters.get_mut(&key) {
            if annotation
                .selected_text
                .as_ref()
                .map_or(false, |text| !text.trim().is_empty())
            {
                chapter.highlight_count += 1;
                if chapter.sample_highlights.len() < 5 {
                    chapter
                        .sample_highlights
                        .push(annotation.selected_text.clone().unwrap_or_default());
                }
            }
            if annotation
                .note
                .as_ref()
                .map_or(false, |note| !note.trim().is_empty())
            {
                chapter.note_count += 1;
            }
        }
    }

    order
        .into_iter()
        .filter_map(|key| chapters.remove(&key))
        .collect()
}

pub fn annotations_for_chapter<'a>(
    annotations: &'a [Annotation],
    chapter_key_value: &str,
) -> Vec<&'a Annotation> {
    annotations
        .iter()
        .enumerate()
        .filter_map(|(index, annotation)| {
            if chapter_key(annotation, index + 1) == chapter_key_value {
                Some(annotation)
            } else {
                None
            }
        })
        .collect()
}

pub fn build_chapter_context(
    book: &Book,
    chapter: &ChapterInfo,
    annotations: &[&Annotation],
) -> String {
    let mut context = String::new();
    context.push_str(&format!("书名: {}\n", book.title));
    context.push_str(&format!("作者: {}\n", book.author));
    if is_fallback_chapter(chapter) {
        context.push_str(&format!("章节序号: 第 {} 章\n", chapter.index));
        context.push_str("章节标题: 未识别（不要围绕 Apple Books 的内部位置编号提问）\n");
    } else {
        context.push_str(&format!("章节: {}\n", chapter.display_title));
    }
    context.push_str("\n本章已有高亮/笔记摘录:\n");

    let mut added = 0usize;
    for annotation in annotations {
        if added >= 12 {
            break;
        }
        let Some(text) = annotation.selected_text.as_ref() else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        added += 1;
        context.push_str(&format!("{}. {}\n", added, compact_text(text, 220)));
        if let Some(note) = annotation
            .note
            .as_ref()
            .filter(|note| !note.trim().is_empty())
        {
            context.push_str(&format!("   读者笔记: {}\n", compact_text(note, 160)));
        }
    }

    if added == 0 {
        context.push_str("- 暂无高亮文本。读前问题应基于书名、章节序号和阅读目的提出，帮助读者进入本章；不要假装知道本章具体内容。\n");
    }

    context
}

pub fn build_coach_prompt(
    step: CoachStep,
    context: &str,
    reading_goal: Option<&str>,
    user_recall: Option<&str>,
    user_context: Option<&str>,
) -> String {
    let goal = reading_goal
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("未提供");
    let scene = user_context
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("未提供");

    match step {
        CoachStep::PreReading => format!(
            r#"你是一个稳重的章节阅读陪练，不是代读总结器。

请遵守：
1. 不要总结本章。
2. 不要替读者下结论。
3. 只提出 3 个读前问题。
4. 问题要帮助读者观察作者想解决什么、如何论证、如何联系自己的经验。
5. 如果章节标题未识别或只是“位置 N”，不要解读“位置”这个词，也不要围绕位置、起点、编号提问。
6. 如果没有高亮/笔记摘录，就按“第几章”的阅读单位，基于书名和阅读目的提出读前观察问题；不要假装知道章节具体内容。

阅读目的: {goal}

{context}

请输出 3 个读前问题，使用编号列表。"#,
        ),
        CoachStep::RecallFeedback => format!(
            r#"你是一个稳重的章节阅读陪练。读者已经读完本章，并先做了自己的复述。

请遵守：
1. 先反馈，不要替读者重写总结。
2. 只基于读者复述和已有高亮判断，不要声称你知道完整原文。
3. 输出三段：抓住的 3 个关键点、可能漏掉的 3 个关键点、可能误解或跳步的地方。
4. 语气直接、具体，避免泛泛鼓励。

阅读目的: {goal}

{context}

读者复述:
{recall}

请按要求反馈。"#,
            recall = user_recall.unwrap_or("未提供")
        ),
        CoachStep::ApplicationQuestion => format!(
            r#"你是一个稳重的章节阅读陪练。

请根据这一章和读者目标，设计 1 个现实应用问题。

要求：
1. 这个问题必须用本章观点才能回答。
2. 不要问概念定义。
3. 问题要贴近读者场景。
4. 先只给问题，不要给答案。

阅读目的: {goal}
读者场景: {scene}

{context}

请只输出 1 个应用问题。"#,
        ),
        CoachStep::ChapterCard => format!(
            r#"你是一个稳重的章节阅读陪练。请把这一章整理成一张章节卡片。

限制：
1. 不要写成完整书摘。
2. 不要超过 800 字。
3. 保留读者表达，不要改得太像标准答案。
4. 如果信息不足，请明确标注“待回原文核验”。

格式：
## 本章一句话

## 我自己的复述

## 作者的关键论证

## 我原来漏掉/误解的地方

## 一个能用起来的应用问题

## 下一章要带着的问题

阅读目的: {goal}
读者场景: {scene}

{context}

读者复述/应用回答:
{recall}

请输出章节卡片。"#,
            recall = user_recall.unwrap_or("未提供")
        ),
    }
}

fn compact_text(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    trimmed
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>()
        + "..."
}

fn normalize_chapter_title(title: &str, chapter_index: usize) -> String {
    if title == "unknown" || title.starts_with("位置 ") {
        return format!("第 {} 章", chapter_index);
    }
    title.to_string()
}

fn display_chapter_title(chapter_index: usize, title: &str) -> String {
    let compact = title.replace(' ', "");
    if compact.starts_with(&format!("第{}章", chapter_index)) {
        title.to_string()
    } else {
        format!("第 {} 章 · {}", chapter_index, title)
    }
}

fn is_fallback_chapter(chapter: &ChapterInfo) -> bool {
    chapter.key.starts_with("unknown-") || chapter.title == format!("第 {} 章", chapter.index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotation(location: &str, text: &str) -> Annotation {
        Annotation {
            asset_id: "book".to_string(),
            selected_text: Some(text.to_string()),
            note: None,
            location: Some(location.to_string()),
            annotation_type: 2,
            creation_date: None,
        }
    }

    #[test]
    fn test_collect_chapters_groups_repeated_locations() {
        let annotations = vec![
            annotation("epubcfi(/6/10[Section0001.xhtml]!/4/2)", "a"),
            annotation("epubcfi(/6/10[Section0001.xhtml]!/4/8)", "b"),
            annotation("epubcfi(/6/12[Section0002.xhtml]!/4/2)", "c"),
        ];

        let chapters = collect_chapters(&annotations);

        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].index, 1);
        assert_eq!(chapters[0].highlight_count, 2);
        assert_eq!(chapters[1].display_title, "第2章");
    }

    #[test]
    fn test_pre_reading_prompt_blocks_summary() {
        let prompt = build_coach_prompt(
            CoachStep::PreReading,
            "章节: 第 1 章",
            Some("理解心理表征"),
            None,
            None,
        );

        assert!(prompt.contains("不要总结本章"));
        assert!(prompt.contains("3 个读前问题"));
    }

    #[test]
    fn test_fallback_chapter_context_blocks_position_guessing() {
        let book = Book {
            asset_id: "book".to_string(),
            title: "趣谈".to_string(),
            author: "作者".to_string(),
            note_count: 1,
        };
        let chapter = ChapterInfo {
            key: "unknown-1".to_string(),
            index: 1,
            title: "第 1 章".to_string(),
            display_title: "第 1 章".to_string(),
            highlight_count: 0,
            note_count: 0,
            sample_highlights: Vec::new(),
        };

        let context = build_chapter_context(&book, &chapter, &[]);

        assert!(context.contains("章节标题: 未识别"));
        assert!(context.contains("不要围绕 Apple Books 的内部位置编号提问"));
    }
}
