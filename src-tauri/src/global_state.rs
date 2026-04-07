// Global state module
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64};

pub static GLOBAL_APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();
pub static HOTKEY_STRING: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

// Win+ hotkeys are now handled via tauri-plugin-global-shortcut.

pub static IS_RECORDING: AtomicBool = AtomicBool::new(false);
pub static IGNORE_BLUR: AtomicBool = AtomicBool::new(false);
pub static WINDOW_PINNED: AtomicBool = AtomicBool::new(false);
pub static CLIPBOARD_MONITOR_PAUSED: AtomicBool = AtomicBool::new(false);

// For macOS: store the name of the frontmost app before we show TieZ,
// so we can re-activate it before pasting.
pub static LAST_ACTIVE_APP_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub static LAST_ACTIVE_APP_NAME: std::sync::OnceLock<std::sync::Mutex<String>> =
    std::sync::OnceLock::new();

pub fn get_last_active_app_name() -> String {
    LAST_ACTIVE_APP_NAME
        .get_or_init(|| std::sync::Mutex::new(String::new()))
        .lock()
        .unwrap()
        .clone()
}

pub fn set_last_active_app_name(name: String) {
    let cell = LAST_ACTIVE_APP_NAME.get_or_init(|| std::sync::Mutex::new(String::new()));
    *cell.lock().unwrap() = name;
}

pub static LAST_APP_SET_HASH: AtomicU64 = AtomicU64::new(0);
pub static LAST_APP_SET_HASH_ALT: AtomicU64 = AtomicU64::new(0);
pub static LAST_APP_SET_TIMESTAMP: AtomicU64 = AtomicU64::new(0);
pub static LAST_TOGGLE_TIMESTAMP: AtomicU64 = AtomicU64::new(0);
pub static LAST_SHOW_TIMESTAMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DockPosition {
    None,
    Top,
    Left,
    Right,
}

pub static CURRENT_DOCK: AtomicI32 = AtomicI32::new(0); // 0: None, 1: Top, 2: Left, 3: Right
pub static IS_HIDDEN: AtomicBool = AtomicBool::new(false);
pub static IS_MOUSE_BUTTON_DOWN: AtomicBool = AtomicBool::new(false);
pub static NAVIGATION_ENABLED: AtomicBool = AtomicBool::new(false);
pub static NAVIGATION_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static IS_MAIN_WINDOW_FOCUSED: AtomicBool = AtomicBool::new(false);
