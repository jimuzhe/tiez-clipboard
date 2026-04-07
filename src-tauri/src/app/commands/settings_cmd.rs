use crate::app::commands::hotkey_cmd::{normalize_quick_paste_modifier, sync_registered_hotkeys};
use crate::app_state::SettingsState;
use crate::database::DbState;
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub fn set_sequential_mode(
    app_handle: AppHandle,
    state: State<'_, crate::app_state::SettingsState>,
    enabled: bool,
) {
    state.sequential_mode.store(enabled, Ordering::Relaxed);
    let db_state = app_handle.state::<DbState>();
    let _ = db_state
        .settings_repo
        .set("app.sequential_mode", &enabled.to_string());
}

#[tauri::command]
pub fn set_sequential_hotkey(
    app_handle: AppHandle,
    state: State<'_, SettingsState>,
    hotkey: String,
) -> AppResult<()> {
    if let Ok(mut guard) = state.sequential_paste_hotkey.lock() {
        *guard = hotkey.clone();
    }

    let db_state = app_handle.state::<DbState>();
    db_state
        .settings_repo
        .set("app.sequential_hotkey", &hotkey)
        .map_err(AppError::from)?;

    sync_registered_hotkeys(&app_handle)
}

#[tauri::command]
pub fn set_rich_paste_hotkey(
    app_handle: AppHandle,
    state: State<'_, SettingsState>,
    hotkey: String,
) -> AppResult<()> {
    if let Ok(mut guard) = state.rich_paste_hotkey.lock() {
        *guard = hotkey.clone();
    }

    let db_state = app_handle.state::<DbState>();
    db_state
        .settings_repo
        .set("app.rich_paste_hotkey", &hotkey)
        .map_err(AppError::from)?;

    sync_registered_hotkeys(&app_handle)
}

#[tauri::command]
pub fn set_search_hotkey(
    app_handle: AppHandle,
    state: State<'_, SettingsState>,
    hotkey: String,
) -> AppResult<()> {
    if let Ok(mut guard) = state.search_hotkey.lock() {
        *guard = hotkey.clone();
    }

    let db_state = app_handle.state::<DbState>();
    db_state
        .settings_repo
        .set("app.search_hotkey", &hotkey)
        .map_err(AppError::from)?;

    sync_registered_hotkeys(&app_handle)
}

#[tauri::command]
pub fn set_quick_paste_modifier(
    app_handle: AppHandle,
    state: State<'_, SettingsState>,
    modifier: String,
) -> AppResult<()> {
    let normalized = normalize_quick_paste_modifier(&modifier);

    if let Ok(mut guard) = state.quick_paste_modifier.lock() {
        *guard = normalized.clone();
    }

    let db_state = app_handle.state::<DbState>();
    db_state
        .settings_repo
        .set("app.quick_paste_modifier", &normalized)
        .map_err(AppError::from)?;

    sync_registered_hotkeys(&app_handle)
}

#[tauri::command]
pub fn set_deduplication(
    app_handle: AppHandle,
    state: State<'_, crate::app_state::SettingsState>,
    enabled: bool,
) {
    state.deduplicate.store(enabled, Ordering::Relaxed);
    let db_state = app_handle.state::<DbState>();
    let _ = db_state
        .settings_repo
        .set("app.deduplicate", &enabled.to_string());
}

