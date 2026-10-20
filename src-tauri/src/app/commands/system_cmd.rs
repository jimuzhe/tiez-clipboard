use crate::app_state::AppDataDir;
use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

fn open_path_in_file_manager(path: &std::path::Path) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::Validation(format!(
            "Directory does not exist: {}",
            path.to_string_lossy()
        )));
    }

    #[cfg(not(target_os = "windows"))]
    use std::process::Command;

    #[cfg(target_os = "macos")]
    {
        let reveal = Command::new("open")
            .arg("-R")
            .arg(path)
            .output()
            .map_err(|e| AppError::Internal(format!("Failed to open folder: {}", e)))?;
        if reveal.status.success() {
            return Ok(());
        }

        let fallback = Command::new("open")
            .arg(path)
            .output()
            .map_err(|e| AppError::Internal(format!("Failed to open folder: {}", e)))?;
        if fallback.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&reveal.stderr).trim().to_string();
        let fallback_stderr = String::from_utf8_lossy(&fallback.stderr).trim().to_string();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !fallback_stderr.is_empty() {
            fallback_stderr
        } else {
            "Unknown Finder error".to_string()
        };

        return Err(AppError::Internal(format!(
            "Failed to open folder: {}",
            detail
        )));
    }

    // explorer.exe exits with 1 even when the folder opened, so its exit code
    // reported every success as a failure. ShellExecute says whether it worked.
    #[cfg(target_os = "windows")]
    {
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let operation = HSTRING::from("open");
        let target = HSTRING::from(path.as_os_str());
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR::from_raw(operation.as_ptr()),
                PCWSTR::from_raw(target.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            )
        };
        // Values up to 32 are error codes (2: not found, 5: access denied).
        if result.0 as isize <= 32 {
            return Err(AppError::Internal(format!(
                "Failed to open folder (ShellExecute error {})",
                result.0 as isize
            )));
        }
        return Ok(());
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let output = Command::new("xdg-open")
            .arg(path)
            .output()
            .map_err(|e| AppError::Internal(format!("Failed to open folder: {}", e)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if stderr.is_empty() {
                "xdg-open returned a non-zero exit status".to_string()
            } else {
                stderr
            };
            return Err(AppError::Internal(format!(
                "Failed to open folder: {}",
                detail
            )));
        }
        return Ok(());
    }
}

fn content_type_to_ext(content_type: &str) -> String {
    let normalized = content_type.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "text" | "code" => "txt".to_string(),
        "rich_text" => "html".to_string(),
        "image" => "png".to_string(),
        "video" => "mp4".to_string(),
        "url" => "html".to_string(),
        other if !other.is_empty() => other.to_string(),
        _ => "txt".to_string(),
    }
}

#[tauri::command]
pub async fn scan_installed_apps() -> Vec<serde_json::Value> {
    #[cfg(target_os = "windows")]
    {
        match crate::infrastructure::windows_api::apps::scan_installed_apps().await {
            Ok(apps) => apps
                .into_iter()
                .filter_map(|app| serde_json::to_value(app).ok())
                .collect(),
            Err(_) => Vec::new(),
        }
    }
    #[cfg(target_os = "macos")]
    {
        crate::infrastructure::macos_api::apps::scan_installed_apps()
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Vec::new()
    }
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_system_default_app(contentType: String) -> String {
    let ext = content_type_to_ext(&contentType);
    #[cfg(target_os = "windows")]
    {
        return crate::infrastructure::windows_api::apps::get_system_default_app(contentType)
            .unwrap_or_default();
    }
    #[cfg(target_os = "macos")]
    {
        let ext = macos_open_with_key(&ext, Some(&contentType));
        return crate::infrastructure::macos_api::apps::get_system_default_app(ext);
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = ext;
        String::new()
    }
}

/// Link entries open in the browser, so on macOS their apps are the
/// browsers rather than everything that opens .html files.
#[cfg(target_os = "macos")]
fn macos_open_with_key<'a>(extension: &'a str, content_type: Option<&str>) -> &'a str {
    if content_type.is_some_and(|t| t.eq_ignore_ascii_case("url")) {
        crate::infrastructure::macos_api::apps::WEB_LINK
    } else {
        extension
    }
}

