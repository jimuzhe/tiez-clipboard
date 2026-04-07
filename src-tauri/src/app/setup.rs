use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{App, AppHandle, Emitter, Manager};

use crate::app::window_manager::{maybe_open_devtools, toggle_window};
#[cfg(target_os = "windows")]
use crate::app::window_manager::{release_modifier_keys, restore_previous_app_focus};
use crate::app_state::{AppDataDir, EncryptionQueueState, PasteQueue, SessionHistory, SettingsState};
use crate::database::{self, DbState};
use crate::global_state::*;
use crate::info;
use crate::infrastructure::repository::clipboard_repo::SqliteClipboardRepository;
use crate::infrastructure::repository::settings_repo::{
    SettingsRepository, SqliteSettingsRepository,
};
use crate::infrastructure::repository::tag_repo::SqliteTagRepository;
use crate::services::encryption_queue::init_encryption_queue;

static WINDOW_SIZE_SAVE_PENDING: AtomicBool = AtomicBool::new(false);
static LAST_WINDOW_SIZE_EVENT_MS: AtomicU64 = AtomicU64::new(0);
static LAST_WINDOW_SIZE: OnceLock<Mutex<(u32, u32)>> = OnceLock::new();

pub fn init(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let app_handle = app.handle().clone();

    // Initialize GLOBAL_APP_HANDLE
    let _ = GLOBAL_APP_HANDLE.set(app_handle.clone());

    // 1. Data Directory & Migration
    let app_dir = resolve_data_dir(app)?;

    // 2. Logger Initialization
    crate::logger::init(app_dir.join("tiez.log"));
    info!(">>> [STARTUP] TieZ starting up...");

    // 3. Database Initialization
    let db_path = app_dir.join("clipboard.db");
    let db_path_str = db_path.to_string_lossy();
    let conn = database::init_db(&db_path_str).map_err(|e| {
        let err_msg = format!("数据库初始化失败: {}", e);
        eprintln!("TieZ Startup Error: {}", err_msg);
        e
    })?;
    let conn_arc = std::sync::Arc::new(std::sync::Mutex::new(conn));
    let settings_repo = SqliteSettingsRepository::new(conn_arc.clone());

    // 4. Initial Settings & Reset Safety
    apply_startup_resets(&settings_repo);

    #[cfg(target_os = "macos")]
    {
        // Avoid stale persisted pin state causing unexpected always-on-top behavior.
        let _ = settings_repo.set("app.window_pinned", "false");
    }

    let settings = load_settings(&settings_repo);

    // 5. App State Management
    setup_state(app, conn_arc.clone(), &settings, app_dir.clone());
    app.manage(EncryptionQueueState(init_encryption_queue(app_handle.clone())));
    // spawn_sensitive_alignment(app_handle.clone());

    // 6. App Visibility
    apply_initial_dock_visibility(app, &settings);

    // 7. Window Initialization (Pinned/Focus)
    setup_main_window(app, &settings);

    // External Drag-Drop (Web Images) Mac OS drag-drop handling will go here if needed
    // 8. Background Services & Monitors
    start_services(app, &settings, app_handle.clone());

    // 9. Tray Setup
    setup_tray(app, settings.hide_tray_icon);

    // 10. Theme Initial Application
    apply_initial_theme(app);

    Ok(())
}

fn resolve_data_dir(app: &App) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let default_app_dir = app.path().app_data_dir()?;

    // Perform migration if needed
    crate::migration::perform_migration_v028(&default_app_dir);

    // Cleanup temp files
    std::thread::spawn(|| {
        let temp_dir = std::env::temp_dir();
        if let Ok(entries) = std::fs::read_dir(&temp_dir) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.starts_with("TieZ_Clip_") {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    });

    let redirect_file = default_app_dir.join("datapath.txt");
    let mut app_dir = if redirect_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&redirect_file) {
            let custom_path = content.trim();
            if custom_path.is_empty() {
                default_app_dir.clone()
            } else {
                let custom_path_obj = std::path::Path::new(custom_path);
                let is_app_bundle = custom_path.to_ascii_lowercase().ends_with(".app");
                if custom_path_obj.exists() && custom_path_obj.is_dir() && !is_app_bundle {
                    std::path::PathBuf::from(custom_path)
                } else {
                    default_app_dir.clone()
                }
            }
        } else {
            default_app_dir.clone()
        }
    } else {
        default_app_dir.clone()
    };

    // Portable mode check
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let portable_data = exe_dir.join("data");
            if portable_data.exists() && portable_data.is_dir() {
                app_dir = portable_data;
            }
        }
    }

    std::fs::create_dir_all(&app_dir)?;
    Ok(app_dir)
}

