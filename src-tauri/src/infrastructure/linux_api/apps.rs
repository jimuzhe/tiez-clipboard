use std::process::Command;
use serde::{Serialize, Deserialize};
use crate::error::{AppResult, AppError};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppInfo {
    pub name: String,
    pub path: String,
}

#[tauri::command]
pub fn get_executable_icon(_executable_path: String) -> AppResult<Option<String>> {
    Ok(None)
}

#[tauri::command]
pub async fn scan_installed_apps() -> AppResult<Vec<AppInfo>> {
    Ok(vec![])
}

#[tauri::command]
pub async fn get_associated_apps(_extension: String) -> AppResult<Vec<AppInfo>> {
    Ok(vec![])
}

#[tauri::command]
pub fn get_system_default_app(_content_type: String) -> AppResult<String> {
    Ok(String::new())
}

pub async fn launch_uwp_with_file(_app_id: &str, _file_path: &str) -> AppResult<()> {
    Ok(())
}

pub fn open_with_default(path: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Failed to open: {}", e))?;

    Ok(())
}