/// `content_type`: the entry type the apps are for (macOS uses it for links).
#[tauri::command]
pub async fn get_associated_apps(
    extension: String,
    content_type: Option<String>,
) -> Vec<serde_json::Value> {
    #[cfg(target_os = "windows")]
    {
        let _ = content_type;
        match crate::infrastructure::windows_api::apps::get_associated_apps(extension).await {
            Ok(apps) => apps
                .into_iter()
                .filter_map(|app| serde_json::to_value(app).ok())
                .collect(),
            Err(_) => Vec::new(),
        }
    }
    #[cfg(target_os = "macos")]
    {
        let key = macos_open_with_key(&extension, content_type.as_deref());
        crate::infrastructure::macos_api::apps::get_associated_apps(key)
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = (extension, content_type);
        Vec::new()
    }
}

#[tauri::command]
pub fn get_executable_icon(executable_path: String) -> AppResult<Option<String>> {
    #[cfg(target_os = "windows")]
    {
        return crate::infrastructure::windows_api::apps::get_executable_icon(executable_path);
    }
    #[cfg(target_os = "macos")]
    {
        return crate::infrastructure::macos_api::apps::get_executable_icon(executable_path)
            .map_err(AppError::Internal);
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = executable_path;
        Ok(None)
    }
}

#[tauri::command]
pub fn get_file_icon(file_path: String) -> AppResult<Option<String>> {
    #[cfg(target_os = "windows")]
    {
        return crate::infrastructure::windows_api::apps::get_file_icon(file_path);
    }
    #[cfg(target_os = "macos")]
    {
        return crate::infrastructure::macos_api::apps::get_file_icon(file_path).map_err(AppError::Internal);
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = file_path;
        Ok(None)
    }
}

#[tauri::command]
pub fn get_data_path(state: State<'_, AppDataDir>) -> AppResult<String> {
    let path = state.0.lock().unwrap();
    Ok(path.to_string_lossy().to_string())
}

/// The usage ping id (see resolve_ping_id); `window_id` is the id the window
/// kept in WebView storage, if any.
#[tauri::command]
pub fn get_ping_id(state: State<'_, crate::database::DbState>, window_id: Option<String>) -> String {
    crate::app::system::resolve_ping_id(&state.settings_repo, window_id.as_deref())
}

#[tauri::command]
pub fn open_folder(path: String) -> AppResult<()> {
    open_path_in_file_manager(std::path::Path::new(&path))
}

#[tauri::command]
pub fn open_data_folder(state: State<'_, AppDataDir>) -> AppResult<()> {
    let path = state.0.lock().unwrap();
    open_path_in_file_manager(&path)
}

#[tauri::command]
pub fn open_file_with_default_app(file_path: String) -> AppResult<()> {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&file_path)
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to open file: {}", e)))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&file_path)
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to open file: {}", e)))?;
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(&file_path)
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to open file: {}", e)))?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_file_location(file_path: String) -> AppResult<()> {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg("/select,")
            .arg(&file_path)
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to open file location: {}", e)))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-R")
            .arg(&file_path)
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to open file location: {}", e)))?;
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let path = std::path::Path::new(&file_path);
        if let Some(parent) = path.parent() {
            Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| AppError::Internal(format!("Failed to open file location: {}", e)))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_autostart(app_handle: AppHandle, enabled: bool) -> AppResult<()> {
    let result = apply_autostart(&app_handle, enabled);
    #[cfg(target_os = "macos")]
    {
        use crate::infrastructure::macos_api::login_item;
        if enabled && result.is_err() && login_item::available() && login_item::requires_approval() {
            // Switched off in System Settings, where only the user can switch
            // it back on: take them there instead of failing silently.
            login_item::open_settings();
        }
    }
    result
}

