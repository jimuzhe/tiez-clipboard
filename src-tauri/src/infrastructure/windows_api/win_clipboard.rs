use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardFormatNameW, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GHND};

// Clipboard format constants
const CF_DIB: u32 = 8; // DIB
const CF_DIBV5: u32 = 17; // DIBV5
const CF_UNICODETEXT: u32 = 13; // Unicode text

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct BITMAPINFOHEADER {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_x_pels_per_meter: i32,
    bi_y_pels_per_meter: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

fn read_u32_le(raw_data: &[u8], offset: usize) -> Option<u32> {
    let bytes = raw_data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn detect_appended_bitfields_masks(
    raw_data: &[u8],
    pixel_data_offset: usize,
    bit_count: usize,
    compression: u32,
) -> usize {
    const BI_BITFIELDS: u32 = 3;
    const BI_ALPHABITFIELDS: u32 = 6;

    if compression != BI_BITFIELDS && compression != BI_ALPHABITFIELDS {
        return 0;
    }

    if bit_count != 16 && bit_count != 24 && bit_count != 32 {
        return 0;
    }

    let Some(red_mask) = read_u32_le(raw_data, pixel_data_offset) else {
        return 0;
    };
    let Some(green_mask) = read_u32_le(raw_data, pixel_data_offset + 4) else {
        return 0;
    };
    let Some(blue_mask) = read_u32_le(raw_data, pixel_data_offset + 8) else {
        return 0;
    };

    let looks_like_known_rgb_masks = matches!(
        (red_mask, green_mask, blue_mask),
        (0x00ff0000, 0x0000ff00, 0x000000ff)
            | (0x000000ff, 0x0000ff00, 0x00ff0000)
            | (0x00007c00, 0x000003e0, 0x0000001f)
            | (0x0000f800, 0x000007e0, 0x0000001f)
    );

    if !looks_like_known_rgb_masks {
        return 0;
    }

    let alpha_mask = read_u32_le(raw_data, pixel_data_offset + 12);
    let looks_like_known_alpha_mask = matches!(
        alpha_mask,
        Some(0x00000000) | Some(0xff000000) | Some(0x00008000)
    );

    if compression == BI_ALPHABITFIELDS {
        if looks_like_known_alpha_mask {
            16
        } else {
            0
        }
    } else if looks_like_known_alpha_mask {
        16
    } else {
        12
    }
}

#[derive(Clone)]
pub struct ImageData {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedClipboardFormat {
    pub name: String,
    pub data: Vec<u8>,
}

unsafe fn copy_hglobal_bytes(h_data: windows::Win32::Foundation::HANDLE) -> Option<Vec<u8>> {
    if h_data.is_invalid() {
        return None;
    }

    let h_global = HGLOBAL(h_data.0 as *mut _);
    let p_data = GlobalLock(h_global);
    if p_data.is_null() {
        return None;
    }

    let data_size = GlobalSize(h_global);
    let mut buffer = vec![0u8; data_size];
    std::ptr::copy_nonoverlapping(p_data as *const u8, buffer.as_mut_ptr(), data_size);
    let _ = GlobalUnlock(h_global);
    Some(buffer)
}

unsafe fn clipboard_format_name(format_id: u32) -> Option<String> {
    let mut buffer = vec![0u16; 256];
    let len = GetClipboardFormatNameW(format_id, &mut buffer);
    if len <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..len as usize]))
}

