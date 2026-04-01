use tauri::{AppHandle, Manager};
use crate::app_state::SettingsState;
use crate::error::{AppResult, AppError};
use crate::global_state::HOTKEY_STRING;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

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
    
    let _ = app_handle.global_shortcut().unregister_all();
    
    if !hotkey.is_empty() {
        let normalized = hotkey.replace("Win", "Super");
        if hotkey.eq_ignore_ascii_case("MouseMiddle") || hotkey.eq_ignore_ascii_case("MButton") {
            // Mouse middle handled in hooks
        } else if let Ok(shortcut) = normalized.parse::<Shortcut>() {
            let _ = app_handle.global_shortcut().register(shortcut);
        }
    }
    
    // sequential hotkey
    let seq_hotkey = {
        let settings = app_handle.state::<SettingsState>();
        let val = settings.sequential_paste_hotkey.lock().unwrap().clone();
        val
    };
    if let Ok(shortcut) = seq_hotkey.replace("Win", "Super").parse::<Shortcut>() {
        let _ = app_handle.global_shortcut().register(shortcut);
    }
    
    // rich paste hotkey
    let rich_hotkey = {
        let settings = app_handle.state::<SettingsState>();
        let val = settings.rich_paste_hotkey.lock().unwrap().clone();
        val
    };
    if !rich_hotkey.is_empty() {
        if let Ok(shortcut) = rich_hotkey.replace("Win", "Super").parse::<Shortcut>() {
            let _ = app_handle.global_shortcut().register(shortcut);
        }
    }

    // search hotkey
    let search_hotkey = {
        let settings = app_handle.state::<SettingsState>();
        let val = settings.search_hotkey.lock().unwrap().clone();
        val
    };
    if !search_hotkey.is_empty() {
        if let Ok(shortcut) = search_hotkey.replace("Win", "Super").parse::<Shortcut>() {
            let _ = app_handle.global_shortcut().register(shortcut);
        }
    }
    
    Ok(())
}

#[tauri::command]
pub fn test_hotkey_available(app_handle: AppHandle, hotkey: String) -> AppResult<bool> {
    if hotkey.is_empty() || hotkey.eq_ignore_ascii_case("MouseMiddle") || hotkey.eq_ignore_ascii_case("MButton") {
        return Ok(true);
    }

    let normalized = hotkey.replace("Win", "Super");
    let shortcut = normalized.parse::<Shortcut>().map_err(|_| AppError::Validation("快捷键格式无效".to_string()))?;

    // If already registered by our own app, it's available
    if app_handle.global_shortcut().is_registered(shortcut.clone()) {
        return Ok(true);
    }

    match app_handle.global_shortcut().register(shortcut.clone()) {
        Ok(_) => {
            let _ = app_handle.global_shortcut().unregister(shortcut);
            Ok(true)
        },
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
