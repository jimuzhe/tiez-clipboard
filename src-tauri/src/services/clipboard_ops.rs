// Clipboard operations module
use crate::app_state::{SessionHistory, SettingsState};
use crate::database::DbState;
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use regex::Regex;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use tauri::{Emitter, Manager, State};
use urlencoding::decode;
// No Windows-specific imports needed for macOS native implementation

const RICH_IMAGE_FALLBACK_PREFIX: &str = "<!--TIEZ_RICH_IMAGE:";
const RICH_IMAGE_FALLBACK_SUFFIX: &str = "-->";

fn split_rich_html_and_image_fallback(html: &str) -> (String, Option<String>) {
    if let Some(start) = html.rfind(RICH_IMAGE_FALLBACK_PREFIX) {
        let marker_start = start + RICH_IMAGE_FALLBACK_PREFIX.len();
        if let Some(end_rel) = html[marker_start..].find(RICH_IMAGE_FALLBACK_SUFFIX) {
            let marker_end = marker_start + end_rel;
            let mut cleaned = String::with_capacity(html.len());
            cleaned.push_str(&html[..start]);
            cleaned.push_str(&html[marker_end + RICH_IMAGE_FALLBACK_SUFFIX.len()..]);

            let payload = html[marker_start..marker_end].trim();
            if payload.is_empty() {
                return (cleaned.trim().to_string(), None);
            }
            // Accept both data URL fallback and persisted local file path fallback.
            return (cleaned.trim().to_string(), Some(payload.to_string()));
        }
    }
    (html.to_string(), None)
}

fn resolve_rich_image_fallback_bytes(payload: &str) -> Option<Vec<u8>> {
    let value = payload.trim();

    if value.starts_with("data:image/") {
        let b64_data = value.split(',').nth(1)?;
        if b64_data.is_empty() {
            return None;
        }
        return general_purpose::STANDARD.decode(b64_data).ok();
    }

    let path_raw = if value.starts_with("file://") {
        value.trim_start_matches("file://")
    } else {
        value
    };

    let decoded_path = decode(path_raw)
        .map(|p| p.into_owned())
        .unwrap_or_else(|_| path_raw.to_string());

    if decoded_path.is_empty() {
        return None;
    }

    std::fs::read(decoded_path).ok()
}

fn rich_image_mime_from_bytes(bytes: &[u8]) -> &'static str {
    match image::guess_format(bytes).ok() {
        Some(image::ImageFormat::Jpeg) => "image/jpeg",
        Some(image::ImageFormat::Gif) => "image/gif",
        Some(image::ImageFormat::WebP) => "image/webp",
        Some(image::ImageFormat::Bmp) => "image/bmp",
        Some(image::ImageFormat::Png) => "image/png",
        _ => "image/png",
    }
}

fn rich_image_payload_to_data_url(payload: &str) -> Option<String> {
    let value = payload.trim();
    if value.starts_with("data:image/") {
        return Some(value.to_string());
    }

    let bytes = resolve_rich_image_fallback_bytes(value)?;
    let mime = rich_image_mime_from_bytes(&bytes);
    let b64 = general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{};base64,{}", mime, b64))
}

