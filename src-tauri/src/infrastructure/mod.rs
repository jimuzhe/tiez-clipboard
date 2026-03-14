#[cfg(target_os = "windows")]
pub mod windows_ext;
pub mod encryption;
pub mod repository;

#[cfg(target_os = "windows")]
pub mod windows_api;

#[cfg(not(target_os = "windows"))]
pub mod windows_api {
    pub mod win_clipboard {
        pub struct ImageData {
            pub width: usize,
            pub height: usize,
            pub bytes: Vec<u8>,
        }

        static SEQ: std::sync::atomic::AtomicU32 =
            std::sync::atomic::AtomicU32::new(1);

        pub fn get_clipboard_sequence_number() -> u32 {
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        }

        pub unsafe fn get_clipboard_image() -> Option<ImageData> {
            None
        }

        pub unsafe fn get_clipboard_files() -> Option<Vec<String>> {
            None
        }

        pub unsafe fn get_clipboard_raw_format(_name: &str) -> Option<Vec<u8>> {
            None
        }

        pub unsafe fn set_clipboard_files(_paths: Vec<String>) -> Result<(), String> {
            Ok(())
        }

        pub unsafe fn set_clipboard_text_and_html(
            _text: &str,
            _html: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        pub unsafe fn append_clipboard_text_and_html(
            _text: &str,
            _html: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        pub unsafe fn set_clipboard_image_with_formats(
            _data: ImageData,
            _gif_bytes: Option<&[u8]>,
            _png_bytes: Option<&[u8]>,
        ) -> Result<Option<String>, String> {
            Ok(None)
        }
    }

    pub mod window_tracker {
        pub fn start_window_tracking(_app_handle: tauri::AppHandle) {}

        pub fn get_active_app_info() -> (String, String) {
            ("Linux".into(), String::new())
        }
    }

    pub mod apps {
        use crate::error::{AppError, AppResult};
        use serde::{Deserialize, Serialize};
        use std::collections::{BTreeMap, BTreeSet};
        use std::path::{Path, PathBuf};
        use std::process::Command;

        #[derive(Serialize, Deserialize, Clone, Debug)]
        pub struct AppInfo {
            pub name: String,
            pub path: String,
        }

        fn application_dirs() -> Vec<PathBuf> {
            let mut dirs = Vec::new();

            if let Some(home) = std::env::var_os("HOME") {
                let home = PathBuf::from(home);
                dirs.push(home.join(".local/share/applications"));
            }

            dirs.push(PathBuf::from("/usr/local/share/applications"));
            dirs.push(PathBuf::from("/usr/share/applications"));
            dirs
        }

        fn mimeapps_files() -> Vec<PathBuf> {
            let mut files = Vec::new();

            if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
                files.push(PathBuf::from(path).join("mimeapps.list"));
            } else if let Some(home) = std::env::var_os("HOME") {
                files.push(PathBuf::from(home).join(".config/mimeapps.list"));
            }

            if let Some(home) = std::env::var_os("HOME") {
                files.push(PathBuf::from(home).join(".local/share/applications/mimeapps.list"));
            }

            files.push(PathBuf::from("/etc/xdg/mimeapps.list"));
            files.push(PathBuf::from("/usr/share/applications/mimeapps.list"));
            files
        }

        fn desktop_entry_path(file_name: &str) -> Option<PathBuf> {
            let name = file_name.trim();
            if name.is_empty() {
                return None;
            }

            let direct = PathBuf::from(name);
            if direct.exists() {
                return Some(direct);
            }

            for dir in application_dirs() {
                let candidate = dir.join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }

            None
        }

        fn read_desktop_entry(path: &Path) -> Option<(String, String)> {
            let contents = std::fs::read_to_string(path).ok()?;
            let mut in_desktop_entry = false;
            let mut name: Option<String> = None;
            let mut exec: Option<String> = None;
            let mut try_exec: Option<String> = None;
            let mut hidden = false;

            for raw_line in contents.lines() {
                let line = raw_line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                if line.starts_with('[') && line.ends_with(']') {
                    in_desktop_entry = line == "[Desktop Entry]";
                    continue;
                }

                if !in_desktop_entry {
                    continue;
                }

                if let Some(value) = line.strip_prefix("Name=") {
                    if name.is_none() {
                        name = Some(value.trim().to_string());
                    }
                    continue;
                }

                if let Some(value) = line.strip_prefix("Exec=") {
                    if exec.is_none() {
                        exec = Some(value.trim().to_string());
                    }
                    continue;
                }

                if let Some(value) = line.strip_prefix("TryExec=") {
                    if try_exec.is_none() {
                        try_exec = Some(value.trim().to_string());
                    }
                    continue;
                }

                if let Some(value) = line.strip_prefix("NoDisplay=") {
                    hidden |= value.eq_ignore_ascii_case("true");
                    continue;
                }

                if let Some(value) = line.strip_prefix("Hidden=") {
                    hidden |= value.eq_ignore_ascii_case("true");
                }
            }

            if hidden {
                return None;
            }

            let name = name?;
            let command = try_exec.or(exec)?;
            Some((name, command))
        }

        fn tokenize_exec_line(exec: &str) -> Vec<String> {
            let mut tokens = Vec::new();
            let mut current = String::new();
            let mut quote: Option<char> = None;
            let mut escaped = false;

            for ch in exec.chars() {
                if escaped {
                    current.push(ch);
                    escaped = false;
                    continue;
                }

                match ch {
                    '\\' if quote != Some('\'') => escaped = true,
                    '"' | '\'' => {
                        if quote == Some(ch) {
                            quote = None;
                        } else if quote.is_none() {
                            quote = Some(ch);
                        } else {
                            current.push(ch);
                        }
                    }
                    c if c.is_whitespace() && quote.is_none() => {
                        if !current.is_empty() {
                            tokens.push(current.clone());
                            current.clear();
                        }
                    }
                    _ => current.push(ch),
                }
            }

            if !current.is_empty() {
                tokens.push(current);
            }

            tokens
        }

        fn resolve_command_path(command: &str) -> Option<String> {
            let candidate = PathBuf::from(command);
            if candidate.is_absolute() && candidate.exists() {
                return Some(candidate.to_string_lossy().to_string());
            }

            if command.contains('/') {
                let cwd_candidate = std::env::current_dir().ok()?.join(command);
                if cwd_candidate.exists() {
                    return Some(cwd_candidate.to_string_lossy().to_string());
                }
                return None;
            }

            let path = std::env::var_os("PATH")?;
            for dir in std::env::split_paths(&path) {
                let full = dir.join(command);
                if full.exists() {
                    return Some(full.to_string_lossy().to_string());
                }
            }

            None
        }

        fn executable_from_exec_line(exec: &str) -> Option<String> {
            let mut tokens: Vec<String> = tokenize_exec_line(exec)
                .into_iter()
                .filter(|token| !token.starts_with('%'))
                .collect();

            if matches!(tokens.first(), Some(token) if token == "env") {
                tokens.remove(0);
                while matches!(tokens.first(), Some(token) if token.contains('=') && !token.starts_with('/')) {
                    tokens.remove(0);
                }
            }

            let candidate = tokens.first()?.trim();
            resolve_command_path(candidate)
        }

        fn app_info_from_desktop_entry(path: &Path) -> Option<AppInfo> {
            let (name, exec) = read_desktop_entry(path)?;
            let executable = executable_from_exec_line(&exec)?;
            Some(AppInfo {
                name,
                path: executable,
            })
        }

        fn read_mimeapps_associations(mime: &str) -> Vec<String> {
            let mut desktop_files = Vec::new();

            for path in mimeapps_files() {
                let Ok(contents) = std::fs::read_to_string(path) else {
                    continue;
                };

                for line in contents.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with('#') || trimmed.is_empty() {
                        continue;
                    }

                    let Some(value) = trimmed.strip_prefix(&format!("{mime}=")) else {
                        continue;
                    };

                    for entry in value.split(';') {
                        let entry = entry.trim();
                        if !entry.is_empty() {
                            desktop_files.push(entry.to_string());
                        }
                    }
                }
            }

            desktop_files
        }

