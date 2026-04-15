// Clipboard operations module
use crate::app_state::{PasteQueue, SessionHistory, SettingsState};
use crate::database::{calc_image_hash_from_rgba, DbState};
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use crate::services::clipboard::{
    attach_rich_image_fallback, attach_rich_named_formats,
    capture_preserved_named_formats_from_clipboard, clipboard_image_fallback_data_url,
    extract_animated_image_data_url_from_html, extract_first_image_data_url_from_html,
    parse_cf_html, split_rich_html_and_image_fallback, split_rich_html_and_named_formats,
};
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use regex::Regex;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use tauri::{Emitter, Manager, State};
use urlencoding::decode;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::AttachThreadInput;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow,
};

#[derive(Clone)]
enum ClipboardSnapshot {
    Empty,
    Text { text: String, html: Option<String> },
    Image { data_url: String },
    Files { paths: Vec<String> },
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

    let path_without_drive_prefix =
        if path_raw.starts_with('/') && path_raw.chars().nth(2) == Some(':') {
            &path_raw[1..]
        } else {
            path_raw
        };

    let decoded_path = decode(path_without_drive_prefix)
        .map(|p| p.into_owned())
        .unwrap_or_else(|_| path_without_drive_prefix.to_string());

    if decoded_path.is_empty() {
        return None;
    }

    std::fs::read(decoded_path).ok()
}

fn capture_clipboard_snapshot() -> ClipboardSnapshot {
    #[cfg(target_os = "windows")]
    unsafe {
        if let Some(files) =
            crate::infrastructure::windows_api::win_clipboard::get_clipboard_files()
        {
            if !files.is_empty() {
                return ClipboardSnapshot::Files { paths: files };
            }
        }
    }

    let text = arboard::Clipboard::new()
        .ok()
        .and_then(|mut clipboard| clipboard.get_text().ok())
        .filter(|value| !value.is_empty());

    #[cfg(target_os = "windows")]
    {
        if let Some(text_value) = text.clone() {
            if let Some(html_raw) = unsafe {
                crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(
                    "HTML Format",
                )
            } {
                if let Some(html) =
                    parse_cf_html(&html_raw).filter(|value| !value.trim().is_empty())
                {
                    let html_animated_gif_fallback =
                        extract_animated_image_data_url_from_html(&html);
                    let mut html_to_store = html;

                    if let Some(data_url) =
                        html_animated_gif_fallback.or_else(clipboard_image_fallback_data_url)
                    {
                        html_to_store = attach_rich_image_fallback(&html_to_store, &data_url);
                    }

                    let preserved_named_formats =
                        capture_preserved_named_formats_from_clipboard(None);
                    if !preserved_named_formats.is_empty() {
                        html_to_store =
                            attach_rich_named_formats(&html_to_store, &preserved_named_formats);
                    }

                    return ClipboardSnapshot::Text {
                        text: text_value,
                        html: Some(html_to_store),
                    };
                }
            }
        }

        if let Some(data_url) = clipboard_image_fallback_data_url() {
            return ClipboardSnapshot::Image { data_url };
        }
    }

    if let Some(text_value) = text {
        return ClipboardSnapshot::Text {
            text: text_value,
            html: None,
        };
    }

    ClipboardSnapshot::Empty
}

async fn restore_clipboard_snapshot(snapshot: ClipboardSnapshot) -> AppResult<()> {
    match snapshot {
        ClipboardSnapshot::Empty => {
            #[cfg(target_os = "windows")]
            unsafe {
                crate::infrastructure::windows_api::win_clipboard::clear_clipboard()
                    .map_err(AppError::Internal)?;
            }

            #[cfg(not(target_os = "windows"))]
            {
                let mut clipboard = arboard::Clipboard::new().map_err(AppError::from)?;
                clipboard
                    .set_text(String::new())
                    .map_err(|e| AppError::Internal(format!("Clipboard error: {}", e)))?;
            }

            Ok(())
        }
        ClipboardSnapshot::Text { text, html } => {
            if let Some(html_content) = html {
                prepare_clipboard_payload(&text, "rich_text", Some(&html_content), true).await
            } else {
                prepare_clipboard_payload(&text, "text", None, false).await
            }
        }
        ClipboardSnapshot::Image { data_url } => {
            prepare_clipboard_payload(&data_url, "image", None, false).await
        }
        ClipboardSnapshot::Files { paths } => {
            prepare_clipboard_payload(&paths.join("\n"), "file", None, false).await
        }
    }
}

