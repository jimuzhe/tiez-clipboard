use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{App, AppHandle, Emitter, Manager};

use crate::app::window_manager::{maybe_open_devtools, toggle_window};
#[cfg(target_os = "windows")]
use crate::app::hooks::{keyboard_proc, mouse_proc};
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
use crate::services::sensitive_align::spawn_sensitive_alignment;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HINSTANCE, HWND, POINT, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowRect, RegisterWindowMessageW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
};

static WINDOW_SIZE_SAVE_PENDING: AtomicBool = AtomicBool::new(false);
static LAST_WINDOW_SIZE_EVENT_MS: AtomicU64 = AtomicU64::new(0);
static LAST_WINDOW_SIZE: OnceLock<Mutex<(u32, u32)>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn register_macos_wake_hotkey_refresh(app_handle: AppHandle) {
    use block2::RcBlock;
    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification};
    use objc2_foundation::NSNotification;
    use std::ptr::NonNull;
    use std::time::Duration;

    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let wake_handler = RcBlock::new(move |_notification: NonNull<NSNotification>| {
        crate::services::network_recovery::request_wake_recovery();
        let app_handle = app_handle.clone();
        std::thread::spawn(move || {
            // macOS may need a short moment to restore Carbon event hotkeys after wake.
            std::thread::sleep(Duration::from_millis(750));
            let main_thread_handle = app_handle.clone();
            let _ = app_handle.run_on_main_thread(move || {
                match crate::app::commands::sync_registered_hotkeys(&main_thread_handle) {
                    Ok(()) => info!(">>> [HOTKEY] Re-registered shortcuts after macOS wake"),
                    Err(error) => {
                        crate::error!(">>> [HOTKEY] Wake re-registration failed: {error}")
                    }
                }
            });
        });
    });

    // NSNotificationCenter owns the returned observer for the process lifetime.
    unsafe {
        let observer = center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidWakeNotification),
            None,
            None,
            &wake_handler,
        );
        let _ = objc2::rc::Retained::into_raw(observer);
    }
}

pub fn init(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let app_handle = app.handle().clone();

    #[cfg(target_os = "macos")]
    register_macos_wake_hotkey_refresh(app_handle.clone());

    // Initialize GLOBAL_APP_HANDLE
    let _ = GLOBAL_APP_HANDLE.set(app_handle.clone());

    // 1. Data Directory & Migration
    let app_dir = resolve_data_dir(app)?;

    // 2. Logger Initialization
    crate::logger::init(app_dir.join("tiez.log"));
    info!(">>> [STARTUP] TieZ starting up...");

    // 2b. Local-at-rest encryption (AES-GCM + OS key store)
    crate::infrastructure::encryption::init(&app_dir);

    // 3. Database Initialization
    let db_path = app_dir.join("clipboard.db");
    let db_path_str = db_path.to_string_lossy();
    let conn = database::init_db(&db_path_str).map_err(|e| {
        let err_msg = format!("无法打开记录存储。\n详情：{}", e);
        eprintln!("TieZ Startup Error: {}", err_msg);
        e
    })?;

    #[cfg(not(feature = "portable"))]
    {
        match crate::infrastructure::encryption::migrate_legacy_ciphertexts(&conn) {
            Ok(n) if n > 0 => info!(">>> [STARTUP] Migrated {} legacy cipher fields", n),
            Ok(_) => {}
            Err(e) => eprintln!(">>> [STARTUP] Legacy encryption migration warning: {}", e),
        }
    }

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
    spawn_sensitive_alignment(app_handle.clone());

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

    #[cfg(target_os = "windows")]
    {
        crate::infrastructure::windows_api::drag_drop::register_emoji_drag_drop(app_handle.clone());
        init_win32_hooks(app);
        setup_taskbar_listener(app);
    }

    Ok(())
}

/// Debug macOS builds keep their encryption key in a local file instead of the
/// Keychain, so they must never share an encrypted database with release builds.
pub(crate) fn default_app_data_dir<R: tauri::Runtime>(
    paths: &tauri::path::PathResolver<R>,
) -> tauri::Result<std::path::PathBuf> {
    let dir = paths.app_data_dir()?;
    // `npm run tauri:dev` already uses the com.tiez.dev identifier; only a
    // debug build run with the release config still needs the suffix.
    #[cfg(all(target_os = "macos", debug_assertions))]
    let dir = match dir.file_name().and_then(|n| n.to_str()) {
        Some(name) if name.ends_with(".dev") => dir,
        name => dir.with_file_name(format!("{}.dev", name.unwrap_or("com.tiez"))),
    };
    Ok(dir)
}

fn resolve_data_dir(app: &App) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let default_app_dir = default_app_data_dir(app.path())?;

    // Perform migration if needed
    crate::migration::perform_migration_v028(&default_app_dir);

    // Cleanup temp files
    std::thread::spawn(|| {
        let temp_dir = std::env::temp_dir();
        if let Ok(entries) = std::fs::read_dir(&temp_dir) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    // Clipboard temp files from earlier runs: pasted images
                    // (TieZ_IMG_/TieZ_GIF_) were never removed and piled up.
                    if ["TieZ_Clip_", "TieZ_IMG_", "TieZ_GIF_"]
                        .iter()
                        .any(|prefix| name.starts_with(prefix))
                    {
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

    if let Some(portable_data) = portable_data_dir() {
        app_dir = portable_data;
    }

    std::fs::create_dir_all(&app_dir)?;
    Ok(app_dir)
}

/// Portable mode: a `data` folder next to the executable holds the data, and
/// always wins over a data folder chosen in settings.
pub(crate) fn portable_data_dir() -> Option<std::path::PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let portable_data = exe_path.parent()?.join("data");
    portable_data.is_dir().then_some(portable_data)
}

fn apply_startup_resets(repo: &impl SettingsRepository) {
    #[cfg(target_os = "windows")]
    {
        let paste_method = repo
            .get("app.paste_method")
            .unwrap_or(Some("auto".to_string()))
            .unwrap_or("auto".to_string());
        if paste_method == "game_mode" && !crate::app::commands::system_cmd::check_is_admin() {
            info!(">>> [STARTUP] Game Mode active without Admin privileges. Resetting to default.");
            let _ = repo.set("app.paste_method", "auto");
        }
    }

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
            "app.plain_paste_hotkey",
            &["Ctrl+Shift+Z", "Command+Alt+Shift+V", "Command+Shift+V"],
            "Alt+Shift+V",
        );
        migrate_hotkey_default(
            repo,
            "app.search_hotkey",
            &["Command+F", "Command+Alt+F"],
            "Alt+F",
        );

        migrate_silent_start_once(repo);
    }
}