fn apply_startup_resets(repo: &impl SettingsRepository) {
    #[cfg(target_os = "macos")]
    {
        fn migrate_hotkey_default(
            repo: &impl SettingsRepository,
            key: &str,
            legacy_defaults: &[&str],
            new_default: &str,
        ) {
            let current = match repo.get(key) {
                Ok(Some(v)) => v,
                _ => return,
            };
            let normalized = current.trim().to_ascii_lowercase();
            if legacy_defaults
                .iter()
                .any(|legacy| normalized == legacy.trim().to_ascii_lowercase())
            {
                let _ = repo.set(key, new_default);
            }
        }

        // Migrate only historical defaults to new mac-friendly defaults.
        // User-customized values are preserved.
        migrate_hotkey_default(
            repo,
            "app.hotkey",
            &["Win+V", "Command+Shift+C", "Command+V"],
            "Alt+C",
        );
        migrate_hotkey_default(
            repo,
            "app.sequential_hotkey",
            &["Command+V", "Command+Alt+V", "Command+Z"],
            "Alt+V",
        );
        migrate_hotkey_default(
            repo,
            "app.rich_paste_hotkey",
            &["Ctrl+Shift+Z", "Command+Alt+Shift+V", "Command+Shift+V"],
            "Alt+Shift+V",
        );
        migrate_hotkey_default(
            repo,
            "app.search_hotkey",
            &["Command+F", "Command+Alt+F"],
            "Alt+F",
        );
    }
}

pub struct StartupSettings {
    pub theme: String,
    pub persistent: bool,
    pub capture_files: bool,
    pub capture_rich_text: bool,
    pub rich_text_snapshot_preview: bool,
    pub deduplicate: bool,
    pub auto_copy_file: bool,
    pub silent_start: bool,
    pub delete_after_paste: bool,
    pub privacy_protection: bool,
    pub privacy_kinds: String,
    pub privacy_custom: String,
    pub cleanup_rules: String,
    pub app_cleanup_policies: String,
    pub sequential_mode: bool,
    pub sequential_hotkey: String,
    pub rich_paste_hotkey: String,
    pub search_hotkey: String,
    pub quick_paste_modifier: String,
    pub sound_enabled: bool,
    pub paste_sound_enabled: bool,
    pub hide_tray_icon: bool,
    pub hide_dock_icon: bool,
    pub edge_docking: bool,
    pub window_pinned: bool,
    pub window_width: Option<u32>,
    pub window_height: Option<u32>,
    pub main_hotkey: String,
    pub arrow_key_selection: bool,
    pub auto_close_server: bool,
    pub sound_volume: f64,
}

fn load_settings(repo: &impl SettingsRepository) -> StartupSettings {
    StartupSettings {
        theme: repo
            .get("app.theme")
            .unwrap_or(Some("mica".to_string()))
            .unwrap_or("mica".to_string()),
        persistent: repo
            .get("app.persistent")
            .unwrap_or(Some("true".to_string()))
            .map(|v| v == "true")
            .unwrap_or(true),
        capture_files: repo
            .get("app.capture_files")
            .unwrap_or(Some("true".to_string()))
            .map(|v| v == "true")
            .unwrap_or(true),
        capture_rich_text: repo
            .get("app.capture_rich_text")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        rich_text_snapshot_preview: repo
            .get("app.rich_text_snapshot_preview")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        deduplicate: repo
            .get("app.deduplicate")
            .unwrap_or(Some("true".to_string()))
            .map(|v| v == "true")
            .unwrap_or(true),
        auto_copy_file: repo
            .get("file_transfer_auto_copy")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        silent_start: repo
            .get("app.silent_start")
            .unwrap_or(Some("true".to_string()))
            .map(|v| v == "true")
            .unwrap_or(true),
        delete_after_paste: repo
            .get("app.delete_after_paste")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        privacy_protection: repo
            .get("app.privacy_protection")
            .unwrap_or(Some("true".to_string()))
            .map(|v| v == "true")
            .unwrap_or(true),
        privacy_kinds: repo
            .get("app.privacy_protection_kinds")
            .unwrap_or(Some("phone,idcard,email,secret".to_string()))
            .unwrap_or("phone,idcard,email,secret".to_string()),
        privacy_custom: repo
            .get("app.privacy_protection_custom_rules")
            .unwrap_or(Some("".to_string()))
            .unwrap_or("".to_string()),
        cleanup_rules: repo
            .get("app.cleanup_rules")
            .unwrap_or(Some("".to_string()))
            .unwrap_or("".to_string()),
        app_cleanup_policies: repo
            .get("app.app_cleanup_policies")
            .unwrap_or(Some("[]".to_string()))
            .unwrap_or("[]".to_string()),
        sequential_mode: repo
            .get("app.sequential_mode")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        sequential_hotkey: repo
            .get("app.sequential_hotkey")
            .unwrap_or(Some("Alt+V".to_string()))
            .unwrap_or("Alt+V".to_string()),
        rich_paste_hotkey: repo
            .get("app.rich_paste_hotkey")
            .unwrap_or(Some("Alt+Shift+V".to_string()))
            .unwrap_or("Alt+Shift+V".to_string()),
        search_hotkey: repo
            .get("app.search_hotkey")
            .unwrap_or(Some("Alt+F".to_string()))
            .unwrap_or("Alt+F".to_string()),
        quick_paste_modifier: repo
            .get("app.quick_paste_modifier")
            .unwrap_or(Some("disabled".to_string()))
            .unwrap_or("disabled".to_string()),
        sound_enabled: repo
            .get("app.sound_enabled")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        paste_sound_enabled: repo
            .get("app.sound_paste_enabled")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        hide_tray_icon: repo
            .get("app.hide_tray_icon")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        hide_dock_icon: repo
            .get("app.hide_dock_icon")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        edge_docking: repo
            .get("app.edge_docking")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        window_pinned: repo
            .get("app.window_pinned")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        window_width: repo
            .get("app.window_width")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u32>().ok()),
        window_height: repo
            .get("app.window_height")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u32>().ok()),
        main_hotkey: repo
            .get("app.hotkey")
            .unwrap_or(Some("Alt+C".to_string()))
            .unwrap_or("Alt+C".to_string()),
        arrow_key_selection: repo
            .get("app.arrow_key_selection")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        auto_close_server: repo
            .get("file_transfer_auto_close")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        sound_volume: repo
            .get("app.sound_volume")
            .unwrap_or(Some("1.0".to_string()))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1.0),
    }
}