/// `toggle_autostart` without sending the user anywhere; for TieZ's own use.
pub(crate) fn apply_autostart(app_handle: &AppHandle, enabled: bool) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        use crate::infrastructure::windows_api::elevated_autostart;

        if enabled {
            if elevated_autostart::is_elevated_autostart_installed() {
                // Keep elevated task; ensure registry Run is not also set.
                elevated_autostart::clear_registry_run_entry()
                    .map_err(AppError::Internal)?;
            } else {
                elevated_autostart::write_registry_run_entry().map_err(AppError::Internal)?;
            }
        } else {
            elevated_autostart::clear_registry_run_entry().map_err(AppError::Internal)?;
            if elevated_autostart::is_elevated_autostart_installed() {
                elevated_autostart::run_elevated_helper(false).map_err(AppError::Internal)?;
            }
        }
        let _ = app_handle;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        use tauri_plugin_autostart::ManagerExt;
        let autolaunch = app_handle.autolaunch();
        #[cfg(target_os = "macos")]
        {
            use crate::infrastructure::macos_api::login_item;
            if login_item::available() {
                // The old ~/Library/LaunchAgents item goes either way, so it
                // can't start TieZ twice or show up under the developer's name.
                if autolaunch.is_enabled().unwrap_or(false) {
                    let _ = autolaunch.disable();
                }
                return login_item::set_enabled(enabled).map_err(AppError::Internal);
            }
        }
        if enabled {
            let _ = autolaunch.enable();
        } else {
            let _ = autolaunch.disable();
        }
        Ok(())
    }
}

#[tauri::command]
pub fn is_autostart_enabled(app_handle: AppHandle) -> AppResult<bool> {
    #[cfg(target_os = "windows")]
    {
        use crate::infrastructure::windows_api::elevated_autostart;
        let _ = app_handle;
        return Ok(elevated_autostart::registry_run_present()
            || elevated_autostart::is_elevated_autostart_installed());
    }

    #[cfg(not(target_os = "windows"))]
    {
        use tauri_plugin_autostart::ManagerExt;
        #[cfg(target_os = "macos")]
        {
            use crate::infrastructure::macos_api::login_item;
            if login_item::available() {
                return Ok(login_item::is_enabled());
            }
        }
        let autolaunch = app_handle.autolaunch();
        Ok(autolaunch.is_enabled().unwrap_or(false))
    }
}

#[tauri::command]
pub fn is_elevated_autostart_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        crate::infrastructure::windows_api::elevated_autostart::is_elevated_autostart_installed()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Enable/disable admin logon autostart (Task Scheduler Highest). Prompts UAC once.
#[tauri::command]
pub fn toggle_elevated_autostart(enabled: bool) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        use crate::infrastructure::windows_api::elevated_autostart;

        if enabled {
            if check_is_admin() {
                elevated_autostart::install_elevated_autostart_task()
                    .map_err(AppError::Internal)?;
            } else {
                elevated_autostart::run_elevated_helper(true).map_err(AppError::Internal)?;
            }
        } else {
            if elevated_autostart::is_elevated_autostart_installed() {
                if check_is_admin() {
                    elevated_autostart::uninstall_elevated_autostart_task()
                        .map_err(AppError::Internal)?;
                } else {
                    elevated_autostart::run_elevated_helper(false).map_err(AppError::Internal)?;
                }
            }
            // Fall back to normal (non-elevated) registry autostart so "开机自启动" stays on.
            elevated_autostart::write_registry_run_entry().map_err(AppError::Internal)?;
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
        Err(AppError::Internal(
            "Elevated autostart is only supported on Windows".into(),
        ))
    }
}

