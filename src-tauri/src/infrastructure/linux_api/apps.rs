use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppInfo {
    pub name: String,
    pub path: String,
}

struct DesktopEntry {
    name: String,
    exec: String,
    icon: Option<String>,
    no_display: bool,
    is_application: bool,
}

// ── .desktop file parsing ──────────────────────────────────────────

fn parse_desktop_file(path: &Path) -> Option<DesktopEntry> {
    let content = fs::read_to_string(path).ok()?;
    let mut in_entry = false;

    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut no_display = false;
    let mut hidden = false;
    let mut is_application = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if trimmed == "[Desktop Entry]" {
            in_entry = true;
            continue;
        }
        if trimmed.starts_with('[') {
            if in_entry {
                break; // past the main section
            }
            continue;
        }
        if !in_entry {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            match key.trim() {
                "Name" => name = Some(value.trim().to_string()),
                "Exec" => exec = Some(value.trim().to_string()),
                "Icon" => icon = Some(value.trim().to_string()),
                "NoDisplay" => no_display = value.trim().eq_ignore_ascii_case("true"),
                "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
                "Type" => is_application = value.trim() == "Application",
                _ => {}
            }
        }
    }

    let name = name?;
    let exec = exec?;

    Some(DesktopEntry {
        name,
        exec: strip_exec_field_codes(&exec),
        icon,
        no_display: no_display || hidden,
        is_application,
    })
}

/// Strip freedesktop.org field codes (%f, %F, %u, %U, etc.) and extract
/// the executable token from an Exec line.
fn strip_exec_field_codes(exec: &str) -> String {
    // Handle quoted executable path: "path with spaces" arg1 arg2 %f
    if exec.starts_with('"') {
        if let Some(end) = exec[1..].find('"') {
            let exe = &exec[1..end + 1];
            return exe.to_string();
        }
    }

    // Otherwise take the first token and strip field codes
    let first_token = exec
        .split_whitespace()
        .next()
        .unwrap_or(exec);

    // Remove field codes: %f %F %u %U %d %D %n %N %i %c %k %v %m %%
    // These are always a single token starting with %
    let mut result = first_token.to_string();
    if result.starts_with('%') {
        result.clear();
    }

    result
}

// ── XDG directory helpers ──────────────────────────────────────────

fn get_app_directories() -> Vec<PathBuf> {
    let mut dirs = vec![];

    // System directories (lower priority first)
    dirs.push(PathBuf::from("/usr/share/applications"));
    dirs.push(PathBuf::from("/usr/local/share/applications"));

    // Flatpak system
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));

    // Snap
    dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));

    // User local (highest priority)
    if let Some(data_dir) = dirs::data_dir() {
        dirs.push(data_dir.join("applications"));
    }

    // Flatpak user
    if let Some(data_dir) = dirs::data_dir() {
        dirs.push(data_dir.join("flatpak/exports/share/applications"));
    }

    dirs.retain(|d| d.exists());
    dirs
}

/// Scan all app directories and parse .desktop files into DesktopEntry list.
fn scan_desktop_entries() -> Vec<(PathBuf, DesktopEntry)> {
    let mut entries: Vec<(PathBuf, DesktopEntry)> = vec![];

    for dir in get_app_directories() {
        if let Ok(readdir) = fs::read_dir(&dir) {
            for entry in readdir.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("desktop") {
                    if let Some(de) = parse_desktop_file(&path) {
                        if de.is_application && !de.no_display {
                            entries.push((path, de));
                        }
                    }
                }
            }
        }
    }

    entries
}

// ── Tauri commands ─────────────────────────────────────────────────

#[tauri::command]
pub async fn scan_installed_apps() -> AppResult<Vec<AppInfo>> {
    let entries = scan_desktop_entries();

    // Deduplicate by exec name (later = higher priority, overwrites earlier)
    let mut seen: HashMap<String, AppInfo> = HashMap::new();
    for (_, de) in entries {
        let key = de.exec.to_lowercase();
        seen.entry(key).and_modify(|existing| {
            // Prefer entries that already have a full path
            if existing.path.contains('/') && !de.exec.contains('/') {
                return;
            }
            existing.name = de.name.clone();
            existing.path = de.exec.clone();
        }).or_insert_with(|| AppInfo {
            name: de.name.clone(),
            path: de.exec.clone(),
        });
    }

    let mut apps: Vec<AppInfo> = seen.into_values().collect();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(apps)
}

