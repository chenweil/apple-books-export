use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use apple_books_exporter::{DB, LLMCache};

pub struct AppState {
    pub db: Mutex<DB>,
    pub cache: Mutex<LLMCache>,
}

impl AppState {
    pub fn new() -> Self {
        let cache_path = apple_books_exporter::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library/Application Support/books-exporter/cache.json");
        std::fs::create_dir_all(cache_path.parent().unwrap()).ok();

        Self {
            db: Mutex::new(DB::open_apple_books().expect("无法打开 Apple Books 数据库")),
            cache: Mutex::new(LLMCache::new(&cache_path)),
        }
    }

    pub fn get_db(&self) -> Result<MutexGuard<'_, DB>, String> {
        self.db.lock().map_err(|e| format!("数据库锁失败: {}", e))
    }
}