#[tauri::command]
pub fn set_windows_clipboard_history(enabled: bool) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let mut needs_restart = false;

        if let Ok((key, _)) = hkcu.create_subkey("Software\\Microsoft\\Clipboard") {
            let value: u32 = if enabled { 1 } else { 0 };
            let _ = key.set_value("EnableClipboardHistory", &value);
            let _ = key.set_value("EnableCloudClipboard", &value);
        }

        if let Ok((adv_key, _)) =
            hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced")
        {
            let current_disabled: String = adv_key.get_value("DisabledHotkeys").unwrap_or_default();
            if current_disabled.to_uppercase().contains('V') {
                let new_val = current_disabled.to_uppercase().replace('V', "");
                if new_val.is_empty() {
                    let _ = adv_key.delete_value("DisabledHotkeys");
                } else {
                    let _ = adv_key.set_value("DisabledHotkeys", &new_val);
                }
                needs_restart = true;
            }
        }

        if let Ok((policy_key, _)) =
            hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer")
        {
            if policy_key
                .get_value::<u32, _>("DisallowClipboardHistory")
                .unwrap_or(0)
                != 0
            {
                let _ = policy_key.delete_value("DisallowClipboardHistory");
                needs_restart = true;
            }
        }

        if enabled {
            if let Ok((sys_policy, _)) =
                hkcu.create_subkey("Software\\Policies\\Microsoft\\Windows\\System")
            {
                if sys_policy
                    .get_value::<u32, _>("AllowClipboardHistory")
                    .unwrap_or(1)
                    == 0
                {
                    let _ = sys_policy.delete_value("AllowClipboardHistory");
                    needs_restart = true;
                }
                if sys_policy
                    .get_value::<u32, _>("AllowCrossDeviceClipboard")
                    .unwrap_or(1)
                    == 0
                {
                    let _ = sys_policy.delete_value("AllowCrossDeviceClipboard");
                    needs_restart = true;
                }
            }
        }

        if needs_restart {
            restart_explorer().ok();
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
        Ok(())
    }
}

#[tauri::command]
pub fn get_windows_clipboard_history() -> AppResult<bool> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        let v_disabled = match hkcu
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced")
        {
            Ok(key) => key
                .get_value::<String, _>("DisabledHotkeys")
                .unwrap_or_default()
                .to_uppercase()
                .contains('V'),
            Err(_) => false,
        };
        let history_enabled = match hkcu.open_subkey("Software\\Microsoft\\Clipboard") {
            Ok(key) => {
                key.get_value::<u32, _>("EnableClipboardHistory")
                    .unwrap_or(1)
                    != 0
            }
            Err(_) => true,
        };
        Ok(history_enabled && !v_disabled)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(true)
    }
}

#[tauri::command]
pub fn set_win_clipboard_disabled(disabled: bool) -> AppResult<()> {
    set_windows_clipboard_history(!disabled)
}

#[tauri::command]
pub fn trigger_registry_win_v_optimization(enable: bool) -> AppResult<bool> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let mut changed = false;

        if let Ok((adv_key, _)) =
            hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced")
        {
            let current: String = adv_key.get_value("DisabledHotkeys").unwrap_or_default();
            if enable && !current.to_uppercase().contains('V') {
                let _ = adv_key.set_value("DisabledHotkeys", &format!("{}V", current));
                changed = true;
            } else if !enable && current.to_uppercase().contains('V') {
                let clean = current.to_uppercase().replace('V', "");
                if clean.is_empty() {
                    let _ = adv_key.delete_value("DisabledHotkeys");
                } else {
                    let _ = adv_key.set_value("DisabledHotkeys", &clean);
                }
                changed = true;
            }
        }

        if let Ok((cb_key, _)) = hkcu.create_subkey("Software\\Microsoft\\Clipboard") {
            let val: u32 = if enable { 0 } else { 1 };
            let prev_history = cb_key.get_value::<u32, _>("EnableClipboardHistory").ok();
            let prev_cloud = cb_key.get_value::<u32, _>("EnableCloudClipboard").ok();
            let _ = cb_key.set_value("EnableClipboardHistory", &val);
            let _ = cb_key.set_value("EnableCloudClipboard", &val);
            if prev_history != Some(val) || prev_cloud != Some(val) {
                changed = true;
            }
        }

        if !enable {
            if let Ok((policy_key, _)) =
                hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer")
            {
                if policy_key
                    .get_value::<u32, _>("DisallowClipboardHistory")
                    .unwrap_or(0)
                    != 0
                {
                    let _ = policy_key.delete_value("DisallowClipboardHistory");
                    changed = true;
                }
            }

            if let Ok((sys_policy, _)) =
                hkcu.create_subkey("Software\\Policies\\Microsoft\\Windows\\System")
            {
                if sys_policy
                    .get_value::<u32, _>("AllowClipboardHistory")
                    .unwrap_or(1)
                    == 0
                {
                    let _ = sys_policy.delete_value("AllowClipboardHistory");
                    changed = true;
                }
                if sys_policy
                    .get_value::<u32, _>("AllowCrossDeviceClipboard")
                    .unwrap_or(1)
                    == 0
                {
                    let _ = sys_policy.delete_value("AllowCrossDeviceClipboard");
                    changed = true;
                }
            }
        }
        Ok(changed)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = enable;
        Ok(false)
    }
}

