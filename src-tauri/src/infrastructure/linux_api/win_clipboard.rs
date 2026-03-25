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

pub unsafe fn set_clipboard_files(paths: Vec<String>) -> Result<(), String> {
    println!("[DEBUG] Setting {} files to clipboard: {:?}", paths.len(), paths);

    // Build text/uri-list format according to freedesktop.org spec
    // Format: file:///absolute/path (not URL-encoded for basic ASCII paths)
    // Only encode special characters like spaces, not the path separators
    let uri_list: String = paths
        .iter()
        .map(|p| {
            // Only URL-encode spaces and special chars, keep / as-is
            let encoded: String = p
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '_' || c == '.' || c == '~' {
                        c.to_string()
                    } else {
                        format!("%{:02X}", c as u8)
                    }
                })
                .collect();
            format!("file://{}\r\n", encoded)
        })
        .collect();

    println!("[DEBUG] URI list: {:?}", uri_list);

    // Method 1: Try xclip (most reliable for file managers like Nautilus)
    // xclip properly handles X11 selection protocol with multiple targets
    if let Ok(xclip_output) = std::process::Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .arg("-t")
        .arg("text/uri-list")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        let mut xclip = xclip_output;
        if let Some(mut stdin) = xclip.stdin.take() {
            use std::io::Write;
            if stdin.write_all(uri_list.as_bytes()).is_ok() {
                let status = xclip.wait();
                println!("[DEBUG] xclip result: {:?}", status);
                if status.map(|s| s.success()).unwrap_or(false) {
                    println!("[DEBUG] Successfully set files via xclip");
                    return Ok(());
                }
            }
        }
    }

    // Method 2: Try x11-clipboard crate as fallback
    if let Ok(clipboard) = x11_clipboard::Clipboard::new() {
        println!("[DEBUG] Trying x11-clipboard as fallback...");

        if let Ok(uri_list_atom) = clipboard.setter.get_atom("text/uri-list") {
            match clipboard.store(
                clipboard.setter.atoms.clipboard,
                uri_list_atom,
                uri_list.as_bytes(),
            ) {
                Ok(()) => {
                    println!("[DEBUG] Successfully set files via x11-clipboard");
                    return Ok(());
                }
                Err(e) => {
                    println!("[WARN] x11-clipboard store failed: {:?}", e);
                }
            }
        }
    }

    // Method 3: Fallback to arboard (won't work with file managers)
    println!("[DEBUG] Falling back to arboard (file paste may not work in file managers)");
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
