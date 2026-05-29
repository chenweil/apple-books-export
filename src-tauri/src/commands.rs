use std::path::PathBuf;

use keyring::Entry;
use tauri::Emitter;

use apple_books_exporter::{
    card_filename, export_book, generate_card, load_config, sanitize_filename, save_config,
    Annotation, Book, CardStyle, Config, ExportFormat, LLMProvider,
};

use crate::state::AppState;

#[tauri::command]
pub fn check_db_access() -> bool {
    match apple_books_exporter::DB::open_apple_books() {
        Ok(db) => {
            db.list_books().is_ok()
        }
        Err(_) => false,
    }
}

#[tauri::command]
pub fn get_books(state: tauri::State<AppState>) -> Result<Vec<Book>, String> {
    let db = state.get_db()?;
    let db = db.as_ref().unwrap();
    db.list_books().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_annotations(
    asset_id: String,
    state: tauri::State<AppState>,
) -> Result<Vec<Annotation>, String> {
    let db = state.get_db()?;
    let db = db.as_ref().unwrap();
    db.get_annotations(&asset_id).map_err(|e| e.to_string())
}

fn get_config_path() -> PathBuf {
    home::home_dir()
        .map(|h| h.join("Library/Application Support/books-exporter/config.json"))
        .unwrap_or_else(|| PathBuf::from("config.json"))
}

#[tauri::command]
pub fn load_app_config() -> Result<Config, String> {
    let config_path = get_config_path();
    let mut config = load_config(Some(&config_path)).map_err(|e| e.to_string())?;

    if let Ok(entry) = Entry::new("apple-books-exporter", "api-key") {
        if let Ok(key) = entry.get_password() {
            if !key.is_empty() {
                config.llm.api_key = key;
            }
        }
    }

    Ok(config)
}

#[tauri::command]
pub fn save_app_config(mut config: Config) -> Result<(), String> {
    let config_path = get_config_path();
    
    let api_key = config.llm.api_key.clone();
    if !api_key.is_empty() {
        if let Ok(entry) = Entry::new("apple-books-exporter", "api-key") {
            entry.set_password(&api_key).map_err(|e| e.to_string())?;
        }
    }

    config.llm.api_key = String::new();
    save_config(&config, Some(&config_path)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_cache_stats(book_id: String, state: tauri::State<AppState>) -> serde_json::Value {
    let cache = state.cache.lock().unwrap();
    let entries = cache.get_all_for_book(&book_id);
    let total = entries.len();
    let cached = entries
        .iter()
        .filter(|(_, e)| !e.explanation.is_empty())
        .count();
    serde_json::json!({
        "total": total,
        "cached": cached,
        "uncached": total - cached,
    })
}

#[tauri::command]
pub fn clear_cache_for_book(book_id: String, state: tauri::State<AppState>) -> Result<(), String> {
    let mut cache = state.cache.lock().map_err(|e| e.to_string())?;
    // Collect the (book_id, highlight) pairs we need to remove
    let entries: Vec<(String, String)> = cache
        .get_all_for_book(&book_id)
        .iter()
        .map(|(_, e)| (e.book_id.clone(), e.highlight.clone()))
        .collect();
    for (bid, highlight) in entries {
        cache
            .remove(&bid, &highlight)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn export_book_cmd(
    book_index: usize,
    output_dir: String,
    format: String,
    state: tauri::State<'_, AppState>,
    window: tauri::WebviewWindow,
) -> Result<String, String> {
    let db = state.get_db()?;
    let books = db.as_ref().unwrap().list_books().map_err(|e| e.to_string())?;

    if book_index == 0 || book_index > books.len() {
        return Err("无效的书籍序号".to_string());
    }

    let book = &books[book_index - 1];
    let annotations = db
        .as_ref()
        .unwrap()
        .get_annotations(&book.asset_id)
        .map_err(|e| e.to_string())?;

    let output_path = PathBuf::from(&output_dir);
    let export_format = match format.as_str() {
        "obsidian" => ExportFormat::Obsidian,
        _ => ExportFormat::Markdown,
    };

    window
        .emit(
            "progress",
            serde_json::json!({
                "current": 0, "total": annotations.len(),
                "status": "exporting", "message": "开始导出..."
            }),
        )
        .ok();

    let llm_results: Vec<Option<apple_books_exporter::LLMResult>> =
        annotations.iter().map(|_| None).collect();

    export_book(book, &annotations, &llm_results, &output_path, export_format)
        .map_err(|e| e.to_string())?;

    window
        .emit(
            "complete",
            serde_json::json!({ "success": 1, "failed": 0 }),
        )
        .ok();

    Ok(format!("{}/{}", output_dir, sanitize_filename(&book.title)))
}

#[tauri::command]
pub async fn enrich_book_cmd(
    book_index: usize,
    force: bool,
    state: tauri::State<'_, AppState>,
    window: tauri::WebviewWindow,
) -> Result<serde_json::Value, String> {
    let config = load_config(None).map_err(|e| e.to_string())?;
    let provider = LLMProvider::new(&config.llm);

    // Collect DB data into owned values so the MutexGuard is dropped before any .await
    let (book, annotations) = {
        let db = state.get_db()?;
        let db_ref = db.as_ref().unwrap();
        let books = db_ref.list_books().map_err(|e| e.to_string())?;

        if book_index == 0 || book_index > books.len() {
            return Err("无效的书籍序号".to_string());
        }

        let book = books[book_index - 1].clone();
        let annotations = db_ref
            .get_annotations(&book.asset_id)
            .map_err(|e| e.to_string())?;
        (book, annotations)
    };

    let to_process: Vec<(usize, &Annotation)> = annotations
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            a.selected_text
                .as_ref()
                .map_or(false, |t| !t.trim().is_empty())
        })
        .collect();

    let total = to_process.len();
    let mut success = 0u32;
    let mut failed = 0u32;

    window
        .emit(
            "progress",
            serde_json::json!({
                "current": 0, "total": total,
                "status": "processing", "message": "开始处理..."
            }),
        )
        .ok();

    for (idx, ann) in to_process.iter() {
        let text = ann.selected_text.as_ref().unwrap();

        if !force {
            let cache = state.cache.lock().unwrap();
            if cache.is_cached(&book.asset_id, text) {
                success += 1;
                continue;
            }
        }

        let prompt = apple_books_exporter::build_enrich_prompt(text, None);
        match provider.complete(&prompt, None).await {
            Ok(content) => match apple_books_exporter::parse_llm_result(&content) {
                Ok(result) => {
                    let mut cache = state.cache.lock().unwrap();
                    cache
                        .put(
                            &book.asset_id,
                            text,
                            "",
                            &book.title,
                            &result.explanation,
                            &result.tags,
                            &result.question,
                        )
                        .ok();
                    success += 1;
                }
                Err(_) => failed += 1,
            },
            Err(e) => {
                window
                    .emit(
                        "error",
                        serde_json::json!({
                            "message": format!("处理第{}条失败: {}", idx + 1, e)
                        }),
                    )
                    .ok();
                failed += 1;
            }
        }

        window
            .emit(
                "progress",
                serde_json::json!({
                    "current": idx + 1, "total": total,
                    "status": "processing",
                    "message": format!("处理第{}/{}条...", idx + 1, total)
                }),
            )
            .ok();
    }

    window
        .emit(
            "complete",
            serde_json::json!({ "success": success, "failed": failed }),
        )
        .ok();

    Ok(serde_json::json!({ "success": success, "failed": failed }))
}

#[tauri::command]
pub async fn test_llm_connection() -> Result<bool, String> {
    let config_path = get_config_path();
    let config = load_config(Some(&config_path)).map_err(|e| e.to_string())?;
    
    let mut llm_config = config.llm;
    if let Ok(entry) = Entry::new("apple-books-exporter", "api-key") {
        if let Ok(key) = entry.get_password() {
            if !key.is_empty() {
                llm_config.api_key = key;
            }
        }
    }
    
    let provider = LLMProvider::new(&llm_config);
    match provider.complete("hello", None).await {
        Ok(_) => Ok(true),
        Err(e) => Err(format!("连接失败: {}", e)),
    }
}

#[tauri::command]
pub fn open_system_settings() {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .spawn()
        .ok();
}

#[tauri::command]
pub fn get_cache_entries(
    book_id: String,
    state: tauri::State<AppState>,
) -> Vec<serde_json::Value> {
    let cache = state.cache.lock().unwrap();
    cache
        .get_all_for_book(&book_id)
        .iter()
        .map(|(key, entry)| {
            serde_json::json!({
                "key": key,
                "highlight": entry.highlight,
                "explanation": entry.explanation,
                "tags": entry.tags,
                "updated": entry.updated,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn generate_cards_cmd(
    book_index: usize,
    style: String,
    output_dir: String,
    state: tauri::State<'_, AppState>,
    window: tauri::WebviewWindow,
) -> Result<serde_json::Value, String> {
    let db = state.get_db()?;
    let books = db.as_ref().unwrap().list_books().map_err(|e| e.to_string())?;

    if book_index == 0 || book_index > books.len() {
        return Err("无效的书籍序号".to_string());
    }

    let book = &books[book_index - 1];
    let annotations = db
        .as_ref()
        .unwrap()
        .get_annotations(&book.asset_id)
        .map_err(|e| e.to_string())?;

    // Get cached LLM results
    let cache = state.cache.lock().unwrap();
    let mut items: Vec<(String, String, String)> = Vec::new();
    for ann in annotations.iter() {
        if let Some(text) = &ann.selected_text {
            if !text.trim().is_empty() {
                if let Some(entry) = cache.get(&book.asset_id, text) {
                    if !entry.explanation.is_empty() {
                        items.push((
                            text.clone(),
                            entry.explanation.clone(),
                            book.title.clone(),
                        ));
                    }
                }
            }
        }
    }
    drop(cache);

    if items.is_empty() {
        return Err("没有已缓存的 LLM 结果，请先运行 AI 增强".to_string());
    }

    let card_style = CardStyle::from_str(&style);
    let output_path = PathBuf::from(&output_dir);
    std::fs::create_dir_all(&output_path).map_err(|e| e.to_string())?;

    let total = items.len();
    let mut success = 0u32;
    let mut failed = 0u32;

    window
        .emit(
            "progress",
            serde_json::json!({
                "current": 0, "total": total,
                "status": "generating", "message": "开始生成卡片..."
            }),
        )
        .ok();

    for (i, (highlight, explanation, title)) in items.iter().enumerate() {
        let filename = card_filename(highlight, i + 1);
        let file_path = output_path.join(&filename);

        match generate_card(highlight, Some(explanation), title, card_style, &file_path) {
            Ok(_) => success += 1,
            Err(_) => failed += 1,
        }

        window
            .emit(
                "progress",
                serde_json::json!({
                    "current": i + 1, "total": total,
                    "status": "generating",
                    "message": format!("生成第{}/{}张...", i + 1, total)
                }),
            )
            .ok();
    }

    window
        .emit(
            "complete",
            serde_json::json!({ "success": success, "failed": failed }),
        )
        .ok();

    Ok(
        serde_json::json!({ "success": success, "failed": failed, "output_dir": output_dir }),
    )
}
