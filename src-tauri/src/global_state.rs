// Global state module
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

pub static GLOBAL_APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();
pub static HOTKEY_STRING: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

// Win+ hotkeys are now handled via tauri-plugin-global-shortcut.

pub static IS_RECORDING: AtomicBool = AtomicBool::new(false);
/// Legacy one-shot blur suppression. New code should use owner-based guards below.
pub static IGNORE_BLUR: AtomicBool = AtomicBool::new(false);
pub static WINDOW_PINNED: AtomicBool = AtomicBool::new(false);
pub static CLIPBOARD_MONITOR_PAUSED: AtomicBool = AtomicBool::new(false);
/// The user paused recording from the tray menu. Separate from
/// CLIPBOARD_MONITOR_PAUSED, which hotkey recording sets and clears on its own.
/// Not saved: a restart records again.
pub static USER_RECORDING_PAUSED: AtomicBool = AtomicBool::new(false);

static BLUR_GUARD_OWNERS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn blur_guard_owners() -> &'static Mutex<HashSet<String>> {
    BLUR_GUARD_OWNERS.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn acquire_blur_guard(owner: &str) -> usize {
    let mut owners = blur_guard_owners()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    owners.insert(owner.to_string());
    owners.len()
}

pub fn release_blur_guard(owner: &str) -> usize {
    let mut owners = blur_guard_owners()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    owners.remove(owner);
    owners.len()
}

pub fn is_blur_ignored() -> bool {
    if IGNORE_BLUR.load(Ordering::Relaxed) {
        return true;
    }
    !blur_guard_owners()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_empty()
}

#[cfg(test)]
mod blur_guard_tests {
    use super::*;

    #[test]
    fn owner_guards_are_nested_and_idempotent() {
        const FIRST: &str = "test:blur-guard:first";
        const SECOND: &str = "test:blur-guard:second";

        IGNORE_BLUR.store(false, Ordering::Relaxed);
        release_blur_guard(FIRST);
        release_blur_guard(SECOND);

        assert_eq!(acquire_blur_guard(FIRST), 1);
        assert_eq!(acquire_blur_guard(FIRST), 1);
        assert_eq!(acquire_blur_guard(SECOND), 2);
        assert!(is_blur_ignored());

        assert_eq!(release_blur_guard(FIRST), 1);
        assert!(is_blur_ignored());
        assert_eq!(release_blur_guard(SECOND), 0);
        assert!(!is_blur_ignored());
    }
}

// For macOS: store the name of the frontmost app before we show TieZ,
// so we can re-activate it before pasting.
pub static LAST_ACTIVE_APP_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub static LAST_ACTIVE_HWND: AtomicUsize = AtomicUsize::new(0);
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
pub static TASKBAR_CREATED_MSG: AtomicU32 = AtomicU32::new(0);

#[cfg(target_os = "windows")]
pub static QUICK_PASTE_DIGIT_MASK: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "windows")]
pub static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "windows")]
pub static HOOK_HANDLE: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
#[cfg(target_os = "windows")]
pub static HOOK_MOUSE_HANDLE: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

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
