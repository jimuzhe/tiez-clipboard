#[cfg(target_os = "windows")]
use crate::app::hooks::{keyboard_proc, mouse_proc};
#[cfg(target_os = "windows")]
use crate::app::system::tray_subclass_proc;
use crate::app::window_manager::{release_win_keys, restore_last_focus, toggle_window};
use crate::app_state::{
    AppDataDir, EncryptionQueueState, PasteQueue, SessionHistory, SettingsState,
};
use crate::database::{self, DbState};
use crate::global_state::*;
use crate::info;
use crate::infrastructure::repository::clipboard_repo::SqliteClipboardRepository;
use crate::infrastructure::repository::settings_repo::{
    SettingsRepository, SqliteSettingsRepository,
};
use crate::infrastructure::repository::tag_repo::SqliteTagRepository;
use crate::infrastructure::windows_ext::WindowExt;
use crate::services::encryption_queue::init_encryption_queue;
use crate::services::sensitive_align::spawn_sensitive_alignment;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{App, AppHandle, Emitter, Manager};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HINSTANCE, HWND, POINT, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::SetWindowSubclass;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowRect, RegisterWindowMessageW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
};

static WINDOW_SIZE_SAVE_PENDING: AtomicBool = AtomicBool::new(false);
static LAST_WINDOW_SIZE_EVENT_MS: AtomicU64 = AtomicU64::new(0);
static LAST_WINDOW_SIZE: OnceLock<Mutex<(u32, u32)>> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
struct WindowRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

pub fn init(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let app_handle = app.handle().clone();

    // Initialize GLOBAL_APP_HANDLE for Win32 hooks
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
        WindowExt::show_error_box("TieZ 启动错误", &err_msg);
        e
    })?;
    let conn_arc = std::sync::Arc::new(std::sync::Mutex::new(conn));
    let settings_repo = SqliteSettingsRepository::new(conn_arc.clone());

    // 4. Initial Settings & Reset Safety
    apply_startup_resets(&settings_repo);

    let settings = load_settings(&settings_repo);

    // 5. App State Management
    setup_state(app, conn_arc.clone(), &settings, app_dir.clone());
    app.manage(EncryptionQueueState(init_encryption_queue(
        app_handle.clone(),
    )));
    spawn_sensitive_alignment(app_handle.clone());

    // 6. Window Initialization (Pinned/Focus)
    setup_main_window(app, &settings);

    // 6.1 External Drag-Drop (Web Images)
    #[cfg(windows)]
    crate::infrastructure::windows_api::drag_drop::register_emoji_drag_drop(app_handle.clone());

    // 7. Background Services & Monitors
    start_services(app, &settings, app_handle.clone());

    // 8. Tray Setup
    setup_tray(app, settings.hide_tray_icon);

    // 9. Theme Initial Application
    apply_initial_theme(app);

    // 10. Win32 Hook Initialization
    #[cfg(target_os = "windows")]
    init_win32_hooks(app);

    // 11. TaskbarCreated & Subclass
    #[cfg(target_os = "windows")]
    setup_taskbar_listener(app);

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
            if !custom_path.is_empty() && std::path::Path::new(custom_path).exists() {
                std::path::PathBuf::from(custom_path)
            } else {
                default_app_dir.clone()
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
    let paste_method = repo
        .get("app.paste_method")
        .unwrap_or(Some("shift_insert".to_string()))
        .unwrap_or("shift_insert".to_string());
    if paste_method == "game_mode" && !crate::app::commands::system_cmd::check_is_admin() {
        info!(">>> [STARTUP] Game Mode active without Admin privileges. Resetting to default.");
        let _ = repo.set("app.paste_method", "shift_insert");
    }
}

pub struct StartupSettings {
    pub theme: String,
    pub persistent: bool,
    pub capture_files: bool,
    pub capture_rich_text: bool,
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
    pub hide_tray_icon: bool,
    pub edge_docking: bool,
    pub follow_mouse: bool,
    pub window_pinned: bool,
    pub window_width: Option<u32>,
    pub window_height: Option<u32>,
    pub main_hotkey: String,
    pub arrow_key_selection: bool,
    pub auto_close_server: bool,
}