#[tauri::command]
pub async fn copy_to_clipboard(
    app_handle: tauri::AppHandle,
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    mut content: String,
    content_type: String,
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
    let mut current_type = content_type;
    if id != 0 {
        if id > 0 {
            // Fetch from Database
            if let Ok(Some((full_content, ctype, html))) =
                state.repo.get_entry_content_with_html(id)
            {
                content = full_content;
                html_content = html;
                current_type = ctype;
            }
        } else {
            // Fetch from Session
            let session_items = session.inner().0.lock().unwrap();
            if let Some(item) = session_items.iter().find(|i| i.id == id) {
                content = item.content.clone();
                html_content = item.html_content.clone();
                current_type = item.content_type.clone();
            }
        }
    }

    if current_type == "rich_text" {
        let normalized =
            crate::services::clipboard::derive_rich_text_content(&content, html_content.as_deref());
        if !normalized.trim().is_empty() {
            content = normalized;
        }
    }

    // 1. Handle Window Visibility and Focus
    if paste {
        remember_recent_paste(
            &app_handle,
            &content,
            &current_type,
            html_content.as_deref(),
        );
        handle_window_focus_for_paste(&app_handle).await?;
    }

    // 2. Copy to system clipboard
    prepare_clipboard_payload(
        &content,
        &current_type,
        html_content.as_deref(),
        paste_with_format
            .unwrap_or(current_type == "rich_text" && html_content.as_deref().is_some()),
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
            &current_type,
            move_to_top,
        )
        .await?;
    }

    Ok(())
}

#[tauri::command]
pub async fn paste_text_directly(app_handle: tauri::AppHandle, content: String) -> AppResult<()> {
    if content.is_empty() {
        return Ok(());
    }

    handle_window_focus_for_paste(&app_handle).await?;
    send_paste_keystroke("game_mode", Some(&content), Some("text"));
    hide_window_after_paste(&app_handle).await;
    play_paste_sound_if_enabled(&app_handle);

    Ok(())
}

