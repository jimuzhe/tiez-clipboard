use crate::error::{AppResult, AppError};

#[tauri::command]
pub async fn download_and_install_update(url: String) -> AppResult<()> {
    use std::process::Command;
    
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;

    let temp_dir = std::env::temp_dir();
    let installer_path = temp_dir.join("TieZ_Update_Installer.exe");
    let installer_path_str = installer_path.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    let download_script = format!(
        "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; $ProgressPreference = 'SilentlyContinue'; Invoke-WebRequest -Uri '{}' -OutFile '{}'",
        url,
        installer_path_str
    );

    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", &download_script])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output()
            .map_err(|e| AppError::Internal(format!("Failed to execute download command: {}", e)))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Internal(format!("Download failed: {}", err)));
        }

        Command::new(&installer_path)
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to launch installer: {}", e)))?;
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows platforms: download using curl or wget
        let _ = url; // suppress unused warning
        return Err(AppError::Internal("Auto-update not supported on this platform".to_string()));
    }
    
    Ok(())
}