#[tauri::command]
pub fn is_registry_win_v_optimized() -> AppResult<bool> {
    Ok(get_registry_win_v_optimized_status())
}

pub fn get_registry_win_v_optimized_status() -> bool {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) =
            hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced")
        {
            return key
                .get_value::<String, _>("DisabledHotkeys")
                .unwrap_or_default()
                .to_uppercase()
                .contains('V');
        }
    }
    false
}

#[tauri::command]
pub fn restart_explorer() -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        let _ = Command::new("cmd")
            .args(["/C", "taskkill /F /IM explorer.exe & start explorer.exe"])
            .creation_flags(0x08000000)
            .spawn();
    }
    Ok(())
}

#[tauri::command]
pub fn get_app_arch() -> String {
    std::env::consts::ARCH.to_string()
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    crate::app::light_mode::request_app_exit(&app);
}

#[tauri::command]
pub fn relaunch(app: AppHandle) {
    use std::process::Command;
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).spawn();
    }
    crate::app::light_mode::request_app_exit(&app);
}

#[tauri::command]
pub fn restart_as_admin(app_handle: AppHandle) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        use std::env;
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let exe_path = env::current_exe().map_err(AppError::from)?;
        let exe_wide: Vec<u16> = OsStr::new(&exe_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let runas: Vec<u16> = OsStr::new("runas")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let result = ShellExecuteW(
                None,
                PCWSTR::from_raw(runas.as_ptr()),
                PCWSTR::from_raw(exe_wide.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );

            if result.0 as usize <= 32 {
                return Err(AppError::Internal(
                    "Failed to restart as administrator. User may have cancelled UAC prompt."
                        .to_string(),
                ));
            }
        }

        crate::app::light_mode::request_app_exit(&app_handle);
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app_handle;
        Err(AppError::Internal(
            "Restart as administrator is only supported on Windows".to_string(),
        ))
    }
}

#[tauri::command]
pub fn check_is_admin() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::c_void;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        unsafe {
            let mut token_handle = HANDLE::default();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle).is_ok() {
                let mut elevation = TOKEN_ELEVATION::default();
                let mut return_length = 0;
                let success = GetTokenInformation(
                    token_handle,
                    TokenElevation,
                    Some(&mut elevation as *mut _ as *mut c_void),
                    std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                    &mut return_length,
                );

                let _ = windows::Win32::Foundation::CloseHandle(token_handle);

                if success.is_ok() {
                    return elevation.TokenIsElevated != 0;
                }
            }
        }
    }
    false
}