#[tauri::command]
pub fn get_system_default_app(content_type: String) -> AppResult<String> {
    let mime = match content_type.as_str() {
        "text" => "text/plain",
        "image" => "image/png",
        "video" => "video/mp4",
        "code" => "text/plain",
        "file" => "text/plain",
        "link" | "url" => "x-scheme-handler/http",
        _ => return Ok(String::new()),
    };

    // Try xdg-mime query default
    let output = Command::new("xdg-mime")
        .args(["query", "default", mime])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let desktop_file = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !desktop_file.is_empty() {
                return resolve_desktop_name(&desktop_file);
            }
        }
    }

    Ok(String::new())
}

#[tauri::command]
pub async fn get_associated_apps(extension: String) -> AppResult<Vec<AppInfo>> {
    let ext = extension.trim_start_matches('.');
    if ext.is_empty() {
        return Ok(vec![]);
    }

    // Determine MIME type(s) for this extension
    let mime_types = resolve_mime_types(ext);

    // Parse mimeinfo.cache to find associated apps
    let mut app_desktop_ids: Vec<String> = vec![];

    for dir in get_app_directories() {
        let cache_path = dir.join("mimeinfo.cache");
        if let Ok(content) = fs::read_to_string(&cache_path) {
            for line in content.lines() {
                if let Some((mime, apps_str)) = line.split_once('=') {
                    if mime_types.contains(&mime.to_string()) {
                        for id in apps_str.split(';') {
                            let id = id.trim().to_string();
                            if !id.is_empty() && !app_desktop_ids.contains(&id) {
                                app_desktop_ids.push(id);
                            }
                        }
                    }
                }
            }
        }
    }

    // Resolve each .desktop ID to AppInfo
    let mut result = vec![];
    for id in &app_desktop_ids {
        if let Some(app_info) = resolve_desktop_id_to_app_info(id) {
            result.push(app_info);
        }
    }

    Ok(result)
}

#[tauri::command]
pub fn get_executable_icon(executable_path: String) -> AppResult<Option<String>> {
    let cache_key = executable_path.trim().to_lowercase();
    if cache_key.is_empty() {
        return Ok(None);
    }

    let cache = ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache.lock().map_err(|e| AppError::Internal(e.to_string()))?.get(&cache_key) {
        return Ok(cached.clone());
    }

    let icon = find_icon_for_exec(&executable_path);
    cache
        .lock()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .insert(cache_key, icon.clone());
    Ok(icon)
}

#[tauri::command]
pub fn get_file_icon(_file_path: String) -> AppResult<Option<String>> {
    Ok(None)
}

static ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

pub fn open_with_default(path: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Failed to open: {}", e))?;

    Ok(())
}

// ── Internal helpers ───────────────────────────────────────────────

/// Resolve a .desktop filename (e.g. "firefox.desktop") to a friendly name.
fn resolve_desktop_name(desktop_file: &str) -> AppResult<String> {
    for dir in get_app_directories() {
        let path = dir.join(desktop_file);
        if let Some(de) = parse_desktop_file(&path) {
            return Ok(de.name);
        }
    }

    // Fallback: strip .desktop and capitalize
    let name = desktop_file
        .strip_suffix(".desktop")
        .unwrap_or(desktop_file);
    let mut chars = name.chars();
    let result = match chars.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
    };
    Ok(result)
}

/// Resolve a .desktop ID to AppInfo by finding and parsing the file.
fn resolve_desktop_id_to_app_info(desktop_id: &str) -> Option<AppInfo> {
    for dir in get_app_directories() {
        let path = dir.join(desktop_id);
        if let Some(de) = parse_desktop_file(&path) {
            return Some(AppInfo {
                name: de.name,
                path: de.exec,
            });
        }
    }
    None
}

