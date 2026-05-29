use std::path::PathBuf;
use std::sync::Mutex;

use apple_books_exporter::{DB, LLMCache};

pub struct AppState {
    pub db: Mutex<Option<DB>>,
    pub cache: Mutex<LLMCache>,
}

impl AppState {
    pub fn new() -> Self {
        let cache_path = home::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library/Application Support/books-exporter/cache.json");
        std::fs::create_dir_all(cache_path.parent().unwrap()).ok();

        Self {
            db: Mutex::new(None),
            cache: Mutex::new(LLMCache::new(&cache_path)),
        }
    }

    pub fn get_db(&self) -> Result<std::sync::MutexGuard<'_, Option<DB>>, String> {
        let mut db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        if db.is_none() {
            *db = Some(DB::open_apple_books().map_err(|e| format!("无法打开数据库: {}", e))?);
        }
        Ok(db)
    }
}
