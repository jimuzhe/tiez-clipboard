use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

#[derive(Clone)]
pub struct ImageData {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}

static SEQ: AtomicU32 = AtomicU32::new(1);

// Use a mutex to prevent concurrent clipboard access issues on Linux
static CLIPBOARD_MUTEX: Mutex<()> = Mutex::new(());

pub fn get_clipboard_sequence_number() -> u32 {
    SEQ.fetch_add(1, Ordering::Relaxed)
}

pub unsafe fn get_clipboard_image() -> Option<ImageData> {
    let _lock = CLIPBOARD_MUTEX.lock().ok()?;

    // First try arboard (handles most cases)
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(image) = clipboard.get_image() {
            return Some(ImageData {
                width: image.width,
                height: image.height,
                bytes: image.bytes.to_vec(),
            });
        }
    }

    // Try x11-clipboard for browser image formats (image/png, image/jpeg, etc.)
    if let Ok(clipboard) = x11_clipboard::Clipboard::new() {
        // Try common image MIME types that browsers use
        for mime_type in ["image/png", "image/jpeg", "image/webp", "image/bmp", "image/gif"] {
            if let Ok(atom) = clipboard.getter.get_atom(mime_type) {
                if let Ok(data) = clipboard.load(
                    clipboard.getter.atoms.clipboard,
                    atom,
                    clipboard.getter.atoms.property,
                    std::time::Duration::from_millis(200),
                ) {
                    if !data.is_empty() {
                        // Try to decode the image
                        if let Ok(img) = image::load_from_memory(&data) {
                            let img = img.to_rgba8();
                            let (width, height) = img.dimensions();
                            return Some(ImageData {
                                width: width as usize,
                                height: height as usize,
                                bytes: img.into_raw(),
                            });
                        }
                    }
                }
            }
        }
    }

    None
}

/// Decode a file:// URI path, handling URL encoding and preserving the original if decoding fails
fn decode_file_uri(uri: &str) -> String {
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let path = path.trim(); // Remove trailing \r\n

    // Try to URL-decode, but keep original if it fails
    match urlencoding::decode(path) {
        Ok(decoded) => {
            decoded.to_string()
        }
        Err(_) => {
            println!("[WARN] Failed to decode URI path: {:?}", path);
            path.to_string()
        }
    }
}

pub unsafe fn get_clipboard_files() -> Option<Vec<String>> {
    let _lock = CLIPBOARD_MUTEX.lock().ok()?;

    // Try x11-clipboard first for text/uri-list (proper file format)
    if let Ok(clipboard) = x11_clipboard::Clipboard::new() {
        // Get the text/uri-list atom
        if let Ok(uri_list_atom) = clipboard.getter.get_atom("text/uri-list") {
            let result = clipboard.load(
                clipboard.getter.atoms.clipboard,
                uri_list_atom,
                clipboard.getter.atoms.property,
                std::time::Duration::from_millis(500),
            );

            if let Ok(data) = result {
                let uri_list = String::from_utf8_lossy(&data);

                let files: Vec<String> = uri_list
                    .lines()
                    .filter(|line| line.starts_with("file://"))
                    .map(|line| decode_file_uri(line))
                    .filter(|path| std::path::Path::new(path).exists())
                    .collect();

                if !files.is_empty() {
                    return Some(files);
                }
            }
        }
    }

    // Try x-special/gnome-copied-files (Nautilus desktop format)
    // When copying from GNOME Desktop, Nautilus may only write this format.
    // Format: "copy\nfile:///path1\nfile:///path2\n" or "cut\n..."
    if let Ok(clipboard) = x11_clipboard::Clipboard::new() {
        if let Ok(gnome_atom) = clipboard.getter.get_atom("x-special/gnome-copied-files") {
            let result = clipboard.load(
                clipboard.getter.atoms.clipboard,
                gnome_atom,
                clipboard.getter.atoms.property,
                std::time::Duration::from_millis(500),
            );

            if let Ok(data) = result {
                let raw = String::from_utf8_lossy(&data);

                let lines: Vec<&str> = raw.lines().collect();
                if lines.len() > 1 {
                    let _action = lines[0].trim(); // "copy" or "cut" — logged for future use
                    let files: Vec<String> = lines[1..]
                        .iter()
                        .filter(|line| line.starts_with("file://"))
                        .map(|line| decode_file_uri(line))
                        .filter(|path| std::path::Path::new(path).exists())
                        .collect();

                    if !files.is_empty() {
                        return Some(files);
                    }
                }
            }
        }
    }

    // Fallback to arboard for plain text with file:// URIs
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let text = clipboard.get_text().ok()?;

    let lines: Vec<String> = text
        .lines()
        .filter(|line| line.starts_with("file://"))
        .map(|line| decode_file_uri(line))
        .filter(|path| std::path::Path::new(path).exists())
        .collect();

    if lines.is_empty() {
        None
    } else {
        Some(lines)
    }
}