#[tauri::command]
pub async fn paste_content_transiently(
    app_handle: tauri::AppHandle,
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    mut content: String,
    content_type: String,
    id: i64,
    paste_with_format: Option<bool>,
) -> AppResult<()> {
    let previous_clipboard = capture_clipboard_snapshot();
    let mut html_content: Option<String> = None;

    let mut current_type = content_type;
    if id != 0 {
        if id > 0 {
            if let Ok(Some((full_content, ctype, html))) =
                state.repo.get_entry_content_with_html(id)
            {
                content = full_content;
                html_content = html;
                current_type = ctype;
            }
        } else {
            let session_items = session.inner().0.lock().unwrap();
            if let Some(item) = session_items.iter().find(|i| i.id == id) {
                content = item.content.clone();
                html_content = item.html_content.clone();
                current_type = item.content_type.clone();
            }
        }
    }

    if current_type == "rich_text" {
        let normalized =
            crate::services::clipboard::derive_rich_text_content(&content, html_content.as_deref());
        if !normalized.trim().is_empty() {
            content = normalized;
        }
    }

    remember_recent_paste(
        &app_handle,
        &content,
        &current_type,
        html_content.as_deref(),
    );
    handle_window_focus_for_paste(&app_handle).await?;

    prepare_clipboard_payload(
        &content,
        &current_type,
        html_content.as_deref(),
        paste_with_format
            .unwrap_or(current_type == "rich_text" && html_content.as_deref().is_some()),
    )
    .await?;

    let paste_result = perform_paste_action(
        &app_handle,
        &state,
        id,
        false,
        Some(&content),
        &current_type,
        None,
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let restore_result = restore_clipboard_snapshot(previous_clipboard).await;

    paste_result?;
    restore_result?;

    Ok(())
}

pub async fn paste_history_item_by_index(
    app_handle: tauri::AppHandle,
    index: usize,
) -> AppResult<bool> {
    let history = crate::app::commands::history_cmd::get_clipboard_history(
        app_handle.state::<DbState>(),
        app_handle.state::<SessionHistory>(),
        (index + 1) as i32,
        0,
        None,
    )?;

    let Some(item) = history.get(index).cloned() else {
        return Ok(false);
    };

    // Only allow quick paste for pinned items
    if !item.is_pinned {
        return Ok(false);
    }

    let delete_after_use = {
        let settings = app_handle.state::<SettingsState>();
        settings.delete_after_paste.load(Ordering::Relaxed)
    };

    copy_to_clipboard(
        app_handle.clone(),
        app_handle.state::<DbState>(),
        app_handle.state::<SessionHistory>(),
        item.content,
        item.content_type,
        true,
        item.id,
        delete_after_use,
        Some(false),
        None,
    )
    .await?;

    Ok(true)
}

async fn handle_window_focus_for_paste(app_handle: &tauri::AppHandle) -> AppResult<()> {
    // 1. Only restore focus if our window actually took focus; avoids unnecessary focus flips
    // that can force fullscreen apps into windowed mode.
    if crate::IS_MAIN_WINDOW_FOCUSED.load(Ordering::Relaxed) {
        let _ = restore_focus_before_paste(app_handle).await;
    }

    // 2. Then handle the specific visibility logic based on pinned state
    if crate::WINDOW_PINNED.load(Ordering::Relaxed) {
        // In pinned mode, stay visible but ensure window does NOT have focus
        if let Some(window) = app_handle.get_webview_window("main") {
            // Make sure the window doesn't steal focus back
            let _ = window.set_focusable(false);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    } else {
        // In auto-hide mode, hide the window now
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.hide();
            crate::IS_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
            crate::app::window_manager::release_win_keys();
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    Ok(())
}

async fn restore_focus_before_paste(_app_handle: &tauri::AppHandle) -> AppResult<()> {
    let last_hwnd_val = crate::LAST_ACTIVE_HWND.load(Ordering::Relaxed);
    if last_hwnd_val == 0 {
        return Err(AppError::Internal(
            "No last active window captured".to_string(),
        ));
    }

    {
        let target_hwnd = HWND(last_hwnd_val as _);
        #[cfg(target_os = "windows")]
        unsafe {
            if !IsWindowVisible(target_hwnd).as_bool() {
                return Err(AppError::Internal(
                    "Target window is no longer visible".to_string(),
                ));
            }

            let fg_hwnd = GetForegroundWindow();
            if fg_hwnd.0 != target_hwnd.0 {
                let fg_thread_id = GetWindowThreadProcessId(fg_hwnd, None);
                let target_thread_id = GetWindowThreadProcessId(target_hwnd, None);

                if fg_thread_id != 0 && target_thread_id != 0 && fg_thread_id != target_thread_id {
                    let _ = AttachThreadInput(fg_thread_id, target_thread_id, true);
                    let _ = SetForegroundWindow(target_hwnd);
                    if IsIconic(target_hwnd).as_bool() {
                        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                            target_hwnd,
                            windows::Win32::UI::WindowsAndMessaging::SW_RESTORE,
                        );
                    }
                    let _ = windows::Win32::UI::WindowsAndMessaging::BringWindowToTop(target_hwnd);
                    let _ = AttachThreadInput(fg_thread_id, target_thread_id, false);
                } else {
                    let _ = SetForegroundWindow(target_hwnd);
                    if IsIconic(target_hwnd).as_bool() {
                        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                            target_hwnd,
                            windows::Win32::UI::WindowsAndMessaging::SW_RESTORE,
                        );
                    }
                    let _ = windows::Win32::UI::WindowsAndMessaging::BringWindowToTop(target_hwnd);
                }
            }
        }
    }

    // Settling time for Windows to process focus change msg
    // Increased to 150ms for heavy games/apps
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
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
    let rich_alt_hash = if paste_with_format {
        html_content
            .map(crate::services::clipboard::repair_html_fragment)
            .map(|html| calculate_content_hash(&html).0)
            .unwrap_or(0)
    } else {
        0
    };
    crate::LAST_APP_SET_HASH.store(content_hash, Ordering::SeqCst);
    crate::LAST_APP_SET_HASH_ALT.store(rich_alt_hash, Ordering::SeqCst);
    crate::LAST_APP_SET_IMAGE_VISUAL_HASH.store(0, Ordering::SeqCst);
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

            if !content.starts_with("data:")
                && (content.starts_with('/') || content.contains(":\\"))
            {
                if content_type == "image" {
                    // For image type with local path, read pixels for better compatibility with chat apps
                    let bytes = std::fs::read(content).map_err(AppError::from)?;
                    let (primary_hash, secondary_hash, visual_hash) =
                        copy_image_bytes_to_clipboard(bytes, current_time)?;
                    crate::LAST_APP_SET_HASH.store(primary_hash, Ordering::SeqCst);
                    crate::LAST_APP_SET_HASH_ALT.store(secondary_hash, Ordering::SeqCst);
                    crate::LAST_APP_SET_IMAGE_VISUAL_HASH.store(visual_hash, Ordering::SeqCst);
                } else {
                    unsafe {
                        crate::infrastructure::windows_api::win_clipboard::set_clipboard_files(
                            vec![content.to_string()],
                        )
                        .map_err(AppError::from)?;
                    }
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

                let (primary_hash, secondary_hash, visual_hash) =
                    copy_image_bytes_to_clipboard(bytes, current_time)?;
                crate::LAST_APP_SET_HASH.store(primary_hash, Ordering::SeqCst);
                crate::LAST_APP_SET_HASH_ALT.store(secondary_hash, Ordering::SeqCst);
                crate::LAST_APP_SET_IMAGE_VISUAL_HASH.store(visual_hash, Ordering::SeqCst);
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
                    let (html_without_named_formats, preserved_named_formats) =
                        split_rich_html_and_named_formats(html);
                    let (clean_html, fallback_image_data_url) =
                        split_rich_html_and_image_fallback(&html_without_named_formats);
                    let html_for_paste = if clean_html.trim().is_empty() {
                        html_without_named_formats.as_str()
                    } else {
                        clean_html.as_str()
                    };
                    let cf_html = generate_cf_html(html_for_paste);

                    let rich_image_bytes = fallback_image_data_url
                        .as_deref()
                        .and_then(resolve_rich_image_fallback_bytes)
                        .or_else(|| {
                            extract_first_image_data_url_from_html(html_for_paste)
                                .and_then(|data_url| resolve_rich_image_fallback_bytes(&data_url))
                        });

                    if let Some(bytes) = rich_image_bytes {
                        let (primary_hash, secondary_hash, visual_hash) =
                            copy_image_bytes_to_clipboard(bytes, current_time)?;
                        crate::LAST_APP_SET_HASH.store(primary_hash, Ordering::SeqCst);
                        crate::LAST_APP_SET_HASH_ALT.store(secondary_hash, Ordering::SeqCst);
                        crate::LAST_APP_SET_IMAGE_VISUAL_HASH.store(visual_hash, Ordering::SeqCst);
                        unsafe {
                            crate::infrastructure::windows_api::win_clipboard::append_clipboard_text_and_html(content, &cf_html)
                                    .map_err(AppError::from)?;
                            crate::infrastructure::windows_api::win_clipboard::append_named_clipboard_formats(&preserved_named_formats)
                                    .map_err(AppError::from)?;
                        }
                    } else {
                        unsafe {
                            crate::infrastructure::windows_api::win_clipboard::set_clipboard_text_and_html(content, &cf_html)
                                .map_err(AppError::from)?;
                            crate::infrastructure::windows_api::win_clipboard::append_named_clipboard_formats(&preserved_named_formats)
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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn clear_recent_paste_marker(app_handle: &tauri::AppHandle) {
    let queue_state = app_handle.state::<PasteQueue>();
    let mut queue = queue_state.inner().0.lock().unwrap();
    queue.last_action_was_paste = false;
    queue.last_pasted_content = None;
    queue.last_pasted_fingerprint = None;
    queue.last_paste_timestamp_ms = 0;
}

pub(crate) fn remember_recent_paste(
    app_handle: &tauri::AppHandle,
    content: &str,
    content_type: &str,
    html_content: Option<&str>,
) {
    let fingerprint = crate::services::clipboard::build_clipboard_text_fingerprint(
        content_type,
        content,
        html_content,
    );
    let queue_state = app_handle.state::<PasteQueue>();
    let mut queue = queue_state.inner().0.lock().unwrap();
    queue.last_action_was_paste = true;
    queue.last_pasted_content = Some(content.to_string());
    queue.last_pasted_fingerprint = if fingerprint.is_empty() {
        None
    } else {
        Some(fingerprint)
    };
    queue.last_paste_timestamp_ms = now_ms();
}

fn generate_cf_html(html: &str) -> String {
    static BODY_OPEN_RE: OnceLock<Regex> = OnceLock::new();
    static BODY_CLOSE_RE: OnceLock<Regex> = OnceLock::new();
    static HTML_TAG_RE: OnceLock<Regex> = OnceLock::new();

    let body_open_re = BODY_OPEN_RE.get_or_init(|| Regex::new(r"(?is)<body\b[^>]*>").unwrap());
    let body_close_re = BODY_CLOSE_RE.get_or_init(|| Regex::new(r"(?is)</body\s*>").unwrap());
    let html_tag_re = HTML_TAG_RE.get_or_init(|| Regex::new(r"(?is)<html\b").unwrap());

    let wrap_with_body = |fragment: &str| {
        format!(
            "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n</head>\n<body>\n<!--StartFragment-->{}<!--EndFragment-->\n</body>\n</html>",
            fragment
        )
    };

    let mut html_content = crate::services::clipboard::repair_html_fragment(html);
    let has_html_tag = html_tag_re.is_match(&html_content);
    let has_start = html_content.contains("<!--StartFragment-->");
    let has_end = html_content.contains("<!--EndFragment-->");

    if !has_html_tag {
        html_content = wrap_with_body(&html_content);
    } else if !(has_start && has_end) {
        if let Some(open_match) = body_open_re.find(&html_content) {
            let open_end = open_match.end();

            if !has_end {
                if let Some(close_match) = body_close_re.find(&html_content) {
                    if close_match.start() >= open_end {
                        html_content.insert_str(close_match.start(), "<!--EndFragment-->");
                    } else {
                        html_content.push_str("<!--EndFragment-->");
                    }
                } else {
                    html_content.push_str("<!--EndFragment-->");
                }
            }

            if !has_start {
                html_content.insert_str(open_end, "<!--StartFragment-->");
            }
        } else {
            html_content = wrap_with_body(&html_content);
        }
    }

    if !(html_content.contains("<!--StartFragment-->")
        && html_content.contains("<!--EndFragment-->"))
    {
        html_content = wrap_with_body(&html_content);
    }

    let header_template = "Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";
    let header_len = header_template.len();

    let start_html = header_len;
    let end_html = start_html + html_content.len();
    let start_fragment = start_html
        + html_content.find("<!--StartFragment-->").unwrap_or(0)
        + "<!--StartFragment-->".len();
    let end_fragment = start_html
        + html_content
            .find("<!--EndFragment-->")
            .unwrap_or(html_content.len());

    let header = format!(
        "Version:1.0\r\nStartHTML:{:0>10}\r\nEndHTML:{:0>10}\r\nStartFragment:{:0>10}\r\nEndFragment:{:0>10}\r\n",
        start_html,
        end_html,
        start_fragment,
        end_fragment
    );
    format!("{}{}", header, html_content)
}
fn copy_image_bytes_to_clipboard(bytes: Vec<u8>, current_time: u64) -> AppResult<(u64, u64, u64)> {
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

    let pixel_hash = {
        let mut hasher = DefaultHasher::new();
        raw_bytes.hash(&mut hasher);
        hasher.finish()
    };
    let visual_hash =
        calc_image_hash_from_rgba(width, height, &raw_bytes).unwrap_or(pixel_hash as i64) as u64;

    let gif_hash = if is_gif {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        Some(hasher.finish())
    } else {
        None
    };

    // Prepare PNG data for better compatibility
    let mut png_buf: Vec<u8> = Vec::new();
    let img = image::load_from_memory(&bytes)
        .map_err(|e| AppError::Internal(format!("加载图像失败: {}", e)))?;
    img.write_to(
        &mut std::io::Cursor::new(&mut png_buf),
        image::ImageFormat::Png,
    )
    .map_err(|e| AppError::Internal(format!("编码 PNG 失败: {}", e)))?;

    let png_hash = {
        let mut hasher = DefaultHasher::new();
        png_buf.hash(&mut hasher);
        hasher.finish()
    };

    let gif_temp_path = unsafe {
        crate::infrastructure::windows_api::win_clipboard::set_clipboard_image_with_formats(
            crate::infrastructure::windows_api::win_clipboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: raw_bytes,
            },
            if is_gif { Some(&bytes) } else { None },
            Some(&png_buf),
        )
        .map_err(AppError::from)?
    };

    if let Some(path) = gif_temp_path {
        // Also hash the temp path to prevent echo on CF_HDROP
        let normalized = path.trim().replace("\r\n", "\n");
        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        let path_hash = hasher.finish();
        return Ok((path_hash, gif_hash.unwrap_or(png_hash), visual_hash));
    }

    Ok((gif_hash.unwrap_or(png_hash), pixel_hash, visual_hash))
}

async fn copy_text_with_retry(content: &str) -> AppResult<()> {
    println!("[DEBUG] Copying text to clipboard: {} chars", content.len());
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
    Ok(())
}

async fn perform_paste_action(
    app_handle: &tauri::AppHandle,
    state: &State<'_, DbState>,
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

    // Settling time is now mostly handled in handle_window_focus_for_paste
    // But we add a small extra buffer here to be absolutely sure the focus is solid
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;

    // Verify foreground window is not our window before pasting
    let mut stole_focus = false;
    #[cfg(target_os = "windows")]
    unsafe {
        let foreground = GetForegroundWindow();
        if let Some(window) = app_handle.get_webview_window("main") {
            if let Ok(hwnd_raw) = window.hwnd() {
                if foreground.0 == hwnd_raw.0 {
                    stole_focus = true;
                }
            }
        }
    }

    if stole_focus {
        println!("[WARN] Clipboard window STOLE focus back, attempting one last restore...");
        let _ = restore_focus_before_paste(app_handle).await;
    }

    // Get paste method from settings
    let paste_method = state
        .settings_repo
        .get("app.paste_method")
        .ok()
        .flatten()
        .unwrap_or_else(|| "shift_insert".to_string());

    // Send paste keystroke
    send_paste_keystroke(&paste_method, content, Some(content_type));

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
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.set_focusable(false);
        }
        let _ = restore_focus_before_paste(app_handle).await;
        return;
    }

    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.set_focusable(false);
        let _ = window.hide();
        crate::IS_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
        crate::NAVIGATION_ENABLED.store(false, Ordering::Relaxed); // Disable navigation like hide_window_cmd does
        crate::app::window_manager::release_win_keys();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

pub fn send_paste_keystroke(method: &str, content: Option<&str>, content_type: Option<&str>) {
    println!("[DEBUG] Sending paste keystroke using method: {}", method);
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            MapVirtualKeyW, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC, VK_CONTROL,
            VK_INSERT, VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN, VK_SHIFT, VK_V,
        };

        // 1. Ensure all modifiers are released (including SHIFT, WIN, ALT, CTRL)
        let release_modifiers = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_LWIN,
                        dwFlags: KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_RWIN,
                        dwFlags: KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_MENU,
                        dwFlags: KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_SHIFT,
                        dwFlags: KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        dwFlags: KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            },
        ];
        SendInput(&release_modifiers, std::mem::size_of::<INPUT>() as i32);

        std::thread::sleep(std::time::Duration::from_millis(50));

        let can_type = matches!(content_type, Some("text" | "code" | "url" | "rich_text"));
        let effective_method = if method == "game_mode" && !can_type {
            "ctrl_v"
        } else {
            method
        };

        if effective_method == "ctrl_v" {
            let v_scan = MapVirtualKeyW(VK_V.0 as u32, MAPVK_VK_TO_VSC) as u16;
            let ctrl_scan = MapVirtualKeyW(VK_CONTROL.0 as u32, MAPVK_VK_TO_VSC) as u16;

            let inputs = [
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
                            wScan: ctrl_scan,
                            dwFlags: KEYEVENTF_SCANCODE,
                            ..Default::default()
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
                            wScan: v_scan,
                            dwFlags: KEYEVENTF_SCANCODE,
                            ..Default::default()
                        },
                    },
                },
            ];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            std::thread::sleep(std::time::Duration::from_millis(50));

            let inputs_up = [
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
                            wScan: v_scan,
                            dwFlags: KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
                            wScan: ctrl_scan,
                            dwFlags: KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                },
            ];
            SendInput(&inputs_up, std::mem::size_of::<INPUT>() as i32);
        } else if effective_method == "game_mode" {
            if let Some(text) = content {
                std::thread::sleep(std::time::Duration::from_millis(250));

                let target_hwnd = GetForegroundWindow();
                let target_thread = GetWindowThreadProcessId(target_hwnd, None);
                let current_thread = windows::Win32::System::Threading::GetCurrentThreadId();
                let mut attached = false;

                if target_thread != 0 && target_thread != current_thread {
                    if AttachThreadInput(current_thread, target_thread, true).as_bool() {
                        attached = true;
                    }
                }

                use windows::Win32::UI::Input::Ime::{
                    ImmGetContext, ImmGetConversionStatus, ImmGetOpenStatus, ImmReleaseContext,
                    ImmSetConversionStatus, ImmSetOpenStatus, IME_CMODE_ALPHANUMERIC,
                    IME_CONVERSION_MODE, IME_SENTENCE_MODE, IME_SMODE_NONE,
                };

                let himc = ImmGetContext(target_hwnd);
                let mut ime_open = false;
                let mut ime_conv = IME_CONVERSION_MODE(0);
                let mut ime_sentence = IME_SENTENCE_MODE(0);
                let mut has_himc = false;

                if !himc.0.is_null() {
                    has_himc = true;
                    ime_open = ImmGetOpenStatus(himc).as_bool();
                    let _ =
                        ImmGetConversionStatus(himc, Some(&mut ime_conv), Some(&mut ime_sentence));

                    if ime_open {
                        let _ = ImmSetOpenStatus(himc, false);
                    }
                    let _ = ImmSetConversionStatus(himc, IME_CMODE_ALPHANUMERIC, IME_SMODE_NONE);
                }

                let total_len = text.chars().count();
                let (down_delay_ms, up_delay_ms, check_interval) = if total_len > 800 {
                    (2u64, 2u64, 40usize)
                } else if total_len > 200 {
                    (4u64, 4u64, 30usize)
                } else {
                    (10u64, 10u64, 20usize)
                };

                let mut idx = 0usize;
                for c in text.encode_utf16() {
                    if idx % check_interval == 0 {
                        let current_hwnd = GetForegroundWindow();
                        if current_hwnd.0 != target_hwnd.0 {
                            println!("[WARN] Game mode paste aborted: foreground window changed");
                            break;
                        }
                    }
                    if c == '\r' as u16 {
                        idx += 1;
                        continue;
                    }
                    if c == '\n' as u16 {
                        let enter_scan = MapVirtualKeyW(VK_RETURN.0 as u32, MAPVK_VK_TO_VSC) as u16;
                        let enter_down = INPUT {
                            r#type: INPUT_KEYBOARD,
                            Anonymous: INPUT_0 {
                                ki: KEYBDINPUT {
                                    wVk: VK_RETURN,
                                    wScan: enter_scan,
                                    dwFlags: KEYEVENTF_SCANCODE,
                                    ..Default::default()
                                },
                            },
                        };
                        let enter_up = INPUT {
                            r#type: INPUT_KEYBOARD,
                            Anonymous: INPUT_0 {
                                ki: KEYBDINPUT {
                                    wVk: VK_RETURN,
                                    wScan: enter_scan,
                                    dwFlags: KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP,
                                    ..Default::default()
                                },
                            },
                        };
                        SendInput(&[enter_down], std::mem::size_of::<INPUT>() as i32);
                        std::thread::sleep(std::time::Duration::from_millis(down_delay_ms));
                        SendInput(&[enter_up], std::mem::size_of::<INPUT>() as i32);
                        std::thread::sleep(std::time::Duration::from_millis(up_delay_ms));
                        idx += 1;
                        continue;
                    }
                    let mut input = INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: INPUT_0 {
                            ki: KEYBDINPUT {
                                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
                                wScan: c,
                                dwFlags:
                                    windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(
                                        4,
                                    ), // KEYEVENTF_UNICODE
                                ..Default::default()
                            },
                        },
                    };
                    SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                    std::thread::sleep(std::time::Duration::from_millis(down_delay_ms));
                    input.Anonymous.ki.dwFlags |=
                        windows::Win32::UI::Input::KeyboardAndMouse::KEYEVENTF_KEYUP;
                    SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                    std::thread::sleep(std::time::Duration::from_millis(up_delay_ms));
                    idx += 1;
                }

                if has_himc {
                    let _ = ImmSetConversionStatus(himc, ime_conv, ime_sentence);
                    if ime_open {
                        let _ = ImmSetOpenStatus(himc, true);
                    }
                    let _ = ImmReleaseContext(target_hwnd, himc);
                }

                if attached {
                    let _ = AttachThreadInput(current_thread, target_thread, false);
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let ctrl_scan = MapVirtualKeyW(VK_CONTROL.0 as u32, MAPVK_VK_TO_VSC) as u16;
                let v_scan = MapVirtualKeyW(VK_V.0 as u32, MAPVK_VK_TO_VSC) as u16;

                let mut input = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
                            wScan: ctrl_scan,
                            dwFlags: KEYEVENTF_SCANCODE,
                            ..Default::default()
                        },
                    },
                };

                let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                std::thread::sleep(std::time::Duration::from_millis(80));
                input.Anonymous.ki.wScan = v_scan;
                let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                std::thread::sleep(std::time::Duration::from_millis(120));
                input.Anonymous.ki.dwFlags |= KEYEVENTF_KEYUP;
                let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                std::thread::sleep(std::time::Duration::from_millis(80));
                input.Anonymous.ki.wScan = ctrl_scan;
                input.Anonymous.ki.dwFlags = KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP;
                let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            }
        } else {
            let shift_scan = MapVirtualKeyW(VK_SHIFT.0 as u32, MAPVK_VK_TO_VSC) as u16;
            let insert_scan = MapVirtualKeyW(VK_INSERT.0 as u32, MAPVK_VK_TO_VSC) as u16;

            let shift_down = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_SHIFT,
                        wScan: shift_scan,
                        dwFlags: KEYEVENTF_SCANCODE,
                        ..Default::default()
                    },
                },
            };
            SendInput(&[shift_down], std::mem::size_of::<INPUT>() as i32);
            std::thread::sleep(std::time::Duration::from_millis(10));

            let insert_down = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_INSERT,
                        wScan: insert_scan,
                        dwFlags: KEYEVENTF_EXTENDEDKEY | KEYEVENTF_SCANCODE,
                        ..Default::default()
                    },
                },
            };
            SendInput(&[insert_down], std::mem::size_of::<INPUT>() as i32);
            std::thread::sleep(std::time::Duration::from_millis(10));

            let insert_up = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_INSERT,
                        wScan: insert_scan,
                        dwFlags: KEYEVENTF_KEYUP | KEYEVENTF_EXTENDEDKEY | KEYEVENTF_SCANCODE,
                        ..Default::default()
                    },
                },
            };
            SendInput(&[insert_up], std::mem::size_of::<INPUT>() as i32);
            std::thread::sleep(std::time::Duration::from_millis(10));

            let shift_up = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_SHIFT,
                        wScan: shift_scan,
                        dwFlags: KEYEVENTF_KEYUP | KEYEVENTF_SCANCODE,
                        ..Default::default()
                    },
                },
            };
            SendInput(&[shift_up], std::mem::size_of::<INPUT>() as i32);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to keystroke \"v\" using command down",
            ])
            .spawn()
            .ok();
    }
}