#[tauri::command]
pub fn save_setting(
    db_state: State<'_, DbState>,
    settings_state: State<'_, crate::app_state::SettingsState>,
    key: String,
    value: String,
) -> AppResult<()> {
    match key.as_str() {
        "app.arrow_key_selection" => {
            settings_state
                .arrow_key_selection
                .store(value == "true", Ordering::Relaxed);
        }
        "app.sequential_mode" => {
            settings_state
                .sequential_mode
                .store(value == "true", Ordering::Relaxed);
        }
        "app.sound_enabled" => {
            settings_state
                .sound_enabled
                .store(value == "true", Ordering::Relaxed);
        }
        "app.sound_paste_enabled" => {
            settings_state
                .delete_after_paste
                .store(value != "false", Ordering::Relaxed);
        }
        "app.persistent" => {
            settings_state
                .persistent
                .store(value != "false", Ordering::Relaxed);
        }
        "app.capture_files" => {
            settings_state
                .capture_files
                .store(value != "false", Ordering::Relaxed);
        }
        "app.capture_rich_text" => {
            settings_state
                .capture_rich_text
                .store(value == "true", Ordering::Relaxed);
        }
        "app.rich_text_snapshot_preview" => {
            settings_state
                .rich_text_snapshot_preview
                .store(value == "true", Ordering::Relaxed);
        }
        "app.silent_start" => {
            settings_state
                .silent_start
                .store(value != "false", Ordering::Relaxed);
        }
        "app.delete_after_paste" => {
            settings_state
                .delete_after_paste
                .store(value == "true", Ordering::Relaxed);
        }
        "app.privacy_protection" => {
            settings_state
                .privacy_protection
                .store(value == "true", Ordering::Relaxed);
        }
        "app.edge_docking" => {
            settings_state
                .edge_docking
                .store(value == "true", Ordering::Relaxed);
        }

        "app.hide_tray_icon" => {
            settings_state
                .hide_tray_icon
                .store(value == "true", Ordering::Relaxed);
        }
        "app.hide_dock_icon" => {
            settings_state
                .hide_dock_icon
                .store(value == "true", Ordering::Relaxed);
        }
        "app.sound_volume" => {
            if let Ok(v) = value.parse::<f64>() {
                if let Ok(mut guard) = settings_state.sound_volume.lock() {
                    *guard = v;
                }
            }
        }
        _ => {}
    }

    db_state
        .settings_repo
        .set(&key, &value)
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_ignore_blur(ignore: bool) {
    crate::IGNORE_BLUR.store(ignore, Ordering::Relaxed);
}

#[tauri::command]
pub fn set_window_pinned(app_handle: AppHandle, state: State<'_, DbState>, pinned: bool) {
    crate::WINDOW_PINNED.store(pinned, Ordering::Relaxed);
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.set_always_on_top(pinned);
        #[cfg(target_os = "windows")]
        let _ = window.set_focusable(!pinned);
        #[cfg(target_os = "macos")]
        let _ = window.set_focusable(!pinned);
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let _ = window.set_focusable(false);
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{
                GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
            };
            if let Ok(hwnd) = window.hwnd() {
                unsafe {
                    let ex_style = GetWindowLongPtrW(HWND(hwnd.0), GWL_EXSTYLE);
                    let _ = SetWindowLongPtrW(
                        HWND(hwnd.0),
                        GWL_EXSTYLE,
                        ex_style | WS_EX_NOACTIVATE.0 as isize,
                    );
                }
            }
        }
    }
    let _ = state
        .settings_repo
        .set("app.window_pinned", &pinned.to_string());
}

#[tauri::command]
pub fn get_settings(
    state: State<'_, DbState>,
) -> AppResult<std::collections::HashMap<String, String>> {
    state.settings_repo.get_all().map_err(AppError::from)
}

