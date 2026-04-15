mod pipeline;
mod utils;

use crate::app_state::SettingsState;
pub use crate::database::DbState;
use crate::database::{
    calc_image_hash, calc_image_hash_from_bytes, calc_image_hash_from_rgba, calc_text_hash,
};
use arboard::Clipboard;
use base64::Engine;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

use utils::*;

const DEFAULT_CLIPBOARD_SETTLE_DELAY_MS: u64 = 100;
const SNIPPING_TOOL_SETTLE_DELAY_MS: u64 = 1200;
const RICH_TEXT_RETRY_DELAYS_MS: [u64; 7] = [0, 40, 80, 140, 220, 360, 560];
const PRESERVED_NAMED_FORMAT_MAX_COUNT: usize = 12;
const PRESERVED_NAMED_FORMAT_MAX_BYTES: usize = 1_500_000;
const PRESERVED_NAMED_FORMAT_TOTAL_BYTES: usize = 4_000_000;

fn clear_recent_image_echo_state() {
    crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
    crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
    crate::LAST_APP_SET_IMAGE_VISUAL_HASH.store(0, Ordering::SeqCst);
}

fn should_ignore_recent_image_echo(raw_hash: u64, visual_hash: u64) -> bool {
    let last_app_hash = crate::LAST_APP_SET_HASH.load(Ordering::SeqCst);
    let last_app_hash_alt = crate::LAST_APP_SET_HASH_ALT.load(Ordering::SeqCst);
    let last_app_visual_hash = crate::LAST_APP_SET_IMAGE_VISUAL_HASH.load(Ordering::SeqCst);
    let last_app_time = crate::LAST_APP_SET_TIMESTAMP.load(Ordering::SeqCst);
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if now_secs.saturating_sub(last_app_time) >= 10 {
        return false;
    }

    ((last_app_hash != 0) && (last_app_hash == raw_hash || last_app_hash_alt == raw_hash))
        || ((visual_hash != 0) && last_app_visual_hash == visual_hash)
}

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

