// Clipboard operations module
use crate::app_state::{SettingsState, SessionHistory};
use crate::database::DbState;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::error::{AppResult, AppError};
use chrono::Utc;
use base64::{engine::general_purpose, Engine as _};
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
    GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible, IsIconic,
    SetForegroundWindow,
};

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

    let path_without_drive_prefix = if path_raw.starts_with('/') && path_raw.chars().nth(2) == Some(':') {
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
    println!("[DEBUG] copy_to_clipboard called: id={}, paste={}, content_type={}, content_len={}", id, paste, content_type, content.len());

    let mut html_content: Option<String> = None;

    // 0. Resolve full content if ID is provided and content is placeholder/truncated
    if id != 0 {
        if id > 0 {
            // Fetch from Database
            if let Ok(Some((full_content, _ctype, html))) = state.repo.get_entry_content_with_html(id) {
                content = full_content;
                html_content = html;
            }
        } else {
            // Fetch from Session
            let session_items = session.inner().0.lock().unwrap();
            if let Some(item) = session_items.iter().find(|i| i.id == id) {
                content = item.content.clone();
                html_content = item.html_content.clone();
            }
        }
    }

    // 1. Handle Window Visibility and Focus
    if paste {
        handle_window_focus_for_paste(&app_handle, &content_type).await?;
    }

    // 2. Calculate content hash for deduplication
    let (content_hash, current_time) = calculate_content_hash(&content);
    crate::LAST_APP_SET_HASH.store(content_hash, Ordering::SeqCst);
    crate::LAST_APP_SET_HASH_ALT.store(0, Ordering::SeqCst);
    crate::LAST_APP_SET_TIMESTAMP.store(current_time, Ordering::SeqCst);

    // 3. Copy to system clipboard
    copy_content_to_system_clipboard(
        &content,
        &content_type,
        html_content.as_deref(),
        paste_with_format.unwrap_or(content_type == "rich_text" && html_content.as_deref().is_some()),
        content_hash,
        current_time,
    )
    .await?;

    // 4. Perform paste action if requested
    if paste {
        perform_paste_action(
            &app_handle,
            &state,
            id,
            delete_after_use,
            Some(&content),
            &content_type,
            move_to_top
        ).await?;
    }

    Ok(())
}

