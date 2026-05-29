//! Apple Books Exporter - Library

pub mod card;
pub mod cache;
pub mod cfi;
pub mod config;
pub mod db;
pub mod exporter;
pub mod models;
pub mod prompt;
pub mod provider;
pub mod utils;

pub use card::{generate_card, card_filename, CardStyle};
pub use utils::sanitize_filename;
pub use cache::LLMCache;
pub use config::{load_config, save_config};
pub use db::DB;
pub use exporter::{export_book, ExportFormat};
pub use models::*;
pub use prompt::build_enrich_prompt;
pub use provider::{parse_llm_result, LLMProvider};