fn setup_state(
    app: &App,
    conn_arc: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    s: &StartupSettings,
    app_dir: std::path::PathBuf,
) {
    let repo = SqliteClipboardRepository::new(conn_arc.clone());
    let settings_repo = SqliteSettingsRepository::new(conn_arc.clone());
    let tag_repo = SqliteTagRepository::new(conn_arc.clone());
    app.manage(DbState {
        conn: conn_arc,
        repo,
        settings_repo,
        tag_repo,
    });

    app.manage(SettingsState {
        deduplicate: AtomicBool::new(s.deduplicate),
        persistent: AtomicBool::new(s.persistent),
        file_server_auto_close: AtomicBool::new(s.auto_close_server),
        theme: std::sync::Mutex::new(s.theme.clone()),
        capture_files: AtomicBool::new(s.capture_files),
        capture_rich_text: AtomicBool::new(s.capture_rich_text),
        rich_text_snapshot_preview: AtomicBool::new(s.rich_text_snapshot_preview),
        auto_copy_file: AtomicBool::new(s.auto_copy_file),
        silent_start: AtomicBool::new(s.silent_start),
        delete_after_paste: AtomicBool::new(s.delete_after_paste),
        privacy_protection: AtomicBool::new(s.privacy_protection),
        privacy_protection_kinds: std::sync::Mutex::new(
            s.privacy_kinds
                .split(',')
                .map(|x| x.trim().to_string())
                .collect(),
        ),
        privacy_protection_custom_rules: std::sync::Mutex::new(
            s.privacy_custom
                .lines()
                .map(|x| x.trim().to_string())
                .collect(),
        ),
        cleanup_rules: std::sync::Mutex::new(s.cleanup_rules.clone()),
        app_cleanup_policies: std::sync::Mutex::new(s.app_cleanup_policies.clone()),
        sequential_mode: AtomicBool::new(s.sequential_mode),
        sequential_paste_hotkey: std::sync::Mutex::new(s.sequential_hotkey.clone()),
        rich_paste_hotkey: std::sync::Mutex::new(s.rich_paste_hotkey.clone()),
        search_hotkey: std::sync::Mutex::new(s.search_hotkey.clone()),
        quick_paste_modifier: std::sync::Mutex::new(s.quick_paste_modifier.clone()),
        sound_enabled: AtomicBool::new(s.sound_enabled),
        paste_sound_enabled: AtomicBool::new(s.paste_sound_enabled),
        hide_tray_icon: AtomicBool::new(s.hide_tray_icon),
        hide_dock_icon: AtomicBool::new(s.hide_dock_icon),
        edge_docking: AtomicBool::new(s.edge_docking),
        arrow_key_selection: AtomicBool::new(s.arrow_key_selection),
        main_hotkey: std::sync::Mutex::new(s.main_hotkey.clone()),
        monitors: std::sync::Mutex::new(Vec::new()),
        sound_volume: std::sync::Mutex::new(s.sound_volume),
    });

    app.manage(SessionHistory(std::sync::Mutex::new(
        std::collections::VecDeque::new(),
    )));
    app.manage(AppDataDir(std::sync::Mutex::new(app_dir)));
    app.manage(crate::services::file_transfer::ChatState::default());
    app.manage(crate::services::file_transfer::SharedFileState(
        std::sync::Mutex::new(std::collections::HashMap::new()),
    ));
    app.manage(crate::services::file_transfer::ServerInfo {
        port: std::sync::atomic::AtomicU16::new(0),
        ip: std::sync::Mutex::new(String::new()),
    });
    app.manage(crate::services::file_transfer::UploadSessions::default());
    app.manage(crate::services::file_transfer::ServerActivityState::default());
    app.manage(crate::services::file_transfer::WsBroadcaster(
        std::sync::Mutex::new(None),
    ));
    app.manage(crate::services::file_transfer::OnlineDevices(
        std::sync::Mutex::new(std::collections::HashMap::new()),
    ));
    app.manage(PasteQueue::default());
}