fn load_settings(repo: &impl SettingsRepository) -> StartupSettings {
    StartupSettings {
        theme: repo
            .get("app.theme")
            .unwrap_or(Some("retro".to_string()))
            .unwrap_or("retro".to_string()),
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
            .unwrap_or(Some("Ctrl+Shift+Z".to_string()))
            .unwrap_or("Ctrl+Shift+Z".to_string()),
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
        hide_tray_icon: repo
            .get("app.hide_tray_icon")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        edge_docking: repo
            .get("app.edge_docking")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        follow_mouse: repo
            .get("app.follow_mouse")
            .unwrap_or(Some("true".to_string()))
            .map(|v| v == "true")
            .unwrap_or(true),
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
            .unwrap_or(Some("Win+V".to_string()))
            .unwrap_or("Win+V".to_string()),
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
        hide_tray_icon: AtomicBool::new(s.hide_tray_icon),
        edge_docking: AtomicBool::new(s.edge_docking),
        follow_mouse: AtomicBool::new(s.follow_mouse),
        arrow_key_selection: AtomicBool::new(s.arrow_key_selection),
        main_hotkey: std::sync::Mutex::new(s.main_hotkey.clone()),
        monitors: std::sync::Mutex::new(Vec::new()),
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
        let _ = window.set_focusable(!effective_pinned);

        #[cfg(windows)]
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                let ex_style = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                    HWND(hwnd.0),
                    GWL_EXSTYLE,
                );
                if effective_pinned {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                        HWND(hwnd.0),
                        GWL_EXSTYLE,
                        ex_style | WS_EX_NOACTIVATE.0 as isize,
                    );
                } else {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                        HWND(hwnd.0),
                        GWL_EXSTYLE,
                        ex_style & !(WS_EX_NOACTIVATE.0 as isize),
                    );
                }
            }
        }

        if repair_window_position_if_needed(&window, s.edge_docking) {
            IS_HIDDEN.store(false, Ordering::Relaxed);
            CURRENT_DOCK.store(0, Ordering::Relaxed);
        }
    }

    schedule_window_position_repair(app.handle().clone(), s.edge_docking);

    // Handle silent start
    let args: Vec<String> = std::env::args().collect();
    let is_autostart =
        args.contains(&"--autostart".to_string()) || args.contains(&"--minimized".to_string());
    if !is_autostart && !s.silent_start {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
        }
    }
}

fn schedule_window_position_repair(app_handle: AppHandle, edge_docking_enabled: bool) {
    std::thread::spawn(move || {
        for _ in 0..8 {
            std::thread::sleep(std::time::Duration::from_millis(250));

            let Some(window) = app_handle.get_webview_window("main") else {
                continue;
            };

            if repair_window_position_if_needed(&window, edge_docking_enabled) {
                IS_HIDDEN.store(false, Ordering::Relaxed);
                CURRENT_DOCK.store(0, Ordering::Relaxed);
                info!(">>> [STARTUP] Repaired off-screen window position after state restore.");
                break;
            }
        }
    });
}

fn repair_window_position_if_needed(
    window: &tauri::WebviewWindow,
    edge_docking_enabled: bool,
) -> bool {
    let Ok(position) = window.outer_position() else {
        return false;
    };
    let Ok(size) = window.outer_size() else {
        return false;
    };
    let Ok(monitors) = window.available_monitors() else {
        return false;
    };
    if monitors.is_empty() {
        return false;
    }

    let rect = WindowRect {
        x: position.x,
        y: position.y,
        width: size.width as i32,
        height: size.height as i32,
    };

    if rect.width <= 0 || rect.height <= 0 {
        return false;
    }

    let visible_enough = monitors
        .iter()
        .any(|monitor| window_rect_has_enough_visible_area(rect, monitor, edge_docking_enabled));
    if visible_enough {
        return false;
    }

    let target_monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| monitors.first().cloned());

    let Some(monitor) = target_monitor else {
        return false;
    };

    let (target_x, target_y) = clamp_window_rect_to_monitor(rect, &monitor);
    if target_x == rect.x && target_y == rect.y {
        return false;
    }

    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
        x: target_x,
        y: target_y,
    }));
    true
}

