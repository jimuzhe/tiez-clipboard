use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

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
            println!("[DEBUG] Linux got image via arboard: {}x{}, {} bytes", image.width, image.height, image.bytes.len());
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
                        println!("[DEBUG] Linux got {} bytes via x11 {} format", data.len(), mime_type);
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

    println!("[DEBUG] Linux: No image found in clipboard");
    None
}

/// Decode a file:// URI path, handling URL encoding and preserving the original if decoding fails
fn decode_file_uri(uri: &str) -> String {
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let path = path.trim(); // Remove trailing \r\n

    // Try to URL-decode, but keep original if it fails
    match urlencoding::decode(path) {
        Ok(decoded) => {
            let result = decoded.to_string();
            // Verify the file exists
            if !std::path::Path::new(&result).exists() {
                println!("[WARN] Decoded path does not exist: {:?}", result);
            }
            result
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
                println!("[DEBUG] Raw text/uri-list: {:?}", uri_list);

                let files: Vec<String> = uri_list
                    .lines()
                    .filter(|line| line.starts_with("file://"))
                    .map(|line| decode_file_uri(line))
                    .filter(|path| std::path::Path::new(path).exists())
                    .collect();

                if !files.is_empty() {
                    println!("[DEBUG] Linux found {} valid files via text/uri-list: {:?}", files.len(), files);
                    return Some(files);
                }
            }
        }
    }

    // Fallback to arboard for plain text with file:// URIs
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let text = clipboard.get_text().ok()?;
    println!("[DEBUG] Linux clipboard text: {:?}", text);

    let lines: Vec<String> = text
        .lines()
        .filter(|line| line.starts_with("file://"))
        .map(|line| decode_file_uri(line))
        .filter(|path| std::path::Path::new(path).exists())
        .collect();

    if lines.is_empty() {
        println!("[DEBUG] No valid files found in clipboard");
        None
    } else {
        println!("[DEBUG] Found {} valid files: {:?}", lines.len(), lines);
        Some(lines)
    }
}

pub unsafe fn get_clipboard_raw_format(_name: &str) -> Option<Vec<u8>> {
    None
}

/// Convert a file path to a file:// URI with percent-encoding for special characters
fn path_to_file_uri(path: &str) -> String {
    let encoded: String = path
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '_' || c == '.' || c == '~' {
                c.to_string()
            } else {
                format!("%{:02X}", c as u8)
            }
        })
        .collect();
    format!("file://{}", encoded)
}

/// Try setting clipboard via xclip with x-special/gnome-copied-files format.
/// xclip handles all X11 event loop complexity internally and is well-tested with Nautilus.
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
                println!("[DEBUG] Files set via xclip (x-special/gnome-copied-files)");
                return true;
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
                println!("[DEBUG] Files set via xclip (text/uri-list)");
                return true;
            }
        }
    }

    false
}

