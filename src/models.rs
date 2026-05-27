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
            epub_mappings: HashMap::new(),
            output_format: "obsidian".to_string(),
            card_style: "dark".to_string(),
            card_output: "~/cards/".to_string(),
            context_chars: 200,
            filename_max_length: 20,
        }
    }
}