/// Determine MIME type(s) for a file extension.
fn resolve_mime_types(ext: &str) -> Vec<String> {
    let mut types = vec![];

    // Special mappings for our content types
    match ext {
        "txt" => {
            types.push("text/plain".to_string());
        }
        "png" => {
            types.push("image/png".to_string());
        }
        "mp4" => {
            types.push("video/mp4".to_string());
        }
        "html" | "htm" => {
            types.push("text/html".to_string());
            types.push("x-scheme-handler/http".to_string());
        }
        _ => {}
    }

    // Use mime_guess as fallback
    if types.is_empty() {
        if let Some(mime) = mime_guess::from_ext(ext).first() {
            types.push(mime.to_string());
        }
    }

    types
}

/// Find icon for an executable path by scanning .desktop files.
fn find_icon_for_exec(exec_path: &str) -> Option<String> {
    let exec_lower = exec_path.to_lowercase();
    let exec_basename = Path::new(exec_path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(exec_path)
        .to_lowercase();

    // Search for a matching .desktop file
    for dir in get_app_directories() {
        if let Ok(readdir) = fs::read_dir(&dir) {
            for entry in readdir.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                if let Some(de) = parse_desktop_file(&path) {
                    let de_exec_lower = de.exec.to_lowercase();
                    let de_exec_basename = Path::new(&de.exec)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(&de.exec)
                        .to_lowercase();

                    if de_exec_lower == exec_lower || de_exec_basename == exec_basename {
                        if let Some(icon_name) = de.icon {
                            return resolve_icon(&icon_name);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Resolve an icon name/path to a base64 PNG data URL.
fn resolve_icon(icon_name: &str) -> Option<String> {
    let icon_path = Path::new(icon_name);

    // Absolute path — use directly
    if icon_path.is_absolute() && icon_path.exists() {
        return encode_icon_file(icon_path);
    }

    // Named icon — search standard directories
    let search_dirs = get_icon_search_dirs();
    let name = icon_name;

    for dir in &search_dirs {
        if let Ok(readdir) = fs::read_dir(dir) {
            for entry in readdir.flatten() {
                let path = entry.path();
                let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                // Match exact name with .png/.svg extension
                let matches = filename == format!("{}.png", name)
                    || filename == format!("{}.svg", name)
                    || filename == name;
                if matches {
                    return encode_icon_file(&path);
                }
            }
        }
    }

    // Search hicolor theme (most common fallback) with size subdirs
    let hicolor_dirs = [
        "/usr/share/icons/hicolor",
        "/usr/share/pixmaps",
    ];
    for base in &hicolor_dirs {
        if let Ok(readdir) = fs::read_dir(base) {
            for entry in readdir.flatten() {
                let subdir = entry.path().join("apps");
                if subdir.exists() {
                    if let Ok(app_readdir) = fs::read_dir(&subdir) {
                        for app_entry in app_readdir.flatten() {
                            let path = app_entry.path();
                            let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                            if filename == format!("{}.png", name)
                                || filename == format!("{}.svg", name)
                            {
                                return encode_icon_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

fn get_icon_search_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/pixmaps"),
        PathBuf::from("/usr/share/icons"),
    ];

    if let Some(data_dir) = dirs::data_dir() {
        dirs.push(data_dir.join("icons"));
    }

    dirs.retain(|d| d.exists());
    dirs
}

/// Read an icon file and encode as base64 data URL.
fn encode_icon_file(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "png" => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Some(format!("data:image/png;base64,{}", b64))
        }
        "jpg" | "jpeg" => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Some(format!("data:image/jpeg;base64,{}", b64))
        }
        "svg" => {
            // Encode SVG as-is (small and scalable)
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Some(format!("data:image/svg+xml;base64,{}", b64))
        }
        _ => {
            // Try to load with image crate and re-encode as PNG
            if let Ok(img) = image::load_from_memory(&bytes) {
                let mut png_bytes = Vec::new();
                use std::io::Cursor;
                if img
                    .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
                    .is_ok()
                {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
                    Some(format!("data:image/png;base64,{}", b64))
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}