#[cfg(target_os = "macos")]
fn apply_initial_dock_visibility(app: &mut App, s: &StartupSettings) {
    let _ = app.set_dock_visibility(!s.hide_dock_icon);
}

#[cfg(not(target_os = "macos"))]
fn apply_initial_dock_visibility(_app: &mut App, _s: &StartupSettings) {}

fn setup_main_window(app: &App, s: &StartupSettings) {
    let effective_pinned = s.window_pinned;
    WINDOW_PINNED.store(effective_pinned, Ordering::Relaxed);

    if let Some(window) = app.get_webview_window("main") {
        if let (Some(w), Some(h)) = (s.window_width, s.window_height) {
            if w >= 360 && h >= 240 {
                let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                    width: w,
                    height: h,
                }));
            }
        }
        let _ = window.set_always_on_top(effective_pinned);
        #[cfg(target_os = "windows")]
        let _ = window.set_focusable(!effective_pinned);
        #[cfg(not(target_os = "windows"))]
        {
            let _ = window.set_focusable(true);
        }

        // macOS: focusing/non-focusing window handling is different.
        // For now, relying on tauri's standard focusable property.
    }

    // Handle silent start
    let args: Vec<String> = std::env::args().collect();
    let is_autostart =
        args.contains(&"--autostart".to_string()) || args.contains(&"--minimized".to_string());

    #[cfg(target_os = "windows")]
    let should_show_main = !is_autostart && !s.silent_start;
    #[cfg(not(target_os = "windows"))]
    let should_show_main = !is_autostart;

    if should_show_main {
        if let Some(window) = app.get_webview_window("main") {
            #[cfg(not(target_os = "windows"))]
            let _ = window.set_focusable(true);
            let _ = window.show();
            #[cfg(not(target_os = "windows"))]
            let _ = window.set_focus();
            maybe_open_devtools(&window);
        }
    } else {
        // Not showing on startup, but ensure window is focusable when it does appear
        #[cfg(not(target_os = "windows"))]
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_focusable(true);
        }
    }
}

fn start_services(app: &App, s: &StartupSettings, app_handle: AppHandle) {
    crate::infrastructure::macos_api::window_tracker::start_window_tracking(app_handle.clone());
    crate::services::clipboard::start_clipboard_monitor(app_handle.clone());
    // crate::services::mqtt_sub::start_mqtt_client(app_handle.clone());
    crate::services::cloud_sync::start_cloud_sync_client(app_handle.clone());
    start_edge_docking_monitor(app_handle.clone());

    let db_state = app.state::<DbState>();
    if db_state
        .settings_repo
        .get("file_server_enabled")
        .unwrap_or(Some("false".to_string()))
        == Some("true".to_string())
    {
        let port = db_state
            .settings_repo
            .get("file_server_port")
            .unwrap_or(None)
            .and_then(|x| x.parse::<u16>().ok());

        let h = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::services::file_transfer::toggle_file_server(h, true, port).await;
        });
    }

    // Anonymous Analytics
    init_analytics(app, &db_state.settings_repo);

    // Register initial hotkey
    let hotkey_str = s.main_hotkey.clone();
    {
        let mut guard = HOTKEY_STRING.lock().unwrap();
        *guard = hotkey_str.clone();
    }
    let _ = crate::app::commands::sync_registered_hotkeys(&app_handle);
}

