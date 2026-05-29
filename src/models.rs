//! Apple Books Exporter - Data Models

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 书籍信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub asset_id: String,
    pub title: String,
    pub author: String,
    pub note_count: u32,
}

/// 笔记/高亮
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub asset_id: String,
    pub selected_text: Option<String>,
    pub note: Option<String>,
    pub location: Option<String>, // CFI
    pub annotation_type: u32,     // 0=bookmark, 1=note, 2=highlight, 3=annotation
    pub creation_date: Option<f64>,
}

/// LLM 处理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResult {
    pub explanation: String,
    pub tags: Vec<String>,
    pub question: String,
}

/// 缓存条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub highlight: String,
    pub file: String,
    pub book_id: String,
    pub book: String,
    pub explanation: String,
    pub tags: Vec<String>,
    pub question: String,
    pub updated: String, // ISO date
}

/// LLM 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub batch_size: u32,
    pub max_retries: u32,
    pub retry_delays: Vec<u64>,
}

/// 卡片生成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardGenConfig {
    pub enrich_prompt: String,      // AI 增强的 prompt 模板
    pub enrich_model: String,       // AI 增强使用的模型（空则用 llm.model）
    pub image_model: String,        // 生成图片的模型（预留）
    pub max_highlight_len: usize,   // 高亮最大字符数
    pub max_explanation_len: usize, // 解释最大字符数
}

impl Default for CardGenConfig {
    fn default() -> Self {
        Self {
            enrich_prompt: "你是一个阅读助手，帮助读者理解书籍内容。请对以下摘录进行分析，提供：\n1. 解释：用通俗易懂的语言解释这段内容的含义\n2. 标签：3-5 个关键词标签，用逗号分隔\n3. 复习问题：一个能帮助读者回顾和理解的问题\n\n请按照以下 JSON 格式返回结果：\n{\n  \"explanation\": \"解释内容\",\n  \"tags\": [\"标签1\", \"标签2\", \"标签3\"],\n  \"question\": \"复习问题\"\n}\n\n摘录内容：\n> {highlight}".to_string(),
            enrich_model: String::new(),
            image_model: String::new(),
            max_highlight_len: 500,
            max_explanation_len: 1000,
        }
    }
}

impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            provider: "openai_compatible".to_string(),
            base_url: "https://token-plan-cn.xiaomimimo.com/v1".to_string(),
            api_key: "".to_string(),
            model: "mimo-v2.5-pro".to_string(),
            batch_size: 10,
            max_retries: 3,
            retry_delays: vec![1, 2, 4],
        }
    }
}

/// EPUB 映射
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpubMapping {
    pub epub: String,
    pub output: String,
}

/// 主配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub llm: LLMConfig,
    pub card_gen: CardGenConfig,
    pub epub_mappings: HashMap<String, EpubMapping>,
    pub output_format: String,
    pub card_style: String,
    pub card_output: String,
    pub context_chars: u32,
    pub filename_max_length: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LLMConfig::default(),
            card_gen: CardGenConfig::default(),
            epub_mappings: HashMap::new(),
            output_format: "obsidian".to_string(),
            card_style: "dark".to_string(),
            card_output: "~/cards/".to_string(),
            context_chars: 200,
            filename_max_length: 20,
        }
    }
}