use crate::error::{AppError, AppResult};

fn launch_downloaded_update(path: &std::path::Path) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new(path)
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to launch installer: {}", e)))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to open update package: {}", e)))?;
        return Ok(());
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let is_appimage = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".AppImage"))
            .unwrap_or(false);

        if is_appimage {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                let mut perms = std::fs::metadata(path).map_err(AppError::from)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(path, perms).map_err(AppError::from)?;
            }

            std::process::Command::new(path)
                .spawn()
                .map_err(|e| AppError::Internal(format!("Failed to launch AppImage: {}", e)))?;
            return Ok(());
        }

        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to open update package: {}", e)))?;
        Ok(())
    }
}

#[tauri::command]
pub async fn download_and_install_update(url: String) -> AppResult<()> {
    let parsed = reqwest::Url::parse(&url)
        .map_err(|e| AppError::Validation(format!("Invalid update URL: {}", e)))?;

    let file_name = parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| !name.is_empty())
        .unwrap_or("TieZ_Update.bin");

    let target_path = std::env::temp_dir().join(file_name);

    let response = reqwest::get(parsed)
        .await
        .map_err(|e| AppError::Network(format!("Failed to download update: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::Network(format!(
            "Update download failed with HTTP {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Network(format!("Failed to read update payload: {}", e)))?;

    std::fs::write(&target_path, &bytes).map_err(AppError::from)?;
    launch_downloaded_update(&target_path)
}