#[cfg(target_os = "macos")]
fn start_edge_docking_monitor(app_handle: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(150));

            let settings = match app_handle.try_state::<SettingsState>() {
                Some(s) => s,
                None => continue,
            };

            if !settings.edge_docking.load(Ordering::Relaxed) {
                if IS_HIDDEN.load(Ordering::Relaxed) {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        if let (Ok(pos), Ok(size), Ok(Some(monitor))) = (
                            window.outer_position().or_else(|_| window.inner_position()),
                            window.outer_size().or_else(|_| window.inner_size()),
                            window.current_monitor(),
                        ) {
                            let screen = monitor.position();
                            let screen_right = screen.x + monitor.size().width as i32;
                            let dock_actual = match CURRENT_DOCK.load(Ordering::Relaxed) {
                                1 => DockPosition::Top,
                                2 => DockPosition::Left,
                                3 => DockPosition::Right,
                                _ => DockPosition::None,
                            };
                            match dock_actual {
                                DockPosition::Top => {
                                    let _ = window.set_position(tauri::PhysicalPosition::new(
                                        pos.x, screen.y,
                                    ));
                                }
                                DockPosition::Left => {
                                    let _ = window.set_position(tauri::PhysicalPosition::new(
                                        screen.x, pos.y,
                                    ));
                                }
                                DockPosition::Right => {
                                    let _ = window.set_position(tauri::PhysicalPosition::new(
                                        screen_right - size.width as i32,
                                        pos.y,
                                    ));
                                }
                                DockPosition::None => {}
                            }
                        }
                        let _ = window.show();
                        IS_HIDDEN.store(false, Ordering::Relaxed);
                        CURRENT_DOCK.store(0, Ordering::Relaxed);
                    }
                }
                continue;
            }

            let Some(window) = app_handle.get_webview_window("main") else {
                continue;
            };

            if window.is_minimized().unwrap_or(false) {
                continue;
            }

            let is_window_visible = window.is_visible().unwrap_or(true);
            let is_hidden_by_edge = IS_HIDDEN.load(Ordering::Relaxed);

            // Skip edge docking checks if window was hidden by other mechanisms.
            if !is_window_visible && !is_hidden_by_edge {
                continue;
            }

            let last_show = LAST_SHOW_TIMESTAMP.load(Ordering::Relaxed);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            // Avoid immediate re-dock right after showing.
            if !is_hidden_by_edge
                && NAVIGATION_ENABLED.load(Ordering::SeqCst)
                && now.saturating_sub(last_show) < 800
            {
                continue;
            }

            if now.saturating_sub(last_show) < 500 {
                continue;
            }

            let pos = match window.outer_position().or_else(|_| window.inner_position()) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let size = match window.outer_size().or_else(|_| window.inner_size()) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let rect_left = pos.x;
            let rect_top = pos.y;
            let rect_right = pos.x.saturating_add(size.width as i32);
            let rect_bottom = pos.y.saturating_add(size.height as i32);

            let cursor = match window.cursor_position() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let cursor_x = cursor.x.round() as i32;
            let cursor_y = cursor.y.round() as i32;

            let monitor = match window.current_monitor() {
                Ok(Some(m)) => m,
                _ => continue,
            };
            let screen_size = monitor.size();
            let screen_pos = monitor.position();
            let screen_left = screen_pos.x;
            let screen_top = screen_pos.y;
            let screen_right = screen_pos.x + screen_size.width as i32;
            let screen_bottom = screen_pos.y + screen_size.height as i32;

            let threshold = 5;
            let is_mouse_near_edge = if is_hidden_by_edge {
                match CURRENT_DOCK.load(Ordering::Relaxed) {
                    1 => {
                        cursor_y <= screen_top + threshold
                            && cursor_x >= rect_left
                            && cursor_x <= rect_right
                    }
                    2 => {
                        cursor_x <= screen_left + threshold
                            && cursor_y >= rect_top
                            && cursor_y <= rect_bottom
                    }
                    3 => {
                        cursor_x >= screen_right - threshold
                            && cursor_y >= rect_top
                            && cursor_y <= rect_bottom
                    }
                    _ => false,
                }
            } else {
                false
            };

            let is_mouse_in = if is_hidden_by_edge {
                is_mouse_near_edge
            } else {
                cursor_x >= rect_left
                    && cursor_x <= rect_right
                    && cursor_y >= rect_top
                    && cursor_y <= rect_bottom
            };

            let window_center_x = (rect_left + rect_right) / 2;
            let window_center_y = (rect_top + rect_bottom) / 2;
            let is_on_current_monitor = window_center_x >= screen_left
                && window_center_x < screen_right
                && window_center_y >= screen_top
                && window_center_y < screen_bottom;

            if !is_hidden_by_edge && !is_on_current_monitor {
                if IS_HIDDEN.load(Ordering::Relaxed) {
                    IS_HIDDEN.store(false, Ordering::Relaxed);
                    CURRENT_DOCK.store(0, Ordering::Relaxed);
                }
                continue;
            }

            let hide_size = 3;
            let mut dock = DockPosition::None;
            if rect_top <= screen_top + threshold {
                dock = DockPosition::Top;
            } else if rect_left <= screen_left + threshold {
                dock = DockPosition::Left;
            } else if rect_right >= screen_right - threshold {
                dock = DockPosition::Right;
            }

            if is_hidden_by_edge {
                if is_mouse_in {
                    let dock_actual = match CURRENT_DOCK.load(Ordering::Relaxed) {
                        1 => DockPosition::Top,
                        2 => DockPosition::Left,
                        3 => DockPosition::Right,
                        _ => DockPosition::None,
                    };

                    if dock_actual != DockPosition::None {
                        let _ = window.show();
                        match dock_actual {
                            DockPosition::Top => {
                                let _ = window.set_position(tauri::Position::Physical(
                                    tauri::PhysicalPosition {
                                        x: rect_left,
                                        y: screen_top,
                                    },
                                ));
                            }
                            DockPosition::Left => {
                                let _ = window.set_position(tauri::Position::Physical(
                                    tauri::PhysicalPosition {
                                        x: screen_left,
                                        y: rect_top,
                                    },
                                ));
                            }
                            DockPosition::Right => {
                                let window_width = rect_right - rect_left;
                                let _ = window.set_position(tauri::Position::Physical(
                                    tauri::PhysicalPosition {
                                        x: screen_right - window_width,
                                        y: rect_top,
                                    },
                                ));
                            }
                            DockPosition::None => {}
                        }
                        IS_HIDDEN.store(false, Ordering::Relaxed);
                        CURRENT_DOCK.store(0, Ordering::Relaxed);
                    }
                }
            } else if dock != DockPosition::None {
                // Keep the window fully visible while cursor is still inside it.
                if is_mouse_in {
                    continue;
                }

                if !IS_HIDDEN.load(Ordering::Relaxed) {
                    // Auto-enable pin when docking occurs (runtime only, no DB write).
                    if !WINDOW_PINNED.load(Ordering::Relaxed) {
                        WINDOW_PINNED.store(true, Ordering::Relaxed);
                        let _ = window.set_always_on_top(true);
                        let _ = window.set_focusable(true);
                        let _ = app_handle.emit("window-pinned-changed", true);
                    }

                    let window_height = rect_bottom - rect_top;
                    let window_width = rect_right - rect_left;
                    match dock {
                        DockPosition::Top => {
                            let _ = window.set_position(tauri::PhysicalPosition::new(
                                rect_left,
                                screen_top - window_height + hide_size,
                            ));
                            CURRENT_DOCK.store(1, Ordering::Relaxed);
                        }
                        DockPosition::Left => {
                            let _ = window.set_position(tauri::PhysicalPosition::new(
                                screen_left - window_width + hide_size,
                                rect_top,
                            ));
                            CURRENT_DOCK.store(2, Ordering::Relaxed);
                        }
                        DockPosition::Right => {
                            let _ = window.set_position(tauri::PhysicalPosition::new(
                                screen_right - hide_size,
                                rect_top,
                            ));
                            CURRENT_DOCK.store(3, Ordering::Relaxed);
                        }
                        DockPosition::None => {}
                    }
                    IS_HIDDEN.store(true, Ordering::Relaxed);
                }
            } else if IS_HIDDEN.load(Ordering::Relaxed) {
                IS_HIDDEN.store(false, Ordering::Relaxed);
                CURRENT_DOCK.store(0, Ordering::Relaxed);

                // Restore pinned state based on user setting when undocked.
                let mut user_pinned = WINDOW_PINNED.load(Ordering::Relaxed);
                if let Some(db_state) = app_handle.try_state::<DbState>() {
                    if let Ok(val) = db_state.settings_repo.get("app.window_pinned") {
                        user_pinned = val.as_deref() == Some("true");
                    }
                }

                let prev = WINDOW_PINNED.swap(user_pinned, Ordering::Relaxed);
                if prev != user_pinned {
                    let _ = window.set_always_on_top(user_pinned);
                    let _ = window.set_focusable(true);
                    let _ = app_handle.emit("window-pinned-changed", user_pinned);
                }
            }
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn start_edge_docking_monitor(_app_handle: AppHandle) {}