pub unsafe fn get_clipboard_raw_format(_name: &str) -> Option<Vec<u8>> {
    None
}

/// Convert a file path to a file:// URI with percent-encoding for special characters.
/// Encodes each byte of the UTF-8 representation for non-ASCII chars (e.g. '中' → %E4%B8%AD).
fn path_to_file_uri(path: &str) -> String {
    let encoded: String = path
        .as_bytes()
        .iter()
        .map(|&b| {
            if b.is_ascii_alphanumeric() || b == b'/' || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
                (b as char).to_string()
            } else {
                format!("%{:02X}", b)
            }
        })
        .collect();
    format!("file://{}", encoded)
}

/// Set clipboard files using x11rb with multi-target support (text/uri-list + x-special/gnome-copied-files).
/// Synchronously acquires X11 CLIPBOARD ownership and verifies it before returning.
/// Spawns a background thread that serves SelectionRequest events until another app takes ownership.
fn try_x11_multi_clipboard(file_uris: &[String]) -> bool {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::*;
    use x11rb::protocol::Event;

    let uri_list_bytes: Vec<u8> = file_uris.iter()
        .map(|uri| format!("{}\r\n", uri))
        .collect::<String>()
        .into_bytes();
    let gnome_bytes: Vec<u8> = format!("copy\n{}", file_uris.join("\n")).into_bytes();
    let text_bytes: Vec<u8> = file_uris.join("\n").into_bytes();

    let (conn, screen_num) = match x11rb::connect(None) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let screen = &conn.setup().roots[screen_num];

    let win = match conn.generate_id() {
        Ok(id) => id,
        Err(_) => return false,
    };

    if conn.create_window(
        0, win, screen.root, -10, -10, 1, 1, 0,
        WindowClass::COPY_FROM_PARENT,
        screen.root_visual,
        &CreateWindowAux::new().override_redirect(1),
    ).is_err() {
        return false;
    }

    let intern = |name: &[u8]| -> Option<Atom> {
        conn.intern_atom(false, name).ok()?.reply().ok().map(|r| r.atom)
    };
    let clipboard_atom = match intern(b"CLIPBOARD") {
        Some(a) => a,
        None => return false,
    };
    let targets_atom = match intern(b"TARGETS") {
        Some(a) => a,
        None => return false,
    };
    let uri_list_atom = match intern(b"text/uri-list") {
        Some(a) => a,
        None => return false,
    };
    let gnome_atom = match intern(b"x-special/gnome-copied-files") {
        Some(a) => a,
        None => return false,
    };
    let timestamp_atom = match intern(b"TIMESTAMP") {
        Some(a) => a,
        None => return false,
    };
    let atom_atom: Atom = Atom::from(AtomEnum::ATOM);
    let string_atom: Atom = Atom::from(AtomEnum::STRING);
    let utf8_string_atom = intern(b"UTF8_STRING");
    let text_plain_atom = intern(b"text/plain");

    if conn.set_selection_owner(win, clipboard_atom, 0u32).is_err() {
        return false;
    }
    if conn.flush().is_err() {
        return false;
    }

    // Verify ownership — this is what makes it truly synchronous
    match conn.get_selection_owner(clipboard_atom).ok().and_then(|c| c.reply().ok()) {
        Some(reply) if reply.owner == win => {}
        _ => return false,
    }

    std::thread::spawn(move || {
        let timeout = std::time::Duration::from_secs(60);
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                break;
            }
            match conn.poll_for_event() {
                Ok(Some(event)) => {
                    let rt = event.response_type() & 0x7f;
                    match rt {
                        30 => { // SelectionRequest
                            if let Event::SelectionRequest(req) = event {
                                let property = if req.property == 0 { req.target } else { req.property };

                                if req.target == targets_atom {
                                    let supported = [targets_atom, timestamp_atom, uri_list_atom, gnome_atom,
                                        utf8_string_atom.unwrap_or(targets_atom), text_plain_atom.unwrap_or(targets_atom), string_atom];
                                    let data: Vec<u8> = supported.iter().flat_map(|a| a.to_ne_bytes()).collect();
                                    let _ = conn.change_property(PropMode::REPLACE, req.requestor, property,
                                        atom_atom, 32, supported.len() as u32, &data);
                                } else if req.target == timestamp_atom {
                                    let _ = conn.change_property(PropMode::REPLACE, req.requestor, property,
                                        Atom::from(AtomEnum::INTEGER), 32, 1, &0u32.to_ne_bytes());
                                } else if req.target == uri_list_atom {
                                    let _ = conn.change_property(PropMode::REPLACE, req.requestor, property,
                                        uri_list_atom, 8, uri_list_bytes.len() as u32, &uri_list_bytes);
                                } else if req.target == gnome_atom {
                                    let _ = conn.change_property(PropMode::REPLACE, req.requestor, property,
                                        gnome_atom, 8, gnome_bytes.len() as u32, &gnome_bytes);
                                } else if utf8_string_atom == Some(req.target) || req.target == string_atom {
                                    let _ = conn.change_property(PropMode::REPLACE, req.requestor, property,
                                        string_atom, 8, text_bytes.len() as u32, &text_bytes);
                                } else {
                                    let _ = conn.change_property(PropMode::REPLACE, req.requestor, property,
                                        Atom::from(AtomEnum::NONE), 8, 0, &[]);
                                }

                                let notify = SelectionNotifyEvent {
                                    response_type: 31, sequence: 0, time: req.time,
                                    requestor: req.requestor, selection: req.selection,
                                    target: req.target, property,
                                };
                                let _ = conn.send_event(false, req.requestor as u32, EventMask::NO_EVENT, notify);
                                let _ = conn.flush();
                            }
                        }
                        29 => break, // SelectionClear
                        _ => {}
                    }
                }
                Ok(None) => { std::thread::sleep(std::time::Duration::from_millis(50)); }
                Err(_) => break,
            }
        }
        let _ = conn.destroy_window(win);
        let _ = conn.flush();
    });

    true
}

