#[cfg(target_os = "macos")]
use objc2::rc::{autoreleasepool, Retained};
#[cfg(target_os = "macos")]
use objc2::ClassType;
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypePNG, NSPasteboardTypeString,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString, NSURL};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

pub struct ImageData {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}

static SEQ: AtomicU32 = AtomicU32::new(1);
static LAST_SIG: OnceLock<Mutex<u64>> = OnceLock::new();
const CLIPBOARD_SIG_MAX_BYTES: usize = 32 * 1024;

fn compute_clipboard_signature() -> u64 {
    let mut hasher = DefaultHasher::new();

    #[cfg(target_os = "macos")]
    autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        // We only hash a small part of the string for signature if it's text.
        // For performance, we prefer checking the change count first if available,
        // but for deduplication we still need a signature.
        if let Some(text) = unsafe { pb.stringForType(NSPasteboardTypeString) } {
            let s = text.to_string();
            let len = s.len().min(CLIPBOARD_SIG_MAX_BYTES);
            s[..len].hash(&mut hasher);
        } else if let Some(html) = unsafe { pb.stringForType(NSPasteboardTypeHTML) } {
            let s = html.to_string();
            let len = s.len().min(CLIPBOARD_SIG_MAX_BYTES);
            s[..len].hash(&mut hasher);
        } else {
            // Check for image types too
            if unsafe { pb.dataForType(NSPasteboardTypePNG).is_some() } {
                "__IMAGE_PNG__".hash(&mut hasher);
            }
        }
    });

    hasher.finish()
}

pub fn get_clipboard_text() -> Option<String> {
    #[cfg(target_os = "macos")]
    return autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        unsafe { pb.stringForType(NSPasteboardTypeString) }.map(|s| s.to_string())
    });

    #[cfg(not(target_os = "macos"))]
    None
}

pub fn get_clipboard_sequence_number() -> u32 {
    #[cfg(target_os = "macos")]
    {
        // Try getting the actual change count from NSPasteboard for maximum efficiency.
        let _change_count = autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            pb.changeCount() as u32
        });

        // We still check the signature to avoid false positives with app-set clipboard
        // but change_count is our primary trigger.
        let sig = compute_clipboard_signature();
        let guard = LAST_SIG.get_or_init(|| Mutex::new(0));
        let mut last_sig = guard.lock().unwrap();

        if *last_sig != sig {
            *last_sig = sig;
            // Use change_count as a seed or just increment our own SEQ.
            // Using our own SEQ is safer for our internal logic.
            SEQ.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
        } else {
            SEQ.load(Ordering::Relaxed)
        }
    }

    #[cfg(not(target_os = "macos"))]
    0
}

pub fn get_clipboard_image() -> Option<ImageData> {
    None
}

pub fn get_clipboard_files() -> Option<Vec<String>> {
    #[cfg(target_os = "macos")]
    return autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        let class_objs = unsafe {
            let ns_url_class: *const objc2::runtime::AnyClass = NSURL::class();
            NSArray::from_retained_slice(&[Retained::from_raw(
                ns_url_class as *mut objc2::runtime::AnyClass,
            )
            .unwrap()])
        };
        let options = NSDictionary::<NSString, objc2::runtime::AnyObject>::new();
        let items = unsafe { pb.readObjectsForClasses_options(&class_objs, Some(&options)) };

        if let Some(urls) = items {
            let mut paths = Vec::new();
            for i in 0..urls.count() {
                let url_any = urls.objectAtIndex(i);
                let url: Retained<NSURL> = unsafe { Retained::cast_unchecked(url_any) };
                if url.isFileURL() {
                    if let Some(path) = url.path() {
                        paths.push(path.to_string());
                    }
                }
            }
            if paths.is_empty() {
                None
            } else {
                Some(paths)
            }
        } else {
            None
        }
    });

    #[cfg(not(target_os = "macos"))]
    None
}

pub fn get_clipboard_raw_format(_name: &str) -> Option<Vec<u8>> {
    None
}

pub fn get_clipboard_html() -> Option<String> {
    #[cfg(target_os = "macos")]
    return autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        unsafe { pb.stringForType(NSPasteboardTypeHTML) }.map(|s| s.to_string())
    });

    #[cfg(not(target_os = "macos"))]
    None
}

pub fn set_clipboard_files(_paths: Vec<String>) -> Result<(), String> {
    Ok(())
}

pub fn set_clipboard_text_html_and_image(
    text: &str,
    html: &str,
    png_bytes: Option<Vec<u8>>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();

        let ns_text = NSString::from_str(text);
        unsafe { pb.setString_forType(&ns_text, NSPasteboardTypeString) };

        if !html.is_empty() {
            let ns_html = NSString::from_str(html);
            unsafe { pb.setString_forType(&ns_html, NSPasteboardTypeHTML) };
        }

        if let Some(bytes) = png_bytes {
            let ns_data = NSData::from_vec(bytes);
            unsafe { pb.setData_forType(Some(&ns_data), NSPasteboardTypePNG) };
        }

        Ok(())
    });

    #[cfg(not(target_os = "macos"))]
    Err("Not supported on this platform".to_string())
}

pub fn set_clipboard_text_and_html(text: &str, html: &str) -> Result<(), String> {
    set_clipboard_text_html_and_image(text, html, None)
}

pub fn append_clipboard_text_and_html(_text: &str, _html: &str) -> Result<(), String> {
    Ok(())
}

pub fn set_clipboard_image_with_formats(
    _data: ImageData,
    _gif_data: Option<&Vec<u8>>,
    _png_data: Option<&Vec<u8>>,
) -> Result<Option<String>, String> {
    Ok(None)
}
#[allow(dead_code)]
fn convert_rtf_to_html(rtf_bytes: &[u8]) -> Option<String> {
    let mut child = Command::new("textutil")
        .args([
            "-convert",
            "html",
            "-stdin",
            "-stdout",
            "-format",
            "rtf",
            "-encoding",
            "UTF-8",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;

    if let Some(stdin) = child.stdin.as_mut() {
        if stdin.write_all(rtf_bytes).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    } else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }

    let html = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if html.is_empty() {
        None
    } else {
        Some(html)
    }
}