fn init_analytics(app: &App, repo: &impl SettingsRepository) {
    let machine_id = crate::app::system::get_machine_id();
    let stored_anon_id = repo.get("app.anon_id").unwrap_or(None);
    let anon_id = stored_anon_id
        .as_deref()
        .and_then(crate::app::system::normalize_anon_id)
        .unwrap_or_else(|| crate::app::system::build_anon_id(&machine_id));
    if stored_anon_id
        .as_deref()
        .map(|value| value.trim() != anon_id)
        .unwrap_or(true)
    {
        let _ = repo.set("app.anon_id", &anon_id);
    }

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    if repo.get("app.last_ping_date").unwrap_or(None).as_deref() != Some(&today) {
        let _ = repo.set("app.last_ping_date", &today);
        let version = app.package_info().version.to_string();
        
        let ping_url_base = crate::build_config::announcement_ping_url();
        
        std::thread::spawn(move || {
            let _ = reqwest::blocking::get(format!(
                "{}?v={}&id={}",
                ping_url_base, version, anon_id
            ));
        });
    }
}

fn setup_tray(app: &App, hide_tray: bool) {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};

    let show_i = MenuItem::with_id(app, "show", "显示主界面", true, None::<&str>).unwrap();
    let quit_i = MenuItem::with_id(app, "quit", "退出 贴汁", true, None::<&str>).unwrap();
    let menu = Menu::with_items(app, &[&show_i, &quit_i]).unwrap();
    let icon =
        tauri::image::Image::from_bytes(include_bytes!("../../icons/tray-icon.png")).unwrap();

    let mut tray_builder = TrayIconBuilder::with_id("main_tray")
        .icon(icon)
        .tooltip("TieZ")
        .show_menu_on_left_click(false)
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id.as_ref() == "show" {
                if let Some(window) = app.get_webview_window("main") {
                    #[cfg(not(target_os = "windows"))]
                    let _ = window.set_focusable(true);
                    let _ = window.show();
                    let _ = window.set_focus();
                    maybe_open_devtools(&window);
                }
            } else if event.id.as_ref() == "quit" {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    #[cfg(not(target_os = "windows"))]
                    let _ = window.set_focusable(true);
                    let _ = window.show();
                    let _ = window.set_focus();
                    maybe_open_devtools(&window);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    LAST_SHOW_TIMESTAMP.store(now, Ordering::Relaxed);
                }
            }
        });

    #[cfg(target_os = "macos")]
    {
        tray_builder = tray_builder.icon_as_template(true);
    }

    let tray = tray_builder.build(app).expect("Failed to build tray");

    let _ = tray.set_visible(!hide_tray);
    app.manage(tray);
}

