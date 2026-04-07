pub mod app;
pub mod app_state;
pub mod build_config;
pub mod database;
pub mod domain;
pub mod error;
pub mod global_state;
pub mod infrastructure;
pub mod logger;
pub mod migration;
pub mod services;

use crate::app::setup;
use crate::error::AppResult;
use crate::global_state::*;

fn main() -> AppResult<()> {
    // Install rustls crypto provider explicitly because rumqttc is built with
    // `use-rustls-no-provider` and will panic if no provider is registered.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    setup::handle_global_shortcut(app, shortcut, event.state());
                })
                .build(),
        );

    let builder = builder.plugin(tauri_plugin_window_state::Builder::default().build());

    let app = builder
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_http::init())
        .setup(|app| {
            setup::init(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::window_manager::toggle_window_cmd,
            app::window_manager::hide_window_cmd,
            app::window_manager::activate_window_focus,
            app::window_manager::focus_clipboard_window,
            app::window_manager::set_navigation_enabled,
            app::window_manager::set_navigation_mode,
            app::hooks::set_recording_mode,
            services::content_handler::open_content,
            services::clipboard_ops::copy_to_clipboard,
            services::clipboard_ops::paste_latest_rich,
            app::commands::get_clipboard_history,
            app::commands::search_clipboard_history,
            app::commands::delete_clipboard_entry,
            app::commands::clear_clipboard_history,
            app::commands::get_tag_items,
            app::commands::get_all_tags_info,
            app::commands::rename_tag_globally,
            app::commands::delete_tag_from_all,
            app::commands::create_new_tag,
            app::commands::update_pinned_order,
            app::commands::get_db_count,
            app::commands::get_clipboard_content,
            app::commands::set_sequential_mode,
            app::commands::set_sequential_hotkey,
            app::commands::set_rich_paste_hotkey,
            app::commands::set_search_hotkey,
            app::commands::set_quick_paste_modifier,
            app::commands::set_deduplication,
            app::commands::save_setting,
            app::commands::set_ignore_blur,
            app::commands::set_window_pinned,
            app::commands::get_settings,
            app::commands::set_file_server_auto_close,
            app::commands::set_persistence,
            app::commands::set_capture_files,
            app::commands::set_capture_rich_text,
            app::commands::set_rich_text_snapshot_preview,
            app::commands::set_auto_copy_file,
            app::commands::set_silent_start,
            app::commands::set_delete_after_paste,
            app::commands::set_privacy_protection,
            app::commands::set_privacy_protection_kinds,
            app::commands::set_privacy_protection_custom_rules,
            app::commands::set_cleanup_rules,
            app::commands::set_app_cleanup_policies,
            app::commands::reset_settings,
            app::commands::get_mqtt_status,
            app::commands::get_mqtt_running,
            app::commands::restart_mqtt_client,
            app::commands::get_cloud_sync_status,
            app::commands::restart_cloud_sync_client,
            app::commands::request_cloud_sync,
            app::commands::stop_cloud_sync_client,
            app::commands::cloud_sync_now,
            app::commands::set_sound_enabled,
            app::commands::set_file_transfer_auto_open,
            app::commands::set_arrow_key_selection,
            app::commands::set_tray_visible,
            app::commands::set_dock_visible,
            app::commands::set_edge_docking,
            app::commands::get_data_path,
            app::commands::scan_installed_apps,
            app::commands::get_system_default_app,
            app::commands::get_associated_apps,
            app::commands::get_executable_icon,
            app::commands::get_file_icon,
            app::commands::open_folder,
            app::commands::open_data_folder,
            app::commands::open_file_with_default_app,
            app::commands::open_file_location,
            app::commands::set_data_path,
            app::commands::toggle_autostart,
            app::commands::is_autostart_enabled,
            app::commands::get_app_arch,
            app::commands::quit,
            app::commands::relaunch,
            app::commands::check_macos_permissions,
            app::commands::request_macos_permissions,
            app::commands::set_theme,
            app::commands::send_system_notification,
            app::commands::register_hotkey,
            app::commands::test_hotkey_available,
            app::commands::download_and_install_update,
            app::commands::toggle_clipboard_pin,
            app::commands::update_tags,
            app::commands::add_manual_item,
            app::commands::update_item_content,
            app::commands::save_emoji_favorite,
            app::commands::remove_emoji_favorite,
            app::commands::save_emoji_favorite_data_url,
            app::commands::save_emoji_favorite_url,
            app::commands::get_file_size,
            app::commands::save_file_copy,
            services::file_transfer::send_chat_message,
            services::file_transfer::get_chat_history,
            services::file_transfer::send_file_to_client,
            services::file_transfer::get_app_logo,
            services::file_transfer::get_local_ip_addr,
            services::file_transfer::get_available_ips,
            services::file_transfer::get_file_server_status,
            services::file_transfer::get_download_url,
            services::file_transfer::toggle_file_server,
            services::file_transfer::get_active_file_transfer_path,
            services::paste_queue::get_paste_queue,
            services::paste_queue::set_paste_queue,
            services::paste_queue::paste_next_step,
            app::commands::get_tag_colors,
            app::commands::set_tag_color,
            app::commands::call_ai,
            app::commands::check_ai_connectivity
        ])
        .on_window_event(|window, event| {
            setup::handle_window_event(window, event);
        })
        .build(tauri::generate_context!());

    match app {
        Ok(app) => {
            info!(">>> [STARTUP] Tauri app built successfully.");
            app.run(|app_handle, event| {
                #[cfg(target_os = "macos")]
                if let tauri::RunEvent::Reopen { .. } = event {
                    let _ = crate::app::window_manager::focus_clipboard_window(app_handle.clone());
                }
            });
            Ok(())
        }
        Err(e) => {
            error!(">>> [STARTUP] Failed to build tauri app: {}", e);
            Err(e.into())
        }
    }
}