unsafe fn set_named_clipboard_format_bytes(format_id: u32, data: &[u8]) -> Result<(), String> {
    let alloc_len = data.len().max(1);
    let h_global = GlobalAlloc(GHND, alloc_len).map_err(|e| e.to_string())?;
    let p_mem = GlobalLock(h_global);
    if p_mem.is_null() {
        return Err("GlobalLock failed".to_string());
    }

    if !data.is_empty() {
        std::ptr::copy_nonoverlapping(data.as_ptr(), p_mem as *mut u8, data.len());
    } else {
        *(p_mem as *mut u8) = 0;
    }

    let _ = GlobalUnlock(h_global);
    SetClipboardData(
        format_id,
        Some(windows::Win32::Foundation::HANDLE(h_global.0 as _)),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Try to get image from Windows clipboard using native API
pub unsafe fn get_clipboard_image() -> Option<ImageData> {
    // Try to open clipboard
    if OpenClipboard(None).is_err() {
        return None;
    }

    // Try CF_DIBV5 first (newer format), then CF_DIB
    let h_dib = match GetClipboardData(CF_DIBV5) {
        Ok(handle) if !handle.is_invalid() => handle,
        _ => match GetClipboardData(CF_DIB) {
            Ok(handle) if !handle.is_invalid() => handle,
            _ => {
                let _ = CloseClipboard();
                return None;
            }
        },
    };

    // Lock the data
    let h_global = HGLOBAL(h_dib.0 as *mut _);
    let p_dib = GlobalLock(h_global);
    if p_dib.is_null() {
        let _ = CloseClipboard();
        return None;
    }

    // Get size and copy raw data to local buffer to minimize lock time
    let data_size = GlobalSize(h_global);
    let mut raw_data = vec![0u8; data_size];
    std::ptr::copy_nonoverlapping(p_dib as *const u8, raw_data.as_mut_ptr(), data_size);

    // Unlock and Close Clipboard IMMEDIATELY
    let _ = GlobalUnlock(h_global);
    let _ = CloseClipboard();

    // Process the data offline (without holding clipboard lock)
    // Wrap in closure just to use ? or early return logic easily
    let result = (|| {
        // Read BITMAPINFOHEADER
        if raw_data.len() < std::mem::size_of::<BITMAPINFOHEADER>() {
            return None;
        }
        let p_raw = raw_data.as_ptr();
        let header = *(p_raw as *const BITMAPINFOHEADER);

        let width = header.bi_width.abs() as usize;
        let height = header.bi_height.abs() as usize;
        let bit_count = header.bi_bit_count as usize;

        // Calculate sizes
        let header_size = header.bi_size as usize;

        // Safety check for header size
        if header_size > raw_data.len() {
            return None;
        }

        let color_table_size = if bit_count <= 8 {
            let num_colors = if header.bi_clr_used != 0 {
                header.bi_clr_used as usize
            } else {
                1 << bit_count
            };
            num_colors * 4 // Each color is 4 bytes (RGBQUAD)
        } else {
            0
        };

        // For BITMAPINFOHEADER (40 bytes), BI_BITFIELDS/BI_ALPHABITFIELDS store
        // channel masks immediately after the header. If we don't skip them,
        // pixel data starts with mask bytes -> image appears horizontally shifted.
        const BI_BITFIELDS: u32 = 3;
        const BI_ALPHABITFIELDS: u32 = 6;
        let bitfields_mask_size = if header.bi_size == 40 {
            match header.bi_compression {
                BI_BITFIELDS => 12,      // R/G/B masks (3 * DWORD)
                BI_ALPHABITFIELDS => 16, // R/G/B/A masks (4 * DWORD)
                _ => 0,
            }
        } else {
            0
        };

        // Pointer to pixel data
        let base_pixel_data_offset = header_size + color_table_size + bitfields_mask_size;
        let extra_mask_size = detect_appended_bitfields_masks(
            &raw_data,
            base_pixel_data_offset,
            bit_count,
            header.bi_compression,
        );
        let pixel_data_offset = base_pixel_data_offset + extra_mask_size;

        if pixel_data_offset > raw_data.len() {
            return None;
        }

        let pixel_data_ptr = p_raw.add(pixel_data_offset);

        // Calculate row stride.
        // Prefer header-reported / buffer-derived stride when valid, because some producers
        // (e.g. Office clipboard) may use wider row alignment than the classic formula.
        let row_stride_formula = ((width * bit_count + 31) / 32) * 4;
        let row_stride_from_header = if header.bi_size_image > 0 && height > 0 {
            let img_size = header.bi_size_image as usize;
            if img_size % height == 0 {
                let candidate = img_size / height;
                if candidate >= row_stride_formula {
                    Some(candidate)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        let row_stride_from_buffer = if height > 0 {
            let available = raw_data.len().saturating_sub(pixel_data_offset);
            let candidate = (available / height) & !3usize; // keep DWORD alignment
                                                            // Keep candidate in a sane range to avoid accidental over-read due to oversized HGLOBAL.
            if candidate >= row_stride_formula && candidate <= row_stride_formula + 256 {
                Some(candidate)
            } else {
                None
            }
        } else {
            None
        };
        let row_stride = row_stride_from_header
            .or(row_stride_from_buffer)
            .unwrap_or(row_stride_formula);
        let required_pixel_data_size = row_stride * height;

        if pixel_data_offset + required_pixel_data_size > raw_data.len() {
            // Incomplete data
            return None;
        }

        // Convert to RGBA
        let mut rgba_data = vec![0u8; width * height * 4];

        if bit_count == 32 {
            // 32-bit BGRA
            let mut alpha_non_zero = 0usize;
            for y in 0..height {
                // DIB is bottom-up, so flip vertically
                let src_y = if header.bi_height > 0 {
                    height - 1 - y
                } else {
                    y
                };
                let src_row = pixel_data_ptr.add(src_y * row_stride);
                let dst_row = (width * 4) * y;

                for x in 0..width {
                    let src_pixel = src_row.add(x * 4);
                    let dst_pixel = dst_row + x * 4;

                    rgba_data[dst_pixel] = *src_pixel.add(2); // R
                    rgba_data[dst_pixel + 1] = *src_pixel.add(1); // G
                    rgba_data[dst_pixel + 2] = *src_pixel; // B
                    let alpha = *src_pixel.add(3);
                    rgba_data[dst_pixel + 3] = alpha; // A
                    if alpha != 0 {
                        alpha_non_zero += 1;
                    }
                }
            }

            // Some producers (notably Office clipboard formats) put valid RGB data
            // with an all-zero alpha channel. Force opaque alpha in that case.
            if alpha_non_zero == 0 {
                for i in (3..rgba_data.len()).step_by(4) {
                    rgba_data[i] = 255;
                }
            }
        } else if bit_count == 24 {
            // 24-bit BGR
            for y in 0..height {
                let src_y = if header.bi_height > 0 {
                    height - 1 - y
                } else {
                    y
                };
                let src_row = pixel_data_ptr.add(src_y * row_stride);
                let dst_row = (width * 4) * y;

                for x in 0..width {
                    let src_pixel = src_row.add(x * 3);
                    let dst_pixel = dst_row + x * 4;

                    rgba_data[dst_pixel] = *src_pixel.add(2); // R
                    rgba_data[dst_pixel + 1] = *src_pixel.add(1); // G
                    rgba_data[dst_pixel + 2] = *src_pixel; // B
                    rgba_data[dst_pixel + 3] = 255; // A (opaque)
                }
            }
        } else {
            // println!("❌ Unsupported bit depth: {}", bit_count); // Fail silently or log
            return None;
        }

        Some(ImageData {
            width,
            height,
            bytes: rgba_data,
        })
    })();

    result
}

#[cfg(test)]
mod tests {
    use super::detect_appended_bitfields_masks;

    #[test]
    fn detects_extra_32bit_masks_after_extended_header() {
        let mut raw = vec![0u8; 124 + 16 + 64];
        raw[124..128].copy_from_slice(&0x00ff0000u32.to_le_bytes());
        raw[128..132].copy_from_slice(&0x0000ff00u32.to_le_bytes());
        raw[132..136].copy_from_slice(&0x000000ffu32.to_le_bytes());
        raw[136..140].copy_from_slice(&0xff000000u32.to_le_bytes());

        let extra = detect_appended_bitfields_masks(&raw, 124, 32, 3);
        assert_eq!(extra, 16);
    }

    #[test]
    fn ignores_normal_pixel_bytes() {
        let raw = vec![0x11u8; 128];
        let extra = detect_appended_bitfields_masks(&raw, 40, 32, 3);
        assert_eq!(extra, 0);
    }
}
const CF_HDROP: u32 = 15;

/// Try to get file paths from Windows clipboard (CF_HDROP)
pub unsafe fn get_clipboard_files() -> Option<Vec<String>> {
    if OpenClipboard(None).is_err() {
        return None;
    }

    let result = (|| {
        let h_drop = match GetClipboardData(CF_HDROP) {
            Ok(handle) if !handle.is_invalid() => handle,
            _ => return None,
        };

        let h_global = HGLOBAL(h_drop.0 as *mut _);
        let p_drop = GlobalLock(h_global);
        if p_drop.is_null() {
            return None;
        }

        // DROPFILES struct manually parsed
        // offset 0: pFiles (DWORD)
        // offset 16: fWide (BOOL)
        let p_base = p_drop as *const u8;
        let p_files_val = *(p_base as *const u32);
        let f_wide = *(p_base.add(16) as *const i32);

        let mut files = Vec::new();
        let files_start = p_base.add(p_files_val as usize);

        if f_wide != 0 {
            // Unicode (UTF-16)
            let mut ptr = files_start as *const u16;
            loop {
                let mut len = 0;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                if len == 0 {
                    break;
                } // Double null terminator found

                let slice = std::slice::from_raw_parts(ptr, len);
                if let Ok(path) = String::from_utf16(slice) {
                    files.push(path);
                }
                ptr = ptr.add(len + 1);
            }
        } else {
            // ANSI - Basic ASCII support as fallback
            let mut ptr = files_start as *const u8;
            loop {
                let mut len = 0;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                if len == 0 {
                    break;
                }

                let slice = std::slice::from_raw_parts(ptr, len);
                if let Ok(path) = std::str::from_utf8(slice) {
                    files.push(path.to_string());
                }
                ptr = ptr.add(len + 1);
            }
        }

        let _ = GlobalUnlock(h_global);

        if files.is_empty() {
            None
        } else {
            Some(files)
        }
    })();

    let _ = CloseClipboard();
    result
}

/// Set files to Windows clipboard (CF_HDROP)
pub unsafe fn set_clipboard_files(paths: Vec<String>) -> Result<(), String> {
    if OpenClipboard(None).is_err() {
        return Err("Cannot open clipboard".into());
    }

    // Prepare payload (Double null terminated list of wide strings)
    let mut buffer: Vec<u16> = Vec::new();
    for path in paths {
        buffer.extend(path.encode_utf16());
        buffer.push(0);
    }
    buffer.push(0); // Double null terminator

    // Calculate size needed
    // DROPFILES struct size + buffer size in bytes
    // pFiles(4) + pt.x(4) + pt.y(4) + fNC(4) + fWide(4) = 20 bytes
    let dropfiles_size = 20;
    let buffer_size = buffer.len() * 2;
    let total_size = dropfiles_size + buffer_size;

    let h_global = GlobalAlloc(GHND, total_size).map_err(|e| e.to_string())?;

    let p_mem = GlobalLock(h_global);
    if p_mem.is_null() {
        let _ = CloseClipboard();
        return Err("GlobalLock failed".into());
    }

    // Write DROPFILES struct
    // Offset 0: pFiles = 20 (size of struct)
    *(p_mem as *mut u32) = 20;

    // Offset 16: fWide = 1
    *(p_mem.add(16) as *mut i32) = 1;

    // Write file paths
    let p_files = p_mem.add(20) as *mut u16;
    std::ptr::copy_nonoverlapping(buffer.as_ptr(), p_files, buffer.len());

    let _ = GlobalUnlock(h_global);

    let _ = EmptyClipboard();
    if SetClipboardData(
        CF_HDROP,
        Some(windows::Win32::Foundation::HANDLE(h_global.0 as _)),
    )
    .is_err()
    {
        let _ = CloseClipboard();
        return Err("SetClipboardData failed".into());
    }

    let _ = CloseClipboard();
    Ok(())
}

pub fn get_clipboard_sequence_number() -> u32 {
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

pub unsafe fn clear_clipboard() -> Result<(), String> {
    if OpenClipboard(None).is_err() {
        return Err("Cannot open clipboard".into());
    }

    let result = EmptyClipboard().map_err(|e| e.to_string());
    let _ = CloseClipboard();
    result.map(|_| ())
}

/// Get raw bytes from a specific clipboard format by name
pub unsafe fn get_clipboard_raw_format(format_name: &str) -> Option<Vec<u8>> {
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;

    if OpenClipboard(None).is_err() {
        return None;
    }

    let result = (|| {
        let name_w: Vec<u16> = format_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let format_id = RegisterClipboardFormatW(windows::core::PCWSTR(name_w.as_ptr()));
        if format_id == 0 {
            return None;
        }

        let h_data = match GetClipboardData(format_id) {
            Ok(handle) => handle,
            Err(_) => return None,
        };

        copy_hglobal_bytes(h_data)
    })();

    let _ = CloseClipboard();
    result
}

/// Enumerate registered clipboard formats with HGLOBAL-backed payloads.
pub unsafe fn get_named_clipboard_formats(
    max_formats: usize,
    max_format_bytes: usize,
    max_total_bytes: usize,
) -> Vec<NamedClipboardFormat> {
    if OpenClipboard(None).is_err() {
        return Vec::new();
    }

    let result = (|| {
        let mut formats = Vec::new();
        let mut total_bytes = 0usize;
        let mut current_id = 0u32;

        loop {
            current_id = EnumClipboardFormats(current_id);
            if current_id == 0 {
                break;
            }

            let Some(name) = clipboard_format_name(current_id) else {
                continue;
            };

            let h_data = match GetClipboardData(current_id) {
                Ok(handle) => handle,
                Err(_) => continue,
            };

            let Some(data) = copy_hglobal_bytes(h_data) else {
                continue;
            };

            if data.is_empty() || data.len() > max_format_bytes {
                continue;
            }

            if total_bytes.saturating_add(data.len()) > max_total_bytes {
                continue;
            }

            total_bytes = total_bytes.saturating_add(data.len());
            formats.push(NamedClipboardFormat { name, data });
            if formats.len() >= max_formats {
                break;
            }
        }

        formats
    })();

    let _ = CloseClipboard();
    result
}

unsafe fn set_clipboard_text_and_html_inner(
    text: &str,
    cf_html: &str,
    clear_existing: bool,
) -> Result<(), String> {
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;

    if OpenClipboard(None).is_err() {
        return Err("Cannot open clipboard".into());
    }

    let result = (|| {
        if clear_existing {
            let _ = EmptyClipboard();
        }

        // 1) Set CF_UNICODETEXT
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let byte_len = wide.len() * 2;
        let h_text = GlobalAlloc(GHND, byte_len).map_err(|e| e.to_string())?;
        let p_text = GlobalLock(h_text);
        if p_text.is_null() {
            return Err("GlobalLock failed".to_string());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, p_text as *mut u8, byte_len);
        let _ = GlobalUnlock(h_text);
        if SetClipboardData(
            CF_UNICODETEXT,
            Some(windows::Win32::Foundation::HANDLE(h_text.0 as _)),
        )
        .is_err()
        {
            return Err("SetClipboardData (CF_UNICODETEXT) failed".to_string());
        }

        // 2) Set CF_HTML
        let format_name = "HTML Format";
        let name_w: Vec<u16> = format_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let format_id = RegisterClipboardFormatW(windows::core::PCWSTR(name_w.as_ptr()));
        if format_id == 0 {
            return Err("RegisterClipboardFormatW failed".to_string());
        }

        let html_bytes = cf_html.as_bytes();
        let h_html = GlobalAlloc(GHND, html_bytes.len() + 1).map_err(|e| e.to_string())?;
        let p_html = GlobalLock(h_html);
        if p_html.is_null() {
            return Err("GlobalLock failed".to_string());
        }
        std::ptr::copy_nonoverlapping(html_bytes.as_ptr(), p_html as *mut u8, html_bytes.len());
        *(p_html.add(html_bytes.len()) as *mut u8) = 0;
        let _ = GlobalUnlock(h_html);
        let _ = SetClipboardData(
            format_id,
            Some(windows::Win32::Foundation::HANDLE(h_html.0 as _)),
        );

        Ok(())
    })();

    let _ = CloseClipboard();
    result
}

/// Set Unicode text and CF_HTML, replacing existing clipboard formats.
pub unsafe fn set_clipboard_text_and_html(text: &str, cf_html: &str) -> Result<(), String> {
    set_clipboard_text_and_html_inner(text, cf_html, true)
}

/// Append/override Unicode text and CF_HTML while keeping existing non-text formats (e.g. image/DIB).
pub unsafe fn append_clipboard_text_and_html(text: &str, cf_html: &str) -> Result<(), String> {
    set_clipboard_text_and_html_inner(text, cf_html, false)
}

/// Append registered clipboard formats while keeping existing data intact.
pub unsafe fn append_named_clipboard_formats(
    formats: &[NamedClipboardFormat],
) -> Result<(), String> {
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;

    if formats.is_empty() {
        return Ok(());
    }

    if OpenClipboard(None).is_err() {
        return Err("Cannot open clipboard".into());
    }

    let result = (|| {
        for format in formats {
            if format.name.trim().is_empty() {
                continue;
            }

            let name_w: Vec<u16> = format
                .name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let format_id = RegisterClipboardFormatW(windows::core::PCWSTR(name_w.as_ptr()));
            if format_id == 0 {
                continue;
            }

            set_named_clipboard_format_bytes(format_id, &format.data)?;
        }

        Ok(())
    })();

    let _ = CloseClipboard();
    result
}

/// Set image (DIB) and optionally a raw format (like GIF) to clipboard in one go
pub unsafe fn set_clipboard_image_and_gif(
    image: ImageData,
    raw_format_name: Option<&str>,
    raw_data: Option<&[u8]>,
) -> Result<(), String> {
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;

    if OpenClipboard(None).is_err() {
        return Err("Cannot open clipboard".into());
    }

    let result = (|| {
        let _ = EmptyClipboard();

        // 1. Set Raw Format (e.g. "GIF")
        if let (Some(primary_name), Some(data)) = (raw_format_name, raw_data) {
            let names = if primary_name == "GIF" {
                vec![
                    "GIF",
                    "Animated GIF",
                    "gif",
                    "image/gif",
                    "Graphics Interchange Format",
                ]
            } else {
                vec![primary_name]
            };

            for name in names {
                let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                let format_id = RegisterClipboardFormatW(windows::core::PCWSTR(name_w.as_ptr()));
                if format_id != 0 {
                    if let Ok(h_global) = GlobalAlloc(GHND, data.len()) {
                        let p_mem = GlobalLock(h_global);
                        if !p_mem.is_null() {
                            std::ptr::copy_nonoverlapping(
                                data.as_ptr(),
                                p_mem as *mut u8,
                                data.len(),
                            );
                            let _ = GlobalUnlock(h_global);
                            let _ = SetClipboardData(
                                format_id,
                                Some(windows::Win32::Foundation::HANDLE(h_global.0 as _)),
                            );
                        }
                    }
                }
            }
        }

        // 2. Set CF_DIB
        // DIB data is BITMAPINFOHEADER + Pixel Data (BGRA, bottom-up)
        let header_size = std::mem::size_of::<BITMAPINFOHEADER>();
        let pixel_data_size = image.width * image.height * 4;
        let total_size = header_size + pixel_data_size;

        let h_global = GlobalAlloc(GHND, total_size).map_err(|e| e.to_string())?;
        let p_mem = GlobalLock(h_global);
        if p_mem.is_null() {
            return Err("GlobalLock failed".to_string());
        }

        let header = BITMAPINFOHEADER {
            bi_size: header_size as u32,
            bi_width: image.width as i32,
            bi_height: image.height as i32, // Top-down if positive in some contexts, but CF_DIB is usually bottom-up
            bi_planes: 1,
            bi_bit_count: 32,
            bi_compression: 0, // BI_RGB
            bi_size_image: pixel_data_size as u32,
            bi_x_pels_per_meter: 0,
            bi_y_pels_per_meter: 0,
            bi_clr_used: 0,
            bi_clr_important: 0,
        };

        // Write header
        std::ptr::copy_nonoverlapping(
            &header as *const _ as *const u8,
            p_mem as *mut u8,
            header_size,
        );

        // Write pixel data (Convert RGBA to BGRA and flip vertically for DIB)
        let p_pixels = p_mem.add(header_size) as *mut u8;
        for y in 0..image.height {
            let src_y = image.height - 1 - y; // Flip vertically
            let src_offset = src_y * image.width * 4;
            let dst_offset = y * image.width * 4;

            for x in 0..image.width {
                let s = src_offset + x * 4;
                let d = dst_offset + x * 4;
                // RGBA -> BGRA
                *p_pixels.add(d) = image.bytes[s + 2]; // B
                *p_pixels.add(d + 1) = image.bytes[s + 1]; // G
                *p_pixels.add(d + 2) = image.bytes[s]; // R
                *p_pixels.add(d + 3) = image.bytes[s + 3]; // A
            }
        }

        let _ = GlobalUnlock(h_global);
        if SetClipboardData(
            CF_DIB,
            Some(windows::Win32::Foundation::HANDLE(h_global.0 as _)),
        )
        .is_err()
        {
            return Err("SetClipboardData (CF_DIB) failed".to_string());
        }

        Ok(())
    })();

    let _ = CloseClipboard();
    result
}

/// Set image with multiple formats: GIF (optional), PNG (optional), and DIB
/// This maximizes compatibility with different applications
/// For GIF: Also sets CF_HDROP with temp file path (WeChat/QQ need this for animated GIFs)
pub unsafe fn set_clipboard_image_with_formats(
    image: ImageData,
    gif_data: Option<&[u8]>,
    png_data: Option<&[u8]>,
) -> Result<Option<String>, String> {
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;

    // For GIF, create temp file first (before opening clipboard)
    let gif_temp_path: Option<String> = if let Some(gif_bytes) = gif_data {
        let temp_dir = std::env::temp_dir();
        let filename = format!(
            "TieZ_GIF_{}.gif",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let path = temp_dir.join(filename);
        if std::fs::write(&path, gif_bytes).is_ok() {
            path.to_str().map(|s| s.to_string())
        } else {
            None
        }
    } else {
        None
    };

    if OpenClipboard(None).is_err() {
        return Err("Cannot open clipboard".into());
    }

    let result = (|| {
        let _ = EmptyClipboard();

        // 1. Set CF_HDROP for GIF (WeChat/QQ need file path for animated GIF)
        if let Some(ref path) = gif_temp_path {
            let mut buffer: Vec<u16> = Vec::new();
            buffer.extend(path.encode_utf16());
            buffer.push(0);
            buffer.push(0); // Double null terminator

            let dropfiles_size = 20;
            let buffer_size = buffer.len() * 2;
            let total_size = dropfiles_size + buffer_size;

            if let Ok(h_global) = GlobalAlloc(GHND, total_size) {
                let p_mem = GlobalLock(h_global);
                if !p_mem.is_null() {
                    // Write DROPFILES struct
                    *(p_mem as *mut u32) = 20; // pFiles offset
                    *(p_mem.add(16) as *mut i32) = 1; // fWide = true

                    // Write file path
                    let p_files = p_mem.add(20) as *mut u16;
                    std::ptr::copy_nonoverlapping(buffer.as_ptr(), p_files, buffer.len());

                    let _ = GlobalUnlock(h_global);
                    let _ = SetClipboardData(
                        CF_HDROP,
                        Some(windows::Win32::Foundation::HANDLE(h_global.0 as _)),
                    );
                }
            }
        }

        // 2. Set GIF formats (if available)
        if let Some(gif_bytes) = gif_data {
            let gif_format_names = [
                "GIF",
                "Animated GIF",
                "gif",
                "image/gif",
                "Graphics Interchange Format",
            ];

            for name in gif_format_names {
                let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                let format_id = RegisterClipboardFormatW(windows::core::PCWSTR(name_w.as_ptr()));
                if format_id != 0 {
                    if let Ok(h_global) = GlobalAlloc(GHND, gif_bytes.len()) {
                        let p_mem = GlobalLock(h_global);
                        if !p_mem.is_null() {
                            std::ptr::copy_nonoverlapping(
                                gif_bytes.as_ptr(),
                                p_mem as *mut u8,
                                gif_bytes.len(),
                            );
                            let _ = GlobalUnlock(h_global);
                            let _ = SetClipboardData(
                                format_id,
                                Some(windows::Win32::Foundation::HANDLE(h_global.0 as _)),
                            );
                        }
                    }
                }
            }
        }

        // 3. Set PNG format (if available) - many apps prefer PNG over DIB
        if let Some(png_bytes) = png_data {
            let png_format_names = ["PNG", "image/png"];

            for name in png_format_names {
                let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                let format_id = RegisterClipboardFormatW(windows::core::PCWSTR(name_w.as_ptr()));
                if format_id != 0 {
                    if let Ok(h_global) = GlobalAlloc(GHND, png_bytes.len()) {
                        let p_mem = GlobalLock(h_global);
                        if !p_mem.is_null() {
                            std::ptr::copy_nonoverlapping(
                                png_bytes.as_ptr(),
                                p_mem as *mut u8,
                                png_bytes.len(),
                            );
                            let _ = GlobalUnlock(h_global);
                            let _ = SetClipboardData(
                                format_id,
                                Some(windows::Win32::Foundation::HANDLE(h_global.0 as _)),
                            );
                        }
                    }
                }
            }
        }

        // 4. Set CF_DIB (universal fallback)
        let header_size = std::mem::size_of::<BITMAPINFOHEADER>();
        let pixel_data_size = image.width * image.height * 4;
        let total_size = header_size + pixel_data_size;

        let h_global = GlobalAlloc(GHND, total_size).map_err(|e| e.to_string())?;
        let p_mem = GlobalLock(h_global);
        if p_mem.is_null() {
            return Err("GlobalLock failed".to_string());
        }

        let header = BITMAPINFOHEADER {
            bi_size: header_size as u32,
            bi_width: image.width as i32,
            bi_height: image.height as i32,
            bi_planes: 1,
            bi_bit_count: 32,
            bi_compression: 0,
            bi_size_image: pixel_data_size as u32,
            bi_x_pels_per_meter: 0,
            bi_y_pels_per_meter: 0,
            bi_clr_used: 0,
            bi_clr_important: 0,
        };

        std::ptr::copy_nonoverlapping(
            &header as *const _ as *const u8,
            p_mem as *mut u8,
            header_size,
        );

        let p_pixels = p_mem.add(header_size) as *mut u8;
        for y in 0..image.height {
            let src_y = image.height - 1 - y;
            let src_offset = src_y * image.width * 4;
            let dst_offset = y * image.width * 4;

            for x in 0..image.width {
                let s = src_offset + x * 4;
                let d = dst_offset + x * 4;
                *p_pixels.add(d) = image.bytes[s + 2];
                *p_pixels.add(d + 1) = image.bytes[s + 1];
                *p_pixels.add(d + 2) = image.bytes[s];
                *p_pixels.add(d + 3) = image.bytes[s + 3];
            }
        }

        let _ = GlobalUnlock(h_global);
        if SetClipboardData(
            CF_DIB,
            Some(windows::Win32::Foundation::HANDLE(h_global.0 as _)),
        )
        .is_err()
        {
            return Err("SetClipboardData (CF_DIB) failed".to_string());
        }

        Ok(gif_temp_path.clone())
    })();

    let _ = CloseClipboard();
    result
}
