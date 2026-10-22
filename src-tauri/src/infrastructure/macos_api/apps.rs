use base64::Engine;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "macos")]
use objc2::rc::{autoreleasepool, Retained};
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyObject, NSObjectProtocol};
#[cfg(target_os = "macos")]
use objc2::{sel, AllocAnyThread};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSImage, NSWorkspace,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    NSArray, NSDictionary, NSFileManager, NSPoint, NSRect, NSSize, NSString, NSURL,
};

#[cfg(target_os = "macos")]
#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    // The only full list of apps for a file before macOS 12, which added
    // NSWorkspace URLsForApplicationsToOpenURL (deprecated since, still there).
    fn LSCopyApplicationURLsForURL(url: &NSURL, roles: u32) -> *mut NSArray<NSURL>;
}

#[cfg(target_os = "macos")]
const LS_ROLES_ALL: u32 = 0xFFFF_FFFF;

/// Stands in for an extension where link entries ask for their apps: links
/// open in the browser, so their apps are the web browsers, not every app
/// that opens .html files.
pub const WEB_LINK: &str = "https";

static EXECUTABLE_ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
static FILE_ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

pub async fn launch_uwp_with_file(
    _package: &str,
    _file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // macOS equivalent: open -a or similar
    Ok(())
}

/// Name of the app macOS opens `ext` files (or, for WEB_LINK, links) with.
pub fn get_system_default_app(ext: &str) -> String {
    autoreleasepool(|_| {
        sample_url(ext)
            .and_then(|url| NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(&url))
            .and_then(|app| url_path(&app))
            .map(|app| app_display_name(&app))
            .unwrap_or_default()
    })
}

/// The apps macOS offers for `ext` files (or, for WEB_LINK, links), the
/// default one first, as in Finder's Open With.
pub fn get_associated_apps(ext: &str) -> Vec<Value> {
    autoreleasepool(|_| {
        let Some(url) = sample_url(ext) else {
            return Vec::new();
        };
        let workspace = NSWorkspace::sharedWorkspace();
        let default_app = workspace
            .URLForApplicationToOpenURL(&url)
            .and_then(|app| url_path(&app));
        // Copies elsewhere (build folders, mounted disk images, the Trash)
        // are registered too; offer only installed apps.
        let roots = app_roots();
        let mut others: Vec<PathBuf> = handler_paths(&workspace, &url)
            .into_iter()
            .filter(|app| roots.iter().any(|root| app.starts_with(root)))
            .collect();
        others.sort_by_key(|app| app_display_name(app).to_lowercase());

        let mut seen = HashSet::new();
        default_app
            .into_iter()
            .chain(others)
            .filter(|app| seen.insert(app.clone()))
            .map(|app| app_entry(&app))
            .collect()
    })
}

/// A URL standing for content of type `ext`: a web link, or an empty file
/// with that extension (LaunchServices finds no apps for a missing file).
fn sample_url(ext: &str) -> Option<Retained<NSURL>> {
    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    if ext.is_empty() || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    if ext == WEB_LINK {
        return NSURL::URLWithString(&NSString::from_str("https://example.com"));
    }
    let dir = std::env::temp_dir().join("tiez-open-with");
    let sample = dir.join(format!("sample.{ext}"));
    if !sample.is_file() {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&sample, b"").ok()?;
    }
    Some(NSURL::fileURLWithPath(&NSString::from_str(&sample.to_string_lossy())))
}

fn handler_paths(workspace: &NSWorkspace, url: &NSURL) -> Vec<PathBuf> {
    let apps = if workspace.respondsToSelector(sel!(URLsForApplicationsToOpenURL:)) {
        Some(workspace.URLsForApplicationsToOpenURL(url))
    } else {
        // SAFETY: a Copy function hands over one reference, which Retained takes.
        unsafe { Retained::from_raw(LSCopyApplicationURLsForURL(url, LS_ROLES_ALL)) }
    };
    apps.map(|apps| apps.to_vec().iter().filter_map(|app| url_path(app)).collect())
        .unwrap_or_default()
}

fn url_path(url: &NSURL) -> Option<PathBuf> {
    url.path().map(|path| PathBuf::from(path.to_string()))
}

/// The name Finder shows, in the user's language ("预览" for Preview.app).
fn app_display_name(app: &Path) -> String {
    let file_name = app
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();
    let shown = NSFileManager::defaultManager()
        .displayNameAtPath(&NSString::from_str(&app.to_string_lossy()))
        .to_string();
    let shown = shown.strip_suffix(".app").unwrap_or(&shown).trim();
    if shown.is_empty() {
        file_name
    } else {
        shown.to_string()
    }
}

fn app_entry(app: &Path) -> Value {
    json!({
        "name": app_display_name(app),
        "path": app.to_string_lossy()
    })
}