fn read_clipboard_text_fresh() -> Option<String> {
    Clipboard::new()
        .ok()
        .and_then(|mut clipboard| clipboard.get_text().ok())
        .filter(|text| !text.trim().is_empty())
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

fn is_likely_rich_text_source(
    source_snapshot: &crate::infrastructure::windows_api::window_tracker::ActiveAppInfo,
) -> bool {
    let mut haystack = source_snapshot.app_name.to_ascii_lowercase();
    if let Some(path) = source_snapshot.process_path.as_deref() {
        if !haystack.is_empty() {
            haystack.push(' ');
        }
        haystack.push_str(&path.to_ascii_lowercase());
    }

    [
        "wps",
        "winword",
        "word",
        "excel",
        "powerpoint",
        "onenote",
        "outlook",
        "soffice",
        "libreoffice",
        "writer",
        "calc",
        "impress",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
}

fn is_likely_spreadsheet_source(
    source_snapshot: &crate::infrastructure::windows_api::window_tracker::ActiveAppInfo,
) -> bool {
    let mut haystack = source_snapshot.app_name.to_ascii_lowercase();
    if let Some(path) = source_snapshot.process_path.as_deref() {
        if !haystack.is_empty() {
            haystack.push(' ');
        }
        haystack.push_str(&path.to_ascii_lowercase());
    }

    ["excel", "et.exe", "wps", "spreadsheet", "calc", "numbers"]
        .iter()
        .any(|needle| haystack.contains(needle))
}

fn is_ignored_named_clipboard_format(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return true;
    }

    if lower == "html format" {
        return true;
    }

    [
        "png",
        "image/png",
        "gif",
        "animated gif",
        "image/gif",
        "jfif",
        "jpeg",
        "image/jpeg",
        "bitmap",
        "dib",
        "dibv5",
        "tiff",
        "image/tiff",
        "webp",
        "image/webp",
        "emf",
        "wmf",
        "metafile",
        "object descriptor",
        "link source descriptor",
        "embed source",
        "embedded object",
        "link source",
        "ownerlink",
        "dataobject",
        "ole private data",
        "ole clipboard persist on flush",
        "shell idlist array",
        "preferred dropeffect",
        "performed dropeffect",
        "logical performed dropeffect",
        "filegroupdescriptorw",
        "filegroupdescriptora",
        "filecontents",
        "uniformresourcelocator",
        "uniformresourcelocatorw",
        "inetturl",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn should_preserve_named_clipboard_format(
    source_snapshot: Option<&crate::infrastructure::windows_api::window_tracker::ActiveAppInfo>,
    format_name: &str,
) -> bool {
    if is_ignored_named_clipboard_format(format_name) {
        return false;
    }

    let lower = format_name.trim().to_ascii_lowercase();
    if lower == "rich text format" {
        return true;
    }

    let Some(source_snapshot) = source_snapshot else {
        return true;
    };

    if !is_likely_spreadsheet_source(source_snapshot) {
        return false;
    }

    true
}

pub fn capture_preserved_named_formats_from_clipboard(
    source_snapshot: Option<&crate::infrastructure::windows_api::window_tracker::ActiveAppInfo>,
) -> Vec<crate::infrastructure::windows_api::win_clipboard::NamedClipboardFormat> {
    let should_retry = source_snapshot
        .map(is_likely_spreadsheet_source)
        .unwrap_or(false);

    for attempt in 0..3 {
        let formats = unsafe {
            crate::infrastructure::windows_api::win_clipboard::get_named_clipboard_formats(
                PRESERVED_NAMED_FORMAT_MAX_COUNT * 2,
                PRESERVED_NAMED_FORMAT_MAX_BYTES,
                PRESERVED_NAMED_FORMAT_TOTAL_BYTES,
            )
        }
        .into_iter()
        .filter(|format| should_preserve_named_clipboard_format(source_snapshot, &format.name))
        .take(PRESERVED_NAMED_FORMAT_MAX_COUNT)
        .collect::<Vec<_>>();

        if !formats.is_empty() || !should_retry || attempt == 2 {
            return formats;
        }

        std::thread::sleep(std::time::Duration::from_millis(35));
    }

    Vec::new()
}

fn has_rich_text_candidate_format() -> bool {
    unsafe {
        ["HTML Format", "Rich Text Format"]
            .iter()
            .any(|format_name| {
                crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(
                    format_name,
                )
                .map(|raw| !raw.is_empty())
                .unwrap_or(false)
            })
    }
}

fn probe_rich_text_payload(
    source_snapshot: &crate::infrastructure::windows_api::window_tracker::ActiveAppInfo,
    initial_text: Option<String>,
) -> Option<(String, String)> {
    let mut latest_text = initial_text.filter(|text| !text.trim().is_empty());
    let mut saw_candidate_format = false;

    for (attempt_idx, delay_ms) in RICH_TEXT_RETRY_DELAYS_MS.iter().enumerate() {
        if attempt_idx > 0 {
            std::thread::sleep(std::time::Duration::from_millis(*delay_ms));
        }

        if let Some(text) = read_clipboard_text_fresh() {
            latest_text = Some(text);
        }

        let parsed_html = unsafe {
            let html_raw =
                crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(
                    "HTML Format",
                );
            if html_raw.is_some() {
                saw_candidate_format = true;
            }
            html_raw
                .and_then(|raw| parse_cf_html(&raw))
                .filter(|html| !html.trim().is_empty())
        };

        if let Some(html) = parsed_html {
            let plain_text = latest_text
                .as_deref()
                .map(|text| derive_rich_text_content(text, Some(&html)))
                .unwrap_or_else(|| derive_rich_text_content("", Some(&html)));
            let normalized_text = if plain_text.trim().is_empty() {
                latest_text
                    .as_deref()
                    .map(normalize_clipboard_plain_text)
                    .unwrap_or_default()
            } else {
                plain_text
            };

            return Some((normalized_text, html));
        }

        unsafe {
            if crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(
                "Rich Text Format",
            )
            .map(|raw| !raw.is_empty())
            .unwrap_or(false)
            {
                saw_candidate_format = true;
            }
        }

        if let Some(text) = latest_text.as_deref() {
            if let Some(html) = infer_rich_html_from_plain_text(
                text,
                &source_snapshot.app_name,
                source_snapshot.process_path.as_deref(),
            ) {
                let plain_text = derive_rich_text_content(text, Some(&html));
                let normalized_text = if plain_text.trim().is_empty() {
                    normalize_clipboard_plain_text(text)
                } else {
                    plain_text
                };
                return Some((normalized_text, html));
            }
        }

        let should_keep_waiting = saw_candidate_format
            || latest_text
                .as_deref()
                .map(looks_like_cf_html_header_text)
                .unwrap_or(false)
            || is_likely_rich_text_source(source_snapshot);

        if !should_keep_waiting {
            break;
        }
    }

    None
}

pub fn clipboard_image_fallback_data_url() -> Option<String> {
    for _ in 0..3 {
        unsafe {
            // 1. Try GIF first to preserve animation
            for name in ["GIF", "Animated GIF", "image/gif"] {
                if let Some(raw) =
                    crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(
                        name,
                    )
                {
                    // Basic check to ensure it's a GIF
                    if raw.len() > 6 && (raw.starts_with(b"GIF87a") || raw.starts_with(b"GIF89a")) {
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
                        return Some(format!("data:image/gif;base64,{}", b64));
                    }
                }
            }

            // 2. Some sources (e.g. Office apps) may provide PNG/JPEG custom formats.
            // Avoid the expensive decode→re-encode round-trip when possible.
            for name in [
                "PNG",
                "image/png",
                "JFIF",
                "JPEG",
                "image/jpeg",
                "image/webp",
                "WebP",
            ] {
                if let Some(raw) =
                    crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(
                        name,
                    )
                {
                    // Fast path: if the raw bytes are already valid PNG/JPEG, skip
                    // image::load_from_memory + re-encode (saves ~200-800ms).
                    if raw.len() > 8 && raw[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
                    {
                        // Valid PNG header — use directly
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
                        return Some(format!("data:image/png;base64,{}", b64));
                    }
                    if raw.len() > 2 && raw[0] == 0xFF && raw[1] == 0xD8 {
                        // Valid JPEG header — use directly
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
                        return Some(format!("data:image/jpeg;base64,{}", b64));
                    }
                    // Fallback: decode and re-encode if format isn't recognized
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

            // 3. Fallback to CF_DIB/CF_DIBV5 decode.
            if let Some(image) =
                crate::infrastructure::windows_api::win_clipboard::get_clipboard_image()
            {
                if let Some(img_buf) =
                    image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.bytes)
                {
                    let mut bytes: Vec<u8> = Vec::new();
                    let mut cursor = std::io::Cursor::new(&mut bytes);
                    if img_buf
                        .write_to(&mut cursor, image::ImageFormat::Png)
                        .is_ok()
                    {
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
    let last_seq =
        crate::infrastructure::windows_api::win_clipboard::get_clipboard_sequence_number();
    let mut last_image_hash = 0u64;

    // We can initialize these with current content to avoid capturing on startup
    if let Ok(mut cb) = Clipboard::new() {
        last_text = normalize_clipboard_plain_text(&cb.get_text().unwrap_or_default());
        last_image_hash = unsafe {
            if let Some(image) =
                crate::infrastructure::windows_api::win_clipboard::get_clipboard_image()
            {
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
        let current_seq =
            crate::infrastructure::windows_api::win_clipboard::get_clipboard_sequence_number();
        if current_seq == monitor_state.last_seq {
            return;
        }
        monitor_state.last_seq = current_seq;
        let source_snapshot =
            crate::infrastructure::windows_api::window_tracker::get_clipboard_source_app_info();

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
        let mut cached_image: Option<
            Option<crate::infrastructure::windows_api::win_clipboard::ImageData>,
        > = None;

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
                normalize_clipboard_plain_text(&text).hash(&mut hasher);
            }

            // Also consider image hash if present
            if let Some(image) = read_clipboard_image_once(&mut cached_image) {
                image.bytes.hash(&mut hasher);
            }

            hasher.finish()
        };

        // If content is identical to last processed content within 2000ms window, skip.
        // Rich text sources (Office/WPS) may fire multiple clipboard events over >500ms
        // because they write formats sequentially and probe_rich_text_payload retries.
        if current_content_hash == monitor_state.last_content_hash
            && current_content_hash != 0
            && now.saturating_sub(monitor_state.last_process_time) < 2000
        {
            return;
        }

        monitor_state.last_content_hash = current_content_hash;
        monitor_state.last_process_time = now;

        let mut handled = false;

        // --- Core processing logic (same as before) ---

        // 1. Check Files
        unsafe {
            if let Some(files) =
                crate::infrastructure::windows_api::win_clipboard::get_clipboard_files()
            {
                let content = files.join("\n");
                if !content.is_empty() {
                    let is_new = content != monitor_state.last_text;
                    let mut should_process = is_new;
                    if !is_new {
                        if let Some(db_state) = app.try_state::<DbState>() {
                            if let Ok(conn) = db_state.conn.lock() {
                                if let Ok(None) = db_state
                                    .repo
                                    .find_by_content_with_conn(&conn, &content, None)
                                {
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
                        let now_secs = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();

                        if (last_app_hash != 0
                            && (last_app_hash == current_hash || last_app_hash_alt == current_hash))
                            && (now_secs - last_app_time) < 10
                        {
                            crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                            crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                        } else {
                            crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                            crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                            monitor_state.last_text = content.clone();

                            let settings = app.state::<SettingsState>();
                            if should_capture_file_entries(
                                settings.capture_files.load(Ordering::Relaxed),
                            ) {
                                process_new_entry(
                                    &app,
                                    ClipboardData::Files(files),
                                    None,
                                    Some(source_snapshot.clone()),
                                );
                            }
                        }
                    }
                    handled = true;
                }
            }
        }

        if !handled {
            let settings = app.state::<SettingsState>();
            let rich_text_enabled = settings.capture_rich_text.load(Ordering::Relaxed);
            let initial_text = read_clipboard_text_once(&mut clipboard, &mut cached_text)
                .filter(|text| !text.trim().is_empty());

            // Fast-path: if the clipboard has a GIF format, skip the expensive
            // rich text probing entirely and let the image handler (section 3)
            // process it directly.  Previously GIFs went through rich text
            // probing → clipboard_image_fallback_data_url → only to be discarded
            // by `prefer_image`, wasting 0.5–2 s.
            let clipboard_has_gif = unsafe {
                ["GIF", "Animated GIF", "gif", "image/gif"]
                    .iter()
                    .any(|name| {
                        crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(
                            name,
                        )
                        .is_some()
                    })
            };

            // When there's no text and the source isn't a dedicated rich text
            // application, this is almost certainly a pure image copy (e.g.
            // right-click → Copy image in a browser).  Rich text probing would
            // just be discarded by `prefer_image = true` later, so skip it
            // entirely to avoid 100–1400 ms of wasted retries.
            let pure_image_copy =
                initial_text.is_none() && !is_likely_rich_text_source(&source_snapshot);

            let should_probe_rich_text = rich_text_enabled
                && !clipboard_has_gif
                && !pure_image_copy
                && (initial_text.is_some()
                    || is_likely_rich_text_source(&source_snapshot)
                    || has_rich_text_candidate_format());

            if should_probe_rich_text {
                if let Some((text, html)) =
                    probe_rich_text_payload(&source_snapshot, initial_text.clone())
                {
                    let normalized_text = normalize_clipboard_plain_text(&text);
                    let current_hash = calc_text_hash(&normalized_text);
                    let current_html_hash =
                        calc_text_hash(&crate::services::clipboard::repair_html_fragment(&html));

                    let last_app_hash = crate::LAST_APP_SET_HASH.load(Ordering::SeqCst);
                    let last_app_hash_alt = crate::LAST_APP_SET_HASH_ALT.load(Ordering::SeqCst);
                    let last_app_time = crate::LAST_APP_SET_TIMESTAMP.load(Ordering::SeqCst);
                    let now_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    if last_app_hash != 0
                        && ((current_hash == last_app_hash || current_hash == last_app_hash_alt)
                            || (current_html_hash == last_app_hash
                                || current_html_hash == last_app_hash_alt))
                        && (now_secs - last_app_time) < 10
                    {
                        crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                        crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
                        monitor_state.last_text = normalized_text;
                        handled = true;
                    } else {
                        let html_animated_gif_fallback =
                            extract_animated_image_data_url_from_html(&html);
                        let mut html_to_store = html;

                        // When the HTML already has <img> tags with local/renderable
                        // sources, skip the very expensive clipboard bitmap capture
                        // (clipboard_image_fallback_data_url reads CF_DIB, does PNG
                        // encoding + base64, typically 500–2000 ms).  HtmlContent
                        // renders those images directly via Tauri asset paths.
                        let html_has_renderable_images =
                            html_to_store.to_ascii_lowercase().contains("<img ");

                        if let Some(data_url) = html_animated_gif_fallback.clone().or_else(|| {
                            if html_has_renderable_images {
                                None
                            } else {
                                clipboard_image_fallback_data_url()
                            }
                        }) {
                            html_to_store = attach_rich_image_fallback(&html_to_store, &data_url);
                        }

                        let preserved_named_formats =
                            capture_preserved_named_formats_from_clipboard(Some(&source_snapshot));
                        if !preserved_named_formats.is_empty() {
                            html_to_store =
                                attach_rich_named_formats(&html_to_store, &preserved_named_formats);
                        }

                        let has_gif = unsafe {
                            let mut found = false;
                            for name in ["GIF", "Animated GIF", "gif", "image/gif"] {
                                if crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(name).is_some() {
                                    found = true;
                                    break;
                                }
                            }
                            found
                        };

                        // If the derived text is empty (or this is a pure image copy from browser),
                        // we prefer the image handler unless this is a dedicated rich text source.
                        // For GIFs, we are especially aggressive because capturing as rich text
                        // results in a static preview snapshot, killing the animation.
                        let prefer_image = (text.trim().is_empty() || has_gif)
                            && !is_likely_rich_text_source(&source_snapshot)
                            && html_animated_gif_fallback.is_none()
                            && (read_clipboard_image_once(&mut cached_image).is_some() || has_gif);

                        if !prefer_image {
                            monitor_state.last_text = normalized_text.clone();
                            process_new_entry(
                                &app,
                                ClipboardData::RichText {
                                    text: normalized_text,
                                    html: html_to_store,
                                },
                                None,
                                Some(source_snapshot.clone()),
                            );
                            handled = true;

                            // Update content hash after rich text processing so that
                            // a subsequent clipboard event with the same raw clipboard
                            // content is correctly deduped within the time window.
                            // We reuse `current_content_hash` (computed from the raw
                            // clipboard text + image at the top of the handler) to
                            // ensure it matches the next event's initial hash.
                            monitor_state.last_content_hash = current_content_hash;
                            monitor_state.last_process_time = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis()
                                as u64;
                        }
                    }
                }
            }
        }

        // 3. Check Image
        if !handled {
            unsafe {
                let mut gif_data_opt = None;
                for name in [
                    "GIF",
                    "Animated GIF",
                    "gif",
                    "image/gif",
                    "Graphics Interchange Format",
                    "image/x-gif",
                ] {
                    if let Some(data) =
                        crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(
                            name,
                        )
                    {
                        gif_data_opt = Some(data);
                        break;
                    }
                }

                let text_animated_gif_fallback = if gif_data_opt.is_none() {
                    read_clipboard_text_fresh()
                        .and_then(|text| extract_animated_image_data_url_from_text(&text))
                } else {
                    None
                };

                if let Some(gif_data) = gif_data_opt {
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    gif_data.hash(&mut hasher);
                    let hash = hasher.finish();
                    let visual_hash =
                        calc_image_hash_from_bytes(&gif_data).unwrap_or(hash as i64) as u64;
                    handled = true;

                    if hash != monitor_state.last_image_hash {
                        if should_ignore_recent_image_echo(hash, visual_hash) {
                            clear_recent_image_echo_state();
                        } else {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(gif_data);
                            process_new_entry(
                                &app,
                                ClipboardData::Image {
                                    data_url: format!("data:image/gif;base64,{}", b64),
                                },
                                None,
                                Some(source_snapshot.clone()),
                            );
                            monitor_state.last_text = String::new();
                        }
                        monitor_state.last_image_hash = hash;
                    }
                } else if let Some(data_url) = text_animated_gif_fallback {
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    data_url.hash(&mut hasher);
                    let hash = hasher.finish();
                    let visual_hash = calc_image_hash(&data_url).unwrap_or(hash as i64) as u64;
                    handled = true;

                    if hash != monitor_state.last_image_hash {
                        if should_ignore_recent_image_echo(hash, visual_hash) {
                            clear_recent_image_echo_state();
                        } else {
                            process_new_entry(
                                &app,
                                ClipboardData::Image { data_url },
                                None,
                                Some(source_snapshot.clone()),
                            );
                            monitor_state.last_text = String::new();
                        }
                        monitor_state.last_image_hash = hash;
                    }
                }

                if !handled {
                    // Fast path: try native PNG/JPEG clipboard formats first.
                    // Browsers often provide these alongside CF_DIB, and using
                    // them directly avoids the expensive bitmap → PNG encode.
                    let fast_data_url: Option<(String, u64, u64)> = {
                        use std::hash::{Hash, Hasher};
                        let mut result = None;
                        for name in ["PNG", "image/png", "JFIF", "JPEG", "image/jpeg"] {
                            if let Some(raw) =
                                crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(name)
                            {
                                if raw.len() > 8 && raw[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
                                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                                    raw.hash(&mut hasher);
                                    let hash = hasher.finish();
                                    let visual_hash =
                                        calc_image_hash_from_bytes(&raw).unwrap_or(hash as i64) as u64;
                                    let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
                                    result = Some((
                                        format!("data:image/png;base64,{}", b64),
                                        hash,
                                        visual_hash,
                                    ));
                                    break;
                                }
                                if raw.len() > 2 && raw[0] == 0xFF && raw[1] == 0xD8 {
                                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                                    raw.hash(&mut hasher);
                                    let hash = hasher.finish();
                                    let visual_hash =
                                        calc_image_hash_from_bytes(&raw).unwrap_or(hash as i64) as u64;
                                    let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
                                    result = Some((
                                        format!("data:image/jpeg;base64,{}", b64),
                                        hash,
                                        visual_hash,
                                    ));
                                    break;
                                }
                            }
                        }
                        result
                    };

                    if let Some((data_url, hash, visual_hash)) = fast_data_url {
                        if hash != monitor_state.last_image_hash {
                            if should_ignore_recent_image_echo(hash, visual_hash) {
                                clear_recent_image_echo_state();
                                handled = true;
                            } else {
                                process_new_entry(
                                    &app,
                                    ClipboardData::Image { data_url },
                                    None,
                                    Some(source_snapshot.clone()),
                                );
                                handled = true;
                            }
                            monitor_state.last_image_hash = hash;
                        } else {
                            handled = true;
                        }
                    }

                    // Slow fallback: read CF_DIB bitmap and encode to PNG
                    if !handled {
                        if let Some(image) = read_clipboard_image_once(&mut cached_image) {
                            use std::hash::{Hash, Hasher};
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            image.bytes.hash(&mut hasher);
                            let hash = hasher.finish();
                            let visual_hash = calc_image_hash_from_rgba(
                                image.width as u32,
                                image.height as u32,
                                &image.bytes,
                            )
                            .unwrap_or(hash as i64)
                                as u64;

                            if hash != monitor_state.last_image_hash {
                                if should_ignore_recent_image_echo(hash, visual_hash) {
                                    clear_recent_image_echo_state();
                                    handled = true;
                                } else {
                                    if let Some(img_buf) = image::RgbaImage::from_raw(
                                        image.width as u32,
                                        image.height as u32,
                                        image.bytes,
                                    ) {
                                        let mut bytes: Vec<u8> = Vec::new();
                                        let mut cursor = std::io::Cursor::new(&mut bytes);
                                        if img_buf
                                            .write_to(&mut cursor, image::ImageFormat::Png)
                                            .is_ok()
                                        {
                                            let b64 = base64::engine::general_purpose::STANDARD
                                                .encode(bytes);
                                            process_new_entry(
                                                &app,
                                                ClipboardData::Image {
                                                    data_url: format!(
                                                        "data:image/png;base64,{}",
                                                        b64
                                                    ),
                                                },
                                                None,
                                                Some(source_snapshot.clone()),
                                            );
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

        // 4. Check Text
        if !handled {
            if let Some(text) = read_clipboard_text_once(&mut clipboard, &mut cached_text) {
                let normalized_text = normalize_clipboard_plain_text(&text);
                if !normalized_text.trim().is_empty() {
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    normalized_text.hash(&mut hasher);
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
                        monitor_state.last_text = normalized_text.clone();
                        return;
                    }

                    if last_app_hash != 0 {
                        crate::LAST_APP_SET_HASH.store(0, Ordering::SeqCst);
                    }
                    monitor_state.last_text = normalized_text.clone();

                    // Update content hash so subsequent events with the same
                    // text content are deduped within the time window.
                    monitor_state.last_content_hash = current_content_hash;
                    monitor_state.last_process_time = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    process_new_entry(
                        &app,
                        ClipboardData::Text(normalized_text),
                        None,
                        Some(source_snapshot.clone()),
                    );
                }
            }
        }
    }));
}

pub use pipeline::{ClipboardData, ClipboardPipeline, PipelineContext};
pub use utils::{
    attach_rich_image_fallback, attach_rich_named_formats, build_clipboard_text_fingerprint,
    build_entry_preview, derive_rich_text_content, extract_animated_image_data_url_from_html,
    extract_animated_image_data_url_from_text, extract_first_image_data_url_from_html,
    parse_cf_html, repair_html_fragment, split_rich_html_and_image_fallback,
    split_rich_html_and_named_formats, truncate_html_for_preview,
};

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
            process_path: Some(
                r"C:\Program Files\Microsoft Office\root\Office16\WINWORD.EXE".to_string(),
            ),
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
