//! Apple Books Exporter - Library

pub mod cache;
pub mod card;
pub mod cfi;
pub mod chapter;
pub mod config;
pub mod db;
pub mod exporter;
pub mod models;
pub mod prompt;
pub mod provider;
pub mod utils;

pub use cache::LLMCache;
pub use card::{card_filename, generate_card, CardStyle};
pub use chapter::{
    annotations_for_chapter, build_chapter_context, build_coach_prompt, collect_chapters, CoachStep,
};
pub use config::{load_config, save_config};
pub use db::DB;
pub use exporter::{export_book, ExportFormat};
pub use models::*;
pub use prompt::{build_enrich_prompt, build_enrich_prompt_with_template};
pub use provider::{parse_llm_result, LLMProvider};
pub use utils::{home_dir, sanitize_filename};
