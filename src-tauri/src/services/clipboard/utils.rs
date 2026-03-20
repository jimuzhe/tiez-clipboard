use crate::domain::models::ClipboardEntry;
use crate::database::save_image_to_file;
use base64::{engine::general_purpose, Engine as _};
use regex::Regex;
use reqwest::header::CONTENT_TYPE;
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use urlencoding::decode;

const HTML_PREVIEW_MAX_CHARS: usize = 5000;
const HTML_PREVIEW_MAX_ROWS: usize = 10;
const HTML_TRUNCATION_SUFFIX: &str = "... [HTML Truncated]";
pub const RICH_IMAGE_FALLBACK_PREFIX: &str = "<!--TIEZ_RICH_IMAGE:";
pub const RICH_IMAGE_FALLBACK_SUFFIX: &str = "-->";
const REMOTE_IMAGE_MAX_BYTES: usize = 8 * 1024 * 1024;
const REMOTE_IMAGE_TIMEOUT_SECS: u64 = 4;

fn normalize_image_ext(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "gif" => Some("gif"),
        "webp" => Some("webp"),
        "bmp" => Some("bmp"),
        _ => None,
    }
}

fn image_ext_from_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

fn image_ext_from_url(url: &str) -> Option<&'static str> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let ext = Path::new(parsed.path())
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    normalize_image_ext(ext)
}

fn image_ext_from_bytes(bytes: &[u8]) -> Option<&'static str> {
    let format = image::guess_format(bytes).ok()?;
    match format {
        image::ImageFormat::Png => Some("png"),
        image::ImageFormat::Jpeg => Some("jpg"),
        image::ImageFormat::Gif => Some("gif"),
        image::ImageFormat::WebP => Some("webp"),
        image::ImageFormat::Bmp => Some("bmp"),
        _ => None,
    }
}

fn image_mime_by_ext(ext: &str) -> &'static str {
    match ext {
        "jpg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}

fn normalize_remote_img_url(src: &str) -> Option<String> {
    let trimmed = src.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }
    if trimmed.starts_with("//") {
        return Some(format!("https:{}", trimmed));
    }
    None
}

fn fetch_remote_image(url: &str) -> Option<(Vec<u8>, &'static str)> {
    static REMOTE_IMG_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();

    let client = REMOTE_IMG_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(REMOTE_IMAGE_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(8))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    });

    let resp = client
        .get(url)
        .header("Accept", "image/*")
        .send()
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let content_len = resp.content_length().unwrap_or(0);
    if content_len > REMOTE_IMAGE_MAX_BYTES as u64 {
        return None;
    }

    let mime = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();

    let mut limited = resp.take((REMOTE_IMAGE_MAX_BYTES as u64) + 1);
    let mut bytes = Vec::new();
    if limited.read_to_end(&mut bytes).is_err() {
        return None;
    }
    if bytes.is_empty() || bytes.len() > REMOTE_IMAGE_MAX_BYTES {
        return None;
    }

    let ext = image_ext_from_mime(&mime)
        .or_else(|| image_ext_from_url(url))
        .or_else(|| image_ext_from_bytes(&bytes))?;

    Some((bytes, ext))
}

fn save_image_bytes_to_attachments(
    bytes: &[u8],
    ext: &str,
    attachments_dir: &Path,
) -> Option<String> {
    let ext = normalize_image_ext(ext)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    bytes.hash(&mut hasher);
    let hash = hasher.finish();

    let file_name = format!("img_{:x}.{}", hash, ext);
    let target = attachments_dir.join(file_name);
    if !target.exists() {
        std::fs::write(&target, bytes).ok()?;
    }
    let path = target.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        Some(format!("file://{}", path))
    } else {
        Some(format!("file:///{}", path))
    }
}

fn truncate_chars_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut = text
        .char_indices()
        .nth(max_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len());
    let mut out = String::with_capacity(cut + suffix.len());
    out.push_str(&text[..cut]);
    out.push_str(suffix);
    out
}