#[tauri::command]
pub fn set_data_path(app_handle: AppHandle, new_path: String) -> AppResult<()> {
    // Moving it would be undone at the next start (see portable_data_dir).
    if crate::app::setup::portable_data_dir().is_some() {
        return Err(AppError::Validation(
            "便携版的数据保存在程序旁的 data 文件夹，不能更换位置。".to_string(),
        ));
    }
    let clean_path = new_path.trim().to_string();
    let new_data_path = std::path::Path::new(&clean_path);
    if !new_data_path.exists() {
        return Err(AppError::Validation("Directory does not exist".to_string()));
    }
    if !new_data_path.is_dir() {
        return Err(AppError::Validation(
            "Selected path is not a directory".to_string(),
        ));
    }
    #[cfg(target_os = "macos")]
    if clean_path.to_ascii_lowercase().ends_with(".app") {
        return Err(AppError::Validation(
            "Cannot use a .app bundle as data directory. Please choose a normal folder."
                .to_string(),
        ));
    }

    let old_path_buf = app_handle.state::<AppDataDir>().0.lock().unwrap().clone();
    if std::fs::canonicalize(&old_path_buf).ok() == std::fs::canonicalize(new_data_path).ok() {
        return Ok(());
    }
    // Keep the original key recoverable and refuse to merge different key stores.
    for name in ["master.key", "master.key.dpapi"] {
        let source = old_path_buf.join(name);
        let destination = new_data_path.join(name);
        if !source.exists() { continue; }
        let bytes = std::fs::read(&source).map_err(AppError::from)?;
        if destination.exists() && std::fs::read(&destination).map_err(AppError::from)? != bytes {
            return Err(AppError::Validation("这个目录使用了其他加密密钥。\n请选择空目录。".into()));
        }
    }
    for name in ["master.key", "master.key.dpapi"] {
        let source = old_path_buf.join(name);
        let destination = new_data_path.join(name);
        if source.exists() && !destination.exists() {
            use std::io::Write;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
            let mut output = options.open(&destination).map_err(AppError::from)?;
            output.write_all(&std::fs::read(&source).map_err(AppError::from)?).map_err(AppError::from)?;
            output.sync_all().map_err(AppError::from)?;
        }
    }

    // 1. Migrate data folders if they exist in the OLD path
    {
        for folder in ["attachments", "emoji_favorites"] {
            let old_folder = old_path_buf.join(folder);
            let new_folder = new_data_path.join(folder);

            if old_folder.exists() && old_folder.is_dir() {
                if let Err(_) = std::fs::rename(&old_folder, &new_folder) {
                    if let Err(copy_err) = copy_dir_recursive(&old_folder, &new_folder) {
                        return Err(AppError::Internal(format!(
                            "Failed to copy {}: {}",
                            folder, copy_err
                        )));
                    } else {
                        let _ = std::fs::remove_dir_all(&old_folder);
                    }
                }
            }
        }

        // 1.2 Migrate database files (main + WAL/SHM)
        let db_files = ["clipboard.db", "clipboard.db-wal", "clipboard.db-shm"];
        for name in db_files {
            let old_db = old_path_buf.join(name);
            if !old_db.exists() {
                continue;
            }
            let new_db = new_data_path.join(name);
            if new_db.exists() {
                // Avoid overwriting any existing DB in new path
                let backup = new_data_path.join(format!("{}.backup", name));
                if backup.exists() {
                    let _ = std::fs::remove_file(&backup);
                }
                let _ = std::fs::rename(&new_db, &backup);
            }
            if let Err(_) = std::fs::rename(&old_db, &new_db) {
                if let Err(copy_err) = std::fs::copy(&old_db, &new_db) {
                    return Err(AppError::Internal(format!(
                        "Failed to copy {}: {}",
                        name, copy_err
                    )));
                } else {
                    let _ = std::fs::remove_file(&old_db);
                }
            }
        }
    }

    // 1.3 Rewrite internal attachment paths inside DB (if DB exists in new path)
    let new_db_path = new_data_path.join("clipboard.db");
    if new_db_path.exists() {
        rewrite_attachment_paths_in_db(&new_db_path, &old_path_buf, new_data_path)?;
        rewrite_emoji_favorites_in_db(&new_db_path, &old_path_buf, new_data_path)?;
        rewrite_custom_background_in_db(&new_db_path, &old_path_buf, new_data_path)?;
    }

    // 2. Save new path to a persistent config file
    let config_dir = crate::app::setup::default_app_data_dir(app_handle.path()).map_err(AppError::from)?;
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir).map_err(AppError::from)?;
    }

    let redirect_file = config_dir.join("datapath.txt");
    std::fs::write(&redirect_file, &clean_path).map_err(AppError::from)?;

    Ok(())
}

