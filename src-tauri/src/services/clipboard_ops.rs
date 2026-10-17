// Clipboard operations module
use crate::app_state::{PasteQueue, SessionHistory, SettingsState};
use crate::database::DbState;
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use crate::services::clipboard::{
    app_likely_word_processor, attach_rich_image_fallback, attach_rich_named_formats,
    build_clipboard_text_fingerprint, build_exact_paste_html,
    capture_preserved_named_formats_from_clipboard, clipboard_image_fallback_data_url,
    derive_rich_text_content, extract_animated_image_data_url_from_html,
    html_has_renderable_rich_body, normalize_plain_text_for_clipboard_paste, parse_cf_html,
    plain_text_from_tabular_html, plain_text_requires_exact_paste, repair_html_fragment,
    rtf_bytes_from_named_formats, sanitize_tabular_html_for_paste,
    should_attach_rich_image_fallback_on_capture,
    split_rich_html_and_image_fallback, split_rich_html_and_named_formats,
};
use arboard::Clipboard;
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
    GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
};

enum ClipboardSnapshot {
    /// macOS: the whole pasteboard, every item and type (lossless).
    #[cfg(target_os = "macos")]
    Native(crate::infrastructure::macos_api::clipboard::PasteboardSnapshot),
    Empty,
    Text { text: String, html: Option<String> },
    Image { data_url: String },
    Files { paths: Vec<String> },
}