/// Set clipboard files using x11rb with multi-target support (text/uri-list + x-special/gnome-copied-files).
/// Nautilus requires both MIME types to accept file paste operations.
/// Spawns a background thread that serves clipboard data until another app takes ownership.
fn try_x11_multi_clipboard(file_uris: &[String]) -> bool {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::*;
    use x11rb::protocol::Event;

    // Build data for both targets
    let uri_list_bytes: Vec<u8> = file_uris.iter()
        .map(|uri| format!("{}\r\n", uri))
        .collect::<String>()
        .into_bytes();
    let gnome_bytes: Vec<u8> = format!("copy\n{}", file_uris.join("\n")).into_bytes();
    // Pre-compute plain text for UTF8_STRING / STRING targets (can't capture &file_uris in move closure)
    let text_bytes: Vec<u8> = file_uris.join("\n").into_bytes();

    // Connect to X11
    let (conn, screen_num) = match x11rb::connect(None) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let screen = &conn.setup().roots[screen_num];

    // Create invisible window
    let win = match conn.generate_id() {
        Ok(id) => id,
        Err(_) => return false,
    };

    if conn.create_window(
        0, // CopyFromParent depth
        win,
        screen.root,
        -10, -10, 1, 1, 0,
        WindowClass::COPY_FROM_PARENT,
        screen.root_visual,
        &CreateWindowAux::new().override_redirect(1), // Prevent WM from managing this window (avoids focus theft)
    ).is_err() {
        return false;
    }

    // Intern atoms
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

    // Extra atoms for better GTK4 compatibility
    let utf8_string_atom = intern(b"UTF8_STRING");
    let text_plain_atom = intern(b"text/plain");

    // Set selection owner
    if conn.set_selection_owner(win, clipboard_atom, 0u32).is_err() {
        return false;
    }
    if conn.flush().is_err() {
        return false;
    }

    // Verify ownership
    match conn.get_selection_owner(clipboard_atom).ok().and_then(|c| c.reply().ok()) {
        Some(reply) if reply.owner == win => {}
        _ => return false,
    }

    println!("[DEBUG] X11 multi-target clipboard owner set (text/uri-list + x-special/gnome-copied-files)");

    // Spawn background thread to serve clipboard data
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
                            // Use Event enum pattern matching
                            if let Event::SelectionRequest(req) = event {
                                let property = if req.property == 0 { req.target } else { req.property };

                                // Debug: log which target is being requested
                                let target_name = if req.target == targets_atom { "TARGETS".to_string() }
                                    else if req.target == timestamp_atom { "TIMESTAMP".to_string() }
                                    else if req.target == uri_list_atom { "text/uri-list".to_string() }
                                    else if req.target == gnome_atom { "x-special/gnome-copied-files".to_string() }
                                    else { format!("unknown({})", req.target) };
                                println!("[DEBUG] X11 SelectionRequest: target={} from window={}", target_name, req.requestor);

                                if req.target == targets_atom {
                                    // TARGETS request: return list of supported atoms
                                    println!("[DEBUG] X11 TARGETS request from window {}", req.requestor);
                                    let supported = [targets_atom, timestamp_atom, uri_list_atom, gnome_atom, utf8_string_atom.unwrap_or(targets_atom), text_plain_atom.unwrap_or(targets_atom), string_atom];
                                    let data: Vec<u8> = supported.iter()
                                        .flat_map(|a| a.to_ne_bytes())
                                        .collect();
                                    if let Err(e) = conn.change_property(
                                        PropMode::REPLACE,
                                        req.requestor,
                                        property,
                                        atom_atom,
                                        32,
                                        supported.len() as u32,
                                        &data,
                                    ) {
                                        println!("[WARN] change_property failed for TARGETS: {:?}", e);
                                    }
                                } else if req.target == timestamp_atom {
                                    let ts = 0u32;
                                    let _ = conn.change_property(
                                        PropMode::REPLACE,
                                        req.requestor,
                                        property,
                                        Atom::from(AtomEnum::INTEGER),
                                        32,
                                        1,
                                        &ts.to_ne_bytes(),
                                    );
                                } else if req.target == uri_list_atom {
                                    if let Err(e) = conn.change_property(
                                        PropMode::REPLACE,
                                        req.requestor,
                                        property,
                                        uri_list_atom,  // Use target atom as type (X11/GTK convention for MIME targets)
                                        8,
                                        uri_list_bytes.len() as u32,
                                        &uri_list_bytes,
                                    ) {
                                        println!("[WARN] change_property failed for text/uri-list: {:?}", e);
                                    } else {
                                        println!("[DEBUG] X11 responding with text/uri-list: {} bytes", uri_list_bytes.len());
                                    }
                                } else if req.target == gnome_atom {
                                    if let Err(e) = conn.change_property(
                                        PropMode::REPLACE,
                                        req.requestor,
                                        property,
                                        gnome_atom,  // Use target atom as type (X11/GTK convention for MIME targets)
                                        8,
                                        gnome_bytes.len() as u32,
                                        &gnome_bytes,
                                    ) {
                                        println!("[WARN] change_property failed for gnome-copied-files: {:?}", e);
                                    } else {
                                        println!("[DEBUG] X11 responding with x-special/gnome-copied-files: {} bytes", gnome_bytes.len());
                                    }
                                } else if utf8_string_atom == Some(req.target) || req.target == string_atom {
                                    // UTF8_STRING or STRING: return URI list as plain text
                                    if let Err(e) = conn.change_property(
                                        PropMode::REPLACE,
                                        req.requestor,
                                        property,
                                        string_atom,
                                        8,
                                        text_bytes.len() as u32,
                                        &text_bytes,
                                    ) {
                                        println!("[WARN] change_property failed for STRING: {:?}", e);
                                    }
                                } else {
                                    // Unknown target — set empty property with None type
                                    let _ = conn.change_property(
                                        PropMode::REPLACE,
                                        req.requestor,
                                        property,
                                        Atom::from(AtomEnum::NONE),
                                        8,
                                        0,
                                        &[],
                                    );
                                }

                                // Send SelectionNotify response
                                let notify = SelectionNotifyEvent {
                                    response_type: 31,
                                    sequence: 0,
                                    time: req.time,
                                    requestor: req.requestor,
                                    selection: req.selection,
                                    target: req.target,
                                    property,
                                };
                                if let Err(e) = conn.send_event(
                                    false,
                                    req.requestor as u32,
                                    EventMask::NO_EVENT,
                                    notify,
                                ) {
                                    println!("[WARN] send_event failed: {:?}", e);
                                }
                                let _ = conn.flush();
                            }
                        }
                        29 => break, // SelectionClear — another app took ownership
                        _ => {}
                    }
                }
                Ok(None) => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(_) => break,
            }
        }

        let _ = conn.destroy_window(win);
        let _ = conn.flush();
    });

    true
}

