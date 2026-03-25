mod pipeline;
mod utils;

pub use crate::database::DbState;
use crate::app_state::SettingsState;
use arboard::Clipboard;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use base64::Engine;

use utils::*;

const DEFAULT_CLIPBOARD_SETTLE_DELAY_MS: u64 = 100;
const SNIPPING_TOOL_SETTLE_DELAY_MS: u64 = 1200;

fn should_capture_file_entries(capture_files_enabled: bool) -> bool {
    capture_files_enabled
}

fn is_snipping_tool_source(
    source_snapshot: &crate::infrastructure::windows_api::window_tracker::ActiveAppInfo,
) -> bool {
    let app_name = source_snapshot.app_name.to_ascii_lowercase();
    let process_path = source_snapshot
        .process_path
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();

    [
        "snippingtool.exe",
        "snipping tool",
        "screenclippinghost.exe",
        "screen clipping host",
        "screensketch.exe",
        "screen sketch",
        "snipandsketch",
        "snip & sketch",
    ]
    .iter()
    .any(|needle| app_name.contains(needle) || process_path.contains(needle))
}

fn read_clipboard_text_once(
    clipboard: &mut Clipboard,
    cache: &mut Option<Option<String>>,
) -> Option<String> {
    if let Some(value) = cache.as_ref() {
        return value.clone();
    }

    let value = clipboard.get_text().ok();
    *cache = Some(value.clone());
    value
}

fn read_clipboard_image_once(
    cache: &mut Option<Option<crate::infrastructure::windows_api::win_clipboard::ImageData>>,
) -> Option<crate::infrastructure::windows_api::win_clipboard::ImageData> {
    if let Some(value) = cache.as_ref() {
        return value.clone();
    }

    let value = unsafe { crate::infrastructure::windows_api::win_clipboard::get_clipboard_image() };
    *cache = Some(value.clone());
    value
}

fn clipboard_image_fallback_data_url() -> Option<String> {
    for _ in 0..3 {
        unsafe {
            // Some sources (e.g. Office apps) may provide PNG/JPEG custom formats.
            for name in ["PNG", "image/png", "JFIF", "JPEG", "image/jpeg"] {
                if let Some(raw) = crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(name) {
                    if let Ok(img) = image::load_from_memory(&raw) {
                        let mut bytes: Vec<u8> = Vec::new();
                        let mut cursor = std::io::Cursor::new(&mut bytes);
                        if img.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                            return Some(format!("data:image/png;base64,{}", b64));
                        }
                    }
                }
            }

            // Fallback to CF_DIB/CF_DIBV5 decode.
            if let Some(image) = crate::infrastructure::windows_api::win_clipboard::get_clipboard_image() {
                if let Some(img_buf) =
                    image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.bytes)
                {
                    let mut bytes: Vec<u8> = Vec::new();
                    let mut cursor = std::io::Cursor::new(&mut bytes);
                    if img_buf.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                        return Some(format!("data:image/png;base64,{}", b64));
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(35));
    }

    None
}

