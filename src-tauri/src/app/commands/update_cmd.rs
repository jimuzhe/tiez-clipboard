use crate::error::{AppError, AppResult};

#[tauri::command]
pub async fn download_and_install_update(_url: String) -> AppResult<()> {
    // macOS auto-update logic goes here later (e.g. sparkel or Tauri built-in updater)
    Err(AppError::Internal(
        "Auto-update is not fully supported on this macOS build yet.".to_string(),
    ))
}
