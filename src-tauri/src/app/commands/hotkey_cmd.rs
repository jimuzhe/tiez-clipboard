use crate::app_state::SettingsState;
use crate::error::{AppError, AppResult};
use crate::global_state::HOTKEY_STRING;
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

const QUICK_PASTE_KEYS: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];

fn is_win_v_hotkey(hotkey: &str) -> bool {
    let parts: Vec<String> = hotkey
        .split('+')
        .map(|p| p.trim().to_uppercase())
        .filter(|p| !p.is_empty())
        .collect();

    if parts.is_empty() {
        return false;
    }

    let mut has_win = false;
    let mut has_v = false;

    for part in &parts {
        match part.as_str() {
            "WIN" | "SUPER" | "COMMAND" | "META" => has_win = true,
            "V" => has_v = true,
            _ => return false,
        }
    }

    has_win && has_v
}

fn should_register_main_hotkey(hotkey: &str) -> bool {
    if hotkey.is_empty() {
        return false;
    }

    // Windows disables Explorer's Win+V handler before assigning it to TieZ.
    // On other platforms, avoid turning a migrated Win+V setting into
    // Command+V and intercepting the system paste shortcut.
    cfg!(target_os = "windows") || !is_win_v_hotkey(hotkey)
}

fn parse_shortcut(hotkey: &str) -> Option<Shortcut> {
    if hotkey.is_empty()
        || hotkey.eq_ignore_ascii_case("MouseMiddle")
        || hotkey.eq_ignore_ascii_case("MButton")
    {
        return None;
    }

    hotkey.replace("Win", "Super").parse::<Shortcut>().ok()
}

pub fn normalize_quick_paste_modifier(modifier: &str) -> String {
    match modifier.trim().to_ascii_lowercase().as_str() {
        "ctrl" | "control" => "ctrl".to_string(),
        "alt" | "option" => "alt".to_string(),
        "shift" => "shift".to_string(),
        "win" | "command" | "meta" | "super" => "win".to_string(),
        _ => "disabled".to_string(),
    }
}

fn quick_paste_modifier_prefix(modifier: &str) -> Option<&'static str> {
    match normalize_quick_paste_modifier(modifier).as_str() {
        "ctrl" => Some("Ctrl"),
        "alt" => Some("Alt"),
        "shift" => Some("Shift"),
        "win" => Some("Super"),
        _ => None,
    }
}

fn quick_paste_shortcut(modifier: &str, index: usize) -> Option<Shortcut> {
    let prefix = quick_paste_modifier_prefix(modifier)?;
    let key = QUICK_PASTE_KEYS.get(index)?;
    parse_shortcut(&format!("{prefix}+{key}"))
}

pub fn quick_paste_index_from_shortcut(modifier: &str, shortcut: &Shortcut) -> Option<usize> {
    (0..QUICK_PASTE_KEYS.len()).find(|index| {
        quick_paste_shortcut(modifier, *index)
            .as_ref()
            .map(|candidate| candidate == shortcut)
            .unwrap_or(false)
    })
}

fn register_unique_shortcut(
    app_handle: &AppHandle,
    hotkey: &str,
    registered: &mut HashSet<String>,
) -> AppResult<()> {
    let shortcut = parse_shortcut(hotkey).ok_or_else(|| {
        AppError::Validation(format!("快捷键格式不正确，请重新设置。\n当前：{hotkey}"))
    })?;
    let key = format!("{shortcut:?}");
    if registered.insert(key) {
        app_handle
            .global_shortcut()
            .register(shortcut)
            .map_err(|e| AppError::Internal(format!("快捷键设置失败，请换一组按键。\n当前：{hotkey}\n详情：{e}")))?;
    }
    Ok(())
}

pub fn sync_registered_hotkeys(app_handle: &AppHandle) -> AppResult<()> {
    let _ = app_handle.global_shortcut().unregister_all();

    let settings = app_handle.state::<SettingsState>();
    let main_hotkey = settings.main_hotkey.lock().unwrap().clone();
    let sequential_hotkey = settings.sequential_paste_hotkey.lock().unwrap().clone();
    let plain_hotkey = settings.plain_paste_hotkey.lock().unwrap().clone();
    let search_hotkey = settings.search_hotkey.lock().unwrap().clone();
    let quick_paste_modifier = settings.quick_paste_modifier.lock().unwrap().clone();
    let sequential_mode = settings.sequential_mode.load(Ordering::Relaxed);
    let mut registered = HashSet::new();
    // Register every shortcut even if one fails (e.g. taken by another app):
    // an early return used to leave the later ones unregistered, so pressing
    // them typed a character instead (Option+F typing "ƒ"). Failures are
    // logged to tiez.log; the first one is returned.
    let mut first_error: Option<AppError> = None;
    let mut register = |hotkey: &str| {
        if let Err(err) = register_unique_shortcut(app_handle, hotkey, &mut registered) {
            crate::error!(">>> [HOTKEY] {err}");
            first_error.get_or_insert(err);
        }
    };

    if should_register_main_hotkey(&main_hotkey) {
        register(&main_hotkey);
    }

    // Only bind sequential paste while the mode is enabled (#125).
    // Do not keep Alt+V registered for stale queue items after the mode is turned off.
    if sequential_mode {
        register(&sequential_hotkey);
    }

    for hotkey in [&plain_hotkey, &search_hotkey] {
        if !hotkey.is_empty() {
            register(hotkey);
        }
    }

    if let Some(prefix) = quick_paste_modifier_prefix(&quick_paste_modifier) {
        for key in QUICK_PASTE_KEYS {
            register(&format!("{prefix}+{key}"));
        }
    }

    first_error.map_or(Ok(()), Err)
}