/// Try setting clipboard via xclip (X11 only, single target — Nautilus may not work)
fn try_xclip_clipboard(paths: &[String]) -> Result<(), String> {
    let uri_list: String = paths
        .iter()
        .map(|p| format!("{}\r\n", path_to_file_uri(p)))
        .collect();

    if let Ok(mut xclip) = std::process::Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .arg("-t")
        .arg("text/uri-list")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = xclip.stdin.take() {
            use std::io::Write;
            if stdin.write_all(uri_list.as_bytes()).is_ok() {
                let status = xclip.wait();
                if status.map(|s| s.success()).unwrap_or(false) {
                    println!("[DEBUG] Files set via xclip (single target, Nautilus may not work)");
                    return Ok(());
                }
            }
        }
    }

    Err("xclip failed".to_string())
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
                    println!("[DEBUG] Files set via wl-copy (x-special/gnome-copied-files)");
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
                    println!("[DEBUG] Files set via wl-copy (text/uri-list)");
                    return true;
                }
            }
        }
    }
    false
}

pub unsafe fn set_clipboard_files(paths: Vec<String>) -> Result<(), String> {
    println!("[DEBUG] Setting {} files to clipboard: {:?}", paths.len(), paths);

    let file_uris: Vec<String> = paths.iter().map(|p| path_to_file_uri(p)).collect();
    println!("[DEBUG] File URIs: {:?}", file_uris);

    // Method 0: Try wl-copy (Wayland native clipboard — works if wl-copy is installed)
    // No platform check: on X11 wl-copy silently fails, on Wayland it works natively.
    if try_wl_copy_gnome_files(&file_uris) {
        return Ok(());
    }

    // Method 1: x11rb multi-target clipboard (SYNCHRONOUS — preferred over xclip)
    // x11rb synchronously acquires X11 CLIPBOARD ownership and verifies it before returning.
    // xclip is async (spawns background process) — clipboard may not be ready when Ctrl+V is sent.
    if try_x11_multi_clipboard(&file_uris) {
        return Ok(());
    }

    // Method 2: xclip with x-special/gnome-copied-files (async fallback)
    if try_xclip_gnome_files(&file_uris) {
        // xclip is async — give it time to acquire clipboard ownership before we return
        std::thread::sleep(std::time::Duration::from_millis(50));
        return Ok(());
    }

    // Method 3: xclip with text/uri-list (async fallback, less reliable with Nautilus)
    if try_xclip_clipboard(&paths).is_ok() {
        std::thread::sleep(std::time::Duration::from_millis(50));
        return Ok(());
    }

    // Method 4: Last resort — arboard (sets plain text, Nautilus won't work)
    println!("[DEBUG] Falling back to arboard (file paste will not work in Nautilus)");
    let text_version: String = paths.join("\n");
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text_version).map_err(|e| e.to_string())?;

    Ok(())
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