fn apply_initial_theme(app: &App) {
    let db_state = app.state::<DbState>();
    let theme = db_state
        .settings_repo
        .get("app.theme")
        .unwrap_or(Some("mica".to_string()))
        .unwrap_or("mica".to_string());
    let mode = db_state
        .settings_repo
        .get("app.color_mode")
        .unwrap_or(Some("system".to_string()));

    if let Some(window) = app.get_webview_window("main") {
        let _ = crate::app::commands::set_theme(
            window,
            app.state::<SettingsState>(),
            db_state,
            theme,
            mode,
        );
    }
}

pub fn handle_global_shortcut(
    app: &AppHandle,
    shortcut: &tauri_plugin_global_shortcut::Shortcut,
    state: tauri_plugin_global_shortcut::ShortcutState,
) {
    use tauri_plugin_global_shortcut::Shortcut;
    use tauri_plugin_global_shortcut::ShortcutState;
    let settings = app.state::<SettingsState>();

    if state == ShortcutState::Pressed {
        if let Ok(main_s) = {
            let val = settings.main_hotkey.lock().unwrap().clone();
            val.replace("Win", "Super").parse::<Shortcut>()
        } {
            if shortcut == &main_s {
                toggle_window(app);
                return;
            }
        }

        if let Ok(search_s) = {
            let val = settings.search_hotkey.lock().unwrap().clone();
            val.replace("Win", "Super").parse::<Shortcut>()
        } {
            if shortcut == &search_s {
                toggle_window(app);
                let _ = app.emit("focus-search-input", ());
            }
        }
    } else if state == ShortcutState::Released {
        if let Ok(seq_s) = {
            let val = settings.sequential_paste_hotkey.lock().unwrap().clone();
            val.replace("Win", "Super").parse::<Shortcut>()
        } {
            if shortcut == &seq_s {
                let is_seq = settings.sequential_mode.load(Ordering::Relaxed);
                let has_items = {
                    let q_notification = app.state::<PasteQueue>().inner().0.lock().unwrap();
                    !q_notification.items.is_empty()
                };
                if is_seq || has_items {
                    crate::services::paste_queue::paste_next_step(app.clone());
                }
            }
        }

        if let Ok(rich_s) = {
            let val = settings.rich_paste_hotkey.lock().unwrap().clone();
            val.replace("Win", "Super").parse::<Shortcut>()
        } {
            if shortcut == &rich_s {
                crate::services::clipboard_ops::paste_latest_rich(app.clone());
                return;
            }
        }

        let quick_paste_modifier = settings.quick_paste_modifier.lock().unwrap().clone();
        if let Some(index) =
            crate::app::commands::quick_paste_index_from_shortcut(&quick_paste_modifier, shortcut)
        {
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ =
                    crate::services::clipboard_ops::paste_history_item_by_index(app_handle, index)
                        .await;
            });
            return;
        }
    }
}

pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    match event {
        tauri::WindowEvent::Focused(focused) => {
            if window.label() != "main" {
                return;
            }
            IS_MAIN_WINDOW_FOCUSED.store(*focused, Ordering::Relaxed);
            if !*focused {
                handle_blur(window);
            }
        }
        tauri::WindowEvent::Resized(size) => {
            if window.label() != "main" {
                return;
            }

            // Avoid querying `is_maximized` on macOS in resize callback.
            // On Tao/WebKit this can trigger style-mask sync and cause resize-event storms.
            if window.is_minimized().unwrap_or(false) {
                return;
            }

            #[cfg(target_os = "windows")]
            if window.is_maximized().unwrap_or(false) {
                return;
            }

            persist_window_size(window, size.width, size.height);
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            if window.label() != "main" {
                return;
            }
            api.prevent_close();
            let _ = window.app_handle().emit("force-hide-compact-preview", ());
            let _ = window.hide();
            NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
            NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
        }
        _ => {}
    }
}

fn persist_window_size(window: &tauri::Window, width: u32, height: u32) {
    if width < 200 || height < 200 {
        return;
    }

    let store = LAST_WINDOW_SIZE.get_or_init(|| Mutex::new((0, 0)));
    {
        let mut guard = store.lock().unwrap();
        *guard = (width, height);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    LAST_WINDOW_SIZE_EVENT_MS.store(now, Ordering::Relaxed);

    if WINDOW_SIZE_SAVE_PENDING.swap(true, Ordering::SeqCst) {
        return;
    }

    let app_handle = window.app_handle().clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let last_event = LAST_WINDOW_SIZE_EVENT_MS.load(Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        if now.saturating_sub(last_event) < 200 {
            continue;
        }

        let (w, h) = {
            let guard = LAST_WINDOW_SIZE.get().unwrap().lock().unwrap();
            *guard
        };

        if let Some(db_state) = app_handle.try_state::<DbState>() {
            let _ = db_state
                .settings_repo
                .set("app.window_width", &w.to_string());
            let _ = db_state
                .settings_repo
                .set("app.window_height", &h.to_string());
        }

        WINDOW_SIZE_SAVE_PENDING.store(false, Ordering::SeqCst);
        break;
    });
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn handle_blur(_window: &tauri::Window) {}

#[cfg(target_os = "macos")]
fn handle_blur(window: &tauri::Window) {
    if IGNORE_BLUR.load(Ordering::Relaxed) || WINDOW_PINNED.load(Ordering::Relaxed) {
        return;
    }

    let settings = window.app_handle().state::<SettingsState>();
    if settings.edge_docking.load(Ordering::Relaxed) {
        return;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    // Ignore blur events that fire immediately after the window was shown
    if now.saturating_sub(LAST_SHOW_TIMESTAMP.load(Ordering::Relaxed)) < 500 {
        return;
    }

    if IS_MOUSE_BUTTON_DOWN.load(Ordering::SeqCst) {
        return;
    }

    let w = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        let down = IS_MOUSE_BUTTON_DOWN.load(Ordering::SeqCst);

        if !down && matches!(w.is_focused(), Ok(false)) {
            if !IGNORE_BLUR.load(Ordering::Relaxed) && !WINDOW_PINNED.load(Ordering::Relaxed) {
                let _ = w.app_handle().emit("force-hide-compact-preview", ());
                let _ = w.hide();
                NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
                crate::app::window_manager::release_modifier_keys();
            }
        }
    });
}

#[cfg(target_os = "windows")]
fn handle_blur(window: &tauri::Window) {
    if IGNORE_BLUR.load(Ordering::Relaxed) || WINDOW_PINNED.load(Ordering::Relaxed) {
        return;
    }

    let settings = window.app_handle().state::<SettingsState>();
    if settings.edge_docking.load(Ordering::Relaxed) {
        return;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    if now.saturating_sub(LAST_SHOW_TIMESTAMP.load(Ordering::Relaxed)) < 500 {
        return;
    }

    if IS_MOUSE_BUTTON_DOWN.load(Ordering::SeqCst) {
        return;
    }

    let w = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let down = IS_MOUSE_BUTTON_DOWN.load(Ordering::SeqCst);

        // Hide window on blur unless mouse is down or pinned.
        if !down && matches!(w.is_focused(), Ok(false)) {
            if !IGNORE_BLUR.load(Ordering::Relaxed) && !WINDOW_PINNED.load(Ordering::Relaxed) {
                let _ = w.app_handle().emit("force-hide-compact-preview", ());
                let _ = w.hide();
                NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
                release_modifier_keys();
                let _ = restore_previous_app_focus(w.app_handle().clone());
            }
        }
    });
}
