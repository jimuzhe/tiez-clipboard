use crate::global_state::*;
#[cfg(feature = "devtools")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

#[cfg(feature = "devtools")]
static DEVTOOLS_OPENED_ONCE: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "devtools")]
fn parse_env_truthy(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|v| {
        let t = v.trim().to_ascii_lowercase();
        t == "1" || t == "true" || t == "yes" || t == "on"
    })
}

#[cfg(feature = "devtools")]
pub fn should_auto_open_devtools() -> bool {
    parse_env_truthy("TIEZ_AUTO_OPEN_DEVTOOLS").unwrap_or(false)
}

#[cfg(not(feature = "devtools"))]
pub fn should_auto_open_devtools() -> bool {
    false
}

#[cfg(feature = "devtools")]
pub fn maybe_open_devtools(window: &WebviewWindow) {
    if !should_auto_open_devtools() || DEVTOOLS_OPENED_ONCE.swap(true, Ordering::Relaxed) {
        return;
    }

    window.open_devtools();

    let app_handle = window.app_handle().clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(450));
        if let Some(win) = app_handle.get_webview_window("main") {
            win.open_devtools();
        }
    });
}

#[cfg(not(feature = "devtools"))]
pub fn maybe_open_devtools(_window: &WebviewWindow) {}

const EDGE_DOCK_VISIBLE_SIZE: i32 = 3;

fn revealed_side_x(dock: i32, tucked_x: i32, window_width: i32) -> Option<i32> {
    match dock {
        2 => Some(tucked_x + window_width - EDGE_DOCK_VISIBLE_SIZE),
        3 => Some(tucked_x - window_width + EDGE_DOCK_VISIBLE_SIZE),
        _ => None,
    }
}

fn reveal_side_docked_window(window: &WebviewWindow, dock: i32) {
    let position = window.outer_position().or_else(|_| window.inner_position());
    let size = window.outer_size().or_else(|_| window.inner_size());
    let (Ok(position), Ok(size)) = (position, size) else {
        return;
    };
    let Some(x) = revealed_side_x(dock, position.x, size.width as i32) else {
        return;
    };

    let _ = window.set_position(tauri::PhysicalPosition::new(x, position.y));
}

pub fn toggle_window(app_handle: &AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        let is_hidden_by_edge = IS_HIDDEN.load(Ordering::Relaxed);

        if is_visible && !is_hidden_by_edge {
            let _ = app_handle.emit("force-hide-compact-preview", ());
            let pinned = WINDOW_PINNED.load(Ordering::Relaxed);
            let _ = window.set_always_on_top(pinned);
            #[cfg(not(target_os = "windows"))]
            let _ = window.set_focusable(false);
            let _ = window.hide();
            let _ = restore_previous_app_focus(app_handle.clone());

            IS_HIDDEN.store(false, Ordering::Relaxed);
            NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
            NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
            return;
        }

        // Before showing TieZ, record whichever app currently has focus so
        // we can re-activate it when the user clicks a clipboard item to paste.
        let (prev_app_name, prev_app_pid) =
            crate::infrastructure::macos_api::window::get_active_app_info();
        let self_pid = std::process::id();
        if !prev_app_name.is_empty() && !prev_app_name.eq_ignore_ascii_case("TieZ") {
            crate::global_state::set_last_active_app_name(prev_app_name);
        }
        if let Ok(pid) = prev_app_pid.parse::<u32>() {
            if pid != 0 && pid != self_pid {
                crate::global_state::LAST_ACTIVE_APP_PID.store(pid, Ordering::Relaxed);
            }
        }

        if is_hidden_by_edge {
            let dock = CURRENT_DOCK.load(Ordering::Relaxed);
            reveal_side_docked_window(&window, dock);
        }
        IS_HIDDEN.store(false, Ordering::Relaxed);
        NAVIGATION_ENABLED.store(true, Ordering::SeqCst);
        CURRENT_DOCK.store(0, Ordering::Relaxed);

        // Basic toggle: show window
        let pinned = WINDOW_PINNED.load(Ordering::Relaxed);
        #[cfg(target_os = "macos")]
        let _ = window.set_always_on_top(true);
        #[cfg(not(target_os = "macos"))]
        let _ = window.set_always_on_top(pinned);
        #[cfg(target_os = "windows")]
        let _ = window.set_focusable(!pinned);
        #[cfg(target_os = "macos")]
        let _ = window.set_focusable(!pinned);
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let _ = window.set_focusable(false);
        let _ = app_handle.emit("window-pinned-changed", pinned);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        LAST_SHOW_TIMESTAMP.store(now, Ordering::Relaxed);

        let _ = window.show();
        #[cfg(target_os = "macos")]
        if !pinned {
            let _ = window.set_focus();
        }
        maybe_open_devtools(&window);
    }
}

#[tauri::command]
pub fn set_navigation_enabled(enabled: bool) -> Result<(), String> {
    NAVIGATION_ENABLED.store(enabled, Ordering::SeqCst);
    if !enabled {
        NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
pub fn set_navigation_mode(active: bool) -> Result<(), String> {
    NAVIGATION_MODE_ACTIVE.store(active, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn activate_window_focus(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        #[cfg(not(target_os = "windows"))]
        let _ = window.set_focusable(true);
        let _ = window.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub fn hide_window_cmd(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = app_handle.emit("force-hide-compact-preview", ());
        if let Some(compact_preview) = app_handle.get_webview_window("compact-preview") {
            let _ = compact_preview.hide();
        }
        let pinned = WINDOW_PINNED.load(Ordering::Relaxed);
        let _ = window.set_always_on_top(pinned);
        #[cfg(not(target_os = "windows"))]
        let _ = window.set_focusable(false);
        let _ = window.hide();
        NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
        NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
        let _ = restore_previous_app_focus(app_handle.clone());
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_window_cmd(app_handle: AppHandle) -> Result<(), String> {
    toggle_window(&app_handle);
    Ok(())
}

#[tauri::command]
pub fn focus_clipboard_window(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        IS_HIDDEN.store(false, Ordering::Relaxed);
        CURRENT_DOCK.store(0, Ordering::Relaxed);
        #[cfg(not(target_os = "windows"))]
        let _ = window.set_focusable(true);
        let _ = window.show();
        let _ = window.set_focus();
        maybe_open_devtools(&window);
        Ok(())
    } else {
        Err("Main window not found".to_string())
    }
}

#[tauri::command]
pub fn restore_previous_app_focus(_app_handle: AppHandle) -> Result<(), String> {
    // macOS focus restoration usually happens automatically via OS
    // Or involves NSRunningApplication
    Ok(())
}

pub fn release_modifier_keys() {
    // Placeholder for macOS
}

pub fn is_main_window_focused() -> bool {
    IS_MAIN_WINDOW_FOCUSED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::revealed_side_x;

    #[test]
    fn reveals_left_docked_window_at_its_previous_edge_position() {
        assert_eq!(revealed_side_x(2, -357, 360), Some(0));
    }

    #[test]
    fn reveals_right_docked_window_at_its_previous_edge_position() {
        assert_eq!(revealed_side_x(3, 1917, 360), Some(1560));
    }

    #[test]
    fn ignores_unknown_dock_position() {
        assert_eq!(revealed_side_x(0, 0, 360), None);
    }
}
