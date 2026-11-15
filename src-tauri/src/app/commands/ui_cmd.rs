use crate::app_state::SettingsState;
use crate::database::DbState;
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, Theme, WebviewWindow};
use tauri_plugin_notification::NotificationExt;

#[derive(Debug, Serialize)]
pub struct PlatformInfo {
    pub platform: String,
    pub is_windows_10: bool,
    pub is_windows_11: bool,
    /// macOS 26 or newer: the 磨砂 theme sits on Liquid Glass, not a
    /// visual effect material (window_manager::apply_macos_window_material).
    pub has_liquid_glass: bool,
    /// Corner radius of a standard window on this macOS, in points (0 off
    /// macOS); the Liquid Glass shell copies it.
    pub window_corner_radius: f64,
}

#[tauri::command]
pub fn get_platform_info() -> PlatformInfo {
    #[cfg(target_os = "windows")]
    {
        let build = windows_version::OsVersion::current().build;
        let is_windows_11 = build >= 22000;
        let is_windows_10 = build >= 10240 && build < 22000;
        PlatformInfo {
            platform: "windows".to_string(),
            is_windows_10,
            is_windows_11,
            has_liquid_glass: false,
            window_corner_radius: 0.0,
        }
    }

    #[cfg(target_os = "macos")]
    {
        PlatformInfo {
            platform: "macos".to_string(),
            is_windows_10: false,
            is_windows_11: false,
            has_liquid_glass: crate::app::window_manager::macos_has_liquid_glass(),
            window_corner_radius: crate::app::window_manager::macos_window_corner_radius(),
        }
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        PlatformInfo {
            platform: std::env::consts::OS.to_string(),
            is_windows_10: false,
            is_windows_11: false,
            has_liquid_glass: false,
            window_corner_radius: 0.0,
        }
    }
}

#[tauri::command]
pub fn configure_compact_preview_window(app: AppHandle) -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::infrastructure::macos_api::window::configure_compact_preview_window(&app)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        true
    }
}

#[tauri::command]
pub fn send_system_notification(app: AppHandle, title: String, body: String) -> AppResult<()> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|err| AppError::Internal(format!("无法发送系统通知。\n详情：{}", err)))?;

    Ok(())
}

#[tauri::command]
pub fn set_theme(
    window: WebviewWindow,
    state: State<'_, SettingsState>,
    db_state: State<'_, DbState>,
    theme: String,
    color_mode: Option<String>,
    show_app_border: Option<bool>,
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
    #[cfg(target_os = "windows")]
    let show_border = show_app_border.unwrap_or_else(|| {
        db_state
            .settings_repo
            .get("app.show_app_border")
            .unwrap_or(Some("true".to_string()))
            .map(|value| value != "false")
        .unwrap_or(true)
    });
    #[cfg(not(target_os = "windows"))]
    let _ = show_app_border;

    if let Ok(mut guard) = state.theme.lock() {
        *guard = theme.clone();
    }

    let native_theme = match effective_color_mode.as_deref() {
        Some("light") => Some(Theme::Light),
        Some("dark") => Some(Theme::Dark),
        _ => None,
    };

    let _ = window.set_theme(native_theme);

    #[cfg(target_os = "windows")]
    {
        use windows::core::BOOL;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWM_WINDOW_CORNER_PREFERENCE, DWMWA_BORDER_COLOR,
            DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        };

        let raw_hwnd = window
            .hwnd()
            .map_err(|error| AppError::Internal(error.to_string()))?;
        let hwnd = HWND(raw_hwnd.0 as _);
        let is_dark = match effective_color_mode.as_deref() {
            Some("light") => false,
            Some("dark") => true,
            _ => window.theme().unwrap_or(Theme::Dark) == Theme::Dark,
        };
        let apply_dwm_frame = |hwnd: HWND| {
            unsafe {
                let dark_mode = BOOL::from(is_dark);
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_USE_IMMERSIVE_DARK_MODE,
                    &dark_mode as *const _ as _,
                    std::mem::size_of::<BOOL>() as u32,
                );

                // Let DWM draw the window border. A CSS border on #root sits on
                // the WebView edge, which rounds differently from the window at
                // fractional scaling and loses its right and bottom sides; the
                // DWM border follows the real (rounded) window edge on all four.
                const DWMWA_COLOR_DEFAULT: u32 = 0xFFFF_FFFF;
                let border_color = DWMWA_COLOR_DEFAULT;
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_BORDER_COLOR,
                    &border_color as *const _ as _,
                    std::mem::size_of::<u32>() as u32,
                );

                // Clip the whole HWND — including the square WebView
                // background — to Windows 11's system corner. CSS fills the
                // client rectangle (--shell-radius: 0) so those corners are covered.
                let corner = DWM_WINDOW_CORNER_PREFERENCE(DWMWCP_ROUND.0);
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &corner as *const _ as _,
                    std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
                );
            }
        };

        apply_dwm_frame(hwnd);

        let build = windows_version::OsVersion::current().build;
        let is_windows_11 = build >= 22000;
        crate::app::window_manager::apply_windows_window_material(&window, &theme, is_dark);
        let _ = window.set_shadow(show_border && is_windows_11);

        apply_dwm_frame(hwnd);
    }

    #[cfg(target_os = "macos")]
    {
        crate::app::window_manager::apply_macos_window_material(&window)?;
    }

    let _ = window.emit("theme-changed", theme);
    Ok(())
}