fn window_rect_has_enough_visible_area(
    rect: WindowRect,
    monitor: &tauri::Monitor,
    edge_docking_enabled: bool,
) -> bool {
    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let monitor_left = monitor_pos.x;
    let monitor_top = monitor_pos.y;
    let monitor_right = monitor_left + monitor_size.width as i32;
    let monitor_bottom = monitor_top + monitor_size.height as i32;

    let visible_left = rect.x.max(monitor_left);
    let visible_top = rect.y.max(monitor_top);
    let visible_right = (rect.x + rect.width).min(monitor_right);
    let visible_bottom = (rect.y + rect.height).min(monitor_bottom);
    let visible_width = (visible_right - visible_left).max(0);
    let visible_height = (visible_bottom - visible_top).max(0);

    if visible_width == 0 || visible_height == 0 {
        return false;
    }

    let min_visible_width = if edge_docking_enabled {
        24.min(rect.width)
    } else {
        1
    };
    let min_visible_height = if edge_docking_enabled {
        24.min(rect.height)
    } else {
        1
    };

    visible_width >= min_visible_width && visible_height >= min_visible_height
}

fn clamp_window_rect_to_monitor(rect: WindowRect, monitor: &tauri::Monitor) -> (i32, i32) {
    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let margin = 10;

    let min_x = monitor_pos.x + margin;
    let min_y = monitor_pos.y + margin;
    let max_x = (monitor_pos.x + monitor_size.width as i32 - rect.width - margin).max(min_x);
    let max_y = (monitor_pos.y + monitor_size.height as i32 - rect.height - margin).max(min_y);

    let target_x = if rect.width + margin * 2 >= monitor_size.width as i32 {
        monitor_pos.x
    } else {
        rect.x.clamp(min_x, max_x)
    };
    let target_y = if rect.height + margin * 2 >= monitor_size.height as i32 {
        monitor_pos.y
    } else {
        rect.y.clamp(min_y, max_y)
    };

    (target_x, target_y)
}

fn start_services(app: &App, s: &StartupSettings, app_handle: AppHandle) {
    crate::infrastructure::windows_api::window_tracker::start_window_tracking(app_handle.clone());
    crate::services::clipboard::start_clipboard_monitor(app_handle.clone());
    crate::services::mqtt_sub::start_mqtt_client(app_handle.clone());
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

    // Daily app announcement ping
    init_announcement_ping(app, &db_state.settings_repo);

    // Register active hotkeys based on current settings.
    let _ = crate::app::commands::register_hotkey(app_handle.clone(), s.main_hotkey.clone());

    // Win+V Optimization
    if db_state
        .settings_repo
        .get("app.use_win_v_shortcut")
        .unwrap_or(Some("false".to_string()))
        == Some("true".to_string())
    {
        if !crate::app::commands::system_cmd::get_registry_win_v_optimized_status() {
            let _ = crate::app::commands::trigger_registry_win_v_optimization(true);
        }
    }
}