pub fn start_clipboard_monitor(app_handle: AppHandle) {
    use std::sync::{Arc, Mutex};

    // Initial state for deduplication and self-copy detection
    let mut last_text = String::new();
    let last_seq = crate::infrastructure::windows_api::win_clipboard::get_clipboard_sequence_number();
    let mut last_image_hash = 0u64;

    // We can initialize these with current content to avoid capturing on startup
    if let Ok(mut cb) = Clipboard::new() {
        last_text = cb.get_text().unwrap_or_default();
        last_image_hash = unsafe {
            if let Some(image) = crate::infrastructure::windows_api::win_clipboard::get_clipboard_image() {
                let mut hash = image.bytes.len() as u64;
                if !image.bytes.is_empty() {
                    hash = hash
                        .wrapping_add(image.bytes[0] as u64)
                        .wrapping_add(image.bytes[image.bytes.len() / 2] as u64)
                        .wrapping_add(image.bytes[image.bytes.len() - 1] as u64);
                }
                hash
            } else {
                0u64
            }
        };
    }

    struct MonitorState {
        last_text: String,
        last_seq: u32,
        last_image_hash: u64,
        last_content_hash: u64,
        last_process_time: u64,
    }

    let state = Arc::new(Mutex::new(MonitorState {
        last_text,
        last_seq,
        last_image_hash,
        last_content_hash: 0,
        last_process_time: 0,
    }));

    let app_clone = app_handle.clone();
    let state_lock = state.clone();

    // Start the native Windows listener
    crate::services::clipboard_listener::listen_clipboard(Arc::new(move || {
        let app = app_clone.clone();
        let mut monitor_state = state_lock.lock().unwrap();

        // 1. Check for pause
        if crate::CLIPBOARD_MONITOR_PAUSED.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        // 2. Sequence check (De-bounce Windows firing multiple events for one copy)
        let current_seq = crate::infrastructure::windows_api::win_clipboard::get_clipboard_sequence_number();
        if current_seq == monitor_state.last_seq {
            return;
        }
        monitor_state.last_seq = current_seq;
        let source_snapshot = crate::infrastructure::windows_api::window_tracker::get_clipboard_source_app_info();

        // Give source apps time to finish writing clipboard payloads before we start
        // probing formats. Snipping Tool needs a longer quiet period or its save
        // pipeline may race with clipboard-manager reads.
        let settle_delay_ms = if is_snipping_tool_source(&source_snapshot) {
            SNIPPING_TOOL_SETTLE_DELAY_MS
        } else {
            DEFAULT_CLIPBOARD_SETTLE_DELAY_MS
        };
        std::thread::sleep(std::time::Duration::from_millis(settle_delay_ms));

        // Initialize clipboard for this thread
        let mut clipboard = match Clipboard::new() {
            Ok(cb) => cb,
            Err(_) => return,
        };

        let mut cached_text: Option<Option<String>> = None;
        let mut cached_image: Option<Option<crate::infrastructure::windows_api::win_clipboard::ImageData>> = None;

        // 3. Content-based deduplication with time window (for Chrome address bar, etc.)
        // Some apps trigger multiple clipboard updates with different sequence numbers
        // but identical content within a short time window
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        
        // Calculate hash of current clipboard content
        let current_content_hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            
            // Hash text content if available
            if let Some(text) = read_clipboard_text_once(&mut clipboard, &mut cached_text) {
                text.hash(&mut hasher);
            }
            
            // Also consider image hash if present
            if let Some(image) = read_clipboard_image_once(&mut cached_image) {
                image.bytes.hash(&mut hasher);
            }
            
            hasher.finish()
        };
        
        // If content is identical to last processed content within 500ms window, skip
        if current_content_hash == monitor_state.last_content_hash 
            && current_content_hash != 0
            && now.saturating_sub(monitor_state.last_process_time) < 500 {
            return;
        }
        
        monitor_state.last_content_hash = current_content_hash;
        monitor_state.last_process_time = now;

        let mut handled = false;

        // --- Core processing logic ---

        // On Linux, check image BEFORE files because browsers may put both
        // image data AND file:// URIs on clipboard when copying images.
        // We want the actual image data, not the temp file path.
        #[cfg(target_os = "linux")]
        {
            // 1. Check Image (Linux priority: Image first)
            let settings = app.state::<SettingsState>();
            let rich_text_enabled = settings.capture_rich_text.load(Ordering::Relaxed);
            let has_text = clipboard
                .get_text()
                .map(|t| !t.trim().is_empty())
                .unwrap_or(false);
            let has_rich_html = if rich_text_enabled && has_text {
                unsafe {
                    crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format("HTML Format")
                        .and_then(|raw| parse_cf_html(&raw))
                        .map(|html| !html.trim().is_empty())
                        .unwrap_or(false)
                }
            } else {
                false
            };

            if !has_rich_html {
                unsafe {
                    if let Some(image) = crate::infrastructure::windows_api::win_clipboard::get_clipboard_image() {
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        use std::hash::{Hash, Hasher};
                        image.bytes.hash(&mut hasher);
                        let hash = hasher.finish();

                        if hash != monitor_state.last_image_hash {
                            let last_app_hash = crate::LAST_APP_SET_HASH.load(Ordering::SeqCst);
                            let last_app_time = crate::LAST_APP_SET_TIMESTAMP.load(Ordering::SeqCst);
                            let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                            let last_app_hash_alt = crate::LAST_APP_SET_HASH_ALT.load(Ordering::SeqCst);

                            if last_app_hash != 0 && (last_app_hash == hash || last_app_hash_alt == hash) && (now_secs - last_app_time) < 10 {
                                crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                                crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                            } else {
                                if let Some(img_buf) = image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.bytes) {
                                    let mut bytes: Vec<u8> = Vec::new();
                                    let mut cursor = std::io::Cursor::new(&mut bytes);
                                    if img_buf.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                                        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                                        process_new_entry(&app, ClipboardData::Image { data_url: format!("data:image/png;base64,{}", b64) }, None, Some(source_snapshot.clone()));
                                        handled = true;
                                    }
                                }
                            }
                            monitor_state.last_image_hash = hash;
                        } else {
                            handled = true;
                        }
                    }
                }
            }
        }

        // 2. Check Files (or 1. on Windows)
        unsafe {
            if let Some(files) = crate::infrastructure::windows_api::win_clipboard::get_clipboard_files() {
                let content = files.join("\n");
                if !content.is_empty() {
                    let is_new = content != monitor_state.last_text;
                    let mut should_process = is_new;
                    if !is_new {
                        if let Some(db_state) = app.try_state::<DbState>() {
                            if let Ok(conn) = db_state.conn.lock() {
                                if let Ok(None) = db_state.repo.find_by_content_with_conn(&conn, &content, None) {
                                    should_process = true;
                                }
                            }
                        }
                    }

                    if should_process {
                        let normalized = content.trim().replace("\r\n", "\n");
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        use std::hash::{Hash, Hasher};
                        normalized.hash(&mut hasher);
                        let current_hash = hasher.finish();

                        let last_app_hash = crate::LAST_APP_SET_HASH.load(Ordering::SeqCst);
                        let last_app_hash_alt = crate::LAST_APP_SET_HASH_ALT.load(Ordering::SeqCst);
                        let last_app_time = crate::LAST_APP_SET_TIMESTAMP.load(Ordering::SeqCst);
                        let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

                        if (last_app_hash != 0 && (last_app_hash == current_hash || last_app_hash_alt == current_hash)) && (now_secs - last_app_time) < 10 {
                            crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                            crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                        } else {
                            crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                            crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                            monitor_state.last_text = content.clone();
                            
                            let settings = app.state::<SettingsState>();
                            if should_capture_file_entries(settings.capture_files.load(Ordering::Relaxed)) {
                                process_new_entry(&app, ClipboardData::Files(files), None, Some(source_snapshot.clone()));
                            }
                        }
                    }
                    handled = true;
                }
            }
        }

        // 2. Check Image
        if !handled {
            let settings = app.state::<SettingsState>();
            let rich_text_enabled = settings.capture_rich_text.load(Ordering::Relaxed);
            let has_text = read_clipboard_text_once(&mut clipboard, &mut cached_text)
                .map(|t| !t.trim().is_empty())
                .unwrap_or(false);
            let has_rich_html = if rich_text_enabled && has_text {
                unsafe {
                    crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format("HTML Format")
                        .and_then(|raw| parse_cf_html(&raw))
                        .map(|html| !html.trim().is_empty())
                        .unwrap_or(false)
                }
            } else {
                false
            };

            // Rich text wins over image when rich HTML exists; image remains fallback for pure image content.
            if !has_rich_html {
                unsafe {
                    let mut gif_data_opt = None;
                    for name in ["GIF", "Animated GIF", "gif", "image/gif", "Graphics Interchange Format"] {
                        if let Some(data) = crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(name) {
                            gif_data_opt = Some(data);
                            break;
                        }
                    }

                    if let Some(gif_data) = gif_data_opt {
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        use std::hash::{Hash, Hasher};
                        gif_data.hash(&mut hasher);
                        let hash = hasher.finish();
                        handled = true;

                        if hash != monitor_state.last_image_hash {
                            let last_app_hash = crate::LAST_APP_SET_HASH.load(Ordering::SeqCst);
                            let last_app_time = crate::LAST_APP_SET_TIMESTAMP.load(Ordering::SeqCst);
                            let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                            let last_app_hash_alt = crate::LAST_APP_SET_HASH_ALT.load(Ordering::SeqCst);

                            if last_app_hash != 0 && (last_app_hash == hash || last_app_hash_alt == hash) && (now_secs - last_app_time) < 10 {
                                crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                                crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                            } else {
                                let b64 = base64::engine::general_purpose::STANDARD.encode(gif_data);
                                process_new_entry(&app, ClipboardData::Image { data_url: format!("data:image/gif;base64,{}", b64) }, None, Some(source_snapshot.clone()));
                                monitor_state.last_text = String::new();
                            }
                            monitor_state.last_image_hash = hash;
                        }
                    }

                    if !handled {
                        if let Some(image) = read_clipboard_image_once(&mut cached_image) {
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            use std::hash::{Hash, Hasher};
                            image.bytes.hash(&mut hasher);
                            let hash = hasher.finish();

                            if hash != monitor_state.last_image_hash {
                                let last_app_hash = crate::LAST_APP_SET_HASH.load(Ordering::SeqCst);
                                let last_app_time = crate::LAST_APP_SET_TIMESTAMP.load(Ordering::SeqCst);
                                let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                                let last_app_hash_alt = crate::LAST_APP_SET_HASH_ALT.load(Ordering::SeqCst);

                                if last_app_hash != 0 && (last_app_hash == hash || last_app_hash_alt == hash) && (now_secs - last_app_time) < 10 {
                                    crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                                    crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                                } else {
                                    if let Some(img_buf) = image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.bytes) {
                                        let mut bytes: Vec<u8> = Vec::new();
                                        let mut cursor = std::io::Cursor::new(&mut bytes);
                                        if img_buf.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                                            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                                            process_new_entry(&app, ClipboardData::Image { data_url: format!("data:image/png;base64,{}", b64) }, None, Some(source_snapshot.clone()));
                                            handled = true;
                                        }
                                    }
                                }
                                monitor_state.last_image_hash = hash;
                            }
                        }
                    }
                }
            }
        }

        // 3. Check Text
        if !handled {
            if let Some(text) = read_clipboard_text_once(&mut clipboard, &mut cached_text) {
                if !text.is_empty() {
                    let settings = app.state::<SettingsState>();
                    
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    text.trim().replace("\r\n", "\n").hash(&mut hasher);
                    let current_hash = hasher.finish();

                    let last_app_hash = crate::LAST_APP_SET_HASH.load(Ordering::SeqCst);
                    let last_app_time = crate::LAST_APP_SET_TIMESTAMP.load(Ordering::SeqCst);
                    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

                    if (last_app_hash != 0 && (current_hash == last_app_hash || current_hash == crate::LAST_APP_SET_HASH_ALT.load(Ordering::SeqCst))) && (now_secs - last_app_time) < 10 {
                        crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                        crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                        monitor_state.last_text = text.clone();
                        return;
                    }

                    if settings.capture_rich_text.load(Ordering::Relaxed) {
                        if let Some(html_raw) = unsafe { crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format("HTML Format") } {
                            if let Some(html) = parse_cf_html(&html_raw) {
                                if !html.trim().is_empty() {
                                    let mut html_to_store = html;

                                    // If source clipboard also carries an image format, keep it as a rich fallback
                                    // so paste targets can choose image/HTML/text based on their own priority rules.
                                    if let Some(data_url) = clipboard_image_fallback_data_url() {
                                        html_to_store = attach_rich_image_fallback(&html_to_store, &data_url);
                                    }

                                    monitor_state.last_text = text.clone();
                                    process_new_entry(
                                        &app,
                                        ClipboardData::RichText {
                                            text: text.clone(),
                                            html: html_to_store,
                                        },
                                        None,
                                        Some(source_snapshot.clone()),
                                    );
                                    handled = true;
                                }
                            }
                        }
                    }

                    if !handled {
                        if last_app_hash != 0 { crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst); }
                        monitor_state.last_text = text.clone();
                        process_new_entry(&app, ClipboardData::Text(text), None, Some(source_snapshot.clone()));
                    }
                }
            }
        }
    }));
}

