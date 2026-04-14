use serde_json::Value;

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
