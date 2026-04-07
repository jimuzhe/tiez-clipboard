use crate::app_state::SettingsState;
use crate::database::DbState;
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use tauri::{AppHandle, Emitter, State, Theme, WebviewWindow};
use tauri_plugin_notification::NotificationExt;
#[cfg(target_os = "macos")]
use window_vibrancy::{
    apply_vibrancy, clear_vibrancy, NSVisualEffectMaterial, NSVisualEffectState,
};

#[cfg(target_os = "macos")]
fn apply_macos_window_material(window: &WebviewWindow, theme: &str) {
    let _ = clear_vibrancy(window);

    let material = match theme {
        "mica" => Some(NSVisualEffectMaterial::Sidebar),
        "acrylic" => Some(NSVisualEffectMaterial::HudWindow),
        _ => None,
    };

    if let Some(material) = material {
        let _ = apply_vibrancy(
            window,
            material,
            Some(NSVisualEffectState::FollowsWindowActiveState),
            Some(12.0),
        );
    }
}

#[tauri::command]
pub fn send_system_notification(app: AppHandle, title: String, body: String) -> AppResult<()> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|err| AppError::Internal(format!("发送系统通知失败: {}", err)))?;

    Ok(())
}

#[tauri::command]
pub fn set_theme(
    window: WebviewWindow,
    state: State<'_, SettingsState>,
    db_state: State<'_, DbState>,
    theme: String,
    color_mode: Option<String>,
) -> AppResult<()> {
    let mut effective_color_mode = color_mode.clone();
    if effective_color_mode
        .as_deref()
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        effective_color_mode = db_state
            .settings_repo
            .get("app.color_mode")
            .unwrap_or(Some("system".to_string()));
    }

    if let Ok(mut guard) = state.theme.lock() {
        *guard = theme.clone();
    }

    let native_theme = match effective_color_mode.as_deref() {
        Some("light") => Some(Theme::Light),
        Some("dark") => Some(Theme::Dark),
        _ => None,
    };

    let _ = window.set_theme(native_theme);

    #[cfg(target_os = "macos")]
    {
        apply_macos_window_material(&window, &theme);
    }

    let _ = window.emit("theme-changed", theme);
    Ok(())
}