fn rewrite_attachment_paths_in_db(
    db_path: &std::path::Path,
    old_base: &std::path::Path,
    new_base: &std::path::Path,
) -> AppResult<()> {
    let old_attach = old_base.join("attachments");
    let new_attach = new_base.join("attachments");
    let old_prefix = old_attach.to_string_lossy().to_string();
    let new_prefix = new_attach.to_string_lossy().to_string();
    if old_prefix == new_prefix {
        return Ok(());
    }

    let old_prefix_slash = old_prefix.replace('\\', "/");
    let new_prefix_slash = new_prefix.replace('\\', "/");

    let conn = Connection::open(db_path).map_err(AppError::from)?;

    let mut stmt = conn
        .prepare("SELECT id, content, html_content FROM clipboard_history WHERE is_external = 1 OR html_content IS NOT NULL")
        .map_err(AppError::from)?;

    let rows = stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let content: String = row.get(1)?;
            let html_content: Option<String> = row.get(2)?;
            Ok((id, content, html_content))
        })
        .map_err(AppError::from)?;

    for row in rows {
        let (id, content_raw, html_raw) = row.map_err(AppError::from)?;
        let mut content_new: Option<String> = None;
        let mut html_new: Option<String> = None;

        if let Some(updated) = rewrite_content_path(
            &content_raw,
            &old_prefix,
            &new_prefix,
            &old_prefix_slash,
            &new_prefix_slash,
        ) {
            content_new = Some(updated);
        }

        if let Some(html) = html_raw.as_ref() {
            if let Some(updated) = rewrite_html_paths(
                html,
                &old_prefix,
                &new_prefix,
                &old_prefix_slash,
                &new_prefix_slash,
            ) {
                html_new = Some(updated);
            }
        }

        if content_new.is_some() || html_new.is_some() {
            let content_final = content_new.as_ref().unwrap_or(&content_raw);
            let html_final = match html_new.as_ref() {
                Some(v) => Some(v.as_str()),
                None => html_raw.as_deref(),
            };
            conn.execute(
                "UPDATE clipboard_history SET content = ?1, html_content = ?2 WHERE id = ?3",
                params![content_final, html_final, id],
            )
            .map_err(AppError::from)?;
        }
    }

    Ok(())
}

fn rewrite_emoji_favorites_in_db(
    db_path: &std::path::Path,
    old_base: &std::path::Path,
    new_base: &std::path::Path,
) -> AppResult<()> {
    let old_dir = old_base.join("emoji_favorites");
    let new_dir = new_base.join("emoji_favorites");
    let old_prefix = old_dir.to_string_lossy().to_string();
    let new_prefix = new_dir.to_string_lossy().to_string();
    if old_prefix == new_prefix {
        return Ok(());
    }

    let old_prefix_slash = old_prefix.replace('\\', "/");
    let new_prefix_slash = new_prefix.replace('\\', "/");

    let conn = Connection::open(db_path).map_err(AppError::from)?;
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'app.emoji_favorites'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::from)?;

    let Some(raw) = value else {
        return Ok(());
    };
    let parsed: Vec<String> = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };

    let mut changed = false;
    let mut updated: Vec<String> = Vec::with_capacity(parsed.len());
    for path in parsed {
        let mut next = path.clone();
        if next.starts_with(&old_prefix) {
            next = format!("{}{}", new_prefix, &next[old_prefix.len()..]);
        } else if next.starts_with(&old_prefix_slash) {
            next = format!("{}{}", new_prefix_slash, &next[old_prefix_slash.len()..]);
        }
        if next != path {
            changed = true;
        }
        updated.push(next);
    }

    if changed {
        let serialized = serde_json::to_string(&updated).unwrap_or(raw);
        conn.execute(
            "UPDATE settings SET value = ?1 WHERE key = 'app.emoji_favorites'",
            params![serialized],
        )
        .map_err(AppError::from)?;
    }

    Ok(())
}