#[tauri::command]
pub fn set_file_server_auto_close(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state
        .file_server_auto_close
        .store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set("file_transfer_auto_close", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_file_transfer_auto_open(db_state: State<'_, DbState>, enabled: bool) -> AppResult<()> {
    db_state
        .settings_repo
        .set("file_transfer_auto_open", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_arrow_key_selection(
    state: State<'_, crate::app_state::SettingsState>,
    enabled: bool,
) -> AppResult<()> {
    state.arrow_key_selection.store(enabled, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn set_persistence(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state.persistent.store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set("app.persistent", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_capture_files(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state.capture_files.store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set("app.capture_files", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_capture_rich_text(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state.capture_rich_text.store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set("app.capture_rich_text", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_rich_text_snapshot_preview(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state
        .rich_text_snapshot_preview
        .store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set("app.rich_text_snapshot_preview", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_auto_copy_file(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state.auto_copy_file.store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set(
            "file_transfer_auto_copy",
            if enabled { "true" } else { "false" },
        )
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_silent_start(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state.silent_start.store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set("app.silent_start", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_delete_after_paste(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state.delete_after_paste.store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set("app.delete_after_paste", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_privacy_protection(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state.privacy_protection.store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set("app.privacy_protection", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_privacy_protection_kinds(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    kinds: Vec<String>,
) -> AppResult<()> {
    let mut guard = state.privacy_protection_kinds.lock().unwrap();
    *guard = kinds.clone();
    let serialized = kinds.join(",");
    db_state
        .settings_repo
        .set("app.privacy_protection_kinds", &serialized)
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_privacy_protection_custom_rules(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    rules: String,
) -> AppResult<()> {
    let list = rules
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let mut guard = state.privacy_protection_custom_rules.lock().unwrap();
    *guard = list;
    db_state
        .settings_repo
        .set("app.privacy_protection_custom_rules", &rules)
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_cleanup_rules(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    rules: String,
) -> AppResult<()> {
    let mut guard = state.cleanup_rules.lock().unwrap();
    *guard = rules.clone();
    db_state
        .settings_repo
        .set("app.cleanup_rules", &rules)
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_app_cleanup_policies(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    policies: String,
) -> AppResult<()> {
    let mut guard = state.app_cleanup_policies.lock().unwrap();
    *guard = policies.clone();
    db_state
        .settings_repo
        .set("app.app_cleanup_policies", &policies)
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_sound_enabled(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state.sound_enabled.store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set("app.sound_enabled", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn get_mqtt_status() -> bool {
    crate::services::mqtt_sub::get_mqtt_status()
}

#[tauri::command]
pub fn get_mqtt_running() -> bool {
    crate::services::mqtt_sub::get_mqtt_running()
}

#[tauri::command]
pub fn restart_mqtt_client(app_handle: AppHandle) {
    crate::services::mqtt_sub::restart_mqtt_client(app_handle)
}

#[tauri::command]
pub fn get_cloud_sync_status() -> crate::services::cloud_sync::CloudSyncStatus {
    crate::services::cloud_sync::get_cloud_sync_status()
}

#[tauri::command]
pub fn restart_cloud_sync_client(app_handle: AppHandle) {
    crate::services::cloud_sync::restart_cloud_sync_client(app_handle);
}

#[tauri::command]
pub fn request_cloud_sync(app_handle: AppHandle) {
    crate::services::cloud_sync::request_cloud_sync(app_handle);
}

#[tauri::command]
pub fn stop_cloud_sync_client(app_handle: AppHandle) {
    crate::services::cloud_sync::stop_cloud_sync_client(app_handle);
}

#[tauri::command]
pub async fn cloud_sync_now(
    app_handle: AppHandle,
) -> AppResult<crate::services::cloud_sync::CloudSyncStatus> {
    crate::services::cloud_sync::cloud_sync_now(app_handle).await
}

#[tauri::command]
pub fn reset_settings(
    app: AppHandle,
    state: State<'_, DbState>,
    settings_state: State<'_, crate::app_state::SettingsState>,
) -> AppResult<()> {
    use crate::database::seed_defaults;

    state.settings_repo.clear().map_err(AppError::from)?;
    {
        let conn = state.conn.lock().unwrap();
        seed_defaults(&conn).map_err(AppError::from)?;
    }

    let machine_id = crate::app::system::get_machine_id();
    let new_id = crate::app::system::build_anon_id(&machine_id);
    state
        .settings_repo
        .set("app.anon_id", &new_id)
        .map_err(AppError::from)?;

    let main_hotkey = state
        .settings_repo
        .get("app.hotkey")
        .unwrap_or(Some("Alt+C".to_string()))
        .unwrap_or("Alt+C".to_string());
    let seq_hotkey = state
        .settings_repo
        .get("app.sequential_hotkey")
        .unwrap_or(Some("Alt+V".to_string()))
        .unwrap_or("Alt+V".to_string());
    let rich_hotkey = state
        .settings_repo
        .get("app.rich_paste_hotkey")
        .unwrap_or(Some("Alt+Shift+V".to_string()))
        .unwrap_or("Alt+Shift+V".to_string());
    let search_hotkey = state
        .settings_repo
        .get("app.search_hotkey")
        .unwrap_or(Some("Alt+F".to_string()))
        .unwrap_or("Alt+F".to_string());
    let quick_paste_modifier = state
        .settings_repo
        .get("app.quick_paste_modifier")
        .unwrap_or(Some("disabled".to_string()))
        .unwrap_or("disabled".to_string());

    {
        let mut guard = settings_state.main_hotkey.lock().unwrap();
        *guard = main_hotkey.clone();
    }
    {
        let mut guard = settings_state.sequential_paste_hotkey.lock().unwrap();
        *guard = seq_hotkey.clone();
    }
    {
        let mut guard = settings_state.rich_paste_hotkey.lock().unwrap();
        *guard = rich_hotkey.clone();
    }
    {
        let mut guard = settings_state.search_hotkey.lock().unwrap();
        *guard = search_hotkey.clone();
    }
    {
        let mut guard = settings_state.quick_paste_modifier.lock().unwrap();
        *guard = quick_paste_modifier;
    }
    {
        let mut guard = crate::global_state::HOTKEY_STRING.lock().unwrap();
        *guard = main_hotkey.clone();
    }

    sync_registered_hotkeys(&app)
}

#[tauri::command]
pub fn set_tray_visible(
    app_handle: AppHandle,
    state: State<'_, crate::app_state::SettingsState>,
    visible: bool,
) -> AppResult<()> {
    state.hide_tray_icon.store(!visible, Ordering::Relaxed);
    if let Some(tray) = app_handle.tray_by_id("main_tray") {
        let _ = tray.set_visible(visible);
    }
    let db_state = app_handle.state::<DbState>();
    db_state
        .settings_repo
        .set("app.hide_tray_icon", &(!visible).to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_dock_visible(
    app_handle: AppHandle,
    state: State<'_, crate::app_state::SettingsState>,
    visible: bool,
) -> AppResult<()> {
    state.hide_dock_icon.store(!visible, Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    {
        app_handle
            .set_dock_visibility(visible)
            .map_err(AppError::from)?;
    }
    let db_state = app_handle.state::<DbState>();
    db_state
        .settings_repo
        .set("app.hide_dock_icon", &(!visible).to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn set_edge_docking(
    app_handle: AppHandle,
    state: State<'_, crate::app_state::SettingsState>,
    enabled: bool,
) -> AppResult<()> {
    state.edge_docking.store(enabled, Ordering::Relaxed);
    let db_state = app_handle.state::<DbState>();
    db_state
        .settings_repo
        .set("app.edge_docking", &enabled.to_string())
        .map_err(AppError::from)
}
