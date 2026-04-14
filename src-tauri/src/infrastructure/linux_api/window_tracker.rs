use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct ActiveAppInfo {
    pub app_name: String,
    pub process_path: Option<String>,
}

pub fn start_window_tracking(_app_handle: tauri::AppHandle) {}

pub fn get_active_app_info() -> ActiveAppInfo {
    ActiveAppInfo {
        app_name: "Unknown".to_string(),
        process_path: None,
    }
}

pub fn get_clipboard_source_app_info() -> ActiveAppInfo {
    get_active_app_info()
}

pub fn launch_uwp_with_file(_package: &str, _file: &str) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

pub fn get_system_default_app(_ext: &str) -> String {
    String::new()
}

pub fn get_executable_icon(_executable_path: String) -> Result<Option<String>, String> {
    Ok(None)
}

pub fn scan_installed_apps() -> Vec<Value> {
    vec![]
}

pub fn get_associated_apps(_ext: &str) -> Vec<Value> {
    vec![]
}
