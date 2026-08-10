//! Apple Books Exporter - Data Models

use serde::{Deserialize, Serialize};

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
    pub id: String,
    pub asset_id: String,
    pub selected_text: Option<String>,
    pub note: Option<String>,
    pub location: Option<String>, // CFI
    pub annotation_type: u32,     // Apple Books 原始值;与内容不对应,勿用于分类,见 CLAUDE.md
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

/// API 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub name: String,           // 配置名称（如 "默认"、"画图"）
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            name: "默认".to_string(),
            base_url: "https://token-plan-cn.xiaomimimo.com/v1".to_string(),
            api_key: String::new(),
            model: "mimo-v2.5-pro".to_string(),
        }
    }
}

/// 卡片生成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardGenConfig {
    pub enrich_prompt: String,      // AI 增强的 prompt 模板
    pub enrich_api: String,         // AI 增强使用的 API 配置名（空则用默认）
}

impl Default for CardGenConfig {
    fn default() -> Self {
        Self {
            enrich_prompt: "你是一个阅读助手，帮助读者理解书籍内容。请对以下摘录进行分析，提供：\n1. 解释：用通俗易懂的语言解释这段内容的含义\n2. 标签：3-5 个关键词标签，用逗号分隔\n3. 复习问题：一个能帮助读者回顾和理解的问题\n\n请按照以下 JSON 格式返回结果：\n{\n  \"explanation\": \"解释内容\",\n  \"tags\": [\"标签1\", \"标签2\", \"标签3\"],\n  \"question\": \"复习问题\"\n}\n\n摘录内容：\n> {highlight}".to_string(),
            enrich_api: String::new(),
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

/// 主配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub llm: LLMConfig,
    pub api_configs: Vec<ApiConfig>,  // 多个 API 配置
    pub card_gen: CardGenConfig,
    pub output_format: String,
    pub card_style: String,
    pub card_output: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LLMConfig::default(),
            api_configs: vec![ApiConfig::default()],
            card_gen: CardGenConfig::default(),
            output_format: "obsidian".to_string(),
            card_style: "dark".to_string(),
            card_output: "~/cards/".to_string(),
        }
    }
}
