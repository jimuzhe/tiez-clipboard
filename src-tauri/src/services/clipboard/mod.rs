mod pipeline;
mod utils;

use crate::app_state::SettingsState;
pub use crate::database::DbState;
use crate::services::clipboard::utils::attach_rich_image_fallback;
use arboard::Clipboard;
use base64::Engine;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
const MAX_MACOS_TEXT_BYTES: usize = 128 * 1024;
#[cfg(target_os = "macos")]
const MIN_CLIPBOARD_EVENT_INTERVAL_MS: u64 = 80; // Reduced from 120ms

fn build_rich_image_fallback_data_url(
    width: usize,
    height: usize,
    rgba_bytes: &[u8],
) -> Option<String> {
    let img_buf = image::RgbaImage::from_raw(width as u32, height as u32, rgba_bytes.to_vec())?;
    let mut bytes: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    img_buf
        .write_to(&mut cursor, image::ImageFormat::Png)
        .ok()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:image/png;base64,{}", b64))
}

pub fn start_clipboard_monitor(app_handle: AppHandle) {
    use std::sync::{Arc, Mutex};

    // Initial state for deduplication and self-copy detection.
    let mut last_image_hash = 0u64;

    if let Ok(mut cb) = Clipboard::new() {
        if let Ok(img) = cb.get_image() {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            use std::hash::{Hash, Hasher};
            img.bytes.hash(&mut hasher);
            last_image_hash = hasher.finish();
        }
    }

    struct MonitorState {
        last_image_hash: u64,
        last_content_hash: u64,
        last_process_time: u64,
    }

    let state = Arc::new(Mutex::new(MonitorState {
        last_image_hash,
        last_content_hash: 0,
        last_process_time: 0,
    }));

    let app_clone = app_handle.clone();
    let state_lock = state.clone();

    // Start the native clipboard listener
    crate::services::clipboard_listener::listen_clipboard(Arc::new(move || {
        let app = app_clone.clone();
        let mut monitor_state = state_lock.lock().unwrap();

        // 1. Check for pause
        if crate::CLIPBOARD_MONITOR_PAUSED.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        // 2. We don't use sequence check on macOS as we rely on polling/listener triggering.

        // Give source app (especially Excel) time to release lock/finish writing
        // Reduced from 100ms to 20ms for better responsiveness.
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Initialize clipboard for this thread
        #[cfg(not(target_os = "macos"))]
        let mut clipboard = match Clipboard::new() {
            Ok(cb) => cb,
            Err(_) => return,
        };
        #[cfg(target_os = "macos")]
        let mut clipboard = Clipboard::new().ok();
        #[cfg(target_os = "macos")]
        let text_from_clipboard = crate::infrastructure::macos_api::clipboard::get_clipboard_text();

        // 3. Content-based deduplication with time window (for Chrome address bar, etc.)
        // Some apps trigger multiple clipboard updates with different sequence numbers
        // but identical content within a short time window
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Storm guard: drop extremely dense events to avoid UI stalls and memory spikes.
        #[cfg(target_os = "macos")]
        if now.saturating_sub(monitor_state.last_process_time) < MIN_CLIPBOARD_EVENT_INTERVAL_MS {
            return;
        }

        // Calculate hash of current clipboard content
        let current_content_hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();

            // Hash text content if available
            #[cfg(target_os = "macos")]
            {
                if let Some(text) = &text_from_clipboard {
                    if text.len() > MAX_MACOS_TEXT_BYTES {
                        "__TEXT_TOO_LARGE__".hash(&mut hasher);
                    } else {
                        text.hash(&mut hasher);
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            if let Ok(text) = clipboard.get_text() {
                text.hash(&mut hasher);
            }

            // Also consider image hash if present
            #[cfg(target_os = "macos")]
            if let Some(cb) = clipboard.as_mut() {
                if let Ok(image) = cb.get_image() {
                    image.bytes.hash(&mut hasher);
                }
            }
            #[cfg(not(target_os = "macos"))]
            if let Ok(image) = clipboard.get_image() {
                image.bytes.hash(&mut hasher);
            }

            hasher.finish()
        };

        // Strict duplicate guard: identical payload should never be processed twice.
        if current_content_hash == monitor_state.last_content_hash && current_content_hash != 0 {
            return;
        }

        monitor_state.last_content_hash = current_content_hash;
        monitor_state.last_process_time = now;

        let mut handled = false;

        // --- Core processing logic (same as before) ---

        // 1. Check Files (macOS)
        #[cfg(target_os = "macos")]
        {
            if let Some(file_paths) =
                crate::infrastructure::macos_api::clipboard::get_clipboard_files()
            {
                let settings = app.state::<SettingsState>();
                if settings.capture_files.load(Ordering::Relaxed) {
                    process_new_entry(&app, ClipboardData::Files(file_paths), None);
                }
                // Whether we captured or skipped, we MUST mark as handled so the file
                // icon (image) and filename (text) are NOT captured.
                return;
            }
        }
        // 2. Check Image
        if !handled {
            let settings = app.state::<SettingsState>();
            let rich_text_enabled = settings.capture_rich_text.load(Ordering::Relaxed);
            #[cfg(target_os = "macos")]
            let has_text = text_from_clipboard
                .as_ref()
                .map(|t| !t.trim().is_empty())
                .unwrap_or(false);
            #[cfg(not(target_os = "macos"))]
            let has_text = clipboard
                .get_text()
                .map(|t| !t.trim().is_empty())
                .unwrap_or(false);
            let has_rich_html = if rich_text_enabled && has_text {
                crate::infrastructure::macos_api::clipboard::get_clipboard_html()
                    .map(|html| !html.trim().is_empty())
                    .unwrap_or(false)
            } else {
                false
            };

            // Rich text wins over image when rich HTML exists; image remains fallback for pure image content.
            if !has_rich_html {
                if !handled {
                    #[cfg(target_os = "macos")]
                    let image_result = clipboard.as_mut().and_then(|cb| cb.get_image().ok());
                    #[cfg(not(target_os = "macos"))]
                    let image_result = clipboard.get_image().ok();

                    if let Some(image) = image_result {
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        use std::hash::{Hash, Hasher};
                        image.bytes.hash(&mut hasher);
                        let hash = hasher.finish();

                        if hash != monitor_state.last_image_hash {
                            let last_app_hash = crate::LAST_APP_SET_HASH.load(Ordering::SeqCst);
                            let last_app_time =
                                crate::LAST_APP_SET_TIMESTAMP.load(Ordering::SeqCst);
                            let now_secs = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let last_app_hash_alt =
                                crate::LAST_APP_SET_HASH_ALT.load(Ordering::SeqCst);

                            if last_app_hash != 0
                                && (last_app_hash == hash || last_app_hash_alt == hash)
                                && (now_secs - last_app_time) < 10
                            {
                                crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                                crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                            } else if let Some(img_buf) = image::RgbaImage::from_raw(
                                image.width as u32,
                                image.height as u32,
                                image.bytes.to_vec(),
                            ) {
                                let mut bytes: Vec<u8> = Vec::new();
                                let mut cursor = std::io::Cursor::new(&mut bytes);
                                if img_buf
                                    .write_to(&mut cursor, image::ImageFormat::Png)
                                    .is_ok()
                                {
                                    let b64 =
                                        base64::engine::general_purpose::STANDARD.encode(bytes);
                                    process_new_entry(
                                        &app,
                                        ClipboardData::Image {
                                            data_url: format!("data:image/png;base64,{}", b64),
                                        },
                                        None,
                                    );
                                    handled = true;
                                }
                            }
                            monitor_state.last_image_hash = hash;
                        }
                    }
                }
            }
        }

        if !handled {
            #[cfg(target_os = "macos")]
            {
                let text = text_from_clipboard.unwrap_or_default();
                if !text.is_empty() {
                    if text.len() > MAX_MACOS_TEXT_BYTES {
                        eprintln!(
                            ">>> [CLIPBOARD] Skip capture: text exceeds {} bytes",
                            MAX_MACOS_TEXT_BYTES
                        );
                        return;
                    }
                    let settings = app.state::<SettingsState>();

                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    text.trim().replace("\r\n", "\n").hash(&mut hasher);
                    let current_hash = hasher.finish();

                    let last_app_hash = crate::LAST_APP_SET_HASH.load(Ordering::SeqCst);
                    let last_app_time = crate::LAST_APP_SET_TIMESTAMP.load(Ordering::SeqCst);
                    let now_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    if (last_app_hash != 0
                        && (current_hash == last_app_hash
                            || current_hash == crate::LAST_APP_SET_HASH_ALT.load(Ordering::SeqCst)))
                        && (now_secs - last_app_time) < 10
                    {
                        crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                        crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                        return;
                    }

                    if settings.capture_rich_text.load(Ordering::Relaxed) {
                        if let Some(mut html) =
                            crate::infrastructure::macos_api::clipboard::get_clipboard_html()
                        {
                            if !html.trim().is_empty() {
                                let image_opt =
                                    clipboard.as_mut().and_then(|cb| cb.get_image().ok());
                                if let Some(image) = image_opt {
                                    if let Some(data_url) = build_rich_image_fallback_data_url(
                                        image.width,
                                        image.height,
                                        &image.bytes,
                                    ) {
                                        html = attach_rich_image_fallback(&html, &data_url);
                                    }
                                }

                                process_new_entry(
                                    &app,
                                    ClipboardData::RichText {
                                        text: text.clone(),
                                        html,
                                    },
                                    None,
                                );
                                handled = true;
                            }
                        }
                    }

                    if !handled {
                        if last_app_hash != 0 {
                            crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                        }
                        process_new_entry(&app, ClipboardData::Text(text), None);
                    }
                }
            }

            #[cfg(not(target_os = "macos"))]
            if let Ok(text) = clipboard.get_text() {
                if !text.is_empty() {
                    let settings = app.state::<SettingsState>();

                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    text.trim().replace("\r\n", "\n").hash(&mut hasher);
                    let current_hash = hasher.finish();

                    let last_app_hash = crate::LAST_APP_SET_HASH.load(Ordering::SeqCst);
                    let last_app_time = crate::LAST_APP_SET_TIMESTAMP.load(Ordering::SeqCst);
                    let now_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    if (last_app_hash != 0
                        && (current_hash == last_app_hash
                            || current_hash == crate::LAST_APP_SET_HASH_ALT.load(Ordering::SeqCst)))
                        && (now_secs - last_app_time) < 10
                    {
                        crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                        crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                        return;
                    }

                    if settings.capture_rich_text.load(Ordering::Relaxed) {
                        if let Some(mut html) =
                            crate::infrastructure::macos_api::clipboard::get_clipboard_html()
                        {
                            if !html.trim().is_empty() {
                                if let Ok(image) = clipboard.get_image() {
                                    if let Some(data_url) = build_rich_image_fallback_data_url(
                                        image.width,
                                        image.height,
                                        &image.bytes,
                                    ) {
                                        html = attach_rich_image_fallback(&html, &data_url);
                                    }
                                }

                                process_new_entry(
                                    &app,
                                    ClipboardData::RichText {
                                        text: text.clone(),
                                        html,
                                    },
                                    None,
                                );
                                handled = true;
                            }
                        }
                    }

                    if !handled {
                        if last_app_hash != 0 {
                            crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                        }
                        process_new_entry(&app, ClipboardData::Text(text), None);
                    }
                }
            }
        }
    }));
}

pub use pipeline::{ClipboardData, ClipboardPipeline, PipelineContext};
pub use utils::{derive_rich_text_content, embed_local_images, truncate_html_for_preview};

const MAX_PIPELINE_TEXT_BYTES: usize = 128 * 1024;
const MAX_PIPELINE_HTML_BYTES: usize = 512 * 1024;

pub fn process_new_entry(
    app_handle: &AppHandle,
    data: ClipboardData,
    source_override: Option<String>,
) {
    let payload_too_large = match &data {
        ClipboardData::Text(text) => text.len() > MAX_PIPELINE_TEXT_BYTES,
        ClipboardData::RichText { text, html } => {
            text.len() > MAX_PIPELINE_TEXT_BYTES || html.len() > MAX_PIPELINE_HTML_BYTES
        }
        _ => false,
    };

    if payload_too_large {
        eprintln!(
            ">>> [CLIPBOARD] Skip pipeline: payload exceeds text/html limits (text:{} bytes, html:{} bytes)",
            MAX_PIPELINE_TEXT_BYTES,
            MAX_PIPELINE_HTML_BYTES
        );
        return;
    }

    let mut ctx = PipelineContext::new(app_handle.clone(), data);
    if let Some(source) = source_override {
        ctx.source_app = source;
        ctx.source_app_path = None;
    }

    let pipeline = ClipboardPipeline::new();
    pipeline.execute(&mut ctx);
}