#[cfg(target_os = "windows")]
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
                        let _ = window.show();
                        IS_HIDDEN.store(false, Ordering::Relaxed);
                        CURRENT_DOCK.store(0, Ordering::Relaxed);
                    }
                }
                continue;
            }

            if let Some(window) = app_handle.get_webview_window("main") {
                // Skip if window is minimized
                if window.is_minimized().unwrap_or(false) {
                    continue;
                }

                let is_window_visible = window.is_visible().unwrap_or(true);
                let is_hidden_by_edge = IS_HIDDEN.load(Ordering::Relaxed);

                // Skip edge docking checks if window was hidden by other mechanisms (paste, blur, etc.)
                if !is_window_visible && !is_hidden_by_edge {
                    continue;
                }

                let last_show = LAST_SHOW_TIMESTAMP.load(Ordering::Relaxed);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                // While the clipboard window is actively shown via hotkey navigation,
                // avoid immediate auto-docking right after showing.
                if !is_hidden_by_edge
                    && NAVIGATION_ENABLED.load(Ordering::SeqCst)
                    && now.saturating_sub(last_show) < 800
                {
                    continue;
                }

                // Grace period after showing to prevent immediate re-dock
                if now.saturating_sub(last_show) < 500 {
                    continue;
                }

                let mut rect = RECT::default();
                let hwnd = match window.hwnd() {
                    Ok(h) => h,
                    Err(_) => continue,
                };
                unsafe {
                    let _ = GetWindowRect(HWND(hwnd.0), &mut rect);
                }

                let mut point = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut point);
                }

                // Get current monitor info and validate
                let monitor = match window.current_monitor() {
                    Ok(Some(m)) => m,
                    _ => continue,
                };
                let screen_size = monitor.size();
                let screen_pos = monitor.position();

                // Calculate monitor boundaries
                let screen_left = screen_pos.x;
                let screen_top = screen_pos.y;
                let screen_right = screen_pos.x + screen_size.width as i32;
                let screen_bottom = screen_pos.y + screen_size.height as i32;

                // When hidden, check if mouse is near the edge sliver
                let threshold = 5;
                let is_mouse_near_edge = if is_hidden_by_edge {
                    let current_dock = CURRENT_DOCK.load(Ordering::Relaxed);
                    match current_dock {
                        1 => {
                            point.y <= screen_top + threshold
                                && point.x >= rect.left
                                && point.x <= rect.right
                        } // Top
                        2 => {
                            point.x <= screen_left + threshold
                                && point.y >= rect.top
                                && point.y <= rect.bottom
                        } // Left
                        3 => {
                            point.x >= screen_right - threshold
                                && point.y >= rect.top
                                && point.y <= rect.bottom
                        } // Right
                        _ => false,
                    }
                } else {
                    false
                };

                let is_mouse_in = if is_hidden_by_edge {
                    is_mouse_near_edge
                } else {
                    point.x >= rect.left
                        && point.x <= rect.right
                        && point.y >= rect.top
                        && point.y <= rect.bottom
                };

                // Ensure window is actually on this monitor
                let window_center_x = (rect.left + rect.right) / 2;
                let window_center_y = (rect.top + rect.bottom) / 2;
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
                if rect.top <= screen_top + threshold {
                    dock = DockPosition::Top;
                } else if rect.left <= screen_left + threshold {
                    dock = DockPosition::Left;
                } else if rect.right >= screen_right - threshold {
                    dock = DockPosition::Right;
                }

                if is_hidden_by_edge {
                    if is_mouse_in {
                        let current_dock = CURRENT_DOCK.load(Ordering::Relaxed);
                        let dock_actual = match current_dock {
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
                                            x: rect.left,
                                            y: screen_top,
                                        },
                                    ));
                                }
                                DockPosition::Left => {
                                    let _ = window.set_position(tauri::Position::Physical(
                                        tauri::PhysicalPosition {
                                            x: screen_left,
                                            y: rect.top,
                                        },
                                    ));
                                }
                                DockPosition::Right => {
                                    let w = rect.right - rect.left;
                                    let _ = window.set_position(tauri::Position::Physical(
                                        tauri::PhysicalPosition {
                                            x: screen_right - w,
                                            y: rect.top,
                                        },
                                    ));
                                }
                                _ => {}
                            }

                            IS_HIDDEN.store(false, Ordering::Relaxed);
                            CURRENT_DOCK.store(0, Ordering::Relaxed);
                        }
                    }
                } else if dock != DockPosition::None {
                    // Don't dock while dragging (Left Mouse Button down)
                    let is_lbutton_down = unsafe { (GetAsyncKeyState(0x01) as u16 & 0x8000) != 0 };
                    if is_mouse_in || is_lbutton_down {
                        continue;
                    }

                    if !IS_HIDDEN.load(Ordering::Relaxed) {
                        // Auto-enable pin only when docking occurs (runtime only, no DB write)
                        if !WINDOW_PINNED.load(Ordering::Relaxed) {
                            WINDOW_PINNED.store(true, Ordering::Relaxed);
                            let _ = window.set_always_on_top(true);
                            let _ = window.set_focusable(false);
                            #[cfg(windows)]
                            if let Ok(hwnd) = window.hwnd() {
                                unsafe {
                                    let ex_style =
                                        windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                                            HWND(hwnd.0),
                                            GWL_EXSTYLE,
                                        );
                                    let _ =
                                        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                                            HWND(hwnd.0),
                                            GWL_EXSTYLE,
                                            ex_style | WS_EX_NOACTIVATE.0 as isize,
                                        );
                                }
                            }
                            let _ = app_handle.emit("window-pinned-changed", true);
                        }

                        let window_height = rect.bottom - rect.top;
                        let window_width = rect.right - rect.left;
                        match dock {
                            DockPosition::Top => {
                                let _ = window.set_position(tauri::PhysicalPosition::new(
                                    rect.left,
                                    screen_top - window_height + hide_size,
                                ));
                                CURRENT_DOCK.store(1, Ordering::Relaxed);
                            }
                            DockPosition::Left => {
                                let _ = window.set_position(tauri::PhysicalPosition::new(
                                    screen_left - window_width + hide_size,
                                    rect.top,
                                ));
                                CURRENT_DOCK.store(2, Ordering::Relaxed);
                            }
                            DockPosition::Right => {
                                let _ = window.set_position(tauri::PhysicalPosition::new(
                                    screen_right - hide_size,
                                    rect.top,
                                ));
                                CURRENT_DOCK.store(3, Ordering::Relaxed);
                            }
                            _ => {}
                        }
                        IS_HIDDEN.store(true, Ordering::Relaxed);
                    }
                } else if IS_HIDDEN.load(Ordering::Relaxed) {
                    IS_HIDDEN.store(false, Ordering::Relaxed);
                    CURRENT_DOCK.store(0, Ordering::Relaxed);

                    // Restore pinned state based on user setting when undocked
                    let mut user_pinned = WINDOW_PINNED.load(Ordering::Relaxed);
                    if let Some(db_state) = app_handle.try_state::<DbState>() {
                        if let Ok(val) = db_state.settings_repo.get("app.window_pinned") {
                            user_pinned = val.as_deref() == Some("true");
                        }
                    }

                    let prev = WINDOW_PINNED.swap(user_pinned, Ordering::Relaxed);
                    if prev != user_pinned {
                        let _ = window.set_always_on_top(user_pinned);
                        let _ = window.set_focusable(!user_pinned);
                        #[cfg(windows)]
                        if let Ok(hwnd) = window.hwnd() {
                            unsafe {
                                let ex_style =
                                    windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                                        HWND(hwnd.0),
                                        GWL_EXSTYLE,
                                    );
                                let next = if user_pinned {
                                    ex_style | WS_EX_NOACTIVATE.0 as isize
                                } else {
                                    ex_style & !(WS_EX_NOACTIVATE.0 as isize)
                                };
                                let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                                    HWND(hwnd.0),
                                    GWL_EXSTYLE,
                                    next,
                                );
                            }
                        }
                        let _ = app_handle.emit("window-pinned-changed", user_pinned);
                    }
                }
            }
        }
    });
}

