use base64::Engine;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "macos")]
use objc2::rc::autoreleasepool;
#[cfg(target_os = "macos")]
use objc2::runtime::AnyObject;
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSImage, NSWorkspace,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSDictionary, NSString};

static EXECUTABLE_ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
static FILE_ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

pub async fn launch_uwp_with_file(
    _package: &str,
    _file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // macOS equivalent: open -a or similar
    Ok(())
}

pub fn get_system_default_app(ext: &str) -> String {
    // Lightweight fallback mapping on macOS.
    // A proper LaunchServices lookup can replace this later.
    let normalized = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    match normalized.as_str() {
        "txt" | "text" | "md" | "code" | "json" | "xml" | "csv" => "TextEdit".to_string(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" | "tiff" => "Preview".to_string(),
        "mp4" | "mov" | "mkv" | "avi" | "wmv" | "webm" | "m4v" => "QuickTime Player".to_string(),
        "html" | "htm" | "url" => "Safari".to_string(),
        _ => String::new(),
    }
}

fn collect_app_bundles(root: &Path, seen: &mut HashSet<String>, output: &mut Vec<Value>) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let is_app = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("app"))
            .unwrap_or(false);
        if !is_app {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        if !seen.insert(path_str.clone()) {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        output.push(json!({
            "name": name,
            "path": path_str
        }));
    }
}

pub fn scan_installed_apps() -> Vec<Value> {
    let mut roots: Vec<PathBuf> = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ];

    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }

    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for root in roots {
        collect_app_bundles(&root, &mut seen, &mut apps);
    }

    apps.sort_by(|a, b| {
        let an = a
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let bn = b
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        an.cmp(&bn)
    });
    apps
}

pub fn get_associated_apps(_ext: &str) -> Vec<Value> {
    // Fallback on macOS: expose installed apps as candidates.
    // Proper LaunchServices association lookup can be added later.
    scan_installed_apps()
}

fn bundle_path_from_target(path: &Path) -> Option<PathBuf> {
    let path_str = path.to_string_lossy();
    if let Some(idx) = path_str.find(".app/") {
        return Some(PathBuf::from(&path_str[..idx + 4]));
    }

    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("app"))
        .unwrap_or(false)
    {
        return Some(path.to_path_buf());
    }

    None
}

fn normalize_icon_cache_key(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let candidate = Path::new(trimmed);
    if let Ok(resolved) = std::fs::canonicalize(candidate) {
        if let Some(bundle) = bundle_path_from_target(&resolved) {
            return bundle.to_string_lossy().to_string();
        }
        return resolved.to_string_lossy().to_string();
    }

    if let Some(bundle) = bundle_path_from_target(candidate) {
        return bundle.to_string_lossy().to_string();
    }

    trimmed.to_string()
}

fn normalize_file_icon_cache_key(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let candidate = Path::new(trimmed);
    if candidate.exists() {
        return normalize_icon_cache_key(trimmed);
    }

    candidate
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_else(|| trimmed.to_string())
}

#[cfg(target_os = "macos")]
fn image_to_data_url(image: &NSImage) -> Result<Option<String>, String> {
    autoreleasepool(|_| {
        let Some(tiff_data) = image.TIFFRepresentation() else {
            return Ok(None);
        };
        let Some(bitmap) = NSBitmapImageRep::imageRepWithData(&tiff_data) else {
            return Ok(None);
        };

        let properties = NSDictionary::<NSBitmapImageRepPropertyKey, AnyObject>::from_slices(
            &[] as &[&NSBitmapImageRepPropertyKey],
            &[] as &[&AnyObject],
        );

        let png_data = unsafe {
            bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
        };
        let Some(png_data) = png_data else {
            return Ok(None);
        };

        Ok(Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png_data.to_vec())
        )))
    })
}

#[cfg(target_os = "macos")]
fn get_workspace_icon_for_file(path: &str) -> Result<Option<String>, String> {
    let normalized = normalize_icon_cache_key(path);
    if normalized.is_empty() {
        return Ok(None);
    }

    let candidate = Path::new(&normalized);
    if !candidate.exists() {
        return Ok(None);
    }

    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let ns_path = NSString::from_str(&normalized);
        let image = workspace.iconForFile(&ns_path);
        image_to_data_url(&image)
    })
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn get_workspace_icon_for_file_type(path_or_ext: &str) -> Result<Option<String>, String> {
    let key = normalize_file_icon_cache_key(path_or_ext);
    if key.is_empty() {
        return Ok(None);
    }

    let file_type = key.trim_start_matches('.').trim();
    if file_type.is_empty() {
        return Ok(None);
    }

    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let ns_type = NSString::from_str(file_type);
        let image = workspace.iconForFileType(&ns_type);
        image_to_data_url(&image)
    })
}

pub fn get_executable_icon(executable_path: String) -> Result<Option<String>, String> {
    let cache_key = normalize_icon_cache_key(&executable_path);
    if cache_key.is_empty() {
        return Ok(None);
    }

    let cache = EXECUTABLE_ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .map_err(|e| e.to_string())?
        .get(&cache_key)
        .cloned()
    {
        return Ok(cached);
    }

    let icon = get_workspace_icon_for_file(&executable_path)?;
    cache
        .lock()
        .map_err(|e| e.to_string())?
        .insert(cache_key, icon.clone());
    Ok(icon)
}

pub fn get_file_icon(file_path: String) -> Result<Option<String>, String> {
    let cache_key = normalize_file_icon_cache_key(&file_path);
    if cache_key.is_empty() {
        return Ok(None);
    }

    let cache = FILE_ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .map_err(|e| e.to_string())?
        .get(&cache_key)
        .cloned()
    {
        return Ok(cached);
    }

    let icon = if Path::new(file_path.trim()).exists() {
        get_workspace_icon_for_file(&file_path)?
    } else {
        get_workspace_icon_for_file_type(&file_path)?
    };

    cache
        .lock()
        .map_err(|e| e.to_string())?
        .insert(cache_key, icon.clone());
    Ok(icon)
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
pub fn activate_app_by_pid(pid: i32) -> bool {
    use objc2_app_kit::NSApplicationActivationOptions;
    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let apps = workspace.runningApplications();
        for i in 0..apps.count() {
            let app = apps.objectAtIndex(i);
            if app.processIdentifier() == pid {
                return app.activateWithOptions(
                    NSApplicationActivationOptions::ActivateIgnoringOtherApps,
                );
            }
        }
        false
    })
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
pub fn activate_app_by_name(name: &str) -> bool {
    use objc2_app_kit::NSApplicationActivationOptions;
    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let apps = workspace.runningApplications();
        let target_name = name.to_lowercase();
        for i in 0..apps.count() {
            let app = apps.objectAtIndex(i);
            if let Some(bundle_name) = app.localizedName().map(|s| s.to_string()) {
                if bundle_name.to_lowercase() == target_name {
                    return app.activateWithOptions(
                        NSApplicationActivationOptions::ActivateIgnoringOtherApps,
                    );
                }
            }
        }
        false
    })
}