/// Try setting clipboard via xclip with x-special/gnome-copied-files format (fallback).
fn try_xclip_gnome_files(file_uris: &[String]) -> bool {
    let gnome_data = format!("copy\n{}\n", file_uris.join("\n"));
    let uri_list: String = file_uris.iter()
        .map(|uri| format!("{}\r\n", uri))
        .collect();

    // Try x-special/gnome-copied-files first (what Nautilus uses internally)
    if let Ok(mut child) = std::process::Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .arg("-t")
        .arg("x-special/gnome-copied-files")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            if stdin.write_all(gnome_data.as_bytes()).is_ok() {
                drop(stdin);
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    return true;
                }
            }
        }
    }

    // Fallback: text/uri-list
    if let Ok(mut child) = std::process::Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .arg("-t")
        .arg("text/uri-list")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            if stdin.write_all(uri_list.as_bytes()).is_ok() {
                drop(stdin);
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    return true;
                }
            }
        }
    }

    false
}


/// Try setting clipboard via wl-copy with x-special/gnome-copied-files (Nautilus preferred format).
/// Works on Wayland; silently fails on X11 (no Wayland compositor).
fn try_wl_copy_gnome_files(file_uris: &[String]) -> bool {
    let gnome_data = format!("copy\n{}\n", file_uris.join("\n"));

    // Try x-special/gnome-copied-files first (Nautilus native format)
    if let Ok(mut child) = std::process::Command::new("wl-copy")
        .arg("--type")
        .arg("x-special/gnome-copied-files")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            if stdin.write_all(gnome_data.as_bytes()).is_ok() {
                drop(stdin);
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    return true;
                }
            }
        }
    }

    // Fallback: text/uri-list
    let uri_list: String = file_uris.iter()
        .map(|uri| format!("{}\r\n", uri))
        .collect();
    if let Ok(mut child) = std::process::Command::new("wl-copy")
        .arg("--type")
        .arg("text/uri-list")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            if stdin.write_all(uri_list.as_bytes()).is_ok() {
                drop(stdin);
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    return true;
                }
            }
        }
    }
    false
}

pub unsafe fn set_clipboard_files(paths: Vec<String>) -> Result<(), String> {
    let file_uris: Vec<String> = paths.iter().map(|p| path_to_file_uri(p)).collect();

    let display = super::detect_display_server();
    match display {
        super::DisplayServer::Wayland => {
            // Wayland: wl-copy (native) → arboard
            if try_wl_copy_gnome_files(&file_uris) {
                return Ok(());
            }
            fallback_arboard(&paths)?;
        }
        super::DisplayServer::X11 | super::DisplayServer::Unknown => {
            // X11: x11rb multi-target (synchronous, preferred) → xclip → arboard
            if try_x11_multi_clipboard(&file_uris) {
                return Ok(());
            }
            if try_xclip_gnome_files(&file_uris) {
                return Ok(());
            }
            fallback_arboard(&paths)?;
        }
    }

    Ok(())
}

fn fallback_arboard(paths: &[String]) -> Result<(), String> {
    let text_version: String = paths.join("\n");
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text_version).map_err(|e| e.to_string())
}

pub unsafe fn set_clipboard_text_and_html(text: &str, _html: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

pub unsafe fn append_clipboard_text_and_html(text: &str, _html: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

pub fn set_clipboard_image_with_formats(
    data: ImageData,
    _gif_data: Option<&Vec<u8>>,
    _png_data: Option<&Vec<u8>>,
) -> Result<Option<String>, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;

    let image = arboard::ImageData {
        width: data.width,
        height: data.height,
        bytes: std::borrow::Cow::Owned(data.bytes),
    };

    clipboard.set_image(image).map_err(|e| e.to_string())?;
    Ok(None)
}