#[cfg(not(target_os = "windows"))]
fn start_edge_docking_monitor(_app_handle: AppHandle) {}

fn init_announcement_ping(app: &App, repo: &impl SettingsRepository) {
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
        if let Ok(base_url) = std::env::var("TIEZ_ANNOUNCEMENT_PING_URL") {
            let base_url = base_url.trim().to_string();
            if !base_url.is_empty() {
                std::thread::spawn(move || {
                    let sep = if base_url.contains('?') { "&" } else { "?" };
                    let ping_url = format!(
                        "{}{}v={}&id={}",
                        base_url,
                        sep,
                        urlencoding::encode(&version),
                        urlencoding::encode(&anon_id)
                    );
                    let _ = reqwest::blocking::get(ping_url);
                });
            }
        }
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

    let tray = TrayIconBuilder::with_id("main_tray")
        .icon(icon)
        .tooltip("TieZ")
        .show_menu_on_left_click(false)
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id.as_ref() == "show" {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
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
                    let _ = window.show();
                    let _ = window.set_focus();
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    LAST_SHOW_TIMESTAMP.store(now, Ordering::Relaxed);
                }
            }
        })
        .build(app)
        .expect("Failed to build tray");

    let _ = tray.set_visible(!hide_tray);
    app.manage(tray);
}

fn apply_initial_theme(app: &App) {
    let db_state = app.state::<DbState>();
    let theme = db_state
        .settings_repo
        .get("app.theme")
        .unwrap_or(Some("retro".to_string()))
        .unwrap_or("retro".to_string());
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
            None,
        );
    }
}