fn handle_post_paste_actions(
    app_handle: &tauri::AppHandle,
    state: &State<'_, DbState>,
    id: i64,
    delete_after_use: bool,
    move_to_top: Option<bool>,
) -> AppResult<()> {
    let mut actual_delete = delete_after_use;
    if actual_delete && id > 0 {
        if let Ok(Some(entry)) = state.repo.get_entry_by_id(id) {
            if entry.is_pinned || !entry.tags.is_empty() {
                actual_delete = false;
            }
        }
    }

    if actual_delete {
        // Handle session items cleanup
        if id < 0 {
            let session = app_handle.state::<SessionHistory>();
            let mut session_items = session.inner().0.lock().unwrap();
            session_items.retain(|item| item.id != id);
        }

        // Cleanup file if needed
        let app_data = app_handle.state::<crate::app_state::AppDataDir>();
        let data_dir = app_data.0.lock().unwrap();

        if state.repo.delete(id, Some(&data_dir)).is_ok() {
            let _ = app_handle.emit("clipboard-removed", id);
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
            let should_promote = state
                .repo
                .get_entry_by_id(id)
                .ok()
                .flatten()
                .map(|entry| !entry.is_pinned)
                .unwrap_or(true);
            if should_promote {
                let _ = state.repo.touch_entry(id, Utc::now().timestamp_millis());
            }
        }
    }

    Ok(())
}

fn play_paste_sound_if_enabled(app_handle: &tauri::AppHandle) {
    let settings = app_handle.state::<SettingsState>();
    if settings.sound_enabled.load(Ordering::Relaxed) {
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