pub fn attach_rich_image_fallback(html: &str, payload: &str) -> String {
    let mut out = String::with_capacity(
        html.len() + RICH_IMAGE_FALLBACK_PREFIX.len() + RICH_IMAGE_FALLBACK_SUFFIX.len() + payload.len() + 1,
    );
    out.push_str(html.trim_end());
    out.push('\n');
    out.push_str(RICH_IMAGE_FALLBACK_PREFIX);
    out.push_str(payload);
    out.push_str(RICH_IMAGE_FALLBACK_SUFFIX);
    out
}

pub fn split_rich_html_and_image_fallback(html: &str) -> (String, Option<String>) {
    if let Some(start) = html.rfind(RICH_IMAGE_FALLBACK_PREFIX) {
        let marker_start = start + RICH_IMAGE_FALLBACK_PREFIX.len();
        if let Some(end_rel) = html[marker_start..].find(RICH_IMAGE_FALLBACK_SUFFIX) {
            let marker_end = marker_start + end_rel;
            let mut cleaned = String::with_capacity(html.len());
            cleaned.push_str(&html[..start]);
            cleaned.push_str(&html[marker_end + RICH_IMAGE_FALLBACK_SUFFIX.len()..]);
            let payload = html[marker_start..marker_end].trim().to_string();
            return (cleaned.trim().to_string(), Some(payload));
        }
    }
    (html.to_string(), None)
}

pub fn externalize_rich_image_fallback(html: &str, data_dir: &Path) -> String {
    let (clean_html, payload_opt) = split_rich_html_and_image_fallback(html);
    let Some(payload) = payload_opt else {
        return html.to_string();
    };

    if !payload.starts_with("data:image/") {
        return html.to_string();
    }

    if let Some(saved_path) = save_image_to_file(&payload, data_dir) {
        let base_html = if clean_html.trim().is_empty() { html } else { clean_html.as_str() };
        return attach_rich_image_fallback(base_html, &saved_path);
    }

    html.to_string()
}

pub fn truncate_entry_for_ui(mut entry: ClipboardEntry) -> ClipboardEntry {
    if (entry.content_type == "text"
        || entry.content_type == "code"
        || entry.content_type == "url"
        || entry.content_type == "rich_text")
        && entry.content.chars().count() > 2000
    {
        entry.content = format!(
            "{}... [Truncated for speed]",
            entry.content.chars().take(2000).collect::<String>()
        );
    }

    // Also truncate HTML content up to a certain point for UI preview
    if let Some(ref html) = entry.html_content {
        if html.chars().count() > HTML_PREVIEW_MAX_CHARS {
            entry.html_content = truncate_html_for_preview(html);
        }
    }

    entry
}

pub fn truncate_html_for_preview(html: &str) -> Option<String> {
    if html.trim().is_empty() {
        return None;
    }

    if html.chars().count() <= HTML_PREVIEW_MAX_CHARS {
        return Some(html.to_string());
    }

    let trimmed = html.trim();
    let lower = trimmed.to_ascii_lowercase();
    let table_pos = lower.find("<table");
    let tr_pos = lower.find("<tr");
    let start_pos = match (table_pos, tr_pos) {
        (Some(t), Some(r)) => Some(std::cmp::min(t, r)),
        (Some(t), None) => Some(t),
        (None, Some(r)) => Some(r),
        (None, None) => None,
    };

    if let Some(start) = start_pos {
        let slice = &trimmed[start..];
        let lower_slice = &lower[start..];
        let mut end_rel = 0usize;
        let mut rows = 0usize;
        let mut search_idx = 0usize;

        while rows < HTML_PREVIEW_MAX_ROWS {
            if let Some(pos) = lower_slice[search_idx..].find("</tr") {
                let close_start = search_idx + pos;
                let close_end = lower_slice[close_start..]
                    .find('>')
                    .map(|p| close_start + p + 1)
                    .unwrap_or(close_start + 4);
                end_rel = close_end;
                rows += 1;
                search_idx = close_end;
            } else {
                break;
            }
        }

        if end_rel == 0 {
            end_rel = slice
                .char_indices()
                .nth(HTML_PREVIEW_MAX_CHARS)
                .map(|(i, _)| i)
                .unwrap_or(slice.len());
        }

        let mut out = slice[..end_rel].to_string();
        if lower_slice.starts_with("<tr") {
            out = format!(
                "<table style=\"border-collapse: collapse; min-width: 100%;\">{}</table>",
                out
            );
        } else if lower_slice.starts_with("<table") {
            if !out.to_ascii_lowercase().contains("</table") {
                out.push_str("</table>");
            }
        }

        if out.chars().count() > HTML_PREVIEW_MAX_CHARS {
            out = truncate_chars_with_suffix(&out, HTML_PREVIEW_MAX_CHARS, HTML_TRUNCATION_SUFFIX);
        }

        return Some(out);
    }

    Some(truncate_chars_with_suffix(
        trimmed,
        HTML_PREVIEW_MAX_CHARS,
        HTML_TRUNCATION_SUFFIX,
    ))
}

