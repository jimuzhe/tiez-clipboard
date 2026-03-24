use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn open_file_or_url(path: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Failed to open file: {}", e))?;

    Ok(())
}

pub fn toggle_autostart(enable: bool, app_id: &str) -> Result<(), String> {
    let autostart_dir = dirs::config_dir()
        .map(|p| p.join("autostart"))
        .ok_or_else(|| "Cannot determine autostart directory".to_string())?;

    if !autostart_dir.exists() {
        fs::create_dir_all(&autostart_dir)
            .map_err(|e| format!("Failed to create autostart directory: {}", e))?;
    }

    let desktop_file = autostart_dir.join(format!("{}.desktop", app_id));

    if enable {
        let current_exe = std::env::current_exe()
            .map_err(|e| format!("Cannot get executable path: {}", e))?;

        let exe_path = current_exe.to_string_lossy();

        let desktop_content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=TieZ\n\
             Exec={}\n\
             Icon={}\n\
             Terminal=false\n\
             Categories=Utility;\n",
            exe_path, app_id
        );

        fs::write(&desktop_file, desktop_content)
            .map_err(|e| format!("Failed to write autostart file: {}", e))?;
    } else {
        if desktop_file.exists() {
            fs::remove_file(&desktop_file)
                .map_err(|e| format!("Failed to remove autostart file: {}", e))?;
        }
    }

    Ok(())
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    toggle_autostart(enabled, "tiez")
}

pub fn is_autostart_enabled() -> bool {
    let autostart_path = get_autostart_path("tiez");
    autostart_path.exists()
}

pub fn get_autostart_path(app_id: &str) -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("autostart").join(format!("{}.desktop", app_id)))
        .unwrap_or_default()
}
