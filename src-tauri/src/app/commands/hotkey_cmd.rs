use crate::app_state::SettingsState;
use crate::error::{AppError, AppResult};
use crate::global_state::HOTKEY_STRING;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

pub(crate) fn parse_hotkey_list(raw: &str) -> Vec<String> {
    raw.split(['\n', '\r'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .collect()
}

fn register_shortcut_if_valid(app: &AppHandle, raw_hotkey: &str) {
    if raw_hotkey.is_empty()
        || raw_hotkey.eq_ignore_ascii_case("MouseMiddle")
        || raw_hotkey.eq_ignore_ascii_case("MButton")
    {
        return;
    }
    if let Ok(shortcut) = raw_hotkey.replace("Win", "Super").parse::<Shortcut>() {
        match app.global_shortcut().register(shortcut.clone()) {
            Ok(_) => crate::info!("[global-shortcut] Registered: {}", raw_hotkey),
            Err(e) => crate::warn!(
                "[global-shortcut] Failed to register '{}': {:?}",
                raw_hotkey,
                e
            ),
        }
    } else {
        crate::warn!("[global-shortcut] Failed to parse hotkey: {}", raw_hotkey);
    }
}

pub fn sync_hotkeys_from_settings(app: &AppHandle) -> AppResult<()> {
    let settings = app
        .try_state::<SettingsState>()
        .ok_or_else(|| AppError::Internal("SettingsState 不可用".to_string()))?;

    let main_hotkey = settings.main_hotkey.lock().unwrap().clone();
    let sequential_hotkey = settings.sequential_paste_hotkey.lock().unwrap().clone();
    let rich_hotkey = settings.rich_paste_hotkey.lock().unwrap().clone();
    let search_hotkey = settings.search_hotkey.lock().unwrap().clone();

    {
        let mut guard = HOTKEY_STRING.lock().unwrap();
        *guard = main_hotkey.clone();
    }

    let _ = app.global_shortcut().unregister_all();
    for item in parse_hotkey_list(&main_hotkey) {
        register_shortcut_if_valid(app, &item);
    }
    register_shortcut_if_valid(app, &sequential_hotkey);
    register_shortcut_if_valid(app, &rich_hotkey);
    register_shortcut_if_valid(app, &search_hotkey);
    Ok(())
}

#[tauri::command]
pub fn register_hotkey(app_handle: AppHandle, hotkey: String) -> AppResult<()> {
    if let Some(settings) = app_handle.try_state::<SettingsState>() {
        let mut guard = settings.main_hotkey.lock().unwrap();
        *guard = hotkey.clone();
    }

    sync_hotkeys_from_settings(&app_handle)
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