#[cfg(target_os = "windows")]
fn init_win32_hooks(_app: &App) {
    std::thread::spawn(move || {
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
            UnhookWindowsHookEx, MSG, WH_KEYBOARD_LL, WH_MOUSE_LL,
        };
        unsafe {
            HOOK_THREAD_ID.store(
                windows::Win32::System::Threading::GetCurrentThreadId(),
                Ordering::Relaxed,
            );
            let h_instance = GetModuleHandleW(None).expect("Failed to get module handle");
            let h_hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_proc),
                Some(HINSTANCE(h_instance.0)),
                0,
            )
            .expect("Failed to set hook");
            HOOK_HANDLE.store(h_hook.0 as _, Ordering::SeqCst);
            let h_mouse_hook = SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_proc),
                Some(HINSTANCE(h_instance.0)),
                0,
            )
            .expect("Failed to set mouse hook");
            HOOK_MOUSE_HANDLE.store(h_mouse_hook.0 as _, Ordering::SeqCst);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = UnhookWindowsHookEx(h_hook);
            let h_mouse = HOOK_MOUSE_HANDLE.swap(null_mut(), Ordering::SeqCst);
            if !h_mouse.is_null() {
                let _ = UnhookWindowsHookEx(windows::Win32::UI::WindowsAndMessaging::HHOOK(
                    h_mouse as _,
                ));
            }
        }
    });
}

#[cfg(target_os = "windows")]
fn setup_taskbar_listener(app: &App) {
    unsafe {
        let msg = RegisterWindowMessageW(windows::core::w!("TaskbarCreated"));
        if msg != 0 {
            TASKBAR_CREATED_MSG.store(msg, Ordering::Relaxed);
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(hwnd) = window.hwnd() {
                    let _ = SetWindowSubclass(HWND(hwnd.0), Some(tray_subclass_proc), 1337, 0);
                }
            }
        }
    }
}

pub fn handle_global_shortcut(app: &AppHandle, shortcut: &tauri_plugin_global_shortcut::Shortcut) {
    use tauri_plugin_global_shortcut::Shortcut;
    let settings = app.state::<SettingsState>();

    if let Ok(main_s) = {
        let val = settings.main_hotkey.lock().unwrap().clone();
        val.replace("Win", "Super").parse::<Shortcut>()
    } {
        if shortcut == &main_s {
            toggle_window(app);
            return;
        }
    }

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
                tauri::async_runtime::spawn({
                    let app = app.clone();
                    async move {
                        crate::services::paste_queue::paste_next_step(app).await;
                    }
                });
            }
        }
    }

    if let Ok(rich_s) = {
        let val = settings.rich_paste_hotkey.lock().unwrap().clone();
        val.replace("Win", "Super").parse::<Shortcut>()
    } {
        if shortcut == &rich_s {
            crate::services::clipboard_ops::paste_latest_rich(app.clone());
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
}

pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    match event {
        tauri::WindowEvent::Focused(focused) => {
            if window.label() != "main" {
                return;
            }
            IS_MAIN_WINDOW_FOCUSED.store(*focused, Ordering::Relaxed);
            if *focused {
                #[cfg(target_os = "windows")]
                unsafe {
                    let hwnd = windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
                    if !hwnd.0.is_null() {
                        if let Ok(h) = window.hwnd() {
                            if hwnd.0 != h.0 {
                                crate::LAST_ACTIVE_HWND.store(hwnd.0 as usize, Ordering::Relaxed);
                            }
                        }
                    }
                }
            } else {
                handle_blur(window);
            }
        }
        tauri::WindowEvent::Resized(size) => {
            if window.label() != "main" {
                return;
            }
            if window.is_minimized().unwrap_or(false) || window.is_maximized().unwrap_or(false) {
                return;
            }
            persist_window_size(window, size.width, size.height);
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            if window.label() != "main" {
                return;
            }
            api.prevent_close();
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
    unsafe {
        if (windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x01) as u16 & 0x8000)
            != 0
            || (windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x02) as u16 & 0x8000)
                != 0
        {
            return;
        }
    }

    let w = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let down = IS_MOUSE_BUTTON_DOWN.load(Ordering::SeqCst)
            || unsafe {
                (windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x01) as u16
                    & 0x8000)
                    != 0
                    || (windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x02) as u16
                        & 0x8000)
                        != 0
            };
        if !down && matches!(w.is_focused(), Ok(false)) {
            if !IGNORE_BLUR.load(Ordering::Relaxed) && !WINDOW_PINNED.load(Ordering::Relaxed) {
                let _ = w.hide();
                NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
                release_win_keys();
                let _ = restore_last_focus(w.app_handle().clone());
            }
        }
    });
}
