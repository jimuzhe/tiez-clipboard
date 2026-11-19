#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

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
#[cfg(test)]
mod upgrade_tests;

use crate::app::setup;
use crate::error::AppResult;
use crate::global_state::*;

fn main() -> AppResult<()> {
    // Install rustls crypto provider explicitly because rumqttc is built with
    // `use-rustls-no-provider` and will panic if no provider is registered.
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Elevated autostart helper: must run before Tauri/single-instance so a UAC
    // child can create/delete the logon task and exit without opening the UI.
    #[cfg(target_os = "windows")]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--install-elevated-autostart") {
            match crate::infrastructure::windows_api::elevated_autostart::install_elevated_autostart_task()
            {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!(">>> [ELEVATED-AUTOSTART] install failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        if crate::infrastructure::windows_api::elevated_autostart::hand_off_to_task(&args) {
            std::process::exit(0);
        }
        if args.iter().any(|a| a == "--uninstall-elevated-autostart") {
            match crate::infrastructure::windows_api::elevated_autostart::uninstall_elevated_autostart_task()
            {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!(">>> [ELEVATED-AUTOSTART] uninstall failed: {e}");
                    std::process::exit(1);
                }
            }
        }
    }

    let context = tauri::generate_context!();

    // A normal start while TieZ runs as administrator: the single-instance
    // plugin can't reach that instance (see single_instance.rs).
    #[cfg(target_os = "windows")]
    {
        let args: Vec<String> = std::env::args().collect();
        if crate::infrastructure::windows_api::single_instance::hand_off_to_elevated_instance(
            &context.config().identifier,
            &args,
        ) {
            std::process::exit(0);
        }
    }

    // Prefer overlay scrollbars (no classic Win32 arrows) in WebView2.
    #[cfg(target_os = "windows")]
    {
        const OVERLAY_ARGS: &str =
            "--enable-features=OverlayScrollbar,FluentOverlayScrollbar";
        match std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
            Ok(existing) if !existing.is_empty() => {
                if !existing.contains("OverlayScrollbar") {
                    std::env::set_var(
                        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
                        format!("{existing} {OVERLAY_ARGS}"),
                    );
                }
            }
            _ => std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", OVERLAY_ARGS),
        }
    }

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    setup::handle_global_shortcut(app, shortcut, event.state());
                })
                .build(),
        );

    // Only the main window's geometry is remembered. The plugin otherwise
    // calls show() on any window it has no saved state for, so the hidden
    // compact preview popped up the first time it was created; and restoring
    // "visible" would fight silent start. The app shows windows itself.
    let builder = builder.plugin(
        tauri_plugin_window_state::Builder::default()
            .with_state_flags(
                tauri_plugin_window_state::StateFlags::all()
                    - tauri_plugin_window_state::StateFlags::VISIBLE,
            )
            .with_filter(|label| label == "main")
            .build(),
    );

    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    let app = builder
        // Launching TieZ again brings the running instance forward: the way
        // back when the tray icon is hidden and the hotkey is forgotten.
        // Off the callback thread: on Windows it runs inside the second
        // process's WM_COPYDATA SendMessage, where rebuilding the WebView2
        // after light mode could fail or keep that process waiting.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // A login-item start (--minimized / --autostart) is not the user
            // asking for the window. On macOS, registering the login item
            // starts one right away while TieZ is already running.
            if args.iter().any(|arg| arg == "--minimized" || arg == "--autostart") {
                return;
            }
            let app = app.clone();
            std::thread::spawn(move || {
                let _ = app::window_manager::focus_clipboard_window(app);
            });
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            setup::init(app)?;
            #[cfg(target_os = "windows")]
            crate::infrastructure::windows_api::single_instance::accept_show_from_normal_starts(
                app.handle(),
                &app.config().identifier,
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::window_manager::toggle_window_cmd,
            app::window_manager::hide_window_cmd,
            app::window_manager::activate_window_focus,
            app::window_manager::activate_app_focus,
            app::window_manager::focus_clipboard_window,
            app::window_manager::main_ui_ready,
            app::window_manager::main_ui_released,
            services::e2e::e2e_key_fingerprint,
            services::e2e::generate_e2e_key,
            app::window_manager::disable_webview_browser_accelerators,
            app::window_manager::set_navigation_enabled,
            app::window_manager::set_navigation_mode,
            app::window_manager::set_search_top_hit,
            app::hooks::set_recording_mode,
            services::content_handler::open_content,
            services::clipboard_ops::copy_to_clipboard,
            app::commands::drag_cmd::start_native_drag,
            services::clipboard_ops::copy_private_text,
            services::clipboard_ops::paste_text_directly,
            services::clipboard_ops::paste_content_transiently,
            app::commands::discover_clipboard_import_sources,
            app::commands::get_clipboard_history,
            app::commands::read_text_file_preview,
            app::commands::import_clipboard_data,
            app::commands::export_secure_transfer,
            app::commands::import_secure_transfer,
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
            app::commands::get_clipboard_content_with_html,
            app::commands::set_sequential_mode,
            app::commands::set_sequential_hotkey,
            app::commands::set_plain_paste_hotkey,
            app::commands::set_search_hotkey,
            app::commands::set_quick_paste_modifier,
            app::commands::set_deduplication,
            app::commands::save_setting,
            app::commands::set_ignore_blur,
            app::commands::acquire_blur_guard,
            app::commands::release_blur_guard,
            app::commands::set_window_pinned,
            app::commands::get_settings,
            app::commands::get_undecryptable_settings,
            app::commands::clear_undecryptable_settings,
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
            app::commands::set_paste_sound_enabled,
            app::commands::play_preview_sound,
            app::commands::set_file_transfer_auto_open,
            app::commands::set_arrow_key_selection,
            app::commands::set_tray_visible,
            app::commands::set_dock_visible,
            app::commands::set_edge_docking,
            app::commands::set_follow_mouse,
            app::commands::get_data_path,
            app::commands::get_ping_id,
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
            app::commands::toggle_elevated_autostart,
            app::commands::is_elevated_autostart_enabled,
            app::commands::get_app_arch,
            app::commands::get_platform_info,
            app::commands::configure_compact_preview_window,
            app::commands::set_windows_clipboard_history,
            app::commands::get_windows_clipboard_history,
            app::commands::set_win_clipboard_disabled,
            app::commands::trigger_registry_win_v_optimization,
            app::commands::is_registry_win_v_optimized,
            app::commands::restart_explorer,
            app::commands::restart_as_admin,
            app::commands::check_is_admin,
            app::commands::quit,
            app::commands::relaunch,
            app::commands::check_macos_permissions,
            app::commands::request_macos_permissions,
            app::commands::get_macos_accessibility_binary_path,
            app::commands::open_macos_accessibility_settings,
            app::commands::set_theme,
            app::commands::install_theme_package,
            app::commands::install_theme_from_store,
            app::commands::list_installed_themes,
            app::commands::get_installed_theme_css,
            app::commands::uninstall_theme_package,
            app::commands::send_system_notification,
            app::commands::register_hotkey,
            app::commands::test_hotkey_available,
            app::commands::get_release_build_info,
            app::commands::check_beta_update,
            app::commands::toggle_clipboard_pin,
            app::commands::update_tags,
            app::commands::update_entry_note,
            app::commands::add_manual_item,
            app::commands::update_item_content,
            app::commands::save_emoji_favorite,
            app::commands::remove_emoji_favorite,
            app::commands::save_emoji_favorite_data_url,
            app::commands::save_emoji_favorite_url,
            app::commands::list_emoji_favorites,
            app::commands::get_file_size,
            app::commands::get_image_thumbnail,
            app::commands::get_video_thumbnail,
            app::commands::save_file_copy,
            app::commands::save_file_batch,
            services::file_transfer::send_chat_message,
            services::file_transfer::get_chat_history,
            services::file_transfer::get_online_devices,
            services::file_transfer::clear_chat_history,
            services::file_transfer::open_file_transfer_folder,
            services::file_transfer::send_file_to_client,
            services::file_transfer::send_files_to_client,
            services::file_transfer::save_temp_image,
            services::file_transfer::expand_transfer_paths,
            services::file_transfer::get_local_ip_addr,
            services::file_transfer::get_available_ips,
            services::file_transfer::get_available_network_interfaces,
            services::file_transfer::set_display_ip,
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
            app::commands::apply_ai_result,
            app::commands::check_ai_connectivity
        ])
        .on_window_event(|window, event| {
            setup::handle_window_event(window, event);
        })
        .build(context);

    match app {
        Ok(app) => {
            info!(">>> [STARTUP] Tauri app built successfully.");
            app.run(|app_handle, event| {
                if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                    if !crate::app::light_mode::ALLOW_APP_EXIT.load(std::sync::atomic::Ordering::SeqCst)
                    {
                        // Keep tray + clipboard daemon alive after WebView light-mode destroy.
                        api.prevent_exit();
                    }
                }
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