fn capture_clipboard_snapshot() -> ClipboardSnapshot {
    // macOS keeps the raw data of every type, so an image, rich text or files
    // come back exactly (an image used to come back as an empty string, rich
    // text as plain text).
    #[cfg(target_os = "macos")]
    if let Some(snapshot) =
        crate::infrastructure::macos_api::clipboard::snapshot_pasteboard(64 * 1024 * 1024)
    {
        return ClipboardSnapshot::Native(snapshot);
    }

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

    #[cfg(target_os = "macos")]
    {
        if let Some(files) =
            crate::infrastructure::macos_api::clipboard::get_clipboard_files()
        {
            if !files.is_empty() {
                return ClipboardSnapshot::Files { paths: files };
            }
        }
    }

    let text = Clipboard::new()
        .ok()
        .and_then(|mut clipboard| clipboard.get_text().ok())
        .filter(|value| !value.is_empty());

    #[cfg(target_os = "windows")]
    {
        let source =
            crate::infrastructure::windows_api::window_tracker::get_clipboard_source_app_info();
        let clipboard_html = unsafe {
            crate::infrastructure::windows_api::win_clipboard::get_clipboard_raw_format(
                "HTML Format",
            )
        }
        .and_then(|raw| parse_cf_html(&raw))
        .filter(|html| !html.trim().is_empty());

        if let Some(html) = clipboard_html {
            let text_value = text
                .clone()
                .unwrap_or_else(|| derive_rich_text_content("", Some(&html)));
            let mut html_to_store = html;
            if let Some(data_url) = extract_animated_image_data_url_from_html(&html_to_store)
                .or_else(clipboard_image_fallback_data_url)
            {
                html_to_store = attach_rich_image_fallback(&html_to_store, &data_url);
            }
            let formats = capture_preserved_named_formats_from_clipboard(Some(&source));
            if !formats.is_empty() {
                html_to_store = attach_rich_named_formats(&html_to_store, &formats);
            }
            return ClipboardSnapshot::Text {
                text: text_value,
                html: Some(html_to_store),
            };
        }

        if let Some(text_value) = text.clone() {
            let formats = capture_preserved_named_formats_from_clipboard(Some(&source));
            if !formats.is_empty() {
                let escaped = text_value
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                return ClipboardSnapshot::Text {
                    text: text_value,
                    html: Some(attach_rich_named_formats(
                        &format!("<pre>{}</pre>", escaped),
                        &formats,
                    )),
                };
            }
        }

        // RTF-only clipboard owners exist (legacy editors and some Office paths).
        // Keep their registered formats even when no Unicode text or CF_HTML is exposed.
        let formats = capture_preserved_named_formats_from_clipboard(Some(&source));
        if !formats.is_empty() {
            return ClipboardSnapshot::Text {
                text: String::new(),
                html: Some(attach_rich_named_formats("<div></div>", &formats)),
            };
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
        #[cfg(target_os = "macos")]
        ClipboardSnapshot::Native(snapshot) => {
            crate::infrastructure::macos_api::clipboard::restore_pasteboard(&snapshot)
                .map_err(AppError::Internal)
        }
        ClipboardSnapshot::Empty => {
            #[cfg(target_os = "windows")]
            unsafe {
                crate::infrastructure::windows_api::win_clipboard::clear_clipboard()
                    .map_err(AppError::Internal)?;
            }

            #[cfg(not(target_os = "windows"))]
            {
                let mut clipboard = Clipboard::new().map_err(AppError::from)?;
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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn remember_recent_paste(
    app_handle: &tauri::AppHandle,
    content: &str,
    content_type: &str,
    html_content: Option<&str>,
) {
    let fingerprint = build_clipboard_text_fingerprint(content_type, content, html_content);
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

/// The HTML to paste plus the picture the entry was copied with, if any.
/// Office and WPS put a bitmap of the selection next to the HTML, and each
/// target picks the format it can use (WeChat the picture, Word the table).
/// Pasting both does the same as pasting from the source app, instead of
/// guessing per target which one to give.
fn materialize_rich_html_for_paste(
    html: &str,
    fallback_image_payload: Option<&str>,
) -> (String, Option<Vec<u8>>) {
    let html_with_embedded_images = crate::services::clipboard::embed_local_images(html);
    let fallback_bytes = fallback_image_payload.and_then(resolve_rich_image_fallback_bytes);

    if html_has_embedded_data_image(&html_with_embedded_images)
        || html_has_renderable_rich_body(&html_with_embedded_images)
    {
        return (html_with_embedded_images, fallback_bytes);
    }

    // Nothing to show in the HTML itself: put the picture in it too.
    let Some(data_url) = fallback_image_payload.and_then(rich_image_payload_to_data_url) else {
        return (html_with_embedded_images, fallback_bytes);
    };
    (
        append_fallback_image_to_html(&html_with_embedded_images, &data_url),
        fallback_bytes,
    )
}

#[cfg(target_os = "windows")]
/// `keep_existing`: an image was just written for this paste; add text and
/// HTML next to it instead of emptying the clipboard (which threw the image
/// fallback away and left only HTML and text).
fn set_windows_formatted_clipboard(
    plain: &str,
    html_fragment: &str,
    named_formats: &[crate::infrastructure::windows_api::win_clipboard::NamedClipboardFormat],
    keep_existing: bool,
) -> AppResult<()> {
    use crate::infrastructure::windows_api::win_clipboard::set_clipboard_rich;
    use crate::services::clipboard::encode_cf_html;

    let cf_html = encode_cf_html(html_fragment);
    let mut registered_formats = Vec::new();
    let mut standard_formats = Vec::new();
    for format in named_formats {
        if let Some(id) = format
            .name
            .strip_prefix(crate::services::clipboard::WINDOWS_STANDARD_FORMAT_PREFIX)
            .and_then(|value| value.parse::<u32>().ok())
        {
            standard_formats.push((id, format.data.clone()));
        } else {
            registered_formats.push(format.clone());
        }
    }
    unsafe {
        set_clipboard_rich(plain, &cf_html, &registered_formats, &standard_formats, keep_existing)
            .map_err(AppError::Internal)?;
    }
    Ok(())
}

/// Whether a paste keeps the entry's formatting. An explicit choice wins: a
/// click keeps the format, a right-click or the plain text hotkey drops it.
/// Callers with no choice of their own (the sequential paste queue) keep the
/// format of rich text entries too.
pub(crate) fn resolve_paste_with_format(
    content: &str,
    content_type: &str,
    html_content: Option<&str>,
    paste_with_format: Option<bool>,
) -> bool {
    let Some(_html) = html_content else {
        return false;
    };
    paste_with_format
        .unwrap_or_else(|| content_type == "rich_text" && !plain_text_requires_exact_paste(content))
}

/// Simulating Cmd+V needs Accessibility on macOS; elsewhere it always works.
fn paste_permission_missing() -> bool {
    #[cfg(target_os = "macos")]
    return !crate::infrastructure::macos_api::permissions::has_accessibility_permission();
    #[cfg(not(target_os = "macos"))]
    return false;
}

fn require_paste_permission(app: &tauri::AppHandle) -> AppResult<()> {
    if paste_permission_missing() {
        let _ = app.emit("accessibility-permission-required", ());
        return Err(AppError::Validation("accessibility_permission_required".into()));
    }
    Ok(())
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
    // Without paste permission the item is still copied, so the user can press
    // Cmd+V themselves (what the permission prompt tells them); only the
    // window hand-off and the simulated keystroke are skipped.
    let paste_blocked = paste && paste_permission_missing();
    let paste = paste && !paste_blocked;
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

    // 1. Handle Window Visibility and Focus
    if paste {
        handle_window_focus_for_paste(&app_handle).await?;
    }

    // 2. Copy to system clipboard
    prepare_clipboard_payload(
        &content,
        &content_type,
        html_content.as_deref(),
        resolve_paste_with_format(
            &content,
            &content_type,
            html_content.as_deref(),
            paste_with_format,
        ),
    )
    .await?;

    if paste_blocked {
        // Emits the event that opens the permission prompt; the error tells the
        // frontend not to treat this as a completed paste.
        return require_paste_permission(&app_handle);
    }

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

/// Held from writing the clipboard until the paste keystroke has gone out.
/// Without it two quick pastes (sequential-paste presses, fast clicks on
/// history items) interleave: the second write lands before the first Cmd/Ctrl+V,
/// so one item is pasted twice and the other never.
pub static PASTE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tauri::command]
pub async fn paste_text_directly(app_handle: tauri::AppHandle, content: String) -> AppResult<()> {
    if content.is_empty() {
        return Ok(());
    }
    let _paste_guard = PASTE_LOCK.lock().await;

    require_paste_permission(&app_handle)?;
    handle_window_focus_for_paste(&app_handle).await?;
    let paste_method = app_handle
        .try_state::<DbState>()
        .and_then(|db| db.settings_repo.get("app.paste_method").ok().flatten())
        .unwrap_or_else(|| "auto".to_string());
    let paste_method = resolve_paste_method_for_target(&paste_method);
    send_paste_keystroke(&paste_method, Some(&content), Some("text"));
    hide_window_after_paste(&app_handle).await;

    Ok(())
}

/// Copies a secret, the LAN transfer pairing link, marked so that clipboard
/// history tools (TieZ itself, Windows' history, password-manager-aware apps)
/// leave it out.
#[tauri::command]
pub fn copy_private_text(text: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return crate::infrastructure::macos_api::clipboard::set_private_text(&text);

    #[cfg(target_os = "windows")]
    return unsafe { crate::infrastructure::windows_api::win_clipboard::set_clipboard_text_private(&text) };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .map_err(|e| e.to_string());
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
    let _paste_guard = PASTE_LOCK.lock().await;
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
    let _paste_guard = PASTE_LOCK.lock().await;
    require_paste_permission(&app_handle)?;
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
        resolve_paste_with_format(
            &content,
            &current_type,
            html_content.as_deref(),
            paste_with_format,
        ),
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

    // Give the target time to read the clipboard before it is restored: slow
    // (Electron) apps sometimes took longer than 250 ms and pasted the old one.
    tokio::time::sleep(std::time::Duration::from_millis(450)).await;
    let restore_result = restore_clipboard_snapshot(previous_clipboard).await;

    paste_result?;
    restore_result?;

    Ok(())
}

pub async fn paste_history_item_by_index(
    app_handle: tauri::AppHandle,
    index: usize,
) -> AppResult<()> {
    let _paste_guard = PASTE_LOCK.lock().await;
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
            .then_with(|| b.sort_at.cmp(&a.sort_at))
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
        crate::infrastructure::macos_api::window::set_window_focusable(&window, false);
        if !window_pinned {
            let _ = window.set_always_on_top(false);
            #[cfg(target_os = "macos")]
            let _ = crate::infrastructure::macos_api::window::hide_clipboard_panel(app_handle);
            #[cfg(not(target_os = "macos"))]
            let _ = window.hide();
            crate::IS_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
            crate::app::window_manager::release_modifier_keys();
        }
    }

    // 2. Hand focus back to the target app before sending the paste keystroke.
    if window_was_visible || window_pinned {
        #[cfg(target_os = "windows")]
        {
            use crate::global_state::LAST_ACTIVE_HWND;
            use crate::infrastructure::windows_ext::WindowExt;

            let last_hwnd_val = LAST_ACTIVE_HWND.load(Ordering::Relaxed);
            if last_hwnd_val != 0 {
                WindowExt::force_focus_window(HWND(last_hwnd_val as _));
            }
        }

        #[cfg(target_os = "macos")]
        {
            let activated = handoff_macos_focus_for_paste();
            tokio::time::sleep(std::time::Duration::from_millis(if activated {
                60
            } else {
                8
            }))
            .await;
        }

        #[cfg(not(target_os = "macos"))]
        tokio::time::sleep(std::time::Duration::from_millis(if window_pinned {
            80
        } else {
            60
        }))
        .await;
    } else {
        // When TieZ never took focus, only wait for the hide animation and hotkey key-up to settle.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    Ok(())
}

/// Give keyboard ownership back to the paste target without needlessly
/// re-activating its whole application. A non-activating NSPanel can be key
/// while the target remains frontmost; calling `activateWithOptions` in that
/// state reorders/flashes the target application's windows.
#[cfg(target_os = "macos")]
fn handoff_macos_focus_for_paste() -> bool {
    if let Some(app_handle) = crate::global_state::GLOBAL_APP_HANDLE.get() {
        let _ = crate::infrastructure::macos_api::window::resign_clipboard_panel_key(app_handle);
    }

    let prev_pid = crate::global_state::LAST_ACTIVE_APP_PID.load(Ordering::Relaxed);
    if prev_pid != 0 {
        let frontmost = crate::infrastructure::macos_api::window::get_active_app_snapshot();
        if frontmost.pid == prev_pid {
            return false;
        }

        if crate::infrastructure::macos_api::apps::activate_app_by_pid_soft(prev_pid as i32) {
            return true;
        }
        if crate::infrastructure::macos_api::apps::activate_app_by_pid(prev_pid as i32) {
            return true;
        }
    }

    let prev_app = crate::global_state::get_last_active_app_name();
    if prev_app.trim().is_empty() {
        return false;
    }
    crate::infrastructure::macos_api::apps::activate_app_by_name(&prev_app)
}

#[cfg(target_os = "macos")]
fn macos_previous_app_is_frontmost() -> bool {
    let prev_pid = crate::global_state::LAST_ACTIVE_APP_PID.load(Ordering::Relaxed);
    prev_pid != 0
        && crate::infrastructure::macos_api::window::get_active_app_snapshot().pid == prev_pid
}

fn calculate_content_hash(content: &str) -> (u64, u64) {
    let normalized = crate::services::clipboard::normalize_clipboard_line_endings(content);
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
    #[cfg(target_os = "windows")]
    let _own_write = crate::infrastructure::windows_api::win_clipboard::begin_own_write();
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

            let file_paths = content
                .lines()
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            let is_file_path_payload = !content.starts_with("data:")
                && !file_paths.is_empty()
                && file_paths
                    .iter()
                    .all(|path| std::path::Path::new(path).is_absolute());

            if is_file_path_payload {
                if content_type == "image" {
                    // For image type with local path, read pixels for better compatibility with chat apps
                    let image_path = &file_paths[0];
                    let bytes = std::fs::read(image_path).map_err(|_| {
                        AppError::Validation("图片文件已不存在，无法粘贴。".to_string())
                    })?;
                    let (primary_hash, _secondary_hash) =
                        copy_image_bytes_to_clipboard(bytes, current_time, Some(image_path))?;
                    // Keep LAST_APP_SET_HASH as content_hash (path hash)
                    // Store pixel/byte hash in HASH_ALT
                    crate::LAST_APP_SET_HASH_ALT.store(primary_hash, Ordering::SeqCst);
                } else {
                    // Paste the files that still exist. A missing one made
                    // Explorer/Finder abort or error while TieZ said it worked.
                    let total = file_paths.len();
                    let file_paths: Vec<String> = file_paths
                        .into_iter()
                        .filter(|path| std::path::Path::new(path).exists())
                        .collect();
                    if file_paths.is_empty() {
                        return Err(AppError::Validation(
                            "这些文件都已不存在，无法粘贴。".to_string(),
                        ));
                    }
                    if file_paths.len() != total {
                        // What the monitor will read back, so it isn't re-recorded.
                        let (hash, _) = calculate_content_hash(&file_paths.join("\n"));
                        crate::LAST_APP_SET_HASH.store(hash, Ordering::SeqCst);
                    }

                    #[cfg(target_os = "windows")]
                    unsafe {
                        crate::infrastructure::windows_api::win_clipboard::set_clipboard_files(
                            file_paths,
                        )
                        .map_err(AppError::Internal)?;
                    }

                    #[cfg(target_os = "macos")]
                    {
                        crate::infrastructure::macos_api::clipboard::set_clipboard_files(
                            file_paths,
                        )
                        .map_err(AppError::Internal)?;
                    }

                    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
                    {
                        let mut clipboard = arboard::Clipboard::new().map_err(AppError::from)?;
                        clipboard
                            .set_text(content.to_string())
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
                    .map_err(|e| AppError::Internal(format!("无法读取图片内容。\n详情：{}", e)))?;

                let (primary_hash, _secondary_hash) =
                    copy_image_bytes_to_clipboard(bytes, current_time, None)?;
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
                    let (html_without_formats, named_formats) =
                        split_rich_html_and_named_formats(html);
                    let (clean_html, fallback_image_data_url) =
                        split_rich_html_and_image_fallback(&html_without_formats);
                    let base_html = if clean_html.trim().is_empty() {
                        repair_html_fragment(html)
                    } else {
                        repair_html_fragment(&clean_html)
                    };
                    let (final_html, image_bytes) = materialize_rich_html_for_paste(
                        &base_html,
                        fallback_image_data_url.as_deref(),
                    );
                    let mut paste_html = sanitize_tabular_html_for_paste(&final_html);
                    let mut paste_plain = plain_text_from_tabular_html(&paste_html)
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| normalize_plain_text_for_clipboard_paste(content));
                    if plain_text_requires_exact_paste(content) {
                        paste_plain = normalize_plain_text_for_clipboard_paste(content);
                        paste_html =
                            sanitize_tabular_html_for_paste(&build_exact_paste_html(content));
                    }

                    #[cfg(target_os = "macos")]
                    {
                        let paste_target = (
                            crate::global_state::get_last_active_app_name(),
                            None::<String>,
                        );
                        let paste_rtf = if app_likely_word_processor(
                            &paste_target.0,
                            paste_target.1.as_deref(),
                        ) {
                            rtf_bytes_from_named_formats(&named_formats)
                        } else {
                            None
                        };
                        crate::infrastructure::macos_api::clipboard::set_clipboard_text_html_rtf_and_image(
                            &paste_plain,
                            &paste_html,
                            paste_rtf,
                            image_bytes,
                        )
                        .map_err(AppError::Internal)?;
                    }

                    #[cfg(target_os = "windows")]
                    {
                        let wrote_image = image_bytes.is_some();
                        if let Some(bytes) = image_bytes {
                            let hash = copy_rich_paste_image(&bytes)?;
                            crate::LAST_APP_SET_HASH_ALT.store(hash, Ordering::SeqCst);
                        }
                        set_windows_formatted_clipboard(
                            &paste_plain,
                            &paste_html,
                            &named_formats,
                            wrote_image,
                        )?;
                    }

                    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
                    {
                        let _ = image_bytes;
                        let mut clipboard = arboard::Clipboard::new().map_err(AppError::from)?;
                        clipboard
                            .set_html(paste_html, Some(paste_plain))
                            .map_err(AppError::from)?;
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

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn image_extension_for_bytes(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 3 && &bytes[0..3] == b"GIF" {
        "gif"
    } else if bytes.len() >= 8
        && bytes[0] == 0x89
        && &bytes[1..4] == b"PNG"
        && bytes[4] == 0x0D
        && bytes[5] == 0x0A
        && bytes[6] == 0x1A
        && bytes[7] == 0x0A
    {
        "png"
    } else if bytes.len() >= 3 && &bytes[0..3] == b"\xFF\xD8\xFF" {
        "jpg"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && bytes.get(8..12) == Some(b"WEBP") {
        "webp"
    } else {
        "png"
    }
}

/// Writes the picture of a rich entry as image formats only. Unlike a plain
/// image paste there is no file next to it: a copied table is not a file, and
/// chat apps would send one as an attachment.
#[cfg(target_os = "windows")]
fn copy_rich_paste_image(bytes: &[u8]) -> AppResult<u64> {
    let rgba = image::load_from_memory(bytes)
        .map_err(|e| AppError::Internal(format!("无法打开图片。\n详情：{}", e)))?
        .to_rgba8();
    let (width, height) = rgba.dimensions();
    let png = if image_extension_for_bytes(bytes) == "png" {
        bytes.to_vec()
    } else {
        encode_png_from_rgba(&rgba)?
    };
    let raw = rgba.into_raw();
    let mut hasher = DefaultHasher::new();
    raw.hash(&mut hasher);
    let hash = hasher.finish();
    unsafe {
        crate::infrastructure::windows_api::win_clipboard::set_clipboard_image_with_formats(
            crate::infrastructure::windows_api::win_clipboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: raw,
            },
            None,
            Some(&png),
            None,
        )
        .map_err(AppError::Internal)?;
    }
    Ok(hash)
}

#[cfg(target_os = "windows")]
fn write_temp_image_path(bytes: &[u8]) -> AppResult<String> {
    let ext = image_extension_for_bytes(bytes);
    let filename = format!(
        "TieZ_IMG_{}.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        ext
    );
    let path = std::env::temp_dir().join(filename);
    std::fs::write(&path, bytes).map_err(AppError::from)?;
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| AppError::Internal("temp image path is not valid UTF-8".to_string()))
}

#[cfg(target_os = "windows")]
fn resolve_image_paste_path(bytes: &[u8], source_path: Option<&str>) -> AppResult<String> {
    if let Some(path) = source_path.map(str::trim).filter(|value| !value.is_empty()) {
        if std::path::Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }
    write_temp_image_path(bytes)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn encode_png_from_rgba(rgba: &image::RgbaImage) -> AppResult<Vec<u8>> {
    let mut png_buf = Vec::new();
    image::DynamicImage::ImageRgba8(rgba.clone())
        .write_to(
            &mut std::io::Cursor::new(&mut png_buf),
            image::ImageFormat::Png,
        )
        .map_err(|e| AppError::Internal(format!("无法生成图片。\n详情：{}", e)))?;
    Ok(png_buf)
}

/// macOS: write the stored bytes as they are (see set_clipboard_image). Only
/// non-PNG images are decoded, for a static PNG of their first frame. The echo
/// hash is the hash of what the monitor reads back first: the GIF bytes for a
/// GIF, otherwise the PNG (it used to compare a GIF-bytes hash with a hash of
/// decoded pixels, so every GIF paste came back as a new flattened entry).
#[cfg(target_os = "macos")]
fn copy_image_bytes_to_clipboard_macos(
    bytes: &[u8],
    is_gif: bool,
    source_path: Option<&str>,
) -> AppResult<(u64, u64)> {
    let extension = image_extension_for_bytes(bytes);
    let static_png = if extension == "png" {
        None
    } else {
        let first_frame = image::load_from_memory(bytes)
            .map_err(|e| AppError::Internal(format!("无法打开图片。\n详情：{}", e)))?
            .to_rgba8();
        Some(encode_png_from_rgba(&first_frame)?)
    };
    // The file itself lets chat apps and Finder take the animated original.
    let file_path = source_path
        .map(str::trim)
        .filter(|path| is_gif && !path.is_empty() && std::path::Path::new(path).is_file());
    crate::infrastructure::macos_api::clipboard::set_clipboard_image(
        bytes,
        extension,
        static_png.as_deref(),
        file_path,
    )
    .map_err(AppError::Internal)?;

    let echo_bytes: &[u8] = if is_gif { bytes } else { static_png.as_deref().unwrap_or(bytes) };
    let mut hasher = DefaultHasher::new();
    echo_bytes.to_vec().hash(&mut hasher);
    Ok((hasher.finish(), 0))
}

fn copy_image_bytes_to_clipboard(
    bytes: Vec<u8>,
    current_time: u64,
    source_path: Option<&str>,
) -> AppResult<(u64, u64)> {
    // Check if it's a GIF by magic number
    let is_gif = bytes.len() > 3 && &bytes[0..3] == b"GIF";

    #[cfg(target_os = "macos")]
    {
        crate::LAST_APP_SET_TIMESTAMP.store(current_time, Ordering::SeqCst);
        return copy_image_bytes_to_clipboard_macos(&bytes, is_gif, source_path);
    }

    #[allow(unreachable_code)]
    let (width, height, raw_bytes) = {
        let img = image::load_from_memory(&bytes)
            .map_err(|e| AppError::Internal(format!("无法打开图片。\n详情：{}", e)))?
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

    #[cfg(target_os = "windows")]
    {
        let paste_path = resolve_image_paste_path(&bytes, source_path)?;
        let png_data = if is_gif {
            None
        } else if image_extension_for_bytes(&bytes) == "png" {
            Some(bytes.clone())
        } else {
            Some(encode_png_from_rgba(
                &image::RgbaImage::from_raw(width, height, raw_bytes.clone())
                    .ok_or_else(|| AppError::Internal("无法处理图片，请重新复制。".to_string()))?,
            )?)
        };

        unsafe {
            crate::infrastructure::windows_api::win_clipboard::set_clipboard_image_with_formats(
                crate::infrastructure::windows_api::win_clipboard::ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: raw_bytes,
                },
                if is_gif { Some(bytes.as_slice()) } else { None },
                png_data.as_deref(),
                Some(paste_path.as_str()),
            )
            .map_err(AppError::Internal)?;
        }

        return Ok((primary_hash, secondary_hash));
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = source_path;
        let mut clipboard = arboard::Clipboard::new().map_err(AppError::from)?;
        clipboard
            .set_image(arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: raw_bytes.into(),
            })
            .map_err(AppError::from)?;
    }

    Ok((primary_hash, secondary_hash))
}

async fn copy_text_with_retry(content: &str) -> AppResult<()> {
    println!("[DEBUG] Copying text to clipboard: {} chars", content.len());
    // Use the platform line-ending conversion without trimming. In particular,
    // trailing spaces and tabs are part of the clipboard payload and must survive
    // a history-item click exactly as captured.
    let paste_text = normalize_plain_text_for_clipboard_paste(content);

    #[cfg(target_os = "macos")]
    {
        crate::infrastructure::macos_api::clipboard::set_clipboard_text_and_html(&paste_text, "")
            .map_err(AppError::Internal)?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut retries = 3;
        while retries > 0 {
            let res = {
                let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
                clipboard.set_text(paste_text.clone())
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

    Ok(())
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

    // Windows needs a short settle period after its real foreground-window
    // switch. macOS uses a non-activating NSPanel and already waits for one
    // run-loop turn in `handle_window_focus_for_paste`, so another unconditional
    // delay here only makes every click feel sluggish.
    #[cfg(target_os = "windows")]
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;

    #[cfg(target_os = "windows")]
    {
        let stole_focus = app_handle
            .get_webview_window("main")
            .and_then(|window| window.is_focused().ok())
            .unwrap_or(false);
        if stole_focus {
            let pinned = crate::WINDOW_PINNED.load(Ordering::Relaxed);
            if pinned {
                println!("[DEBUG] Pinned panel still reports focus; handing key back...");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            } else {
                println!("[WARN] Clipboard window STOLE focus back, attempting manual hide...");
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.hide();
                    crate::IS_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
                    crate::app::window_manager::release_modifier_keys();
                }
                // Extra settle time for focus handoff before issuing the paste keystroke.
                tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            }
        }
    }

    // This is only a recovery path. The normal macOS path is already frontmost
    // and proceeds immediately after the initial 8 ms responder handoff.
    #[cfg(target_os = "macos")]
    if !macos_previous_app_is_frontmost() {
        if handoff_macos_focus_for_paste() {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }
    }

    // Send paste keystroke
    let paste_method = state
        .settings_repo
        .get("app.paste_method")
        .ok()
        .flatten()
        .unwrap_or_else(|| "auto".to_string());
    let paste_method = resolve_paste_method_for_target(&paste_method);
    send_paste_keystroke(&paste_method, content, Some(content_type));

    // Hide after paste if not pinned
    hide_window_after_paste(app_handle).await;

    // Handle post-paste actions
    handle_post_paste_actions(app_handle, state, id, delete_after_use, move_to_top)?;

    Ok(())
}

async fn hide_window_after_paste(app_handle: &tauri::AppHandle) {
    if crate::WINDOW_PINNED.load(Ordering::Relaxed) {
        // In pinned mode, keep window non-focusable and restore focus back to last app
        if let Some(_window) = app_handle.get_webview_window("main") {
            #[cfg(target_os = "macos")]
            crate::infrastructure::macos_api::window::set_window_focusable(&_window, false);
        }
        // On macOS, focus restoration is implicit after hiding a non-focusable window.
        return;
    }

    if let Some(_window) = app_handle.get_webview_window("main") {
        let _ = app_handle.emit("force-hide-compact-preview", ());
        if let Some(compact_preview) = app_handle.get_webview_window("compact-preview") {
            let _ = compact_preview.hide();
        }
        #[cfg(target_os = "macos")]
        crate::infrastructure::macos_api::window::set_window_focusable(&_window, false);
        #[cfg(target_os = "macos")]
        let _ = crate::infrastructure::macos_api::window::hide_clipboard_panel(app_handle);
        #[cfg(not(target_os = "macos"))]
        let _ = _window.hide();
        crate::IS_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
        crate::NAVIGATION_ENABLED.store(false, Ordering::Relaxed); // Disable navigation like hide_window_cmd does
        crate::app::window_manager::release_modifier_keys();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

pub(crate) fn resolve_paste_method_for_target(method: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        return resolve_windows_paste_method(method);
    }
    #[cfg(not(target_os = "windows"))]
    {
        method.to_string()
    }
}

/// "auto" (the default) picks the key per target app; any other method is
/// sent as chosen. Apps not listed below get Shift+Insert.
#[cfg(target_os = "windows")]
fn resolve_windows_paste_method(method: &str) -> String {
    if method != "auto" {
        return method.to_string();
    }
    let method = "shift_insert";

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return method.to_string();
        }

        let mut class_buf = [0u16; 256];
        let class_len = GetClassNameW(hwnd, &mut class_buf);
        let class_name = if class_len > 0 {
            String::from_utf16_lossy(&class_buf[..class_len as usize])
        } else {
            String::new()
        };

        let app_name = crate::infrastructure::windows_api::window_tracker::get_active_app_info()
            .app_name
            .to_ascii_lowercase();

        // mintty (Git Bash), PuTTY and ConEmu paste with Shift+Insert out of
        // the box but bind Ctrl+Shift+V only when the user turns it on, so
        // they keep Shift+Insert (#112).
        if class_name.contains("ConsoleWindowClass")
            || class_name.contains("CASCADIA_HOSTING_WINDOW_CLASS")
            || app_name.contains("windowsterminal")
        {
            return "ctrl_shift_v".to_string();
        }

        if class_name.contains("Chrome_WidgetWin")
            || app_name.contains("qq")
            || app_name.contains("telegram")
            || app_name.contains("wechat")
            || app_name.contains("weixin")
            || app_name.contains("txplatform")
        {
            return "ctrl_v".to_string();
        }
    }

    method.to_string()
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
        } else if effective_method == "ctrl_shift_v" {
            let v_scan = MapVirtualKeyW(VK_V.0 as u32, MAPVK_VK_TO_VSC) as u16;
            let ctrl_scan = MapVirtualKeyW(VK_CONTROL.0 as u32, MAPVK_VK_TO_VSC) as u16;
            let shift_scan = MapVirtualKeyW(VK_SHIFT.0 as u32, MAPVK_VK_TO_VSC) as u16;

            let inputs_down = [
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_SHIFT,
                            wScan: shift_scan,
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
            SendInput(&inputs_down, std::mem::size_of::<INPUT>() as i32);
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
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_SHIFT,
                            wScan: shift_scan,
                            dwFlags: KEYEVENTF_KEYUP | KEYEVENTF_SCANCODE,
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
        let _method = method;
        let _content = content;
        // Focus was already handed off and allowed to settle before the
        // clipboard payload was prepared. Do not schedule another NSPanel
        // resign here: it can race the Command+V event and adds avoidable work
        // to every paste.

        if !crate::infrastructure::macos_api::permissions::has_accessibility_permission() {
            println!(
                "[WARN] Accessibility permission missing; open Settings → Privacy → Accessibility"
            );
            return;
        }

        crate::infrastructure::macos_api::paste_key_monitor::suppress_paste_sound_briefly();
        if crate::infrastructure::macos_api::permissions::send_command_v() {
            if let Some(app) = crate::global_state::GLOBAL_APP_HANDLE.get() {
                crate::services::ui_sound::schedule_paste_sound(app);
            }
        } else {
            println!("[WARN] Native Command+V dispatch failed");
        }
    }

    #[cfg(target_os = "windows")]
    if let Some(app) = crate::global_state::GLOBAL_APP_HANDLE.get() {
        crate::services::ui_sound::schedule_paste_sound(app);
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

                if state.repo.delete_after_paste(id, Some(&data_dir)).is_ok() {
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
                .map(|v| v == "true")
                .unwrap_or(false),
        };

        if should_move_to_top {
            let _ = state.repo.touch_entry(id, Utc::now().timestamp_millis());
        }
    }

    Ok(())
}

/// A click pastes with the format; this hotkey is the plain text way (#148).
pub fn paste_latest_plain(app_handle: tauri::AppHandle) {
    paste_latest(app_handle, false);
}

fn paste_latest(app_handle: tauri::AppHandle, with_format: bool) {
    let app_handle_clone = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let delete_after = {
            let settings = app_handle_clone.state::<SettingsState>();
            settings.delete_after_paste.load(Ordering::Relaxed)
        };

        let history = crate::app::commands::history_cmd::get_clipboard_history(
            app_handle_clone.state::<DbState>(),
            app_handle_clone.state::<SessionHistory>(),
            app_handle_clone.state::<crate::app_state::AppDataDir>(),
            1,
            0, // offset
            None,
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
                    Some(with_format),
                    None,
                )
                .await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::resolve_paste_with_format;

    const HTML: Option<&str> = Some("<a href=\"https://www.baidu.com\">点击跳转百度</a>");

    #[test]
    fn an_explicit_choice_wins() {
        assert!(!resolve_paste_with_format("点击跳转百度", "rich_text", HTML, Some(false)));
        assert!(resolve_paste_with_format("点击跳转百度", "rich_text", HTML, Some(true)));
    }

    #[test]
    fn no_choice_keeps_the_format_of_rich_text_entries() {
        assert!(resolve_paste_with_format("点击跳转百度", "rich_text", HTML, None));
        assert!(!resolve_paste_with_format(" 点击跳转百度", "rich_text", HTML, None));
        assert!(!resolve_paste_with_format("点击跳转百度", "rich_text", None, Some(true)));
    }
}
