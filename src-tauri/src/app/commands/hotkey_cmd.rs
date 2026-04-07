use crate::app_state::SettingsState;
use crate::error::{AppError, AppResult};
use crate::global_state::HOTKEY_STRING;
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

pub fn sync_registered_hotkeys(app_handle: &AppHandle) -> AppResult<()> {
    let _ = app_handle.global_shortcut().unregister_all();

    let settings = app_handle.state::<SettingsState>();
    let main_hotkey = settings.main_hotkey.lock().unwrap().clone();
    let sequential_hotkey = settings.sequential_paste_hotkey.lock().unwrap().clone();
    let rich_hotkey = settings.rich_paste_hotkey.lock().unwrap().clone();
    let search_hotkey = settings.search_hotkey.lock().unwrap().clone();
    let quick_paste_modifier = settings.quick_paste_modifier.lock().unwrap().clone();

    if !main_hotkey.is_empty() && !is_win_v_hotkey(&main_hotkey) {
        if let Some(shortcut) = parse_shortcut(&main_hotkey) {
            let _ = app_handle.global_shortcut().register(shortcut);
        }
    }

    if let Some(shortcut) = parse_shortcut(&sequential_hotkey) {
        let _ = app_handle.global_shortcut().register(shortcut);
    }

    if let Some(shortcut) = parse_shortcut(&rich_hotkey) {
        let _ = app_handle.global_shortcut().register(shortcut);
    }

    if let Some(shortcut) = parse_shortcut(&search_hotkey) {
        let _ = app_handle.global_shortcut().register(shortcut);
    }

    for index in 0..QUICK_PASTE_KEYS.len() {
        if let Some(shortcut) = quick_paste_shortcut(&quick_paste_modifier, index) {
            let _ = app_handle.global_shortcut().register(shortcut);
        }
    }

    Ok(())
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

    sync_registered_hotkeys(&app_handle)
}

#[tauri::command]
pub fn test_hotkey_available(app_handle: AppHandle, hotkey: String) -> AppResult<bool> {
    if hotkey.is_empty()
        || hotkey.eq_ignore_ascii_case("MouseMiddle")
        || hotkey.eq_ignore_ascii_case("MButton")
    {
        return Ok(true);
    }

    let normalized = hotkey.replace("Win", "Super");
    let shortcut = normalized
        .parse::<Shortcut>()
        .map_err(|_| AppError::Validation("快捷键格式无效".to_string()))?;

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