/// `sync_registered_hotkeys` for background threads. The shortcut plugin
/// holds its lock while it waits for the main thread, and the main thread
/// takes that lock to deliver a hotkey press, so a press at the wrong moment
/// froze both. On the main thread the two can't overlap.
pub fn sync_registered_hotkeys_on_main_thread(app_handle: &AppHandle) {
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        let _ = sync_registered_hotkeys(&handle);
    });
}

#[tauri::command]
pub fn register_hotkey(app_handle: AppHandle, hotkey: String) -> AppResult<()> {
    {
        let mut guard = HOTKEY_STRING.lock().unwrap();
        *guard = hotkey.clone();
    }

    if let Some(settings) = app_handle.try_state::<SettingsState>() {
        let mut guard = settings.main_hotkey.lock().unwrap();
        *guard = hotkey.clone();
    }

    let result = sync_registered_hotkeys(&app_handle);
    // "Open TieZ" in the tray menu shows this hotkey.
    crate::app::tray::refresh(&app_handle);
    result
}

/// Shift alone (or no modifier) plus a key that types text: registered
/// globally it swallows that character in every app. Mirrors isTypingHotkey
/// in src/shared/lib/hotkeyKey.ts.
pub fn is_typing_hotkey(hotkey: &str) -> bool {
    let parts: Vec<String> = hotkey
        .split('+')
        .map(|p| p.trim().to_uppercase())
        .filter(|p| !p.is_empty())
        .collect();
    let Some(first) = parts.first() else { return false };
    if first == "MOUSEMIDDLE" || first == "MBUTTON" {
        return false;
    }
    const MODIFIERS: [&str; 10] =
        ["CTRL", "CONTROL", "ALT", "OPTION", "MENU", "WIN", "SUPER", "COMMAND", "CMD", "META"];
    if parts.iter().any(|p| MODIFIERS.contains(&p.as_str())) {
        return false;
    }
    let Some(key) = parts.iter().filter(|p| p.as_str() != "SHIFT").last() else { return false };
    let silent = (key.len() >= 2
        && key.starts_with('F')
        && key[1..].parse::<u8>().map_or(false, |n| (1..=24).contains(&n)))
        || matches!(
            key.as_str(),
            "ESC" | "ESCAPE" | "INSERT" | "DELETE" | "HOME" | "END" | "PAGEUP" | "PAGEDOWN"
                | "UP" | "DOWN" | "LEFT" | "RIGHT" | "ARROWUP" | "ARROWDOWN" | "ARROWLEFT"
                | "ARROWRIGHT" | "PRINTSCREEN" | "SCROLLLOCK" | "PAUSE" | "NUMLOCK"
        );
    !silent
}

#[tauri::command]
pub fn test_hotkey_available(app_handle: AppHandle, hotkey: String) -> AppResult<bool> {
    if hotkey.is_empty()
        || hotkey.eq_ignore_ascii_case("MouseMiddle")
        || hotkey.eq_ignore_ascii_case("MButton")
    {
        return Ok(true);
    }
    if is_typing_hotkey(&hotkey) {
        return Err(AppError::Validation(
            "快捷键需要包含 Ctrl、Option/Alt 或 Command/Win。".to_string(),
        ));
    }

    let normalized = hotkey.replace("Win", "Super");
    let shortcut = normalized
        .parse::<Shortcut>()
        .map_err(|_| AppError::Validation("请重新设置快捷键组合。".to_string()))?;

    match app_handle.global_shortcut().register(shortcut.clone()) {
        Ok(_) => {
            let _ = app_handle.global_shortcut().unregister(shortcut);
            Ok(true)
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            let user_msg = if err_str.contains("AlreadyRegistered") {
                "该快捷键已被其他程序占用".to_string()
            } else {
                "快捷键不可用".to_string()
            };
            Err(AppError::Internal(user_msg))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn typing_hotkeys_are_refused() {
        for h in ["Shift+S", "Shift+1", "C", "Space"] {
            assert!(super::is_typing_hotkey(h), "{h}");
        }
        for h in ["Alt+C", "Command+Shift+V", "Ctrl+1", "F5", "Shift+F5", "MouseMiddle", ""] {
            assert!(!super::is_typing_hotkey(h), "{h}");
        }
    }

    use super::{is_win_v_hotkey, should_register_main_hotkey};

    #[test]
    fn recognizes_win_v_aliases_without_extra_modifiers() {
        assert!(is_win_v_hotkey("Win+V"));
        assert!(is_win_v_hotkey("Super + v"));
        assert!(!is_win_v_hotkey("Ctrl+Win+V"));
        assert!(!is_win_v_hotkey("Win+C"));
    }

    #[test]
    fn registers_win_v_only_on_windows() {
        assert!(should_register_main_hotkey("Alt+C"));
        assert!(!should_register_main_hotkey(""));
        assert_eq!(
            should_register_main_hotkey("Win+V"),
            cfg!(target_os = "windows")
        );
    }
}