fn is_app_bundle(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("app"))
}

/// Apps directly in `root`, and `depth` folders down: suites keep theirs in
/// a folder (/Applications/Microsoft Office/…, /Applications/Adobe …/…).
fn collect_app_bundles(root: &Path, depth: u8, seen: &mut HashSet<PathBuf>, output: &mut Vec<Value>) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !is_app_bundle(&path) {
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if depth > 0 && !hidden {
                collect_app_bundles(&path, depth - 1, seen, output);
            }
            continue;
        }
        if seen.insert(path.clone()) {
            output.push(app_entry(&path));
        }
    }
}

fn app_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots
}

pub fn scan_installed_apps() -> Vec<Value> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    autoreleasepool(|_| {
        for root in app_roots() {
            // One level down also covers Utilities in both roots.
            collect_app_bundles(&root, 1, &mut seen, &mut apps);
        }
    });

    apps.sort_by(|a, b| {
        let an = a
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_lowercase();
        let bn = b
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_lowercase();
        an.cmp(&bn)
    });
    apps
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

/// App icons show at 14–20 pt (source app, Open With list); 64 px covers
/// that on Retina screens. Encoding the whole icon (every size up to
/// 1024 px) took 0.1–0.5 s on the main thread and ~1 MB per app.
#[cfg(target_os = "macos")]
const APP_ICON_PX: f64 = 64.0;

#[cfg(target_os = "macos")]
fn image_to_small_data_url(image: &NSImage, px: f64) -> Option<String> {
    autoreleasepool(|_| {
        let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(px, px));
        // SAFETY: `rect` lives across the call; no context or hints.
        let cg_image = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
        let bitmap = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg_image);
        let properties = NSDictionary::<NSBitmapImageRepPropertyKey, AnyObject>::from_slices(
            &[] as &[&NSBitmapImageRepPropertyKey],
            &[] as &[&AnyObject],
        );
        let png_data = unsafe {
            bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
        }?;
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png_data.to_vec())
        ))
    })
}

#[cfg(target_os = "macos")]
fn get_app_icon(path: &str) -> Option<String> {
    let normalized = normalize_icon_cache_key(path);
    if normalized.is_empty() || !Path::new(&normalized).exists() {
        return None;
    }
    autoreleasepool(|_| {
        let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&normalized));
        image_to_small_data_url(&image, APP_ICON_PX)
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

    let icon = get_app_icon(&executable_path);
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

/// Soft activation for overlay show — restores keyboard without yanking windows.
#[cfg(target_os = "macos")]
#[allow(deprecated)]
pub fn activate_app_by_pid_soft(pid: i32) -> bool {
    use objc2_app_kit::NSApplicationActivationOptions;
    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let apps = workspace.runningApplications();
        for i in 0..apps.count() {
            let app = apps.objectAtIndex(i);
            if app.processIdentifier() == pid {
                // Empty options: prefer keyboard restore without forcing our
                // panel under other apps or stealing the next click.
                return app.activateWithOptions(NSApplicationActivationOptions::empty());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(apps: &[Value]) -> Vec<String> {
        apps.iter().filter_map(|app| app["path"].as_str().map(str::to_string)).collect()
    }

    #[test]
    fn open_with_apps_come_from_launch_services_default_first() {
        let apps = get_associated_apps(".txt");
        let listed = paths(&apps);
        assert!(listed.iter().any(|p| p.ends_with("/TextEdit.app")), "{listed:?}");

        let default_app = autoreleasepool(|_| {
            sample_url("txt")
                .and_then(|url| NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(&url))
                .and_then(|app| url_path(&app))
        })
        .expect("a default app for .txt");
        assert_eq!(listed.first().map(String::as_str), default_app.to_str());
        assert_eq!(get_system_default_app("txt"), app_display_name(&default_app));
        assert!(apps.iter().all(|app| !app["name"].as_str().unwrap_or("").is_empty()));
    }

    #[test]
    fn app_icons_are_small() {
        let icon = get_executable_icon("/System/Applications/TextEdit.app".into())
            .unwrap()
            .expect("TextEdit has an icon");
        assert!(icon.starts_with("data:image/png;base64,"));
        // The full icon was ~1 MB.
        assert!(icon.len() < 200_000, "{} bytes", icon.len());
    }

    #[test]
    fn odd_extensions_get_no_apps() {
        assert!(get_associated_apps("").is_empty());
        assert!(get_associated_apps("../x").is_empty());
        assert_eq!(get_system_default_app("a b"), "");
    }

    #[test]
    fn installed_apps_include_utilities_one_folder_down() {
        let listed = paths(&scan_installed_apps());
        assert!(listed.iter().any(|p| p.ends_with("/Utilities/Terminal.app")), "{listed:?}");
    }
}
