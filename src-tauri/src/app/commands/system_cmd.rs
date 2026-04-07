use crate::app_state::AppDataDir;
use crate::database::ENCRYPT_PREFIX;
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

    #[cfg(target_os = "windows")]
    {
        let output = Command::new("explorer")
            .arg(path)
            .output()
            .map_err(|e| AppError::Internal(format!("Failed to open folder: {}", e)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if stderr.is_empty() {
                "Explorer returned a non-zero exit status".to_string()
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
pub fn scan_installed_apps() -> Vec<Value> {
    crate::infrastructure::macos_api::apps::scan_installed_apps()
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_system_default_app(contentType: String) -> String {
    let ext = content_type_to_ext(&contentType);
    crate::infrastructure::macos_api::apps::get_system_default_app(&ext)
}

#[tauri::command]
pub fn get_associated_apps(extension: String) -> Vec<Value> {
    crate::infrastructure::macos_api::apps::get_associated_apps(&extension)
}

#[tauri::command]
pub fn get_executable_icon(executable_path: String) -> AppResult<Option<String>> {
    crate::infrastructure::macos_api::apps::get_executable_icon(executable_path)
        .map_err(AppError::Internal)
}

#[tauri::command]
pub fn get_file_icon(file_path: String) -> AppResult<Option<String>> {
    crate::infrastructure::macos_api::apps::get_file_icon(file_path).map_err(AppError::Internal)
}

#[tauri::command]
pub fn get_data_path(state: State<'_, AppDataDir>) -> AppResult<String> {
    let path = state.0.lock().unwrap();
    Ok(path.to_string_lossy().to_string())
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
    Command::new("open")
        .arg(&file_path)
        .spawn()
        .map_err(|e| AppError::Internal(format!("Failed to open file: {}", e)))?;
    Ok(())
}

#[tauri::command]
pub fn open_file_location(file_path: String) -> AppResult<()> {
    use std::process::Command;
    Command::new("open")
        .arg("-R")
        .arg(&file_path)
        .spawn()
        .map_err(|e| AppError::Internal(format!("Failed to open file location: {}", e)))?;
    Ok(())
}

#[tauri::command]
pub fn toggle_autostart(app_handle: AppHandle, enabled: bool) -> AppResult<()> {
    // In Mac, we recommend using tauri-plugin-autostart's rust/JS APIs directly.
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app_handle.autolaunch();
    if enabled {
        let _ = autolaunch.enable();
    } else {
        let _ = autolaunch.disable();
    }
    Ok(())
}

#[tauri::command]
pub fn is_autostart_enabled(app_handle: AppHandle) -> AppResult<bool> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app_handle.autolaunch();
    Ok(autolaunch.is_enabled().unwrap_or(false))
}

// Windows specific commands removed: set_windows_clipboard_history, get_windows_clipboard_history, set_win_clipboard_disabled, trigger_registry_win_v_optimization, is_registry_win_v_optimized, restart_explorer

#[tauri::command]
pub fn get_app_arch() -> String {
    std::env::consts::ARCH.to_string()
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn relaunch(app: AppHandle) {
    use std::process::Command;
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).spawn();
    }
    app.exit(0);
}

// Windows admin stubs removed: restart_as_admin, check_is_admin

#[tauri::command]
pub fn set_data_path(app_handle: AppHandle, new_path: String) -> AppResult<()> {
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
    if clean_path.to_ascii_lowercase().ends_with(".app") {
        return Err(AppError::Validation(
            "Cannot use a .app bundle as data directory. Please choose a normal folder."
                .to_string(),
        ));
    }

    let old_path_buf = app_handle.state::<AppDataDir>().0.lock().unwrap().clone();

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
    let config_dir = app_handle.path().app_data_dir().map_err(AppError::from)?;
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

    if value.starts_with(ENCRYPT_PREFIX) {
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

    if value.starts_with(ENCRYPT_PREFIX) {
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
    crate::infrastructure::macos_api::permissions::has_accessibility_permission()
}

#[tauri::command]
pub fn request_macos_permissions() -> bool {
    crate::infrastructure::macos_api::permissions::request_accessibility_permission()
}