/// Marks that `app.silent_start` was reset once on this Mac.
pub(crate) const SILENT_START_MAC_RESET_KEY: &str = "app.silent_start_mac_reset";

/// 「后台启动」 used to do nothing on macOS while being seeded on for every
/// install. Now that it works, set it once to what Mac users actually got (the
/// window shows), or every Mac would open without a window after updating.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn migrate_silent_start_once(repo: &impl SettingsRepository) {
    if repo.get(SILENT_START_MAC_RESET_KEY).ok().flatten().is_some() {
        return;
    }
    if repo.set("app.silent_start", "false").is_ok() {
        let _ = repo.set(SILENT_START_MAC_RESET_KEY, "true");
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
    pub plain_paste_hotkey: String,
    pub search_hotkey: String,
    pub quick_paste_modifier: String,
    pub sound_enabled: bool,
    pub paste_sound_enabled: bool,
    pub hide_tray_icon: bool,
    pub hide_dock_icon: bool,
    pub follow_mouse: bool,
    pub edge_docking: bool,
    pub last_edge_dock: String,
    pub window_pinned: bool,
    pub window_width: Option<u32>,
    pub window_height: Option<u32>,
    pub main_hotkey: String,
    pub arrow_key_selection: bool,
    pub light_mode_enabled: bool,
    pub light_mode_idle_minutes: u64,
    pub auto_close_server: bool,
    pub sound_volume: f64,
}

