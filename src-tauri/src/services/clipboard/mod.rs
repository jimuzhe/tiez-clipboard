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

        // Give source app (especially Excel) time to release lock/finish writing
        // fixes "Another application is using the clipboard" error
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Initialize clipboard for this thread
        let mut clipboard = match Clipboard::new() {
            Ok(cb) => cb,
            Err(_) => return,
        };

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
            if let Ok(text) = clipboard.get_text() {
                text.hash(&mut hasher);
            }
            
            // Also consider image hash if present
            unsafe {
                if let Some(image) = crate::infrastructure::windows_api::win_clipboard::get_clipboard_image() {
                    image.bytes.hash(&mut hasher);
                }
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

        // --- Core processing logic (same as before) ---

        // 1. Check Files
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
                            if settings.capture_files.load(Ordering::Relaxed) || files.len() == 1 {
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
                            }
                        }
                    }
                }
            }
        }

        // 3. Check Text
        if !handled {
            if let Ok(text) = clipboard.get_text() {
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
pub use utils::truncate_html_for_preview;

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
    use super::*;

}