pub use pipeline::{ClipboardData, ClipboardPipeline, PipelineContext};
pub use utils::{build_entry_preview, derive_rich_text_content, repair_html_fragment, truncate_html_for_preview};

pub fn process_new_entry(
    app_handle: &AppHandle,
    data: ClipboardData,
    source_override: Option<String>,
    source_snapshot: Option<crate::infrastructure::windows_api::window_tracker::ActiveAppInfo>,
) {
    let mut ctx = PipelineContext::new(app_handle.clone(), data, source_snapshot);
    if let Some(source) = source_override {
        ctx.source_app = source;
        ctx.source_app_path = None;
    }

    let pipeline = ClipboardPipeline::new();
    pipeline.execute(&mut ctx);
}


#[cfg(test)]
mod tests {
    use super::{is_snipping_tool_source, should_capture_file_entries};
    use crate::infrastructure::windows_api::window_tracker::ActiveAppInfo;

    #[test]
    fn detects_snipping_tool_by_process_name() {
        let source = ActiveAppInfo {
            app_name: "SnippingTool.exe".to_string(),
            process_path: Some(r"C:\Windows\System32\SnippingTool.exe".to_string()),
        };

        assert!(is_snipping_tool_source(&source));
    }

    #[test]
    fn detects_legacy_screen_sketch_source() {
        let source = ActiveAppInfo {
            app_name: "ApplicationFrameHost.exe".to_string(),
            process_path: Some(
                r"C:\Program Files\WindowsApps\Microsoft.ScreenSketch_2022.2405.32.0_x64__8wekyb3d8bbwe\ScreenSketch.exe"
                    .to_string(),
            ),
        };

        assert!(is_snipping_tool_source(&source));
    }

    #[test]
    fn ignores_normal_apps() {
        let source = ActiveAppInfo {
            app_name: "WINWORD.EXE".to_string(),
            process_path: Some(r"C:\Program Files\Microsoft Office\root\Office16\WINWORD.EXE".to_string()),
        };

        assert!(!is_snipping_tool_source(&source));
    }

    #[test]
    fn file_capture_follows_setting_when_disabled() {
        assert!(!should_capture_file_entries(false));
    }

    #[test]
    fn file_capture_follows_setting_when_enabled() {
        assert!(should_capture_file_entries(true));
    }
}
