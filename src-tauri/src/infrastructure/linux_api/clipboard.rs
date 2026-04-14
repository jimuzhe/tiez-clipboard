use std::sync::atomic::{AtomicU32, Ordering};

static SEQ: AtomicU32 = AtomicU32::new(1);

pub struct ImageData {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}

pub fn get_clipboard_sequence_number() -> u32 {
    SEQ.fetch_add(1, Ordering::Relaxed)
}

pub fn get_clipboard_image() -> Option<ImageData> {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(image_data) = clipboard.get_image() {
            return Some(ImageData {
                width: image_data.width,
                height: image_data.height,
                bytes: image_data.bytes.to_vec(),
            });
        }
    }
    None
}

pub fn get_clipboard_files() -> Option<Vec<String>> {
    use std::time::Duration;

    let clipboard = x11_clipboard::Clipboard::new().ok()?;
    let uri_list_atom = clipboard.getter.get_atom("text/uri-list").ok()?;
    let data = clipboard
        .load(
            clipboard.getter.atoms.clipboard,
            uri_list_atom,
            clipboard.getter.atoms.property,
            Duration::from_millis(200),
        )
        .ok()?;
    let text = String::from_utf8(data).ok()?;

    let files: Vec<String> = text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let path = line.trim();
            if let Some(stripped) = path.strip_prefix("file://") {
                stripped.to_string()
            } else {
                path.to_string()
            }
        })
        .map(|path| match urlencoding::decode(&path) {
            Ok(decoded) => decoded.to_string(),
            Err(_) => path,
        })
        .collect();

    if files.is_empty() {
        None
    } else {
        Some(files)
    }
}

pub fn get_clipboard_raw_format(_name: &str) -> Option<Vec<u8>> {
    None
}

pub fn set_clipboard_files(paths: Vec<String>) -> Result<(), String> {
    let clipboard =
        x11_clipboard::Clipboard::new().map_err(|e| format!("无法连接 X11 剪贴板: {}", e))?;
    let uri_list_atom = clipboard
        .getter
        .get_atom("text/uri-list")
        .map_err(|e| format!("无法获取 text/uri-list atom: {}", e))?;

    let mut uri_list = String::new();
    for path in paths {
        uri_list.push_str(&format!("file://{}\r\n", path));
    }

    clipboard
        .store(
            clipboard.setter.atoms.clipboard,
            uri_list_atom,
            uri_list.into_bytes(),
        )
        .map_err(|e| format!("写入文件到剪贴板失败: {}", e))?;

    Ok(())
}

pub fn set_clipboard_text_and_html(text: &str, _html: &str) -> Result<(), String> {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        clipboard
            .set_text(text.to_string())
            .map_err(|e| format!("设置剪贴板失败: {}", e))?;
    }
    Ok(())
}

pub fn set_clipboard_image_with_formats(data: ImageData) -> Result<(), String> {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let image = arboard::ImageData {
            width: data.width,
            height: data.height,
            bytes: std::borrow::Cow::Owned(data.bytes),
        };
        clipboard
            .set_image(image)
            .map_err(|e| format!("设置图像到剪贴板失败: {}", e))?;
    }
    Ok(())
}