pub fn detect_content_type(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("www.") || trimmed.contains("://") && trimmed.split("://").next().map_or(false, |s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')) {
        return "url".to_string();
    }

    let mut score = 0;
    let keywords = [
        "import ", "const ", "let ", "var ", "function ", "class ", "pub fn ", "impl ",
        "#include", "package ", "interface ", "namespace ", "void ", "return ", "if (", "for (", "while (", "=>",
    ];

    for k in keywords {
        if text.contains(k) { score += 1; }
    }

    if text.contains(";") { score += 1; }
    if text.contains("{") && text.contains("}") { score += 1; }
    if text.contains("</") && text.contains(">") { score += 2; }

    if score >= 2 { return "code".to_string(); }

    if trimmed.starts_with("{") && trimmed.ends_with("}") && text.contains(":") && text.contains("\"") {
        return "code".to_string();
    }

    "text".to_string()
}

pub fn contains_sensitive_info(text: &str, kinds: &[String], custom_rules: &[String]) -> bool {
    static PHONE_RE: OnceLock<Regex> = OnceLock::new();
    static IDCARD_RE: OnceLock<Regex> = OnceLock::new();
    static EMAIL_RE: OnceLock<Regex> = OnceLock::new();
    static SECRET_RE: OnceLock<Regex> = OnceLock::new();

    static URL_RE: OnceLock<Regex> = OnceLock::new();

    if text.len() > 5000 || text.starts_with("data:") { return false; }

    let has_kind = |k: &str| kinds.iter().any(|t| t == k);

    if has_kind("url") {
        let re = URL_RE.get_or_init(|| Regex::new(r"(?i)(?:[a-zA-Z][a-zA-Z0-9+\-.]*://|www\.)\S+").unwrap());
        if re.is_match(text) { return true; }
    }
    if has_kind("phone") {
        let re = PHONE_RE.get_or_init(|| Regex::new(r"(?:\+?86)?[-\s\(]*1[3-9]\d{1}[-\s\)]*\d{4}[-\s]*\d{4}").unwrap());
        if re.is_match(text) { return true; }
    }
    if has_kind("idcard") {
        let re = IDCARD_RE.get_or_init(|| Regex::new(r"\b[1-9]\d{5}[1-9]\d{3}((0\d)|(1[0-2]))(([0|1|2]\d)|3[0-1])\d{3}([0-9Xx])\b").unwrap());
        if re.is_match(text) { return true; }
    }
    if has_kind("email") {
        let re = EMAIL_RE.get_or_init(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap());
        if re.is_match(text) { return true; }
    }
    if has_kind("secret") {
        let re = SECRET_RE.get_or_init(|| Regex::new(r"(?ix)((?:sk|pk|ghp|gho|github_pat|AIza|AKIA|ya29)[-_][\w\-]{20,}|(?:password|secret|api[_-]?key|access[_-]?key|token|bearer)[\s:=]+[\w\-]{16,})").unwrap());
        if re.is_match(text) { return true; }
    }
    if has_kind("password") {
        if text.len() >= 8 && text.len() <= 64 && !text.contains(' ') && !text.contains('\n') {
            let has_upper = text.chars().any(|c| c.is_uppercase());
            let has_lower = text.chars().any(|c| c.is_lowercase());
            let has_digit = text.chars().any(|c| c.is_numeric());
            let has_special = text.chars().any(|c| !c.is_alphanumeric());
            if has_upper && has_lower && has_digit && has_special { return true; }
        }
    }

    for rule in custom_rules {
        if let Ok(re) = Regex::new(rule) { if re.is_match(text) { return true; } }
    }
    false
}

