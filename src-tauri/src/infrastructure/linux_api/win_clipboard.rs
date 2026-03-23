use std::sync::atomic::{AtomicU32, Ordering};

pub struct ImageData {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}

static SEQ: AtomicU32 = AtomicU32::new(1);

pub fn get_clipboard_sequence_number() -> u32 {
    SEQ.fetch_add(1, Ordering::Relaxed)
}

pub unsafe fn get_clipboard_image() -> Option<ImageData> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let image = clipboard.get_image().ok()?;

    Some(ImageData {
        width: image.width,
        height: image.height,
        bytes: image.bytes.to_vec(),
    })
}

pub unsafe fn get_clipboard_files() -> Option<Vec<String>> {
    let mut clipboard = arboard::Clipboard::new().ok()?;

    let text = clipboard.get_text().ok()?;
    let lines: Vec<String> = text
        .lines()
        .filter(|line| line.starts_with("file://"))
        .map(|line| {
            let path = line.strip_prefix("file://").unwrap_or(line);
            urlencoding::decode(path).unwrap_or_default().to_string()
        })
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

pub unsafe fn set_clipboard_files(paths: Vec<String>) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;

    let text = paths
        .iter()
        .map(|p| format!("file://{}", p))
        .collect::<Vec<_>>()
        .join("\n");

    clipboard.set_text(text).map_err(|e| e.to_string())
}

pub unsafe fn set_clipboard_text_and_html(text: &str, _html: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

pub fn set_clipboard_image_with_formats(data: ImageData) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;

    let image = arboard::ImageData {
        width: data.width,
        height: data.height,
        bytes: std::borrow::Cow::Owned(data.bytes),
    };

    clipboard.set_image(image).map_err(|e| e.to_string())
}
