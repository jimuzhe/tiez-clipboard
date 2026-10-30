use crate::app::commands::hotkey_cmd::{normalize_quick_paste_modifier, sync_registered_hotkeys};
use crate::app_state::{PasteQueue, SettingsState};
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

    if !enabled {
        let paste_queue = app_handle.state::<PasteQueue>();
        let mut queue = paste_queue.inner().0.lock().unwrap();
        queue.items.clear();
        queue.last_action_was_paste = false;
        queue.last_pasted_content = None;
        queue.last_pasted_fingerprint = None;
    }

    let _ = sync_registered_hotkeys(&app_handle);
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
pub fn set_plain_paste_hotkey(
    app_handle: AppHandle,
    state: State<'_, SettingsState>,
    hotkey: String,
) -> AppResult<()> {
    if let Ok(mut guard) = state.plain_paste_hotkey.lock() {
        *guard = hotkey.clone();
    }

    let db_state = app_handle.state::<DbState>();
    db_state
        .settings_repo
        .set("app.plain_paste_hotkey", &hotkey)
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
                .paste_sound_enabled
                .store(value == "true", Ordering::Relaxed);
        }
        "app.sound_volume" => {
            if let Some(v) = crate::database::sound_volume_from_setting(&value) {
                if let Ok(mut guard) = settings_state.sound_volume.lock() {
                    *guard = v;
                }
            }
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
        "app.follow_mouse" => {
            settings_state
                .follow_mouse
                .store(value != "false", Ordering::Relaxed);
        }
        "app.light_mode_enabled" => {
            settings_state
                .light_mode_enabled
                .store(value == "true", Ordering::Relaxed);
        }
        "app.light_mode_idle_minutes" => {
            if let Ok(mins) = value.parse::<u64>() {
                settings_state
                    .light_mode_idle_minutes
                    .store(mins.clamp(1, 240), Ordering::Relaxed);
            }
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
pub fn acquire_blur_guard(owner: String) -> Result<usize, String> {
    let owner = owner.trim();
    if owner.is_empty() {
        return Err("Blur guard owner cannot be empty".to_string());
    }
    Ok(crate::global_state::acquire_blur_guard(owner))
}

#[tauri::command]
pub fn release_blur_guard(owner: String) -> Result<usize, String> {
    let owner = owner.trim();
    if owner.is_empty() {
        return Err("Blur guard owner cannot be empty".to_string());
    }
    Ok(crate::global_state::release_blur_guard(owner))
}

#[tauri::command]
pub fn set_window_pinned(app_handle: AppHandle, state: State<'_, DbState>, pinned: bool) {
    crate::WINDOW_PINNED.store(pinned, Ordering::Relaxed);
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.set_always_on_top(pinned);
        #[cfg(target_os = "windows")]
        let _ = window.set_focusable(!pinned);
        #[cfg(target_os = "macos")]
        {
            // NSPanel focusability cannot be changed through Tauri after panel
            // conversion. In pinned mode, let ordinary clicks pass through
            // without making the clipboard key; editable controls opt in via
            // the shared focus coordinator.
            let _ = crate::infrastructure::macos_api::window::set_clipboard_panel_pinned_interaction(
                &app_handle,
                pinned,
            );
        }
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
                    let next = if pinned {
                        ex_style | WS_EX_NOACTIVATE.0 as isize
                    } else {
                        ex_style & !(WS_EX_NOACTIVATE.0 as isize)
                    };
                    let _ = SetWindowLongPtrW(HWND(hwnd.0), GWL_EXSTYLE, next);
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

/// Sensitive settings this device can't decrypt; get_settings leaves them out.
#[tauri::command]
pub fn get_undecryptable_settings(state: State<'_, DbState>) -> AppResult<Vec<String>> {
    state.settings_repo.undecryptable_keys().map_err(AppError::from)
}

/// The user opened the setting marked for re-entry: drop its unreadable value
/// (it can't be recovered on this device) so the field starts empty.
#[tauri::command]
pub fn clear_undecryptable_settings(state: State<'_, DbState>, keys: Vec<String>) -> AppResult<Vec<String>> {
    state.settings_repo.clear_undecryptable(&keys).map_err(AppError::from)
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
        .map_err(AppError::from)?;
    if enabled {
        let volume = state.sound_volume.lock().map(|v| *v).unwrap_or(1.0);
        crate::services::ui_sound::preload_ui_sounds(volume);
    }
    Ok(())
}

#[tauri::command]
pub fn set_paste_sound_enabled(
    state: State<'_, crate::app_state::SettingsState>,
    db_state: State<'_, DbState>,
    enabled: bool,
) -> AppResult<()> {
    state
        .paste_sound_enabled
        .store(enabled, Ordering::Relaxed);
    db_state
        .settings_repo
        .set("app.sound_paste_enabled", &enabled.to_string())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn play_preview_sound(app_handle: tauri::AppHandle, kind: Option<String>) {
    let kind = kind.unwrap_or_else(|| "copy".to_string());
    crate::services::ui_sound::play_ui_sound(&app_handle, &kind);
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

    // A reset must not make this device count as a new one.
    let ping_id = state
        .settings_repo
        .get(crate::app::system::PING_ID_KEY)
        .unwrap_or(None);
    state.settings_repo.clear().map_err(AppError::from)?;
    {
        let conn = state.conn.lock().unwrap();
        seed_defaults(&conn).map_err(AppError::from)?;
    }
    if let Some(ping_id) = ping_id {
        let _ = state
            .settings_repo
            .set(crate::app::system::PING_ID_KEY, &ping_id);
    }

    let machine_id = crate::app::system::get_machine_id();
    let new_id = crate::app::system::build_anon_id(&machine_id);
    state
        .settings_repo
        .set("app.anon_id", &new_id)
        .map_err(AppError::from)?;

    // Running state follows the defaults now, not after the next start.
    let fresh = crate::app::setup::load_settings(&state.settings_repo);
    settings_state.take_values_from(crate::app::setup::settings_state_from(&fresh));
    *crate::global_state::HOTKEY_STRING.lock().unwrap() = fresh.main_hotkey.clone();
    // The window applies the rest when it reloads, but not these two.
    let _ = set_tray_visible(app.clone(), app.state(), !fresh.hide_tray_icon);
    let _ = set_dock_visible(app.clone(), app.state(), !fresh.hide_dock_icon);

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
        crate::infrastructure::macos_api::window::set_dock_visibility_preserving_panel(
            &app_handle, visible,
        ).map_err(AppError::from)?;
        if !visible {
            // Tao intentionally drops a hide within one second of showing the
            // Dock icon. Retry beyond that interval, but respect a newer toggle.
            let app = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
                let app_on_main = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let settings = app_on_main.state::<crate::app_state::SettingsState>();
                    if settings.hide_dock_icon.load(Ordering::Relaxed) {
                        if let Err(error) = crate::infrastructure::macos_api::window::set_dock_visibility_preserving_panel(&app_on_main, false) {
                            eprintln!("Failed to retry hiding Dock icon: {error}");
                        }
                    }
                });
            });
        }
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
        .map_err(AppError::from)?;
    if !enabled {
        crate::app::setup::persist_edge_dock(&app_handle, 0);
    }
    Ok(())
}

#[tauri::command]
pub fn set_follow_mouse(
    app_handle: AppHandle,
    state: State<'_, crate::app_state::SettingsState>,
    enabled: bool,
) -> AppResult<()> {
    state.follow_mouse.store(enabled, Ordering::Relaxed);
    let db_state = app_handle.state::<DbState>();
    db_state
        .settings_repo
        .set("app.follow_mouse", &enabled.to_string())
        .map_err(AppError::from)
}