fn html_has_embedded_data_image(html: &str) -> bool {
    static IMG_DATA_RE: OnceLock<Regex> = OnceLock::new();

    IMG_DATA_RE
        .get_or_init(|| {
            Regex::new(r#"(?is)<img\b[^>]*\bsrc\s*=\s*['"]data:image/[^'"]+['"]"#).unwrap()
        })
        .is_match(html)
}

fn append_fallback_image_to_html(html: &str, data_url: &str) -> String {
    let img_tag = format!(r#"<img src="{}" />"#, data_url);
    let trimmed = html.trim();

    if trimmed.is_empty() {
        return format!("<html><body>{}</body></html>", img_tag);
    }

    let lower = html.to_ascii_lowercase();
    if let Some(idx) = lower.rfind("</body>") {
        let mut out = String::with_capacity(html.len() + img_tag.len() + 1);
        out.push_str(&html[..idx]);
        if !html[..idx].ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&img_tag);
        out.push_str(&html[idx..]);
        return out;
    }

    let mut out = String::with_capacity(html.len() + img_tag.len() + 1);
    out.push_str(html.trim_end());
    out.push('\n');
    out.push_str(&img_tag);
    out
}

fn materialize_rich_html_for_paste(
    html: &str,
    fallback_image_payload: Option<&str>,
) -> (String, Option<Vec<u8>>) {
    let html_with_embedded_images = crate::services::clipboard::embed_local_images(html);
    let fallback_bytes = fallback_image_payload.and_then(resolve_rich_image_fallback_bytes);

    if html_has_embedded_data_image(&html_with_embedded_images) {
        return (html_with_embedded_images, fallback_bytes);
    }

    let Some(payload) = fallback_image_payload else {
        return (html_with_embedded_images, fallback_bytes);
    };

    let Some(data_url) = rich_image_payload_to_data_url(payload) else {
        return (html_with_embedded_images, fallback_bytes);
    };

    (
        append_fallback_image_to_html(&html_with_embedded_images, &data_url),
        fallback_bytes,
    )
}

async fn copy_to_clipboard_inner(
    app_handle: tauri::AppHandle,
    state: &DbState,
    session: &SessionHistory,
    mut content: String,
    mut content_type: String,
    paste: bool,
    id: i64,
    delete_after_use: bool,
    paste_with_format: Option<bool>,
    move_to_top: Option<bool>,
) -> AppResult<()> {
    println!(
        "[DEBUG] copy_to_clipboard called: id={}, paste={}, content_type={}, content_len={}",
        id,
        paste,
        content_type,
        content.len()
    );

    let mut html_content: Option<String> = None;

    // 0. Resolve full content if ID is provided and content is placeholder/truncated
    if id != 0 {
        if id > 0 {
            // Fetch from Database
            if let Ok(Some((full_content, ctype, html))) =
                state.repo.get_entry_content_with_html(id)
            {
                content = full_content;
                html_content = html;
                content_type = ctype;
            }
        } else {
            // Fetch from Session
            let session_items = session.0.lock().unwrap();
            if let Some(item) = session_items.iter().find(|i| i.id == id) {
                content = item.content.clone();
                html_content = item.html_content.clone();
                content_type = item.content_type.clone();
            }
        }
    }

    if content_type == "rich_text" {
        let normalized =
            crate::services::clipboard::derive_rich_text_content(&content, html_content.as_deref());
        if !normalized.trim().is_empty() {
            content = normalized;
        }
    }

    // 1. Handle Window Visibility and Focus
    if paste {
        handle_window_focus_for_paste(&app_handle).await?;
    }

    // 2. Copy to system clipboard
    prepare_clipboard_payload(
        &content,
        &content_type,
        html_content.as_deref(),
        paste_with_format
            .unwrap_or(content_type == "rich_text" && html_content.as_deref().is_some()),
    )
    .await?;

    // 3. Perform paste action if requested
    if paste {
        perform_paste_action(
            &app_handle,
            &state,
            id,
            delete_after_use,
            Some(&content),
            &content_type,
            move_to_top,
        )
        .await?;
    }

    Ok(())
}

#[tauri::command]
pub async fn copy_to_clipboard(
    app_handle: tauri::AppHandle,
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    content: String,
    content_type: String,
    paste: bool,
    id: i64,
    delete_after_use: bool,
    paste_with_format: Option<bool>,
    move_to_top: Option<bool>,
) -> AppResult<()> {
    copy_to_clipboard_inner(
        app_handle,
        &state,
        &session,
        content,
        content_type,
        paste,
        id,
        delete_after_use,
        paste_with_format,
        move_to_top,
    )
    .await
}

pub async fn paste_history_item_by_index(
    app_handle: tauri::AppHandle,
    index: usize,
) -> AppResult<()> {
    let db_state = app_handle.state::<DbState>();
    let session = app_handle.state::<SessionHistory>();
    let app_handle_clone = app_handle.clone();

    let mut history = db_state.repo.get_history((index + 1) as i32, 0, None)?;
    {
        let session_items = session.0.lock().unwrap();
        for item in session_items.iter().rev() {
            if !history.iter().any(|h| h.id == item.id && item.id != 0) {
                history.push(item.clone());
            }
        }
    }

    history.sort_by(|a, b| {
        b.is_pinned
            .cmp(&a.is_pinned)
            .then_with(|| b.pinned_order.cmp(&a.pinned_order))
            .then_with(|| b.timestamp.cmp(&a.timestamp))
            .then_with(|| b.id.cmp(&a.id))
    });

    if history.len() > index + 1 {
        history.truncate(index + 1);
    }

    let Some(item) = history.get(index) else {
        return Ok(());
    };

    if !item.is_pinned {
        return Ok(());
    }

    copy_to_clipboard_inner(
        app_handle_clone,
        &db_state,
        &session,
        item.content.clone(),
        item.content_type.clone(),
        true,
        item.id,
        false,
        None,
        None,
    )
    .await
}

async fn handle_window_focus_for_paste(app_handle: &tauri::AppHandle) -> AppResult<()> {
    let window_pinned = crate::WINDOW_PINNED.load(Ordering::Relaxed);
    let mut window_was_visible = false;

    // 1. Make TieZ non-focusable before we hand control back to the target app.
    if let Some(window) = app_handle.get_webview_window("main") {
        window_was_visible = window.is_visible().unwrap_or(false);
        let _ = app_handle.emit("force-hide-compact-preview", ());
        #[cfg(not(target_os = "windows"))]
        let _ = window.set_focusable(false);
        if !window_pinned {
            let _ = window.set_always_on_top(false);
            let _ = window.hide();
            crate::IS_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
            crate::app::window_manager::release_modifier_keys();
        }
    }

    // 2. If the clipboard window was visible for an item click/selection, re-activate the
    // previously focused app before sending Command+V. On macOS a non-focusable window can
    // still cause the source text field to lose first-responder status without becoming
    // the official focused window, so checking visibility is more reliable than focus here.
    if window_was_visible {
        let prev_pid = crate::global_state::LAST_ACTIVE_APP_PID.load(Ordering::Relaxed);
        let mut reactivated = false;
        if prev_pid != 0 {
            reactivated =
                crate::infrastructure::macos_api::apps::activate_app_by_pid(prev_pid as i32);
        }

        if !reactivated {
            let prev_app = crate::global_state::get_last_active_app_name();
            if !prev_app.is_empty() {
                println!("[DEBUG] Reactivating previous app by name: {}", prev_app);
                let _ = crate::infrastructure::macos_api::apps::activate_app_by_name(&prev_app);
            }
        }

        // Give macOS time to complete the focus transfer before we send the paste keystroke.
        // Reduced from 150ms to 60ms as native activation is much faster.
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    } else if window_pinned {
        // Keep a small settle delay for non-activating pinned windows.
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    } else {
        // When TieZ never took focus, only wait for the hide animation and hotkey key-up to settle.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    Ok(())
}

fn calculate_content_hash(content: &str) -> (u64, u64) {
    let normalized = content.trim().replace("\r\n", "\n");
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    let content_hash = hasher.finish();

    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    (content_hash, current_time)
}

pub async fn prepare_clipboard_payload(
    content: &str,
    content_type: &str,
    html_content: Option<&str>,
    paste_with_format: bool,
) -> AppResult<()> {
    let (content_hash, current_time) = calculate_content_hash(content);
    crate::LAST_APP_SET_HASH.store(content_hash, Ordering::SeqCst);
    crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
    crate::LAST_APP_SET_TIMESTAMP.store(current_time, Ordering::SeqCst);

    copy_content_to_system_clipboard(
        content,
        content_type,
        html_content,
        paste_with_format,
        content_hash,
        current_time,
    )
    .await
}

async fn copy_content_to_system_clipboard(
    content: &str,
    content_type: &str,
    html_content: Option<&str>,
    paste_with_format: bool,
    content_hash: u64,
    current_time: u64,
) -> AppResult<()> {
    match content_type {
        "image" | "video" | "file" => {
            if content_hash == 0 {
                crate::LAST_APP_SET_HASH.store(1, Ordering::SeqCst);
            }

            if !content.starts_with("data:") && content.starts_with('/') {
                if content_type == "image" {
                    // For image type with local path, read pixels for better compatibility with chat apps
                    let bytes = std::fs::read(content).map_err(AppError::from)?;
                    let (primary_hash, _secondary_hash) =
                        copy_image_bytes_to_clipboard(bytes, current_time)?;
                    // Keep LAST_APP_SET_HASH as content_hash (path hash)
                    // Store pixel/byte hash in HASH_ALT
                    crate::LAST_APP_SET_HASH_ALT.store(primary_hash, Ordering::SeqCst);
                } else {
                    // For video/file types, macOS clipboard doesn't directly support file paths
                    // as a "file" type. It's usually handled by copying the file itself.
                    // For now, we'll just copy the path as text.
                    let mut clipboard = arboard::Clipboard::new().map_err(AppError::from)?;
                    clipboard
                        .set_text(content.to_string())
                        .map_err(AppError::from)?;
                }
            } else if content_type == "image" {
                let b64_data = if content.starts_with("data:image") {
                    content.split(',').nth(1).unwrap_or(content)
                } else {
                    content
                };

                let bytes = general_purpose::STANDARD
                    .decode(b64_data)
                    .map_err(|e| AppError::Internal(format!("Base64 解码失败: {}", e)))?;

                let (primary_hash, _secondary_hash) =
                    copy_image_bytes_to_clipboard(bytes, current_time)?;
                // Keep LAST_APP_SET_HASH as content_hash (dataurl hash)
                // Store pixel/byte hash in HASH_ALT
                crate::LAST_APP_SET_HASH_ALT.store(primary_hash, Ordering::SeqCst);
            } else {
                let mut clipboard = arboard::Clipboard::new().map_err(AppError::from)?;
                clipboard
                    .set_text(content.to_string())
                    .map_err(AppError::from)?;
            }
        }
        ct if ct == "rich_text" || (paste_with_format && html_content.is_some()) => {
            if let Some(html) = html_content {
                if paste_with_format {
                    let (clean_html, fallback_image_data_url) =
                        split_rich_html_and_image_fallback(html);
                    let base_html = if clean_html.trim().is_empty() {
                        html.to_string()
                    } else {
                        clean_html
                    };
                    let (final_html, image_bytes) = materialize_rich_html_for_paste(
                        &base_html,
                        fallback_image_data_url.as_deref(),
                    );

                    #[cfg(target_os = "macos")]
                    {
                        crate::infrastructure::macos_api::clipboard::set_clipboard_text_html_and_image(
                            content,
                            &final_html,
                            image_bytes
                        ).map_err(AppError::Internal)?;
                    }

                    #[cfg(not(target_os = "macos"))]
                    {
                        if let Some(bytes) = image_bytes {
                            let (primary_hash, _secondary_hash) =
                                copy_image_bytes_to_clipboard(bytes, current_time)?;
                            crate::LAST_APP_SET_HASH_ALT.store(primary_hash, Ordering::SeqCst);
                        } else {
                            let mut clipboard =
                                arboard::Clipboard::new().map_err(AppError::from)?;
                            clipboard
                                .set_html(final_html, Some(content.to_string()))
                                .map_err(AppError::from)?;
                        }
                    }
                } else {
                    copy_text_with_retry(content).await?;
                }
            } else {
                copy_text_with_retry(content).await?;
            }
        }
        _ => {
            copy_text_with_retry(content).await?;
        }
    }

    Ok(())
}

fn copy_image_bytes_to_clipboard(bytes: Vec<u8>, current_time: u64) -> AppResult<(u64, u64)> {
    // Check if it's a GIF by magic number
    let is_gif = bytes.len() > 3 && &bytes[0..3] == b"GIF";

    let (width, height, raw_bytes) = {
        let img = image::load_from_memory(&bytes)
            .map_err(|e| AppError::Internal(format!("加载图像失败: {}", e)))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        (w, h, img.into_raw())
    };

    crate::LAST_APP_SET_TIMESTAMP.store(current_time, Ordering::SeqCst);

    let (primary_hash, secondary_hash) = if is_gif {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let byte_hash = hasher.finish();

        // Calculate pixel hash of the first frame as a secondary fingerprint
        let pixel_count = (width as u64) * (height as u64);
        let mut h = pixel_count;
        if !raw_bytes.is_empty() {
            h = h
                .wrapping_add(raw_bytes[0] as u64)
                .wrapping_add(raw_bytes[raw_bytes.len() / 2] as u64)
                .wrapping_add(raw_bytes[raw_bytes.len() - 1] as u64);
        }
        (byte_hash, h)
    } else {
        // Hash full pixel bytes so the monitor can skip our own image copy
        let mut hasher = DefaultHasher::new();
        raw_bytes.hash(&mut hasher);
        let byte_hash = hasher.finish();
        (byte_hash, 0)
    };

    let mut clipboard = arboard::Clipboard::new().map_err(AppError::from)?;
    clipboard
        .set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: raw_bytes.into(),
        })
        .map_err(AppError::from)?;

    // On macOS, there's no direct equivalent to CF_HDROP for images copied this way.
    // The image is copied as pixel data.
    // The LAST_APP_SET_HASH will be the content_hash (e.g., data URL hash or file path hash).
    // The LAST_APP_SET_HASH_ALT will be the pixel hash.

    Ok((primary_hash, secondary_hash))
}