pub fn embed_local_images(html: &str) -> String {
    let re = match Regex::new(r#"(<img\s+[^>]*src=["'])([^"']+)(["'][^>]*>)"#) {
        Ok(r) => r,
        Err(_) => return html.to_string(),
    };

    re.replace_all(html, |caps: &regex::Captures| {
        let prefix = &caps[1];
        let src = &caps[2];
        let suffix = &caps[3];

        let is_local = src.starts_with("file://") || 
            (src.len() > 2 && src.chars().nth(1) == Some(':') && (src.chars().nth(2) == Some('\\') || src.chars().nth(2) == Some('/')));

        if is_local {
            let path_str = if src.starts_with("file://") {
                let raw_path = src.trim_start_matches("file://");
                if raw_path.starts_with('/') && raw_path.chars().nth(2) == Some(':') { &raw_path[1..] } else { raw_path }
            } else { src };

            let decoded_path = decode(path_str).map(|p| p.into_owned()).unwrap_or(path_str.to_string());
            let clean_path = decoded_path.split('?').next().unwrap_or(&decoded_path).split('#').next().unwrap_or(&decoded_path);

            let path = std::path::Path::new(clean_path);
            if path.exists() {
                if let Ok(data) = std::fs::read(path) {
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("png").to_lowercase();
                    let mime = match ext.as_str() {
                        "jpg" | "jpeg" => "image/jpeg",
                        "gif" => "image/gif",
                        "webp" => "image/webp",
                        "bmp" => "image/bmp",
                        "svg" => "image/svg+xml",
                        _ => "image/png",
                    };
                    let b64 = general_purpose::STANDARD.encode(&data);
                    return format!("{}{}{}", prefix, format!("data:{};base64,{}", mime, b64), suffix);
                }
            }
        }

        if let Some(remote_url) = normalize_remote_img_url(src) {
            if let Some((bytes, ext)) = fetch_remote_image(&remote_url) {
                let b64 = general_purpose::STANDARD.encode(&bytes);
                let mime = image_mime_by_ext(ext);
                let data_url = format!("data:{};base64,{}", mime, b64);
                return format!("{}{}{}", prefix, data_url, suffix);
            }
        }
        format!("{}{}{}", prefix, src, suffix)
    }).to_string()
}

pub fn process_local_images_in_html(html: &str, data_dir: &std::path::Path) -> String {
    let attachments_dir = data_dir.join("attachments");
    if !attachments_dir.exists() { let _ = std::fs::create_dir_all(&attachments_dir); }

    let re = match Regex::new(r#"(<img\s+[^>]*src=["'])([^"']+)(["'][^>]*>)"#) {
        Ok(r) => r,
        Err(_) => return html.to_string(),
    };

    re.replace_all(html, |caps: &regex::Captures| {
        let prefix = &caps[1];
        let src = &caps[2];
        let suffix = &caps[3];

        let is_local = src.starts_with("file://") || 
            (src.len() > 2 && src.chars().nth(1) == Some(':') && (src.chars().nth(2) == Some('\\') || src.chars().nth(2) == Some('/')));

        if is_local {
            let path_str = if src.starts_with("file://") {
                let raw_path = src.trim_start_matches("file://");
                if raw_path.starts_with('/') && raw_path.chars().nth(2) == Some(':') { &raw_path[1..] } else { raw_path }
            } else { src };

            let decoded_path = decode(path_str).map(|p| p.into_owned()).unwrap_or(path_str.to_string());
            let clean_path = decoded_path.split('?').next().unwrap_or(&decoded_path).split('#').next().unwrap_or(&decoded_path);
            let path = std::path::Path::new(clean_path);
            
            if path.starts_with(&attachments_dir) { return format!("{}{}{}", prefix, src, suffix); }

            if path.exists() {
                if let Ok(data) = std::fs::read(path) {
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    data.hash(&mut hasher);
                    let hash = hasher.finish();
                    
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("png").to_lowercase();
                    let new_filename = format!("img_{:x}.{}", hash, ext);
                    let new_path = attachments_dir.join(&new_filename);
                    
                    if !new_path.exists() { let _ = std::fs::write(&new_path, &data); }
                    
                    let new_src = new_path.to_string_lossy().replace('\\', "/");
                    let final_src = if new_src.starts_with('/') { format!("file://{}", new_src) } else { format!("file:///{}", new_src) };
                    return format!("{}{}{}", prefix, final_src, suffix);
                }
            }
        }

        if let Some(remote_url) = normalize_remote_img_url(src) {
            if let Some((bytes, ext)) = fetch_remote_image(&remote_url) {
                if let Some(file_src) = save_image_bytes_to_attachments(&bytes, ext, &attachments_dir) {
                    return format!("{}{}{}", prefix, file_src, suffix);
                }
            }
        }
        format!("{}{}{}", prefix, src, suffix)
    }).to_string()
}