async fn handle_window_focus_for_paste(app_handle: &tauri::AppHandle, content_type: &str) -> AppResult<()> {
    #[cfg(target_os = "linux")]
    {
        // On Linux, focus management is unreliable after hiding windows.
        // Strategy: save target window → hide → explicitly activate target.
        // We must NOT rely on IS_MAIN_WINDOW_FOCUSED because clicking in a
        // set_focusable(false) webview does not trigger the Focused(true) event.

        // Step 1: Identify the target window (where paste should go)
        let our_pid = std::process::id().to_string();
        let our_windows: Vec<u64> = std::process::Command::new("xdotool")
            .args(["search", "--pid", &our_pid])
            .output()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| l.trim().parse::<u64>().ok())
                    .collect()
            })
            .unwrap_or_default();

        let mut target_wid: Option<u64> = None;

        // Try getactivewindow (current WM active window)
        if let Ok(output) = std::process::Command::new("xdotool")
            .args(["getactivewindow"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(w) = stdout.parse::<u64>() {
                if !our_windows.contains(&w) {
                    if let Ok(name_out) = std::process::Command::new("xdotool")
                        .args(["getwindowname", &w.to_string()])
                        .output()
                    {
                        let name = String::from_utf8_lossy(&name_out.stdout).trim().to_string();
                        println!("[DEBUG] Linux getactivewindow: {} ({})", w, name);
                    }
                    target_wid = Some(w);
                }
            }
        }

        // Fallback: previously saved window (from continuous tracker or toggle_window)
        if target_wid.is_none() {
            let saved = crate::LAST_ACTIVE_X11_WINDOW.load(Ordering::Relaxed);
            if saved != 0 && !our_windows.contains(&saved) {
                target_wid = Some(saved);
            }
        }

        // For file content, check if tracked window is a file manager.
        // If not, search for a file manager window (including minimized ones).
        if content_type == "file" {
            let is_file_manager = target_wid.map(|wid| {
                std::process::Command::new("xdotool")
                    .args(["getwindowname", &wid.to_string()])
                    .output()
                    .ok()
                    .map(|o| {
                        let name = String::from_utf8_lossy(&o.stdout).to_lowercase();
                        name.contains("nautilus") || name.contains("dolphin")
                            || name.contains("thunar") || name.contains("nemo")
                            || name.contains("pcmanfm") || name.contains("files")
                    })
                    .unwrap_or(false)
            }).unwrap_or(false);

            if !is_file_manager {
                // Search WITHOUT --onlyvisible so we also find minimized file
                // browser windows.  Nautilus desktop windows are filtered out
                // by name blacklist.
                for class in ["org.gnome.Nautilus", "nautilus", "dolphin", "thunar", "nemo", "pcmanfm"] {
                    if let Ok(output) = std::process::Command::new("xdotool")
                        .args(["search", "--class", class])
                        .output()
                    {
                        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        let wids: Vec<u64> = stdout
                            .lines()
                            .filter_map(|l| l.trim().parse::<u64>().ok())
                            .filter(|wid| !our_windows.contains(wid))
                            .collect();

                        if wids.is_empty() { continue; }

                        // Blacklist: GNOME internal helper windows (name matches
                        // WM_CLASS) and Nautilus desktop manager windows.
                        // Do NOT blacklist directory names — a folder named "tmp"
                        // or "desktop" is a valid file browser window.
                        let blacklist = ["org.gnome.nautilus", "nautilus", "x-nautilus-desktop"];
                        let mut visible_candidates: Vec<u64> = Vec::new();
                        let mut minimized_candidates: Vec<u64> = Vec::new();

                        for wid in &wids {
                            let name = std::process::Command::new("xdotool")
                                .args(["getwindowname", &wid.to_string()])
                                .output()
                                .ok()
                                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_lowercase())
                                .unwrap_or_default();

                            if blacklist.iter().any(|b| name == *b) {
                                println!("[DEBUG] Linux skipping helper/desktop window: {} (name={})", wid, name);
                                continue;
                            }

                            // Check if minimized (iconic)
                            let is_minimized = std::process::Command::new("xdotool")
                                .args(["getwindowstate", &wid.to_string()])
                                .output()
                                .ok()
                                .map(|o| String::from_utf8_lossy(&o.stdout).contains("ICONIC"))
                                .unwrap_or(false);

                            println!("[DEBUG] Linux candidate FM window: {} (name={}, minimized={})", wid, name, is_minimized);

                            if is_minimized {
                                minimized_candidates.push(*wid);
                            } else {
                                visible_candidates.push(*wid);
                            }
                        }

                        // Prefer visible windows; fall back to minimized (will be un-minimized by windowactivate)
                        let pool = if !visible_candidates.is_empty() { &visible_candidates } else { &minimized_candidates };
                        if let Some(&wid) = pool.last() {
                            println!("[DEBUG] Linux found file manager window: {} ({})", wid, class);
                            target_wid = Some(wid);
                            break;
                        }
                    }
                }
            }
        }

        if let Some(wid) = target_wid {
            crate::LAST_ACTIVE_X11_WINDOW.store(wid, Ordering::Relaxed);
            if let Ok(output) = std::process::Command::new("xdotool")
                .args(["getwindowname", &wid.to_string()])
                .output()
            {
                let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
                println!("[DEBUG] Linux target paste window: {} ({})", wid, name);
            }
        } else {
            println!("[DEBUG] Linux: no target window found for paste");
        }

        // Step 2: Hide our window (non-pinned only)
        // Pinned mode: window is already set_focusable(false) — xdotool can
        // activate the target window without hiding ours. No flicker.
        if !crate::WINDOW_PINNED.load(Ordering::Relaxed) {
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.hide();
                crate::IS_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
                crate::app::window_manager::release_win_keys();
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }

        // Step 3: Explicitly activate the target window.
        // On GNOME Shell, _NET_ACTIVE_WINDOW and actual keyboard focus can
        // diverge, so we use both WM-level activation and direct X11 focus.
        if let Some(wid) = target_wid {
            println!("[DEBUG] Linux activating window {} for paste", wid);
            // WM-level activation (un-minimizes, raises, sets _NET_ACTIVE_WINDOW)
            let _ = std::process::Command::new("xdotool")
                .args(["windowactivate", "--sync", &wid.to_string()])
                .output();
            // Direct X11 focus — bypasses WM focus management to ensure
            // keyboard input routes to this window
            let _ = std::process::Command::new("xdotool")
                .args(["windowfocus", &wid.to_string()])
                .output();
            // Give GNOME Shell time to settle focus before we send keystroke
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }

        return Ok(());
    }

    // Non-Linux: original behavior
    if crate::IS_MAIN_WINDOW_FOCUSED.load(Ordering::Relaxed) {
        let _ = restore_focus_before_paste(app_handle).await;
    }

    if crate::WINDOW_PINNED.load(Ordering::Relaxed) {
        #[cfg(not(target_os = "linux"))]
        {
            // On Windows, set non-focusable + restore focus via Win32 API
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.set_focusable(false);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
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

#[cfg(target_os = "windows")]
async fn restore_focus_before_paste(_app_handle: &tauri::AppHandle) -> AppResult<()> {
    let last_hwnd_val = crate::LAST_ACTIVE_HWND.load(Ordering::Relaxed);
    if last_hwnd_val == 0 {
        return Err(AppError::Internal("No last active window captured".to_string()));
    }

    {
        let target_hwnd = HWND(last_hwnd_val as _);
        unsafe {
            if !IsWindowVisible(target_hwnd).as_bool() {
                 return Err(AppError::Internal("Target window is no longer visible".to_string()));
            }

            let fg_hwnd = GetForegroundWindow();
            if fg_hwnd.0 != target_hwnd.0 {
                let fg_thread_id = GetWindowThreadProcessId(fg_hwnd, None);
                let target_thread_id = GetWindowThreadProcessId(target_hwnd, None);

                if fg_thread_id != 0 && target_thread_id != 0 && fg_thread_id != target_thread_id {
                    let _ = AttachThreadInput(fg_thread_id, target_thread_id, true);
                    let _ = SetForegroundWindow(target_hwnd);
                    if IsIconic(target_hwnd).as_bool() {
                        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(target_hwnd, windows::Win32::UI::WindowsAndMessaging::SW_RESTORE);
                    }
                    let _ = windows::Win32::UI::WindowsAndMessaging::BringWindowToTop(target_hwnd);
                    let _ = AttachThreadInput(fg_thread_id, target_thread_id, false);
                } else {
                    let _ = SetForegroundWindow(target_hwnd);
                    if IsIconic(target_hwnd).as_bool() {
                        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(target_hwnd, windows::Win32::UI::WindowsAndMessaging::SW_RESTORE);
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

#[cfg(not(target_os = "windows"))]
async fn restore_focus_before_paste(_app_handle: &tauri::AppHandle) -> AppResult<()> {
    #[cfg(target_os = "linux")]
    {
        let last_wid = crate::LAST_ACTIVE_X11_WINDOW.load(Ordering::Relaxed);
        if last_wid != 0 {
            println!("[DEBUG] Linux restoring focus to X11 window: {}", last_wid);
            let _ = std::process::Command::new("xdotool")
                .args(["windowactivate", "--sync", &last_wid.to_string()])
                .output();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
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

            if !content.starts_with("data:") && (content.starts_with('/') || content.contains(":\\"))
            {
                if content_type == "image" {
                    #[cfg(target_os = "linux")]
                    {
                        // On Linux, copy images as file URIs for Nautilus/GNOME file manager compatibility.
                        // File managers expect text/uri-list + x-special/gnome-copied-files, not raw pixels.
                        unsafe {
                            crate::infrastructure::windows_api::win_clipboard::set_clipboard_files(vec![content.to_string()])
                                .map_err(AppError::from)?;
                        }
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        // On Windows/macOS, read pixels for better compatibility with chat apps
                        let bytes = std::fs::read(content).map_err(AppError::from)?;
                        let (primary_hash, _secondary_hash) = copy_image_bytes_to_clipboard(bytes, current_time)?;
                        // Keep LAST_APP_SET_HASH as content_hash (path hash)
                        // Store pixel/byte hash in HASH_ALT
                        crate::LAST_APP_SET_HASH_ALT.store(primary_hash, Ordering::SeqCst);
                    }
                } else {
                    // Content may contain newline-separated paths for multiple files
                    let paths: Vec<String> = content
                        .lines()
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                    if !paths.is_empty() {
                        unsafe {
                            crate::infrastructure::windows_api::win_clipboard::set_clipboard_files(paths)
                                .map_err(AppError::from)?;
                        }
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
                
                let (primary_hash, _secondary_hash) = copy_image_bytes_to_clipboard(bytes, current_time)?;
                // Keep LAST_APP_SET_HASH as content_hash (dataurl hash)
                // Store pixel/byte hash in HASH_ALT
                crate::LAST_APP_SET_HASH_ALT.store(primary_hash, Ordering::SeqCst);
            } else {
                let mut clipboard = arboard::Clipboard::new().map_err(AppError::from)?;
                clipboard.set_text(content.to_string()).map_err(AppError::from)?;
            }
        }
        ct if ct == "rich_text" || (paste_with_format && html_content.is_some()) => {
            if let Some(html) = html_content {
                if paste_with_format {
                    let (clean_html, fallback_image_data_url) = split_rich_html_and_image_fallback(html);
                    let html_for_paste = if clean_html.trim().is_empty() {
                        html
                    } else {
                        clean_html.as_str()
                    };
                    let cf_html = generate_cf_html(html_for_paste);

                    if let Some(payload) = fallback_image_data_url {
                        if let Some(bytes) = resolve_rich_image_fallback_bytes(&payload) {
                            let (primary_hash, _secondary_hash) = copy_image_bytes_to_clipboard(bytes, current_time)?;
                            crate::LAST_APP_SET_HASH_ALT.store(primary_hash, Ordering::SeqCst);
                            unsafe {
                                crate::infrastructure::windows_api::win_clipboard::append_clipboard_text_and_html(content, &cf_html)
                                    .map_err(AppError::from)?;
                            }
                        } else {
                            unsafe {
                                crate::infrastructure::windows_api::win_clipboard::set_clipboard_text_and_html(content, &cf_html)
                                    .map_err(AppError::from)?;
                            }
                        }
                    } else {
                        unsafe {
                            crate::infrastructure::windows_api::win_clipboard::set_clipboard_text_and_html(content, &cf_html)
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

    let mut html_content = html.to_string();
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

    if !(html_content.contains("<!--StartFragment-->") && html_content.contains("<!--EndFragment-->")) {
        html_content = wrap_with_body(&html_content);
    }

    let header_placeholder = format!(
        "Version:0.9\r\nStartHTML:{:0>10}\r\nEndHTML:{:0>10}\r\nStartFragment:{:0>10}\r\nEndFragment:{:0>10}\r\n",
        0,
        0,
        0,
        0
    );
    let start_html = header_placeholder.len();
    let start_fragment = start_html + html_content.find("<!--StartFragment-->").unwrap() + "<!--StartFragment-->".len();
    let end_fragment = start_html + html_content.find("<!--EndFragment-->").unwrap();
    let end_html = start_html + html_content.len();

    let header = format!(
        "Version:0.9\r\nStartHTML:{:0>10}\r\nEndHTML:{:0>10}\r\nStartFragment:{:0>10}\r\nEndFragment:{:0>10}\r\n",
        start_html,
        end_html,
        start_fragment,
        end_fragment
    );
    format!("{}{}", header, html_content)
}
fn copy_image_bytes_to_clipboard(
    bytes: Vec<u8>,
    current_time: u64,
) -> AppResult<(u64, u64)> {
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
            h = h.wrapping_add(raw_bytes[0] as u64)
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

    // Prepare PNG data for better compatibility
    let mut png_buf: Vec<u8> = Vec::new();
    let img = image::load_from_memory(&bytes)
        .map_err(|e| AppError::Internal(format!("加载图像失败: {}", e)))?;
    img.write_to(&mut std::io::Cursor::new(&mut png_buf), image::ImageFormat::Png)
        .map_err(|e| AppError::Internal(format!("编码 PNG 失败: {}", e)))?;

    let gif_temp_path = unsafe {
        crate::infrastructure::windows_api::win_clipboard::set_clipboard_image_with_formats(
            crate::infrastructure::windows_api::win_clipboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: raw_bytes,
            },
            if is_gif { Some(&bytes) } else { None },
            Some(&png_buf),
        ).map_err(AppError::from)?
    };

    if let Some(path) = gif_temp_path {
        // Also hash the temp path to prevent echo on CF_HDROP
        let normalized = path.trim().replace("\r\n", "\n");
        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        let path_hash = hasher.finish();
        crate::LAST_APP_SET_HASH.store(path_hash, Ordering::SeqCst);
    }

    Ok((primary_hash, secondary_hash))
}

async fn copy_text_with_retry(
    content: &str,
) -> AppResult<()> {
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
                println!("[DEBUG] Clipboard set failed, retrying... ({} left)", retries);
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
    println!("[DEBUG] perform_paste_action: pinned={}", crate::WINDOW_PINNED.load(Ordering::Relaxed));
    
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
    let paste_method = state.settings_repo.get("app.paste_method").ok().flatten().unwrap_or_else(|| "ctrl_v".to_string());

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
        // Pinned mode: window was never hidden — nothing to restore.
        // Window stays visible with set_focusable(false) so it doesn't steal focus.
        println!("[DEBUG] Pinned window: no hide/show needed after paste");
        return;
    }

    // Non-pinned (auto-hide) mode: hide the window
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.set_focusable(false);
        let _ = window.hide();
        crate::IS_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
        crate::NAVIGATION_ENABLED.store(false, Ordering::Relaxed);
        crate::app::window_manager::release_win_keys();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

pub fn send_paste_keystroke(method: &str, content: Option<&str>, content_type: Option<&str>) {
    println!("[DEBUG] Sending paste keystroke using method: {}", method);
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_INSERT, VK_CONTROL, VK_V, KEYEVENTF_EXTENDEDKEY,
            MapVirtualKeyW, MAPVK_VK_TO_VSC, KEYEVENTF_SCANCODE, VK_RETURN,
        };

        // 1. Ensure all modifiers are released (including SHIFT, WIN, ALT, CTRL)
        let release_modifiers = [
            INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_LWIN, dwFlags: KEYEVENTF_KEYUP, ..Default::default() } } },
            INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_RWIN, dwFlags: KEYEVENTF_KEYUP, ..Default::default() } } },
            INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_MENU, dwFlags: KEYEVENTF_KEYUP, ..Default::default() } } },
            INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_SHIFT, dwFlags: KEYEVENTF_KEYUP, ..Default::default() } } },
            INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_CONTROL, dwFlags: KEYEVENTF_KEYUP, ..Default::default() } } },
        ];
        SendInput(&release_modifiers, std::mem::size_of::<INPUT>() as i32);
        
        std::thread::sleep(std::time::Duration::from_millis(50));

        let can_type =
            matches!(content_type, Some("text" | "code" | "url" | "rich_text"));
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
                    ImmGetContext, ImmGetOpenStatus, ImmSetOpenStatus, ImmReleaseContext,
                    ImmSetConversionStatus, ImmGetConversionStatus, IME_CMODE_ALPHANUMERIC, IME_SMODE_NONE,
                    IME_CONVERSION_MODE, IME_SENTENCE_MODE
                };
                
                let himc = ImmGetContext(target_hwnd);
                let mut ime_open = false;
                let mut ime_conv = IME_CONVERSION_MODE(0);
                let mut ime_sentence = IME_SENTENCE_MODE(0);
                let mut has_himc = false;

                if !himc.0.is_null() {
                    has_himc = true;
                    ime_open = ImmGetOpenStatus(himc).as_bool();
                    let _ = ImmGetConversionStatus(himc, Some(&mut ime_conv), Some(&mut ime_sentence));

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
                                dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(4), // KEYEVENTF_UNICODE
                                ..Default::default()
                            },
                        },
                    };
                    SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                    std::thread::sleep(std::time::Duration::from_millis(down_delay_ms));
                    input.Anonymous.ki.dwFlags |= windows::Win32::UI::Input::KeyboardAndMouse::KEYEVENTF_KEYUP;
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

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("osascript")
            .args(["-e", "tell application \"System Events\" to keystroke \"v\" using command down"])
            .spawn()
            .ok();
    }

    #[cfg(target_os = "linux")]
    {
        let display = crate::infrastructure::linux_api::detect_display_server();
        let is_wayland = display == crate::infrastructure::linux_api::DisplayServer::Wayland;
        let shift = method == "shift_insert";

        // Debug: show which window has focus before we send the keystroke
        if !is_wayland {
            let _ = std::process::Command::new("xdotool")
                .args(["getactivewindow", "getwindowname"])
                .output()
                .map(|o| {
                    let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    println!("[DEBUG] Active window before paste: {}", name);
                });
        }

        // On Wayland, native tools first (xdotool only reaches XWayland apps, not native Wayland)
        let tools: &[&str] = if is_wayland {
            &["wtype", "ydotool", "xdotool"]
        } else {
            &["xdotool", "ydotool", "wtype"]
        };

        for tool in tools {
            let args: &[&str] = match (*tool, shift) {
                ("xdotool", true)  => &["key", "shift+Insert"],
                ("xdotool", false) => &["key", "ctrl+v"],
                ("ydotool", true)  => &["key", "42:1", "110:1", "110:0", "42:0"],
                ("ydotool", false) => &["key", "29:1", "47:1", "47:0", "29:0"],
                ("wtype", true)    => &["-M", "shift", "-k", "Insert", "-m", "shift"],
                ("wtype", false)   => &["-M", "ctrl", "-k", "v", "-m", "ctrl"],
                _ => continue,
            };
            if std::process::Command::new(*tool).args(args).spawn().is_ok() {
                println!("[DEBUG] Paste keystroke sent via {} ({}, display={:?})", tool, method, display);
                return;
            }
        }
        println!("[WARN] No input simulation tool found (install xdotool for X11 or ydotool/wtype for Wayland)");
    }
}

fn handle_post_paste_actions(
    app_handle: &tauri::AppHandle,
    state: &State<'_, DbState>,
    id: i64,
    delete_after_use: bool,
    move_to_top: Option<bool>,
) -> AppResult<()> {
    if delete_after_use {
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
            0,  // offset
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
                    true,        // paste
                    item.id,
                    delete_after,       // delete_after_use
                    Some(true),  // paste_with_format
                    None,
                ).await;
            }
        }
    });
}