fn rewrite_custom_background_in_db(
    db_path: &std::path::Path,
    old_base: &std::path::Path,
    new_base: &std::path::Path,
) -> AppResult<()> {
    let conn = Connection::open(db_path).map_err(AppError::from)?;
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'app.custom_background'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::from)?;

    let Some(raw_path) = value else {
        return Ok(());
    };
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let old_path = std::path::PathBuf::from(trimmed);
    if !old_path.starts_with(old_base) {
        return Ok(());
    }

    let Ok(relative) = old_path.strip_prefix(old_base) else {
        return Ok(());
    };
    let new_path = new_base.join(relative);

    if old_path != new_path && old_path.exists() {
        if let Some(parent) = new_path.parent() {
            std::fs::create_dir_all(parent).map_err(AppError::from)?;
        }
        if !new_path.exists() {
            if let Err(_) = std::fs::rename(&old_path, &new_path) {
                std::fs::copy(&old_path, &new_path).map_err(AppError::from)?;
                let _ = std::fs::remove_file(&old_path);
            }
        }
    }

    let new_value = new_path.to_string_lossy().to_string();
    if new_value != raw_path {
        conn.execute(
            "UPDATE settings SET value = ?1 WHERE key = 'app.custom_background'",
            params![new_value],
        )
        .map_err(AppError::from)?;
    }

    Ok(())
}

fn rewrite_content_path(
    value: &str,
    old_prefix: &str,
    new_prefix: &str,
    old_prefix_slash: &str,
    new_prefix_slash: &str,
) -> Option<String> {
    let replace_prefix = |v: &str| -> Option<String> {
        if v.starts_with(old_prefix) {
            return Some(format!("{}{}", new_prefix, &v[old_prefix.len()..]));
        }
        if v.starts_with(old_prefix_slash) {
            return Some(format!(
                "{}{}",
                new_prefix_slash,
                &v[old_prefix_slash.len()..]
            ));
        }
        None
    };

    if crate::infrastructure::encryption::is_encrypted_value(value) {
        #[cfg(not(feature = "portable"))]
        {
            let plain = crate::database::encryption::decrypt_value(value)
                .unwrap_or_else(|| value.to_string());
            if let Some(updated_plain) = replace_prefix(&plain) {
                let encrypted = crate::database::encryption::encrypt_value(&updated_plain)
                    .unwrap_or(updated_plain);
                return Some(encrypted);
            }
        }
        return None;
    }

    replace_prefix(value)
}

fn rewrite_html_paths(
    value: &str,
    old_prefix: &str,
    new_prefix: &str,
    old_prefix_slash: &str,
    new_prefix_slash: &str,
) -> Option<String> {
    let replace_any = |v: &str| -> Option<String> {
        let mut updated = v.replace(old_prefix, new_prefix);
        updated = updated.replace(old_prefix_slash, new_prefix_slash);
        if updated == v {
            None
        } else {
            Some(updated)
        }
    };

    if crate::infrastructure::encryption::is_encrypted_value(value) {
        #[cfg(not(feature = "portable"))]
        {
            let plain = crate::database::encryption::decrypt_value(value)
                .unwrap_or_else(|| value.to_string());
            if let Some(updated_plain) = replace_any(&plain) {
                let encrypted = crate::database::encryption::encrypt_value(&updated_plain)
                    .unwrap_or(updated_plain);
                return Some(encrypted);
            }
        }
        return None;
    }

    replace_any(value)
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn check_macos_permissions() -> bool {
    #[cfg(target_os = "macos")]
    {
        return crate::infrastructure::macos_api::permissions::has_accessibility_permission();
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
pub fn request_macos_permissions() -> bool {
    #[cfg(target_os = "macos")]
    {
        return crate::infrastructure::macos_api::permissions::request_accessibility_permission();
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
pub fn get_macos_accessibility_binary_path() -> String {
    #[cfg(target_os = "macos")]
    {
        return std::env::current_exe()
            .ok()
            .and_then(|path| path.into_os_string().into_string().ok())
            .unwrap_or_else(|| "tiez-app".to_string());
    }
    #[cfg(not(target_os = "macos"))]
    {
        String::new()
    }
}

#[tauri::command]
pub fn open_macos_accessibility_settings() -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        const URLS: &[&str] = &[
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility",
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            "x-apple.systempreferences:com.apple.preference.security",
        ];
        for url in URLS {
            if Command::new("open")
                .arg(url)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                return Ok(());
            }
        }
        if Command::new("open")
            .args(["-b", "com.apple.systempreferences"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        return Err(AppError::Internal(
            "请手动打开系统「辅助功能」设置。".to_string(),
        ));
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}
