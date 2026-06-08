mod commands;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg(target_os = "macos")]
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};

// 编译时嵌入图标
const ICON_DATA: &[u8] = include_bytes!("../icons/icon.png");

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .setup(|app| {
            // 设置窗口图标
            let window = app.get_webview_window("main").unwrap();
            #[cfg(target_os = "macos")]
            apply_vibrancy(
                &window,
                NSVisualEffectMaterial::Sidebar,
                Some(NSVisualEffectState::FollowsWindowActiveState),
                Some(12.0),
            )
            .expect("failed to apply macOS vibrancy");

            let img = image::load_from_memory(ICON_DATA)
                .expect("failed to load icon")
                .to_rgba8();
            let (width, height) = img.dimensions();
            let icon = tauri::image::Image::new_owned(
                img.into_raw(),
                width,
                height,
            );
            window.set_icon(icon).expect("failed to set window icon");
            Ok(())
        })
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
            commands::generate_cards_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