pub fn parse_cf_html(raw: &[u8]) -> Option<String> {
    enum HtmlEncoding { Utf8, Utf16Le }

    let detect_encoding = |data: &[u8]| -> HtmlEncoding {
        if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xFE { return HtmlEncoding::Utf16Le; }
        if data.len() % 2 == 0 {
            let zero_count = data.iter().filter(|b| **b == 0).count();
            if zero_count > data.len() / 4 { return HtmlEncoding::Utf16Le; }
        }
        HtmlEncoding::Utf8
    };

    let decode_bytes = |data: &[u8], encoding: &HtmlEncoding| -> String {
        match encoding {
            HtmlEncoding::Utf8 => String::from_utf8_lossy(data).to_string(),
            HtmlEncoding::Utf16Le => {
                let mut u16_buf = Vec::with_capacity(data.len() / 2);
                let mut i = 0;
                while i + 1 < data.len() {
                    u16_buf.push(u16::from_le_bytes([data[i], data[i + 1]]));
                    i += 2;
                }
                String::from_utf16_lossy(&u16_buf)
            }
        }
    };

    let encoding = detect_encoding(raw);
    let raw_str = decode_bytes(raw, &encoding);
    let mut start_fragment: Option<usize> = None;
    let mut end_fragment: Option<usize> = None;
    let mut start_html: Option<usize> = None;
    let mut end_html: Option<usize> = None;

    for line in raw_str.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("StartFragment:") {
            if let Ok(pos) = val.trim().parse::<usize>() { start_fragment = Some(pos); }
        } else if let Some(val) = trimmed.strip_prefix("EndFragment:") {
            if let Ok(pos) = val.trim().parse::<usize>() { end_fragment = Some(pos); }
        } else if let Some(val) = trimmed.strip_prefix("StartHTML:") {
            if let Ok(pos) = val.trim().parse::<usize>() { start_html = Some(pos); }
        } else if let Some(val) = trimmed.strip_prefix("EndHTML:") {
            if let Ok(pos) = val.trim().parse::<usize>() { end_html = Some(pos); }
        }
        if trimmed.starts_with("<") { break; }
    }

    if let (Some(frag_s), Some(frag_e)) = (start_fragment, end_fragment) {
        if frag_s < frag_e && frag_e <= raw.len() {
            let fragment = decode_bytes(&raw[frag_s..frag_e], &encoding);
            let trimmed = fragment.trim();
            let wrapped_fragment = if (trimmed.contains("<tr") || trimmed.contains("<td") || trimmed.contains("<col"))
                && !trimmed.to_lowercase().contains("<table")
            {
                format!("<table style=\"border-collapse: collapse; min-width: 100%;\">{}</table>", fragment)
            } else {
                fragment.clone()
            };

            if let (Some(html_s), Some(html_e)) = (start_html, end_html) {
                if html_s < html_e && html_e <= raw.len() {
                    let mut full_html = decode_bytes(&raw[html_s..html_e], &encoding);
                    let start_marker = "<!--StartFragment-->";
                    let end_marker = "<!--EndFragment-->";

                    if let Some(start_idx) = full_html.find(start_marker) {
                        let after_start = start_idx + start_marker.len();
                        if let Some(end_rel) = full_html[after_start..].find(end_marker) {
                            let end_idx = after_start + end_rel;
                            full_html = format!(
                                "{}{}{}",
                                &full_html[..after_start],
                                wrapped_fragment,
                                &full_html[end_idx..]
                            );
                        }
                    }

                    return Some(full_html);
                }
            }

            return Some(wrapped_fragment);
        }
    }

    let raw_text = raw_str.to_string();
    if let Some(start_idx) = raw_text.find("<!--StartFragment-->") {
        if let Some(end_idx) = raw_text.find("<!--EndFragment-->") {
            let fragment = &raw_text[start_idx + "<!--StartFragment-->".len()..end_idx];
            return Some(fragment.to_string());
        }
    }
    if raw_text.trim().starts_with("<") { return Some(raw_text); }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    mod detect_content_type_tests {
        use super::*;

        #[test]
        fn http_url() {
            assert_eq!(detect_content_type("http://example.com"), "url");
        }

        #[test]
        fn https_url() {
            assert_eq!(detect_content_type("https://example.com/path?q=1"), "url");
        }

        #[test]
        fn ftp_url() {
            assert_eq!(detect_content_type("ftp://files.example.com/doc.pdf"), "url");
        }

        #[test]
        fn custom_protocol_url() {
            assert_eq!(detect_content_type("myapp+custom://open/page"), "url");
        }

        #[test]
        fn www_url() {
            assert_eq!(detect_content_type("www.example.com"), "url");
        }

        #[test]
        fn url_with_whitespace() {
            assert_eq!(detect_content_type("  https://example.com  "), "url");
        }

        #[test]
        fn plain_text_not_url() {
            assert_eq!(detect_content_type("hello world"), "text");
        }

        #[test]
        fn colon_slash_slash_in_plain_text_no_valid_scheme() {
            // "://foo" alone — the part before :// is empty
            assert_eq!(detect_content_type("://foo"), "text");
        }

        #[test]
        fn code_snippet() {
            assert_eq!(detect_content_type("const x = 1; function foo() {}"), "code");
        }
    }

    mod contains_sensitive_info_tests {
        use super::*;

        fn kinds(list: &[&str]) -> Vec<String> {
            list.iter().map(|s| s.to_string()).collect()
        }

        #[test]
        fn detects_url() {
            assert!(contains_sensitive_info(
                "visit https://secret.internal/admin",
                &kinds(&["url"]),
                &[],
            ));
        }

        #[test]
        fn detects_ftp_url() {
            assert!(contains_sensitive_info(
                "ftp://files.company.com/secret.zip",
                &kinds(&["url"]),
                &[],
            ));
        }

        #[test]
        fn detects_www_url() {
            assert!(contains_sensitive_info(
                "visit www.example.com/admin",
                &kinds(&["url"]),
                &[],
            ));
        }

        #[test]
        fn no_url_kind_skips_url_check() {
            assert!(!contains_sensitive_info(
                "https://example.com",
                &kinds(&["phone"]),
                &[],
            ));
        }

        #[test]
        fn detects_phone() {
            assert!(contains_sensitive_info(
                "call me 13812345678",
                &kinds(&["phone"]),
                &[],
            ));
        }

        #[test]
        fn detects_email() {
            assert!(contains_sensitive_info(
                "send to user@example.com",
                &kinds(&["email"]),
                &[],
            ));
        }

        #[test]
        fn skips_data_uri() {
            assert!(!contains_sensitive_info(
                "data:image/png;base64,iVBOR...",
                &kinds(&["url", "phone", "email"]),
                &[],
            ));
        }

        #[test]
        fn skips_oversized_text() {
            let big = "a".repeat(5001);
            assert!(!contains_sensitive_info(
                &big,
                &kinds(&["phone"]),
                &[],
            ));
        }

        #[test]
        fn custom_regex_rule() {
            assert!(contains_sensitive_info(
                "order-12345",
                &kinds(&[]),
                &["order-\\d+".to_string()],
            ));
        }
    }
}