pub(crate) fn load_settings(repo: &impl SettingsRepository) -> StartupSettings {
    StartupSettings {
        theme: repo
            .get("app.theme")
            .unwrap_or(Some(crate::build_config::default_theme_id().to_string()))
            .unwrap_or_else(|| crate::build_config::default_theme_id().to_string()),
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
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
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
        plain_paste_hotkey: repo
            .get("app.plain_paste_hotkey")
            .ok()
            .flatten()
            .unwrap_or_default(),
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
            .unwrap_or(Some("true".to_string()))
            .map(|v| v == "true")
            .unwrap_or(true),
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
        follow_mouse: repo
            .get("app.follow_mouse")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        edge_docking: repo
            .get("app.edge_docking")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        last_edge_dock: repo
            .get("app.last_edge_dock")
            .unwrap_or(Some("none".to_string()))
            .unwrap_or("none".to_string()),
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
            .unwrap_or(Some("true".to_string()))
            .map(|v| v == "true")
            .unwrap_or(true),
        light_mode_enabled: repo
            .get("app.light_mode_enabled")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        light_mode_idle_minutes: repo
            .get("app.light_mode_idle_minutes")
            .unwrap_or(Some("15".to_string()))
            .and_then(|v| v.parse::<u64>().ok())
            .map(|v| v.clamp(1, 240))
            .unwrap_or(15),
        auto_close_server: repo
            .get("file_transfer_auto_close")
            .unwrap_or(Some("false".to_string()))
            .map(|v| v == "true")
            .unwrap_or(false),
        sound_volume: repo
            .get("app.sound_volume")
            .unwrap_or(Some("1.0".to_string()))
            .and_then(|v| crate::database::sound_volume_from_setting(&v))
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

    app.manage(settings_state_from(s));

    app.manage(SessionHistory(std::sync::Mutex::new(
        std::collections::VecDeque::new(),
    )));
    crate::services::clipboard::start_session_file_sweeper(app.handle().clone(), &app_dir);
    crate::app::commands::thumbnail_cmd::init_thumbnail_cache(app.handle());
    app.manage(AppDataDir(std::sync::Mutex::new(app_dir)));
    app.manage(crate::services::file_transfer::ChatState::default());
    app.manage(crate::services::file_transfer::SharedFileState(
        std::sync::Mutex::new(std::collections::HashMap::new()),
    ));
    app.manage(crate::services::file_transfer::ServerInfo {
        port: std::sync::atomic::AtomicU16::new(0),
        ip: std::sync::Mutex::new(String::new()),
        access_token: std::sync::Mutex::new(String::new()),
        lan_key: std::sync::Mutex::new(None),
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

/// The in-memory settings for `s`; also rebuilt by 「恢复默认设置」.
pub(crate) fn settings_state_from(s: &StartupSettings) -> SettingsState {
    SettingsState {
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
        plain_paste_hotkey: std::sync::Mutex::new(s.plain_paste_hotkey.clone()),
        search_hotkey: std::sync::Mutex::new(s.search_hotkey.clone()),
        quick_paste_modifier: std::sync::Mutex::new(s.quick_paste_modifier.clone()),
        sound_enabled: AtomicBool::new(s.sound_enabled),
        paste_sound_enabled: AtomicBool::new(s.paste_sound_enabled),
        hide_tray_icon: AtomicBool::new(s.hide_tray_icon),
        hide_dock_icon: AtomicBool::new(s.hide_dock_icon),
        follow_mouse: AtomicBool::new(s.follow_mouse),
        edge_docking: AtomicBool::new(s.edge_docking),
        arrow_key_selection: AtomicBool::new(s.arrow_key_selection),
        light_mode_enabled: AtomicBool::new(s.light_mode_enabled),
        light_mode_idle_minutes: std::sync::atomic::AtomicU64::new(s.light_mode_idle_minutes),
        main_hotkey: std::sync::Mutex::new(s.main_hotkey.clone()),
        monitors: std::sync::Mutex::new(Vec::new()),
        sound_volume: std::sync::Mutex::new(s.sound_volume),
    }
}

#[cfg(target_os = "macos")]
fn apply_initial_dock_visibility(app: &mut App, s: &StartupSettings) {
    let _ = app.set_dock_visibility(!s.hide_dock_icon);
}

#[cfg(not(target_os = "macos"))]
fn apply_initial_dock_visibility(_app: &mut App, _s: &StartupSettings) {}

/// Configure macOS Space / fullscreen overlay behavior for the main window.
#[cfg(target_os = "macos")]
fn set_window_all_spaces(window: &tauri::WebviewWindow) {
    crate::infrastructure::macos_api::window::configure_overlay_space_behavior(window);
}

fn parse_dock_token(token: &str) -> i32 {
    match token.trim().to_ascii_lowercase().as_str() {
        "top" => 1,
        "left" => 2,
        "right" => 3,
        _ => 0,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn persist_edge_dock(_app_handle: &AppHandle, _dock: i32) {}

fn dock_token_for(dock: i32) -> &'static str {
    match dock {
        1 => "top",
        2 => "left",
        3 => "right",
        _ => "none",
    }
}

#[cfg(target_os = "macos")]
pub fn persist_edge_dock(app_handle: &AppHandle, dock: i32) {
    if let Some(db_state) = app_handle.try_state::<DbState>() {
        let _ = db_state
            .settings_repo
            .set("app.last_edge_dock", dock_token_for(dock));
    }
}

#[cfg(target_os = "macos")]
fn apply_dock_position_on_main(window: &tauri::WebviewWindow, dock: i32) {
    let (Ok(pos), Ok(size), Ok(Some(monitor))) = (
        window.outer_position().or_else(|_| window.inner_position()),
        window.outer_size().or_else(|_| window.inner_size()),
        window.current_monitor(),
    ) else {
        return;
    };
    let sp = monitor.position();
    let ss = monitor.size();
    let screen_left = sp.x;
    let screen_top = sp.y;
    let screen_right = sp.x + ss.width as i32;
    let hide_size = 3i32;

    match dock {
        1 => {
            let _ = window.set_position(tauri::PhysicalPosition::new(
                pos.x,
                screen_top - size.height as i32 + hide_size,
            ));
        }
        2 => {
            let _ = window.set_position(tauri::PhysicalPosition::new(
                screen_left - size.width as i32 + hide_size,
                pos.y,
            ));
        }
        3 => {
            let _ = window.set_position(tauri::PhysicalPosition::new(
                screen_right - hide_size,
                pos.y,
            ));
        }
        _ => {}
    }
}

/// Whether the main window is tucked to a screen edge (by flags or geometry).
pub fn is_window_edge_tucked(window: &tauri::WebviewWindow) -> bool {
    if IS_HIDDEN.load(Ordering::Relaxed) || CURRENT_DOCK.load(Ordering::Relaxed) != 0 {
        return true;
    }

    let (Ok(pos), Ok(size), Ok(Some(monitor))) = (
        window.outer_position().or_else(|_| window.inner_position()),
        window.outer_size().or_else(|_| window.inner_size()),
        window.current_monitor(),
    ) else {
        return false;
    };

    let screen = monitor.position();
    let screen_size = monitor.size();
    let rect_left = pos.x;
    let rect_top = pos.y;
    let rect_right = pos.x + size.width as i32;
    let rect_bottom = pos.y + size.height as i32;
    let screen_left = screen.x;
    let screen_top = screen.y;
    let screen_right = screen.x + screen_size.width as i32;
    let screen_bottom = screen.y + screen_size.height as i32;

    #[cfg(target_os = "macos")]
    {
        visibility_ratio_on_monitor(
            rect_left,
            rect_top,
            rect_right,
            rect_bottom,
            screen_left,
            screen_top,
            screen_right,
            screen_bottom,
        ) < 0.18
    }
    #[cfg(not(target_os = "macos"))]
    {
        let threshold = 5;
        rect_top <= screen_top + threshold
            || rect_left <= screen_left + threshold
            || rect_right >= screen_right - threshold
    }
}

fn infer_edge_dock_from_geometry(
    rect_left: i32,
    rect_top: i32,
    rect_right: i32,
    screen_left: i32,
    screen_top: i32,
    screen_right: i32,
) -> i32 {
    let threshold = 5;
    if rect_top <= screen_top + threshold {
        1
    } else if rect_left <= screen_left + threshold {
        2
    } else if rect_right >= screen_right - threshold {
        3
    } else {
        0
    }
}

/// The edge a window is tucked into: pushed past it, with only a sliver (3 px,
/// plus slack for DPI rounding) left on screen. 0 when it isn't.
fn offscreen_tuck_edge(
    rect_left: i32,
    rect_right: i32,
    rect_bottom: i32,
    screen_left: i32,
    screen_top: i32,
    screen_right: i32,
) -> i32 {
    let slack = 10;
    if rect_bottom <= screen_top + slack {
        1
    } else if rect_right <= screen_left + slack {
        2
    } else if rect_left >= screen_right - slack {
        3
    } else {
        0
    }
}

#[cfg(test)]
mod edge_tuck_tests {
    use super::{infer_edge_dock_from_geometry, offscreen_tuck_edge};

    // A 1920-wide screen at the origin and a 400-wide, 600-tall window.
    #[test]
    fn a_window_by_the_edge_is_not_a_tuck() {
        // Left edge, fully on screen: docking reads it as docked left...
        assert_eq!(infer_edge_dock_from_geometry(2, 300, 402, 0, 0, 1920), 2);
        // ...but it isn't tucked, so without docking it stays where it is.
        assert_eq!(offscreen_tuck_edge(2, 402, 900, 0, 0, 1920), 0);
        assert_eq!(offscreen_tuck_edge(800, 1200, 600, 0, 0, 1920), 0);
        assert_eq!(offscreen_tuck_edge(1518, 1918, 900, 0, 0, 1920), 0);
    }

    #[test]
    fn a_tucked_window_is_found_on_its_edge() {
        assert_eq!(offscreen_tuck_edge(800, 1200, 3, 0, 0, 1920), 1);
        assert_eq!(offscreen_tuck_edge(-397, 3, 900, 0, 0, 1920), 2);
        assert_eq!(offscreen_tuck_edge(1917, 2317, 900, 0, 0, 1920), 3);
        // On a second screen to the right.
        assert_eq!(offscreen_tuck_edge(1523, 1923, 900, 1920, 0, 3840), 2);
    }
}

/// Edge tuck and reveal change always-on-top while the user works in another
/// app. On Windows that must not activate TieZ: a hover-reveal took focus from
/// the app underneath, which closed an Explorer / Directory Opus rename box.
/// Tauri's own flag is left as is, so it keeps tracking the user's pin.
fn set_edge_dock_topmost(window: &tauri::WebviewWindow, topmost: bool) {
    #[cfg(target_os = "windows")]
    if let Ok(hwnd) = window.hwnd() {
        crate::infrastructure::windows_ext::WindowExt::set_topmost_no_activate(
            HWND(hwnd.0),
            topmost,
        );
        return;
    }
    let _ = window.set_always_on_top(topmost);
}

fn restore_pin_state_after_edge_expand(app_handle: &AppHandle, window: &tauri::WebviewWindow) {
    let mut user_pinned = WINDOW_PINNED.load(Ordering::Relaxed);
    if let Some(db_state) = app_handle.try_state::<DbState>() {
        if let Ok(val) = db_state.settings_repo.get("app.window_pinned") {
            user_pinned = val.as_deref() == Some("true");
        }
    }

    let prev = WINDOW_PINNED.swap(user_pinned, Ordering::Relaxed);
    if prev != user_pinned {
        let _ = window.set_always_on_top(user_pinned);
        #[cfg(target_os = "macos")]
        crate::infrastructure::macos_api::window::set_window_focusable(&window, !user_pinned);
        let _ = app_handle.emit("window-pinned-changed", user_pinned);
    } else {
        set_edge_dock_topmost(window, user_pinned);
    }
}

/// The window-state plugin restores the last position, which is off-screen
/// when TieZ quit while tucked into an edge, but nothing marked the window as
/// tucked on Windows. The hotkey then hid and "showed" it out of sight, and
/// only a tray click (which infers the dock from geometry) brought it back.
/// Adopt such a position as a tuck. Returns true if it did.
#[cfg(target_os = "windows")]
pub fn adopt_offscreen_tuck(app_handle: &AppHandle, window: &tauri::WebviewWindow) -> bool {
    if IS_HIDDEN.load(Ordering::Relaxed) {
        return false;
    }
    let edge_docking = app_handle
        .try_state::<SettingsState>()
        .is_some_and(|s| s.edge_docking.load(Ordering::Relaxed));
    if !edge_docking {
        return false;
    }
    let (Ok(pos), Ok(size), Ok(Some(monitor))) = (
        window.outer_position(),
        window.outer_size(),
        window.current_monitor(),
    ) else {
        return false;
    };
    let screen = monitor.position();
    let dock = offscreen_tuck_edge(
        pos.x,
        pos.x + size.width as i32,
        pos.y + size.height as i32,
        screen.x,
        screen.y,
        screen.x + monitor.size().width as i32,
    );
    if dock == 0 {
        return false;
    }
    CURRENT_DOCK.store(dock, Ordering::Relaxed);
    IS_HIDDEN.store(true, Ordering::Relaxed);
    true
}

/// Pull the window fully on-screen after edge tuck. Returns true if a tuck was expanded.
pub fn expand_from_edge_dock(app_handle: &AppHandle, window: &tauri::WebviewWindow) -> bool {
    let mut dock = CURRENT_DOCK.load(Ordering::Relaxed);

    let (Ok(pos), Ok(size), Ok(Some(monitor))) = (
        window.outer_position().or_else(|_| window.inner_position()),
        window.outer_size().or_else(|_| window.inner_size()),
        window.current_monitor(),
    ) else {
        return false;
    };

    let screen = monitor.position();
    let screen_size = monitor.size();
    let screen_left = screen.x;
    let screen_top = screen.y;
    let screen_right = screen.x + screen_size.width as i32;
    let rect_left = pos.x;
    let rect_top = pos.y;
    let rect_right = pos.x + size.width as i32;
    let rect_bottom = pos.y + size.height as i32;
    let window_width = size.width as i32;

    if dock == 0 {
        let edge_docking = app_handle
            .try_state::<SettingsState>()
            .is_some_and(|s| s.edge_docking.load(Ordering::Relaxed));
        dock = if edge_docking {
            infer_edge_dock_from_geometry(
                rect_left,
                rect_top,
                rect_right,
                screen_left,
                screen_top,
                screen_right,
            )
        } else {
            // Without edge docking a window by the screen edge was just left
            // there, and showing it moves it to the mouse if that is on. Only
            // one still pushed off screen (tucked before docking was turned
            // off) needs pulling back.
            offscreen_tuck_edge(
                rect_left,
                rect_right,
                rect_bottom,
                screen_left,
                screen_top,
                screen_right,
            )
        };
    }

    if dock == 0 {
        return false;
    }

    match dock {
        1 => {
            let _ = window.set_position(tauri::PhysicalPosition::new(rect_left, screen_top));
        }
        2 => {
            let _ = window.set_position(tauri::PhysicalPosition::new(screen_left, rect_top));
        }
        3 => {
            let _ = window.set_position(tauri::PhysicalPosition::new(
                screen_right - window_width,
                rect_top,
            ));
        }
        _ => return false,
    }

    let _ = window.show();
    IS_HIDDEN.store(false, Ordering::Relaxed);
    CURRENT_DOCK.store(0, Ordering::Relaxed);
    persist_edge_dock(app_handle, 0);
    restore_pin_state_after_edge_expand(app_handle, window);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    LAST_SHOW_TIMESTAMP.store(now, Ordering::Relaxed);
    NAVIGATION_ENABLED.store(true, Ordering::SeqCst);

    true
}

fn setup_main_window(app: &App, s: &StartupSettings) {
    let effective_pinned = s.window_pinned;
    WINDOW_PINNED.store(effective_pinned, Ordering::Relaxed);

    let restore_dock = if s.edge_docking {
        parse_dock_token(&s.last_edge_dock)
    } else {
        0
    };

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
        #[cfg(target_os = "linux")]
        let _ = window.set_focusable(true);
        // macOS: avoid set_focusable after NSPanel conversion (tao ivar panic).

        // macOS: Space flags before panel conversion.
        #[cfg(target_os = "macos")]
        set_window_all_spaces(&window);

        // Drop WebView2 browser accelerators (Ctrl+F find, F1 help, reload…) so
        // they cannot collide with TieZ / third-party global hotkeys (#159).
        #[cfg(target_os = "windows")]
        crate::infrastructure::windows_api::webview_keys::disable_browser_accelerator_keys_for_window(
            &window,
        );
    }

    // Handle silent start
    let args: Vec<String> = std::env::args().collect();
    let is_autostart =
        args.contains(&"--autostart".to_string()) || args.contains(&"--minimized".to_string());

    let should_show_main = !is_autostart && !s.silent_start;

    if should_show_main {
        if let Some(window) = app.get_webview_window("main") {
            #[cfg(not(target_os = "macos"))]
            {
                #[cfg(not(target_os = "windows"))]
                crate::infrastructure::macos_api::window::set_window_focusable(&window, true);
            }
            let _ = window.show();

            #[cfg(target_os = "macos")]
            {
                // Convert AFTER final show/focusable work — to_panel drops tao ivars.
                if let Err(err) =
                    crate::infrastructure::macos_api::window::setup_clipboard_panel(&window)
                {
                    eprintln!("[macos-overlay] setup_clipboard_panel failed: {err}");
                }
                crate::infrastructure::macos_api::window::configure_overlay_space_behavior(
                    &window,
                );
                let window_spaces = window.clone();
                std::thread::spawn(move || {
                    for delay_ms in [50_u64, 200, 600, 1200] {
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                        crate::infrastructure::macos_api::window::configure_overlay_space_behavior_async(
                            &window_spaces,
                        );
                    }
                });

                if restore_dock != 0 {
                    // The window was docked when the app last quit. Skip set_focus
                    // (which would pull the offscreen window back onscreen) and
                    // re-apply the dock position. Retry briefly in case the
                    // window-state plugin restores geometry asynchronously.
                    CURRENT_DOCK.store(restore_dock, Ordering::Relaxed);
                    IS_HIDDEN.store(true, Ordering::Relaxed);
                    let _ = window.set_always_on_top(true);
                    apply_dock_position_on_main(&window, restore_dock);

                    let window_clone = window.clone();
                    std::thread::spawn(move || {
                        for _ in 0..6 {
                            std::thread::sleep(std::time::Duration::from_millis(80));
                            apply_dock_position_on_main(&window_clone, restore_dock);
                        }
                    });
                } else {
                    let _ = window.set_focus();
                }
            }

            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            let _ = window.set_focus();

            maybe_open_devtools(&window);
        }
    } else {
        // The main window is created visible (tauri.conf.json), so autostart
        // and silent start must hide it, or it pops up at login (#107). On
        // macOS this has to happen before the panel conversion below.
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
        // Not showing on startup, but ensure window is focusable when it does appear
        #[cfg(not(target_os = "windows"))]
        if let Some(window) = app.get_webview_window("main") {
            #[cfg(not(target_os = "macos"))]
            crate::infrastructure::macos_api::window::set_window_focusable(&window, true);
            #[cfg(target_os = "macos")]
            {
                if let Err(err) =
                    crate::infrastructure::macos_api::window::setup_clipboard_panel(&window)
                {
                    eprintln!("[macos-overlay] setup_clipboard_panel failed: {err}");
                }
            }
        }
    }

    let _ = restore_dock;
}

/// New installs launch at login. Runs once, on the first start of a fresh
/// database (see `database::AUTOSTART_PENDING_KEY`); upgrades never touch the
/// user's login items.
fn enable_autostart_on_first_run(app_handle: &AppHandle, db_state: &DbState) {
    if db_state.settings_repo.get(database::AUTOSTART_PENDING_KEY).ok().flatten().is_none() {
        return;
    }
    let done = || {
        if let Ok(conn) = db_state.conn.lock() {
            let _ = conn.execute("DELETE FROM settings WHERE key = ?1", [database::AUTOSTART_PENDING_KEY]);
        }
    };
    // Dev builds and the portable exe move around; a login item would point
    // at a stale path.
    if cfg!(debug_assertions) || cfg!(feature = "portable") {
        return;
    }
    // Run from the mounted DMG or a quarantine copy (App Translocation): the
    // path is temporary, so wait for a start from the installed location.
    #[cfg(target_os = "macos")]
    if let Ok(exe) = std::env::current_exe() {
        let path = exe.to_string_lossy();
        if path.starts_with("/Volumes/") || path.contains("/AppTranslocation/") {
            return;
        }
    }
    use crate::app::commands::system_cmd::{apply_autostart, is_autostart_enabled};
    // apply_autostart ignores the launcher's error on macOS; check the result.
    let _ = apply_autostart(app_handle, true);
    if is_autostart_enabled(app_handle.clone()).unwrap_or(false) {
        done();
    } else if login_item_switched_off() {
        // Registered, but the user switched TieZ off in System Settings
        // before (a reinstall): their choice, and nothing left to retry.
        done();
    } else {
        eprintln!(">>> [STARTUP] enabling launch at login failed; retrying next start");
    }
}

#[cfg(target_os = "macos")]
fn login_item_switched_off() -> bool {
    use crate::infrastructure::macos_api::login_item;
    login_item::available() && login_item::requires_approval()
}

#[cfg(not(target_os = "macos"))]
fn login_item_switched_off() -> bool {
    false
}

/// Moves a login item made the old way (~/Library/LaunchAgents, shown under
/// the developer's name) to the in-bundle agent shown as TieZ. Only when it
/// was on: an upgrade never turns launch at login on by itself.
#[cfg(target_os = "macos")]
fn migrate_macos_login_item(app_handle: &AppHandle) {
    use crate::infrastructure::macos_api::login_item;
    use tauri_plugin_autostart::ManagerExt;
    if cfg!(debug_assertions) || !login_item::available() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let path = exe.to_string_lossy();
        if path.starts_with("/Volumes/") || path.contains("/AppTranslocation/") {
            return;
        }
    }
    let autolaunch = app_handle.autolaunch();
    if !autolaunch.is_enabled().unwrap_or(false) {
        return;
    }
    match login_item::set_enabled(true) {
        Ok(()) => {
            let _ = autolaunch.disable();
        }
        // Moved, but switched off in System Settings: the user's choice.
        Err(_) if login_item::requires_approval() => {
            let _ = autolaunch.disable();
        }
        // Keep the old item rather than lose launch at login.
        Err(error) => eprintln!(">>> [STARTUP] moving the login item failed: {error}"),
    }
}

fn start_services(app: &App, s: &StartupSettings, app_handle: AppHandle) {
    #[cfg(target_os = "macos")]
    crate::infrastructure::macos_api::window_tracker::start_window_tracking(app_handle.clone());
    #[cfg(target_os = "windows")]
    crate::infrastructure::windows_api::window_tracker::start_window_tracking(app_handle.clone());
    #[cfg(target_os = "macos")]
    crate::infrastructure::macos_api::paste_key_monitor::start_paste_key_monitor();
    crate::services::clipboard::start_clipboard_monitor(app_handle.clone());
    if s.sound_enabled {
        crate::services::ui_sound::preload_ui_sounds(s.sound_volume);
    }
    crate::services::mqtt_sub::start_mqtt_client(app_handle.clone());
    crate::services::cloud_sync::start_cloud_sync_client(app_handle.clone());
    crate::services::network_recovery::start_network_recovery(app_handle.clone());
    start_edge_docking_monitor(app_handle.clone());
    crate::app::light_mode::start_idle_watcher(app_handle.clone());
    crate::app::light_mode::seed_hidden_if_needed(&app_handle);

    let db_state = app.state::<DbState>();
    #[cfg(target_os = "macos")]
    migrate_macos_login_item(&app_handle);
    enable_autostart_on_first_run(&app_handle, &db_state);
    #[cfg(target_os = "windows")]
    if crate::app::commands::system_cmd::check_is_admin() {
        std::thread::spawn(crate::infrastructure::windows_api::elevated_autostart::refresh_outdated_task);
    }
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
    if let Err(error) = crate::app::commands::sync_registered_hotkeys(&app_handle) {
        crate::error!(">>> [HOTKEY] Initial registration failed: {error}");
    }

    #[cfg(target_os = "windows")]
    {
        if db_state
            .settings_repo
            .get("app.use_win_v_shortcut")
            .unwrap_or(Some("false".to_string()))
            == Some("true".to_string())
        {
            if !crate::app::commands::system_cmd::get_registry_win_v_optimized_status() {
                let _ = crate::app::commands::system_cmd::trigger_registry_win_v_optimization(true);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn visibility_ratio_on_monitor(
    rect_left: i32,
    rect_top: i32,
    rect_right: i32,
    rect_bottom: i32,
    screen_left: i32,
    screen_top: i32,
    screen_right: i32,
    screen_bottom: i32,
) -> f64 {
    let vis_left = rect_left.max(screen_left);
    let vis_top = rect_top.max(screen_top);
    let vis_right = rect_right.min(screen_right);
    let vis_bottom = rect_bottom.min(screen_bottom);
    let iw = (vis_right.saturating_sub(vis_left)).max(0) as u64;
    let ih = (vis_bottom.saturating_sub(vis_top)).max(0) as u64;
    let visible_area = iw.saturating_mul(ih);
    let ww = (rect_right.saturating_sub(rect_left)).max(1) as u64;
    let wh = (rect_bottom.saturating_sub(rect_top)).max(1) as u64;
    let total_area = ww.saturating_mul(wh).max(1);
    visible_area as f64 / total_area as f64
}

/// A screen in the units screens are laid out in: points on macOS, where each
/// screen's pixels are scaled by its own factor, pixels on Windows.
#[derive(Clone, Copy, Debug)]
struct LayoutRect {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

fn layout_scale(monitor: &tauri::Monitor) -> f64 {
    if cfg!(target_os = "macos") {
        monitor.scale_factor()
    } else {
        1.0
    }
}

fn layout_rect(monitor: &tauri::Monitor) -> LayoutRect {
    let scale = layout_scale(monitor);
    let pos = monitor.position();
    let size = monitor.size();
    LayoutRect {
        left: pos.x as f64 / scale,
        top: pos.y as f64 / scale,
        right: (pos.x as f64 + size.width as f64) / scale,
        bottom: (pos.y as f64 + size.height as f64) / scale,
    }
}

/// Whether another screen borders `screen` along the `dock` edge (1 top,
/// 2 left, 3 right) where the window lies, `from..to` along that edge.
fn edge_borders_screen(
    screen: LayoutRect,
    others: &[LayoutRect],
    dock: i32,
    from: f64,
    to: f64,
) -> bool {
    const SLACK: f64 = 2.0;
    others.iter().any(|other| {
        let (touches, other_from, other_to) = match dock {
            1 => ((other.bottom - screen.top).abs() <= SLACK, other.left, other.right),
            2 => ((other.right - screen.left).abs() <= SLACK, other.top, other.bottom),
            3 => ((other.left - screen.right).abs() <= SLACK, other.top, other.bottom),
            _ => return false,
        };
        touches && other_from < to && other_to > from
    })
}

/// Whether the window may tuck into `dock`'s edge of `monitor`. Not where
/// another screen continues: tucked past that edge the window sits on the
/// neighbour in plain sight, which then counts as its screen, so the tuck
/// can't be revealed (it stuck on Windows and bounced on macOS).
fn can_dock_on_edge(
    window: &tauri::WebviewWindow,
    monitor: &tauri::Monitor,
    dock: i32,
    rect: (i32, i32, i32, i32),
) -> bool {
    let Ok(monitors) = window.available_monitors() else {
        return true;
    };
    let others: Vec<LayoutRect> = monitors.iter().map(layout_rect).collect();
    let scale = layout_scale(monitor);
    let (left, top, right, bottom) = rect;
    let (from, to) = if dock == 1 { (left, right) } else { (top, bottom) };
    !edge_borders_screen(
        layout_rect(monitor),
        &others,
        dock,
        from as f64 / scale,
        to as f64 / scale,
    )
}

#[cfg(test)]
mod edge_tests {
    use super::{edge_borders_screen, LayoutRect};

    fn rect(left: f64, top: f64, width: f64, height: f64) -> LayoutRect {
        LayoutRect { left, top, right: left + width, bottom: top + height }
    }

    #[test]
    fn edges_shared_with_another_screen_are_found() {
        let main = rect(0.0, 0.0, 1440.0, 900.0);
        let right = rect(1440.0, 0.0, 1920.0, 1080.0);
        let screens = [main, right];
        assert!(edge_borders_screen(main, &screens, 3, 100.0, 700.0));
        assert!(edge_borders_screen(right, &screens, 2, 100.0, 700.0));
        // Outer edges.
        assert!(!edge_borders_screen(main, &screens, 2, 100.0, 700.0));
        assert!(!edge_borders_screen(main, &screens, 1, 100.0, 500.0));
        assert!(!edge_borders_screen(right, &screens, 3, 100.0, 700.0));
        // The taller screen's left edge continues only down to 900.
        assert!(!edge_borders_screen(right, &screens, 2, 920.0, 1070.0));
    }

    #[test]
    fn a_screen_above_blocks_the_top_edge() {
        let main = rect(0.0, 0.0, 1920.0, 1080.0);
        let above = rect(200.0, -1080.0, 1920.0, 1080.0);
        let screens = [main, above];
        assert!(edge_borders_screen(main, &screens, 1, 600.0, 1000.0));
        assert!(!edge_borders_screen(main, &screens, 1, 0.0, 150.0));
    }
}

/// Drop stale edge-hide state when it disagrees with real geometry (resume, monitor layout, startup restore).
#[cfg(target_os = "macos")]
fn reconcile_stale_edge_hide(
    rect_left: i32,
    rect_top: i32,
    rect_right: i32,
    rect_bottom: i32,
    screen_left: i32,
    screen_top: i32,
    screen_right: i32,
    screen_bottom: i32,
) {
    if !IS_HIDDEN.load(Ordering::Relaxed) {
        return;
    }
    let dock = CURRENT_DOCK.load(Ordering::Relaxed);
    if dock == 0 {
        IS_HIDDEN.store(false, Ordering::Relaxed);
        return;
    }
    let ratio = visibility_ratio_on_monitor(
        rect_left,
        rect_top,
        rect_right,
        rect_bottom,
        screen_left,
        screen_top,
        screen_right,
        screen_bottom,
    );
    // Truly tucked windows expose only a thin strip (~1–5% area). Much more visible ⇒ flags are stale.
    if ratio > 0.18 {
        IS_HIDDEN.store(false, Ordering::Relaxed);
        CURRENT_DOCK.store(0, Ordering::Relaxed);
    }
}

#[cfg(target_os = "macos")]
fn start_edge_docking_monitor(app_handle: AppHandle) {
    std::thread::spawn(move || {
        // The docked state and offscreen position are applied during
        // setup_main_window (driven by the persisted `app.last_edge_dock` value).
        // Wait briefly so the window-state plugin's async restore can settle
        // before reconciliation runs, otherwise a partially-restored geometry
        // could wipe the just-restored docked state.
        let startup_grace_until = std::time::Instant::now()
            + std::time::Duration::from_millis(if IS_HIDDEN.load(Ordering::Relaxed) {
                900
            } else {
                0
            });
        loop {
            std::thread::sleep(std::time::Duration::from_millis(150));

            let in_startup_grace = std::time::Instant::now() < startup_grace_until;

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
                        persist_edge_dock(&app_handle, 0);
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

            if !in_startup_grace {
                reconcile_stale_edge_hide(
                    rect_left,
                    rect_top,
                    rect_right,
                    rect_bottom,
                    screen_left,
                    screen_top,
                    screen_right,
                    screen_bottom,
                );
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

            // Grace period after showing — but never block mouse-reveal while tucked.
            if !is_hidden_by_edge && now.saturating_sub(last_show) < 500 {
                continue;
            }

            // In this screen's pixels, like its edges and the window; tao
            // scales the raw cursor by the primary screen's factor instead.
            let monitor_scale = monitor.scale_factor();
            let (cursor_x, cursor_y) = crate::app::window_manager::global_cursor_layout(&window)
                .map(|(x, y)| ((x * monitor_scale).round() as i32, (y * monitor_scale).round() as i32))
                .unwrap_or((-9999, -9999));

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
                    persist_edge_dock(&app_handle, 0);
                }
                continue;
            }

            let hide_size = 3;
            let window_rect = (rect_left, rect_top, rect_right, rect_bottom);
            let mut dock = DockPosition::None;
            if rect_top <= screen_top + threshold
                && can_dock_on_edge(&window, &monitor, 1, window_rect)
            {
                dock = DockPosition::Top;
            } else if rect_left <= screen_left + threshold
                && can_dock_on_edge(&window, &monitor, 2, window_rect)
            {
                dock = DockPosition::Left;
            } else if rect_right >= screen_right - threshold
                && can_dock_on_edge(&window, &monitor, 3, window_rect)
            {
                dock = DockPosition::Right;
            }

            if is_hidden_by_edge {
                if is_mouse_in {
                    let _ = expand_from_edge_dock(&app_handle, &window);
                }
            } else if dock != DockPosition::None {
                // Keep the window fully visible while cursor is still inside it.
                if is_mouse_in {
                    continue;
                }

                if !IS_HIDDEN.load(Ordering::Relaxed) {
                    // Keep top-most z-order for the edge sliver without flipping WINDOW_PINNED.
                    if !WINDOW_PINNED.load(Ordering::Relaxed) {
                        let _ = window.set_always_on_top(true);
                    }

                    let window_height = rect_bottom - rect_top;
                    let window_width = rect_right - rect_left;
                    let mut new_dock = 0i32;
                    match dock {
                        DockPosition::Top => {
                            let _ = window.set_position(tauri::PhysicalPosition::new(
                                rect_left,
                                screen_top - window_height + hide_size,
                            ));
                            CURRENT_DOCK.store(1, Ordering::Relaxed);
                            new_dock = 1;
                        }
                        DockPosition::Left => {
                            let _ = window.set_position(tauri::PhysicalPosition::new(
                                screen_left - window_width + hide_size,
                                rect_top,
                            ));
                            CURRENT_DOCK.store(2, Ordering::Relaxed);
                            new_dock = 2;
                        }
                        DockPosition::Right => {
                            let _ = window.set_position(tauri::PhysicalPosition::new(
                                screen_right - hide_size,
                                rect_top,
                            ));
                            CURRENT_DOCK.store(3, Ordering::Relaxed);
                            new_dock = 3;
                        }
                        DockPosition::None => {}
                    }
                    IS_HIDDEN.store(true, Ordering::Relaxed);
                    if new_dock != 0 {
                        persist_edge_dock(&app_handle, new_dock);
                    }
                }
            } else if IS_HIDDEN.load(Ordering::Relaxed) {
                IS_HIDDEN.store(false, Ordering::Relaxed);
                CURRENT_DOCK.store(0, Ordering::Relaxed);
                persist_edge_dock(&app_handle, 0);

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
                    crate::infrastructure::macos_api::window::set_window_focusable(&window, true);
                    let _ = app_handle.emit("window-pinned-changed", user_pinned);
                }
            }
        }
    });
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

                // Grace period after showing — but never block mouse-reveal while tucked.
                if !is_hidden_by_edge && now.saturating_sub(last_show) < 500 {
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

                if !is_hidden_by_edge && adopt_offscreen_tuck(&app_handle, &window) {
                    continue;
                }

                if !is_hidden_by_edge && !is_on_current_monitor {
                    if IS_HIDDEN.load(Ordering::Relaxed) {
                        IS_HIDDEN.store(false, Ordering::Relaxed);
                        CURRENT_DOCK.store(0, Ordering::Relaxed);
                    }
                    continue;
                }

                let hide_size = 3;

                let window_rect = (rect.left, rect.top, rect.right, rect.bottom);
                let mut dock = DockPosition::None;
                if rect.top <= screen_top + threshold
                    && can_dock_on_edge(&window, &monitor, 1, window_rect)
                {
                    dock = DockPosition::Top;
                } else if rect.left <= screen_left + threshold
                    && can_dock_on_edge(&window, &monitor, 2, window_rect)
                {
                    dock = DockPosition::Left;
                } else if rect.right >= screen_right - threshold
                    && can_dock_on_edge(&window, &monitor, 3, window_rect)
                {
                    dock = DockPosition::Right;
                }

                if is_hidden_by_edge {
                    if is_mouse_in {
                        let _ = expand_from_edge_dock(&app_handle, &window);
                    }
                } else if dock != DockPosition::None {
                    // Don't dock while dragging (Left Mouse Button down)
                    let is_lbutton_down = unsafe { (GetAsyncKeyState(0x01) as u16 & 0x8000) != 0 };
                    if is_mouse_in || is_lbutton_down {
                        continue;
                    }

                    if !IS_HIDDEN.load(Ordering::Relaxed) {
                        // Edge tuck needs top-most z-order, but must not flip WINDOW_PINNED:
                        // click-outside-to-hide checks WINDOW_PINNED (user pin only).
                        if !WINDOW_PINNED.load(Ordering::Relaxed) {
                            set_edge_dock_topmost(&window, true);
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

                    // Restore always-on-top / focus from user pin (WINDOW_PINNED was never toggled by tuck).
                    let user_pinned = WINDOW_PINNED.load(Ordering::Relaxed);
                    set_edge_dock_topmost(&window, user_pinned);
                    #[cfg(windows)]
                    if let Ok(hwnd) = window.hwnd() {
                        unsafe {
                            let ex_style = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
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
                }
            }
        }
    });
}

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

    // The window's ping id (see resolve_ping_id). On the first start after an
    // update it isn't stored yet: only the window pings, and stores it, so
    // the device keeps the id it had.
    let Some(ping_id) = repo
        .get(crate::app::system::PING_ID_KEY)
        .unwrap_or(None)
        .as_deref()
        .and_then(crate::app::system::valid_ping_id)
    else {
        return;
    };

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    if repo.get("app.last_ping_date").unwrap_or(None).as_deref() != Some(&today) {
        let _ = repo.set("app.last_ping_date", &today);
        let version = app.package_info().version.to_string();
        
        let ping_url_base = crate::build_config::announcement_ping_url();
        
        std::thread::spawn(move || {
            let _ = reqwest::blocking::get(format!(
                "{}?v={}&id={}",
                ping_url_base, version, ping_id
            ));
        });
    }
}

fn setup_tray(app: &App, hide_tray: bool) {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};

    let show_i = MenuItem::with_id(app, "show", "打开窗口", true, None::<&str>).unwrap();
    let quit_i = MenuItem::with_id(app, "quit", "退出应用", true, None::<&str>).unwrap();
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
                let _ = crate::app::window_manager::focus_clipboard_window(app.clone());
            } else if event.id.as_ref() == "quit" {
                crate::app::light_mode::request_app_exit(app);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                // Same intentional-open path as Dock reopen (make-key + LAST_SHOW).
                let _ = crate::app::window_manager::focus_clipboard_window(tray.app_handle().clone());
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
        .unwrap_or(Some(crate::build_config::default_theme_id().to_string()))
        .unwrap_or_else(|| crate::build_config::default_theme_id().to_string());
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
fn init_win32_hooks(app: &App) {
    if let Some(window) = app.get_webview_window("main") {
        crate::app::hooks::remember_main_hwnd(&window);
    }
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
fn setup_taskbar_listener(_app: &App) {
    unsafe {
        let msg = RegisterWindowMessageW(windows::core::w!("TaskbarCreated"));
        if msg != 0 {
            TASKBAR_CREATED_MSG.store(msg, Ordering::Relaxed);
        }
    }
    // Not the main window: light mode destroys and recreates it.
    crate::app::system::start_system_event_window();
}

fn plain_paste_goes_to_selection(app: &AppHandle) -> bool {
    NAVIGATION_MODE_ACTIVE.load(Ordering::SeqCst)
        && !IS_HIDDEN.load(Ordering::SeqCst)
        && app
            .get_webview_window("main")
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false)
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

    } else if state == ShortcutState::Released {
        if let Ok(seq_s) = {
            let val = settings.sequential_paste_hotkey.lock().unwrap().clone();
            val.replace("Win", "Super").parse::<Shortcut>()
        } {
            if shortcut == &seq_s {
                if settings.sequential_mode.load(Ordering::Relaxed) {
                    crate::services::paste_queue::paste_next_step(app.clone());
                }
            }
        }

        if let Ok(plain_s) = {
            let val = settings.plain_paste_hotkey.lock().unwrap().clone();
            val.replace("Win", "Super").parse::<Shortcut>()
        } {
            if shortcut == &plain_s {
                // A registered hotkey never reaches the window, so with an
                // entry picked by Up/Down the window pastes that one;
                // otherwise the newest entry is pasted.
                if plain_paste_goes_to_selection(app) {
                    let _ = app.emit("navigation-action", "plain");
                } else {
                    crate::services::clipboard_ops::paste_latest_plain(app.clone());
                }
                return;
            }
        }

        if let Ok(search_s) = {
            let val = settings.search_hotkey.lock().unwrap().clone();
            val.replace("Win", "Super").parse::<Shortcut>()
        } {
            if shortcut == &search_s {
                let _ = app.emit("focus-search", ());
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
            #[cfg(target_os = "macos")]
            if !crate::infrastructure::macos_api::window::hide_clipboard_panel(
                window.app_handle(),
            ) {
                let _ = window.hide();
            }
            #[cfg(not(target_os = "macos"))]
            let _ = window.hide();
            IS_HIDDEN.store(false, Ordering::Relaxed);
            CURRENT_DOCK.store(0, Ordering::Relaxed);
            NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
            NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
            crate::app::light_mode::on_window_hidden();
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
fn handle_blur(_window: &tauri::Window) {
    if is_blur_ignored() || WINDOW_PINNED.load(Ordering::Relaxed) {
        return;
    }

    // The NSPanel resign-key delegate owns macOS auto-hide. Keep Tauri's
    // duplicate focus event passive so it cannot race the panel lifecycle.
}

#[cfg(target_os = "windows")]
fn handle_blur(window: &tauri::Window) {
    if is_blur_ignored() || WINDOW_PINNED.load(Ordering::Relaxed) {
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
        let tucked_by_edge = IS_HIDDEN.load(Ordering::Relaxed)
            || CURRENT_DOCK.load(Ordering::Relaxed) != 0;
        if !down && !tucked_by_edge && matches!(w.is_focused(), Ok(false)) {
            if !is_blur_ignored() && !WINDOW_PINNED.load(Ordering::Relaxed) {
                let _ = w.app_handle().emit("force-hide-compact-preview", ());
                let _ = w.hide();
                NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
                release_modifier_keys();
                let _ = restore_previous_app_focus(w.app_handle().clone());
            }
        }
    });
}

#[cfg(test)]
mod silent_start_tests {
    use super::{migrate_silent_start_once, SILENT_START_MAC_RESET_KEY};
    use crate::infrastructure::repository::settings_repo::{
        SettingsRepository, SqliteSettingsRepository,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn mac_reset_runs_once_and_then_keeps_the_users_choice() {
        let conn = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()));
        conn.lock()
            .unwrap()
            .execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        let repo = SqliteSettingsRepository::new(conn);
        repo.set("app.silent_start", "true").unwrap();
        migrate_silent_start_once(&repo);
        assert_eq!(repo.get("app.silent_start").unwrap().as_deref(), Some("false"));
        assert!(repo.get(SILENT_START_MAC_RESET_KEY).unwrap().is_some());

        repo.set("app.silent_start", "true").unwrap();
        migrate_silent_start_once(&repo);
        assert_eq!(repo.get("app.silent_start").unwrap().as_deref(), Some("true"));
    }
}
