mod commands;
mod state;

use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::check_db_access,
            commands::get_books,
            commands::get_annotations,
            commands::load_app_config,
            commands::save_app_config,
            commands::get_cache_stats,
            commands::clear_cache_for_book,
            commands::export_book_cmd,
            commands::enrich_book_cmd,
            commands::test_llm_connection,
            commands::open_system_settings,
            commands::get_cache_entries,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