        fn xdg_default_desktop_file(mime: &str) -> Option<String> {
            let output = Command::new("xdg-mime")
                .args(["query", "default", mime])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }

            let value = String::from_utf8(output.stdout).ok()?;
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }

        fn mime_for_content_type(content_type: &str) -> Option<String> {
            match content_type {
                "image" => Some("image/png".to_string()),
                "video" => Some("video/mp4".to_string()),
                "code" | "text" | "file" => Some("text/plain".to_string()),
                "link" | "url" => Some("x-scheme-handler/https".to_string()),
                _ => None,
            }
        }

        #[tauri::command]
        pub async fn scan_installed_apps() -> AppResult<Vec<AppInfo>> {
            let mut apps_by_path: BTreeMap<String, AppInfo> = BTreeMap::new();

            for dir in application_dirs() {
                let Ok(entries) = std::fs::read_dir(dir) else {
                    continue;
                };

                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                        continue;
                    }

                    if let Some(app) = app_info_from_desktop_entry(&path) {
                        apps_by_path.entry(app.path.clone()).or_insert(app);
                    }
                }
            }

            Ok(apps_by_path.into_values().collect())
        }

        #[tauri::command]
        pub async fn get_associated_apps(extension: String) -> AppResult<Vec<AppInfo>> {
            let ext = extension.trim().trim_start_matches('.');
            if ext.is_empty() {
                return Ok(Vec::new());
            }

            let mime = mime_guess::from_ext(ext)
                .first_raw()
                .map(str::to_string)
                .unwrap_or_else(|| "text/plain".to_string());

            let mut desktop_files = BTreeSet::new();

            if let Some(default_file) = xdg_default_desktop_file(&mime) {
                desktop_files.insert(default_file);
            }

            for desktop in read_mimeapps_associations(&mime) {
                desktop_files.insert(desktop);
            }

            let mut apps = Vec::new();
            for desktop in desktop_files {
                if let Some(path) = desktop_entry_path(&desktop) {
                    if let Some(app) = app_info_from_desktop_entry(&path) {
                        apps.push(app);
                    }
                }
            }

            Ok(apps)
        }

        #[tauri::command]
        pub fn get_system_default_app(content_type: String) -> AppResult<String> {
            let Some(mime) = mime_for_content_type(&content_type) else {
                return Ok("系统默认".to_string());
            };

            let Some(desktop_file) = xdg_default_desktop_file(&mime) else {
                return Ok("系统默认".to_string());
            };

            let Some(path) = desktop_entry_path(&desktop_file) else {
                return Ok("系统默认".to_string());
            };

            Ok(
                app_info_from_desktop_entry(&path)
                    .map(|app| app.name)
                    .unwrap_or_else(|| "系统默认".to_string()),
            )
        }

        pub async fn launch_uwp_with_file(app_id: &str, file_path: &str) -> AppResult<()> {
            let app_path = if Path::new(app_id).exists() {
                Some(app_id.to_string())
            } else if let Some(desktop_path) = desktop_entry_path(app_id) {
                app_info_from_desktop_entry(&desktop_path).map(|app| app.path)
            } else {
                None
            };

            if let Some(app_path) = app_path {
                Command::new(&app_path)
                    .arg(file_path)
                    .spawn()
                    .map_err(|e| AppError::Internal(format!("Failed to launch app: {}", e)))?;
                return Ok(());
            }

            Command::new("xdg-open")
                .arg(file_path)
                .spawn()
                .map_err(|e| AppError::Internal(format!("Failed to open file: {}", e)))?;
            Ok(())
        }
    }

    pub mod drag_drop {
        pub fn register_emoji_drag_drop(_app_handle: tauri::AppHandle) {}
    }
}