async fn copy_text_with_retry(content: &str) -> AppResult<()> {
    println!("[DEBUG] Copying text to clipboard: {} chars", content.len());

    #[cfg(target_os = "macos")]
    {
        crate::infrastructure::macos_api::clipboard::set_clipboard_text_and_html(content, "")
            .map_err(AppError::Internal)?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut retries = 3;
        while retries > 0 {
            let res = {
                let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
                clipboard.set_text(content.to_string())
            };

            match res {
                Ok(_) => {
                    println!("[DEBUG] Text copied to clipboard successfully");
                    return Ok(());
                }
                Err(_e) if retries > 1 => {
                    retries -= 1;
                    println!(
                        "[DEBUG] Clipboard set failed, retrying... ({} left)",
                        retries
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(e) => return Err(AppError::Internal(format!("Clipboard error: {}", e))),
            }
        }
    }
}

async fn perform_paste_action(
    app_handle: &tauri::AppHandle,
    state: &DbState,
    id: i64,
    delete_after_use: bool,
    content: Option<&str>,
    content_type: &str,
    move_to_top: Option<bool>,
) -> AppResult<()> {
    println!(
        "[DEBUG] perform_paste_action: pinned={}",
        crate::WINDOW_PINNED.load(Ordering::Relaxed)
    );

    // Give macOS enough time to switch focus back to the previously active app
    // after the clipboard window is hidden.
    // Reduced from 90ms to 40ms.
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;

    // Verify foreground window is not our window before pasting
    // Focus management for macOS
    let mut stole_focus = false;
    if let Some(window) = app_handle.get_webview_window("main") {
        if window.is_focused().unwrap_or(false) {
            stole_focus = true;
        }
    }

    if stole_focus {
        let pinned = crate::WINDOW_PINNED.load(Ordering::Relaxed);
        if pinned {
            println!("[WARN] Pinned window still focused, attempting manual reactivation...");
            // Try to give focus away again using native API if we have a PID
            let prev_pid = crate::global_state::LAST_ACTIVE_APP_PID.load(Ordering::Relaxed);
            if prev_pid != 0 {
                crate::infrastructure::macos_api::apps::activate_app_by_pid(prev_pid as i32);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        } else {
            println!("[WARN] Clipboard window STOLE focus back, attempting manual hide...");
            if let Some(window) = app_handle.get_webview_window("main") {
                #[cfg(not(target_os = "windows"))]
                let _ = window.set_focusable(false);
                let _ = window.hide();
                crate::IS_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
                crate::app::window_manager::release_modifier_keys();
            }
            // Extra settle time for focus handoff before issuing the paste keystroke.
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        }
    }

    // Send paste keystroke
    send_paste_keystroke(content, Some(content_type));

    // Hide after paste if not pinned
    hide_window_after_paste(app_handle).await;

    // Handle post-paste actions
    handle_post_paste_actions(app_handle, state, id, delete_after_use, move_to_top)?;

    // Play sound if enabled
    play_paste_sound_if_enabled(app_handle);

    Ok(())
}

async fn hide_window_after_paste(app_handle: &tauri::AppHandle) {
    if crate::WINDOW_PINNED.load(Ordering::Relaxed) {
        // In pinned mode, keep window non-focusable and restore focus back to last app
        if let Some(_window) = app_handle.get_webview_window("main") {
            #[cfg(target_os = "windows")]
            let _ = _window.set_focusable(false);
        }
        // On macOS, focus restoration is implicit after hiding a non-focusable window.
        return;
    }

    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = app_handle.emit("force-hide-compact-preview", ());
        if let Some(compact_preview) = app_handle.get_webview_window("compact-preview") {
            let _ = compact_preview.hide();
        }
        #[cfg(target_os = "windows")]
        let _ = window.set_focusable(false);
        let _ = window.hide();
        crate::IS_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
        crate::NAVIGATION_ENABLED.store(false, Ordering::Relaxed); // Disable navigation like hide_window_cmd does
        crate::app::window_manager::release_modifier_keys();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

pub fn send_paste_keystroke(_content: Option<&str>, _content_type: Option<&str>) {
    println!("[DEBUG] Sending paste keystroke (Command+V) on macOS");
    // Use AppleScript only for app re-activation. Sending synthetic key presses
    // via `osascript`/`System Events` is much less reliable under TCC and may
    // fail even when the main app itself has Accessibility permission.
    //
    // For the actual paste keystroke we prefer our native CGEvent-based path.
    let prev_pid = crate::global_state::LAST_ACTIVE_APP_PID.load(Ordering::Relaxed);
    let mut reactivated = false;
    if prev_pid != 0 {
        reactivated = crate::infrastructure::macos_api::apps::activate_app_by_pid(prev_pid as i32);
    }

    if !reactivated {
        let prev_app = crate::global_state::get_last_active_app_name();
        if !prev_app.trim().is_empty() {
            crate::infrastructure::macos_api::apps::activate_app_by_name(&prev_app);
        }
    }

    if !crate::infrastructure::macos_api::permissions::has_accessibility_permission() {
        println!("[WARN] Accessibility permission missing; requesting permission prompt");
        let _ = crate::infrastructure::macos_api::permissions::request_accessibility_permission();
        return;
    }

    if !crate::infrastructure::macos_api::permissions::send_command_v() {
        println!("[WARN] Native Command+V dispatch failed");
    }
}

fn handle_post_paste_actions(
    app_handle: &tauri::AppHandle,
    state: &DbState,
    id: i64,
    delete_after_use: bool,
    move_to_top: Option<bool>,
) -> AppResult<()> {
    if delete_after_use {
        // Fetch metadata to check for pinned or tagging protection
        let is_protected = if id != 0 {
            if id > 0 {
                state
                    .repo
                    .get_entry_by_id(id)
                    .map(|e| e.is_some_and(|i| i.is_pinned || !i.tags.is_empty()))
                    .unwrap_or(false)
            } else {
                let session = app_handle.state::<crate::app_state::SessionHistory>();
                let s = session.0.lock().unwrap();
                s.iter()
                    .find(|i| i.id == id)
                    .map(|i| i.is_pinned || !i.tags.is_empty())
                    .unwrap_or(false)
            }
        } else {
            false
        };

        if !is_protected {
            // First remove from SessionHistory
            let session = app_handle.state::<crate::app_state::SessionHistory>();
            {
                let mut s = session.0.lock().unwrap();
                if let Some(pos) = s.iter().position(|i| i.id == id) {
                    s.remove(pos);
                }
            }

            if id > 0 {
                // Cleanup persistent file and DB entry if needed
                let app_data = app_handle.state::<crate::app_state::AppDataDir>();
                let data_dir = app_data.0.lock().unwrap();

                if state.repo.delete(id, Some(&data_dir)).is_ok() {
                    let _ = app_handle.emit("clipboard-removed", id);
                }
            } else {
                let _ = app_handle.emit("clipboard-removed", id);
            }
        }
    } else if id > 0 {
        let _ = state.repo.increment_use_count(id);

        let should_move_to_top = match move_to_top {
            Some(val) => val,
            None => state
                .settings_repo
                .get("app.move_to_top_after_paste")
                .ok()
                .flatten()
                .map(|v| v != "false")
                .unwrap_or(true),
        };

        if should_move_to_top {
            let _ = state.repo.touch_entry(id, Utc::now().timestamp_millis());
        }
    }

    Ok(())
}

fn play_paste_sound_if_enabled(app_handle: &tauri::AppHandle) {
    let settings = app_handle.state::<SettingsState>();
    if settings.sound_enabled.load(Ordering::Relaxed)
        && settings.paste_sound_enabled.load(Ordering::Relaxed)
    {
        let _ = app_handle.emit("play-sound", "paste");
    }
}

#[tauri::command]
pub fn paste_latest_rich(app_handle: tauri::AppHandle) {
    let app_handle_clone = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let delete_after = {
            let settings = app_handle_clone.state::<SettingsState>();
            settings.delete_after_paste.load(Ordering::Relaxed)
        };

        let history = crate::app::commands::history_cmd::get_clipboard_history(
            app_handle_clone.state::<DbState>(),
            app_handle_clone.state::<SessionHistory>(),
            1,
            0, // offset
            None,
        );

        if let Ok(items) = history {
            if let Some(item) = items.first() {
                let _ = copy_to_clipboard(
                    app_handle_clone.clone(),
                    app_handle_clone.state::<DbState>(),
                    app_handle_clone.state::<SessionHistory>(),
                    item.content.clone(),
                    item.content_type.clone(),
                    true, // paste
                    item.id,
                    delete_after, // delete_after_use
                    Some(true),   // paste_with_format
                    None,
                )
                .await;
            }
        }
    });
}
