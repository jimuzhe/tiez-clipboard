use crate::database::save_image_to_file;
use crate::domain::models::ClipboardEntry;
use base64::{engine::general_purpose, Engine as _};
use regex::Regex;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use urlencoding::decode;

const HTML_PREVIEW_MAX_CHARS: usize = 5000;
const HTML_PREVIEW_MAX_ROWS: usize = 10;
const HTML_TRUNCATION_SUFFIX: &str = "... [HTML Truncated]";
const TEXT_PREVIEW_MAX_CHARS: usize = 500;
const TEXT_PREVIEW_TRUNCATED_CHARS: usize = TEXT_PREVIEW_MAX_CHARS - 3;
const RICH_TEXT_PREVIEW_FALLBACK: &str = "[Rich Text Content]";
pub const RICH_IMAGE_FALLBACK_PREFIX: &str = "<!--TIEZ_RICH_IMAGE:";
pub const RICH_IMAGE_FALLBACK_SUFFIX: &str = "-->";
pub const RICH_NAMED_FORMATS_PREFIX: &str = "<!--TIEZ_RICH_FORMATS:";
pub const RICH_NAMED_FORMATS_SUFFIX: &str = "-->";
const REMOTE_IMAGE_MAX_BYTES: usize = 8 * 1024 * 1024;
const REMOTE_IMAGE_TIMEOUT_SECS: u64 = 4;

#[derive(Serialize, Deserialize)]
struct StoredNamedClipboardFormat {
    name: String,
    data_base64: String,
}

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

    let resp = client.get(url).header("Accept", "image/*").send().ok()?;

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

fn normalize_html_image_src_candidate(src: &str) -> Option<String> {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return None;
    }

    let normalized = if trimmed.starts_with("data:") {
        trimmed.to_string()
    } else {
        let first_candidate = trimmed
            .split(',')
            .next()
            .unwrap_or(trimmed)
            .split_whitespace()
            .next()
            .unwrap_or(trimmed)
            .trim();
        first_candidate.replace("&amp;", "&")
    };

    if normalized.is_empty()
        || normalized.starts_with("blob:")
        || normalized.starts_with("javascript:")
    {
        return None;
    }

    Some(normalized)
}

fn looks_like_gif_image_src(src: &str) -> bool {
    let lower = src.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }

    lower.starts_with("data:image/gif")
        || lower.contains(".gif")
        || lower.contains("format=gif")
        || lower.contains("fm=gif")
        || lower.contains("mime=image/gif")
        || lower.contains("image/gif")
}

fn resolve_local_image_src_path(src: &str) -> Option<std::path::PathBuf> {
    let is_local = src.starts_with("file://")
        || (src.len() > 2
            && src.chars().nth(1) == Some(':')
            && (src.chars().nth(2) == Some('\\') || src.chars().nth(2) == Some('/')));
    if !is_local {
        return None;
    }

    let path_str = if src.starts_with("file://") {
        let raw_path = src.trim_start_matches("file://");
        if raw_path.starts_with('/') && raw_path.chars().nth(2) == Some(':') {
            &raw_path[1..]
        } else {
            raw_path
        }
    } else {
        src
    };

    let decoded_path = decode(path_str)
        .map(|p| p.into_owned())
        .unwrap_or(path_str.to_string());
    let clean_path = decoded_path
        .split('?')
        .next()
        .unwrap_or(&decoded_path)
        .split('#')
        .next()
        .unwrap_or(&decoded_path);
    let path = std::path::Path::new(clean_path);
    if !path.exists() {
        return None;
    }

    Some(path.to_path_buf())
}

fn gif_data_url_from_bytes(bytes: &[u8]) -> Option<String> {
    let ext = image_ext_from_bytes(bytes).or_else(|| {
        if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some("gif")
        } else {
            None
        }
    })?;
    if ext != "gif" {
        return None;
    }

    let b64 = general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{};base64,{}", image_mime_by_ext(ext), b64))
}

fn image_data_url_from_bytes(bytes: &[u8]) -> Option<String> {
    let ext = image_ext_from_bytes(bytes).or_else(|| {
        if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some("gif")
        } else {
            None
        }
    })?;

    let b64 = general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{};base64,{}", image_mime_by_ext(ext), b64))
}

fn resolve_image_src_to_data_url(src: &str) -> Option<String> {
    let value = src.trim();
    if value.starts_with("data:image/") {
        return Some(value.to_string());
    }

    if let Some(path) = resolve_local_image_src_path(value) {
        let bytes = std::fs::read(&path).ok()?;
        return image_data_url_from_bytes(&bytes);
    }

    None
}

fn resolve_animated_image_src_to_data_url(src: &str) -> Option<String> {
    let value = src.trim();
    if value.starts_with("data:image/gif") {
        return Some(value.to_string());
    }

    if !looks_like_gif_image_src(value) {
        return None;
    }

    if let Some(path) = resolve_local_image_src_path(value) {
        let bytes = std::fs::read(&path).ok()?;
        return gif_data_url_from_bytes(&bytes);
    }

    if let Some(remote_url) = normalize_remote_img_url(value) {
        let (bytes, ext) = fetch_remote_image(&remote_url)?;
        if ext == "gif" {
            return gif_data_url_from_bytes(&bytes);
        }
    }

    None
}

pub fn extract_animated_image_data_url_from_html(html: &str) -> Option<String> {
    if html.trim().is_empty() {
        return None;
    }

    static IMG_TAG_RE: OnceLock<Regex> = OnceLock::new();
    static IMG_ATTR_RE: OnceLock<Regex> = OnceLock::new();

    let img_tag_re = IMG_TAG_RE.get_or_init(|| Regex::new(r"(?is)<img\b[^>]*>").unwrap());
    let img_attr_re = IMG_ATTR_RE.get_or_init(|| {
        Regex::new(
            r#"(?is)(src|data-src|data-original|data-actualsrc|srcset)\s*=\s*["']([^"']+)["']"#,
        )
        .unwrap()
    });

    for tag in img_tag_re.find_iter(html) {
        for caps in img_attr_re.captures_iter(tag.as_str()) {
            let Some(raw_src) = caps.get(2).map(|m| m.as_str()) else {
                continue;
            };
            let Some(candidate) = normalize_html_image_src_candidate(raw_src) else {
                continue;
            };
            if let Some(data_url) = resolve_animated_image_src_to_data_url(&candidate) {
                return Some(data_url);
            }
        }
    }

    None
}

pub fn extract_first_image_data_url_from_html(html: &str) -> Option<String> {
    if html.trim().is_empty() {
        return None;
    }

    static IMG_TAG_RE: OnceLock<Regex> = OnceLock::new();
    static IMG_ATTR_RE: OnceLock<Regex> = OnceLock::new();

    let img_tag_re = IMG_TAG_RE.get_or_init(|| Regex::new(r"(?is)<img\b[^>]*>").unwrap());
    let img_attr_re = IMG_ATTR_RE.get_or_init(|| {
        Regex::new(
            r#"(?is)(src|data-src|data-original|data-actualsrc|srcset)\s*=\s*["']([^"']+)["']"#,
        )
        .unwrap()
    });

    for tag in img_tag_re.find_iter(html) {
        for caps in img_attr_re.captures_iter(tag.as_str()) {
            let Some(raw_src) = caps.get(2).map(|m| m.as_str()) else {
                continue;
            };
            let Some(candidate) = normalize_html_image_src_candidate(raw_src) else {
                continue;
            };
            if let Some(data_url) = resolve_image_src_to_data_url(&candidate) {
                return Some(data_url);
            }
        }
    }

    None
}

pub fn extract_animated_image_data_url_from_text(text: &str) -> Option<String> {
    let candidate = normalize_html_image_src_candidate(text)?;
    resolve_animated_image_src_to_data_url(&candidate)
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

fn collapse_preview_whitespace(text: &str) -> String {
    static WHITESPACE_RE: OnceLock<Regex> = OnceLock::new();

    let normalized = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', " ");
    WHITESPACE_RE
        .get_or_init(|| Regex::new(r"\s+").unwrap())
        .replace_all(&normalized, " ")
        .trim()
        .to_string()
}

pub fn build_clipboard_text_fingerprint(
    content_type: &str,
    content: &str,
    html_content: Option<&str>,
) -> String {
    match content_type {
        "rich_text" => {
            collapse_preview_whitespace(&derive_rich_text_content(content, html_content))
        }
        "text" | "code" | "url" => {
            collapse_preview_whitespace(&normalize_clipboard_plain_text(content))
        }
        _ => String::new(),
    }
}

fn collapse_line_whitespace(text: &str) -> String {
    static WHITESPACE_RE: OnceLock<Regex> = OnceLock::new();

    WHITESPACE_RE
        .get_or_init(|| Regex::new(r"[^\S\r\n]+").unwrap())
        .replace_all(text.trim(), " ")
        .trim()
        .to_string()
}

fn normalize_plain_text_layout(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();

    for raw_line in normalized.lines() {
        let line = collapse_line_whitespace(raw_line);
        if line.is_empty() {
            if !lines
                .last()
                .map(|last: &String| last.is_empty())
                .unwrap_or(false)
            {
                lines.push(String::new());
            }
        } else {
            lines.push(line);
        }
    }

    let start = lines
        .iter()
        .position(|line| !line.is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map(|idx| idx + 1)
        .unwrap_or(start);

    lines[start..end].join("\n")
}

fn decode_basic_html_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn is_office_style_definition_text(text: &str) -> bool {
    static OFFICE_STYLE_SIGNAL_RE: OnceLock<Regex> = OnceLock::new();

    let normalized = collapse_preview_whitespace(text);
    normalized.len() > 24
        && OFFICE_STYLE_SIGNAL_RE
            .get_or_init(|| {
                Regex::new(
                    r"(?is)(/\*\s*style definitions\s*\*/|mso-style-name|mso-style-noshow|mso-style-priority|mso-padding-alt|mso-para-margin|table\.mso|mso-|microsoftinternetexplorer\d*|documentnotspecified|wps office|office word|msonormal|mso normal|normal\s+\d+\s+false)"
                )
                .unwrap()
            })
            .is_match(&normalized)
}

fn strip_leading_office_metadata_text(text: &str) -> String {
    static OFFICE_METADATA_PREFIX_RE: OnceLock<Regex> = OnceLock::new();

    let normalized = normalize_plain_text_layout(text);
    if normalized.is_empty() {
        return normalized;
    }

    // Strip CF_HTML header if present in this context
    let stripped_header = normalize_clipboard_plain_text(&normalized);
    if stripped_header.is_empty() {
        return String::new();
    }

    let lower = stripped_header.to_ascii_lowercase();
    if !(lower.contains("microsoftinternetexplorer") || lower.contains("documentnotspecified")) {
        return stripped_header;
    }

    let stripped = OFFICE_METADATA_PREFIX_RE
        .get_or_init(|| {
            Regex::new(
                r"(?is)^\s*(?:(?:\d+|false|true|[a-z]{2}(?:-[a-z]{2})?|x-none|normal|documentnotspecified|microsoftinternetexplorer\d*|[\d.]+(?:pt|px|磅))\s+)+"
            )
            .unwrap()
        })
        .replace(&stripped_header, "")
        .trim()
        .to_string();

    if stripped.is_empty() {
        stripped_header
    } else {
        stripped
    }
}

fn extract_renderable_html_region(html: &str) -> String {
    static BODY_RE: OnceLock<Regex> = OnceLock::new();
    static HEAD_RE: OnceLock<Regex> = OnceLock::new();

    let repaired = repair_html_fragment(html);
    let trimmed = repaired.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Some(start_idx) = trimmed.find("<!--StartFragment-->") {
        let start = start_idx + "<!--StartFragment-->".len();
        if let Some(end_rel) = trimmed[start..].find("<!--EndFragment-->") {
            return trimmed[start..start + end_rel].trim().to_string();
        }
    }

    // Fallback: If it has CF_HTML header but no markers, try to strip the header
    if looks_like_cf_html_header_text(trimmed) {
        let stripped = normalize_clipboard_plain_text(trimmed);
        if !stripped.is_empty() && stripped.len() < trimmed.len() {
            return stripped;
        }
    }

    if let Some(captures) = BODY_RE
        .get_or_init(|| Regex::new(r"(?is)<body\b[^>]*>([\s\S]*?)</body\s*>").unwrap())
        .captures(trimmed)
    {
        if let Some(body) = captures.get(1) {
            return body.as_str().trim().to_string();
        }
    }

    HEAD_RE
        .get_or_init(|| Regex::new(r"(?is)<head\b[\s\S]*?</head\s*>").unwrap())
        .replace_all(trimmed, " ")
        .trim()
        .to_string()
}

pub fn repair_html_fragment(html: &str) -> String {
    static MISSING_LEADING_TAG_RE: OnceLock<Regex> = OnceLock::new();

    let trimmed = html.trim();
    if trimmed.is_empty() || trimmed.starts_with('<') {
        return trimmed.to_string();
    }

    let tag_like = MISSING_LEADING_TAG_RE
        .get_or_init(|| {
            Regex::new(
                r"(?is)^(table|tbody|thead|tfoot|tr|td|th|colgroup|col|div|span|p|ul|ol|li|blockquote|pre|h[1-6]|meta|style|img|a)\b[^>]*>"
            )
            .unwrap()
        })
        .is_match(trimmed);

    if tag_like {
        format!("<{}", trimmed)
    } else {
        trimmed.to_string()
    }
}

fn strip_office_preview_noise(text: &str) -> String {
    static OFFICE_STYLE_BLOCK_RE: OnceLock<Regex> = OnceLock::new();
    static OFFICE_XML_BLOCK_RE: OnceLock<Regex> = OnceLock::new();
    static CONDITIONAL_COMMENT_RE: OnceLock<Regex> = OnceLock::new();
    static RENDERABLE_CONTENT_TAG_RE: OnceLock<Regex> = OnceLock::new();

    let mut processed = extract_renderable_html_region(text);
    if processed.trim().is_empty() {
        return processed.trim().to_string();
    }

    processed = OFFICE_XML_BLOCK_RE
        .get_or_init(|| Regex::new(r"(?is)<xml\b[\s\S]*?</xml>").unwrap())
        .replace_all(&processed, |caps: &regex::Captures| {
            let block = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
            if is_office_style_definition_text(block) {
                " ".to_string()
            } else {
                block.to_string()
            }
        })
        .into_owned();

    processed = OFFICE_STYLE_BLOCK_RE
        .get_or_init(|| Regex::new(r"(?is)<style\b[\s\S]*?</style>").unwrap())
        .replace_all(&processed, |caps: &regex::Captures| {
            let block = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
            if is_office_style_definition_text(block) {
                " ".to_string()
            } else {
                block.to_string()
            }
        })
        .into_owned();

    processed = CONDITIONAL_COMMENT_RE
        .get_or_init(|| Regex::new(r"(?is)<!--[\s\S]*?-->").unwrap())
        .replace_all(&processed, |caps: &regex::Captures| {
            let block = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
            if is_office_style_definition_text(block) {
                " ".to_string()
            } else {
                block.to_string()
            }
        })
        .into_owned();

    if let Some(renderable_match) = RENDERABLE_CONTENT_TAG_RE
        .get_or_init(|| {
            Regex::new(r"(?is)<(table|p|div|span|img|a|ul|ol|li|blockquote|pre|h[1-6])\b").unwrap()
        })
        .find(&processed)
    {
        let prefix = &processed[..renderable_match.start()];
        if is_office_style_definition_text(prefix) {
            processed = processed[renderable_match.start()..].to_string();
        }
    }

    processed.trim().to_string()
}

fn looks_like_html_fragment_shallow(text: &str) -> bool {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('<') {
        return true;
    }

    if looks_like_cf_html_header_text(trimmed) {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    [
        "table ", "tbody", "thead", "tfoot", "tr ", "td ", "th ", "col ", "colgroup", "div ",
        "span ", "p ", "meta ", "style ",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
        || (lower.contains("cellpadding=") && lower.contains("cellspacing="))
}

fn looks_like_html_fragment(text: &str) -> bool {
    let repaired = strip_office_preview_noise(text);
    looks_like_html_fragment_shallow(&repaired)
}

fn sanitize_rich_text_plain_text(text: &str) -> String {
    let normalized = normalize_plain_text_layout(text);
    if normalized.is_empty() {
        return normalized;
    }

    let stripped = strip_leading_office_metadata_text(&normalized);
    if is_office_style_definition_text(&collapse_preview_whitespace(&stripped)) {
        String::new()
    } else {
        stripped
    }
}

fn extract_plain_text_from_htmlish(text: &str) -> String {
    static BREAK_TAG_RE: OnceLock<Regex> = OnceLock::new();
    static TAG_RE: OnceLock<Regex> = OnceLock::new();

    let repaired = strip_office_preview_noise(text);
    if repaired.is_empty() {
        return String::new();
    }
    let with_breaks = BREAK_TAG_RE
        .get_or_init(|| {
            Regex::new(
                r"(?is)</?(?:br|p|div|li|tr|td|th|table|h[1-6]|section|article|ul|ol)\b[^>]*>",
            )
            .unwrap()
        })
        .replace_all(&repaired, "\n");
    let without_tags = TAG_RE
        .get_or_init(|| Regex::new(r"(?is)<[^>]+>").unwrap())
        .replace_all(with_breaks.as_ref(), " ");
    let collapsed = normalize_plain_text_layout(&decode_basic_html_entities(without_tags.as_ref()));
    let cleaned = strip_leading_office_metadata_text(&collapsed);
    if cleaned.is_empty() {
        return String::new();
    }
    if is_office_style_definition_text(&collapse_preview_whitespace(&cleaned)) {
        String::new()
    } else {
        cleaned
    }
}

fn looks_like_obsidian_callout_markdown(text: &str) -> bool {
    static OBSIDIAN_CALLOUT_RE: OnceLock<Regex> = OnceLock::new();

    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let first_non_empty = normalized
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");

    OBSIDIAN_CALLOUT_RE
        .get_or_init(|| Regex::new(r"(?i)^>\s*\[\![a-z0-9_-]+\](?:[+-])?(?:\s+.+)?$").unwrap())
        .is_match(first_non_empty)
}

fn source_app_likely_formats_rich_text(source_app: &str, source_app_path: Option<&str>) -> bool {
    let mut haystack = source_app.to_ascii_lowercase();
    if let Some(path) = source_app_path {
        if !haystack.is_empty() {
            haystack.push(' ');
        }
        haystack.push_str(&path.to_ascii_lowercase());
    }

    [
        "wps",
        "winword",
        "word",
        "excel",
        "powerpoint",
        "onenote",
        "outlook",
        "soffice",
        "libreoffice",
        "writer",
        "calc",
        "impress",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
}

fn plain_text_has_rich_html_signals(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();

    lower.contains("<!--startfragment-->")
        || lower.contains("<!--endfragment-->")
        || lower.contains("<html")
        || lower.contains("<body")
        || lower.contains("<meta")
        || lower.contains("<style")
        || lower.contains("mso-")
        || lower.contains("documentnotspecified")
        || lower.contains("microsoftinternetexplorer")
        || lower.contains("class=mso")
        || lower.contains("class=\"mso")
        || lower.contains("cellpadding=")
        || lower.contains("cellspacing=")
}

pub fn looks_like_cf_html_header_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("version:0.9")
        || (lower.contains("starthtml:") && lower.contains("startfragment:"))
}

pub fn normalize_clipboard_plain_text(text: &str) -> String {
    static INLINE_CF_HTML_HEADER_RE: OnceLock<Regex> = OnceLock::new();

    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    if !looks_like_cf_html_header_text(&normalized) {
        return normalized;
    }

    // Try parsing as CF_HTML first to get any legitimate fragments
    if let Some(html) = parse_cf_html(normalized.as_bytes()) {
        let plain = extract_plain_text_from_htmlish(&html);
        if !plain.trim().is_empty() {
            return plain;
        }
    }

    // Aggressively strip header metadata lines if present
    let mut lines = normalized.lines();
    let mut cleaned_lines = Vec::new();
    let mut in_header = true;

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if in_header {
            let lower = trimmed.to_lowercase();
            let is_header_key = lower.starts_with("version:")
                || lower.starts_with("starthtml:")
                || lower.starts_with("endhtml:")
                || lower.starts_with("startfragment:")
                || lower.starts_with("endfragment:")
                || lower.starts_with("sourceurl:");

            if is_header_key || trimmed.is_empty() {
                continue;
            }
            // First line that doesn't look like a header key ends the header
            in_header = false;
        }
        cleaned_lines.push(line);
    }

    let result = cleaned_lines.join("\n").trim().to_string();
    if !result.is_empty() && result != normalized {
        if looks_like_html_fragment(&result) {
            let plain = extract_plain_text_from_htmlish(&result);
            if !plain.trim().is_empty() {
                return plain;
            }
        }
        return result;
    }

    let stripped_inline = INLINE_CF_HTML_HEADER_RE
        .get_or_init(|| {
            Regex::new(
                r"(?is)\b(?:version:\s*[^\s]+|starthtml:\s*\d+|endhtml:\s*\d+|startfragment:\s*\d+|endfragment:\s*\d+|sourceurl:\s*\S+)",
            )
            .unwrap()
        })
        .replace_all(&normalized, " ");
    let inline_result = normalize_plain_text_layout(stripped_inline.as_ref())
        .trim()
        .to_string();

    if inline_result.is_empty() {
        return normalized;
    }

    if looks_like_html_fragment(&inline_result) {
        let plain = extract_plain_text_from_htmlish(&inline_result);
        if !plain.trim().is_empty() {
            return plain;
        }
    }

    inline_result
}

pub fn infer_rich_html_from_plain_text(
    text: &str,
    source_app: &str,
    source_app_path: Option<&str>,
) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let html = parse_cf_html(trimmed.as_bytes())?;
    let plain_text = extract_plain_text_from_htmlish(&html);
    if plain_text.is_empty() {
        return None;
    }

    let normalized_source = collapse_preview_whitespace(trimmed);
    let normalized_plain = collapse_preview_whitespace(&plain_text);
    let materially_differs = normalized_source != normalized_plain;

    if plain_text_has_rich_html_signals(trimmed)
        || (source_app_likely_formats_rich_text(source_app, source_app_path) && materially_differs)
    {
        return Some(html);
    }

    None
}

pub fn derive_rich_text_content(content: &str, html_content: Option<&str>) -> String {
    let sanitized_plain = sanitize_rich_text_plain_text(content);
    if looks_like_obsidian_callout_markdown(&sanitized_plain) {
        return sanitized_plain;
    }

    let html_text = html_content
        .map(extract_plain_text_from_htmlish)
        .filter(|text| !text.is_empty());
    if let Some(text) = html_text {
        return text;
    }

    if looks_like_html_fragment(content) {
        let content_text = extract_plain_text_from_htmlish(content);
        if !content_text.is_empty() {
            return content_text;
        }
    }

    sanitized_plain
}

pub fn build_entry_preview(
    content_type: &str,
    content: &str,
    html_content: Option<&str>,
) -> String {
    if content_type == "image" {
        return "[Image Content]".to_string();
    }

    let preview_text = if content_type == "rich_text" {
        let clean_text = derive_rich_text_content(content, html_content);
        let preview = collapse_preview_whitespace(&clean_text);
        let normalized_content = collapse_preview_whitespace(content);

        if clean_text.is_empty()
            || preview.is_empty()
            || (html_content.is_none()
                && looks_like_html_fragment(content)
                && preview == normalized_content)
        {
            RICH_TEXT_PREVIEW_FALLBACK.to_string()
        } else {
            preview
        }
    } else {
        collapse_preview_whitespace(&normalize_clipboard_plain_text(content))
    };

    if preview_text.chars().count() > TEXT_PREVIEW_MAX_CHARS {
        let preview_text: String = preview_text
            .chars()
            .take(TEXT_PREVIEW_TRUNCATED_CHARS)
            .collect();
        format!("{}...", preview_text)
    } else {
        preview_text
    }
}

pub fn attach_rich_image_fallback(html: &str, payload: &str) -> String {
    let mut out = String::with_capacity(
        html.len()
            + RICH_IMAGE_FALLBACK_PREFIX.len()
            + RICH_IMAGE_FALLBACK_SUFFIX.len()
            + payload.len()
            + 1,
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

pub fn attach_rich_named_formats(
    html: &str,
    formats: &[crate::infrastructure::windows_api::win_clipboard::NamedClipboardFormat],
) -> String {
    let stored: Vec<StoredNamedClipboardFormat> = formats
        .iter()
        .filter(|format| !format.name.trim().is_empty() && !format.data.is_empty())
        .map(|format| StoredNamedClipboardFormat {
            name: format.name.clone(),
            data_base64: general_purpose::STANDARD.encode(&format.data),
        })
        .collect();

    if stored.is_empty() {
        return html.to_string();
    }

    let Ok(payload_json) = serde_json::to_vec(&stored) else {
        return html.to_string();
    };

    let payload = general_purpose::STANDARD.encode(payload_json);
    let mut out = String::with_capacity(
        html.len()
            + RICH_NAMED_FORMATS_PREFIX.len()
            + RICH_NAMED_FORMATS_SUFFIX.len()
            + payload.len()
            + 1,
    );
    out.push_str(html.trim_end());
    out.push('\n');
    out.push_str(RICH_NAMED_FORMATS_PREFIX);
    out.push_str(&payload);
    out.push_str(RICH_NAMED_FORMATS_SUFFIX);
    out
}

pub fn split_rich_html_and_named_formats(
    html: &str,
) -> (
    String,
    Vec<crate::infrastructure::windows_api::win_clipboard::NamedClipboardFormat>,
) {
    if let Some(start) = html.rfind(RICH_NAMED_FORMATS_PREFIX) {
        let marker_start = start + RICH_NAMED_FORMATS_PREFIX.len();
        if let Some(end_rel) = html[marker_start..].find(RICH_NAMED_FORMATS_SUFFIX) {
            let marker_end = marker_start + end_rel;
            let payload = html[marker_start..marker_end].trim();

            let decoded = general_purpose::STANDARD.decode(payload);
            let parsed = decoded
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Vec<StoredNamedClipboardFormat>>(&bytes).ok())
                .map(|items| {
                    items
                        .into_iter()
                        .filter_map(|item| {
                            let data = general_purpose::STANDARD.decode(item.data_base64).ok()?;
                            if item.name.trim().is_empty() || data.is_empty() {
                                return None;
                            }
                            Some(
                                crate::infrastructure::windows_api::win_clipboard::NamedClipboardFormat {
                                    name: item.name,
                                    data,
                                },
                            )
                        })
                        .collect::<Vec<_>>()
                });

            if let Some(formats) = parsed {
                let mut cleaned = String::with_capacity(html.len());
                cleaned.push_str(&html[..start]);
                cleaned.push_str(&html[marker_end + RICH_NAMED_FORMATS_SUFFIX.len()..]);
                return (cleaned.trim().to_string(), formats);
            }
        }
    }
    (html.to_string(), Vec::new())
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
        let base_html = if clean_html.trim().is_empty() {
            html
        } else {
            clean_html.as_str()
        };
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
    let repaired = repair_html_fragment(html);
    if repaired.trim().is_empty() {
        return None;
    }

    let (without_named_formats, named_formats) = split_rich_html_and_named_formats(&repaired);
    let (clean_html, image_fallback) = split_rich_html_and_image_fallback(&without_named_formats);
    let renderable_html = strip_office_preview_noise(&clean_html);
    let cleaned_repaired = repair_html_fragment(if renderable_html.trim().is_empty() {
        &clean_html
    } else {
        &renderable_html
    });
    let reattach_preview_metadata = |html: String| {
        let with_image = if let Some(payload) = image_fallback.as_deref() {
            attach_rich_image_fallback(&html, payload)
        } else {
            html
        };
        if named_formats.is_empty() {
            with_image
        } else {
            attach_rich_named_formats(&with_image, &named_formats)
        }
    };

    if cleaned_repaired.chars().count() <= HTML_PREVIEW_MAX_CHARS {
        return Some(reattach_preview_metadata(cleaned_repaired));
    }

    let trimmed = cleaned_repaired.trim();
    let lower = trimmed.to_ascii_lowercase();

    // Strategy 1: Table-based HTML — truncate by rows
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
            return Some(reattach_preview_metadata(slice.to_string()));
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

        return Some(reattach_preview_metadata(out));
    }

    // Strategy 2: Generic HTML — truncate at a safe tag boundary
    // Find the last '>' before the char limit to avoid cutting inside a tag.
    let limit = HTML_PREVIEW_MAX_CHARS;
    let byte_limit = trimmed
        .char_indices()
        .nth(limit)
        .map(|(idx, _)| idx)
        .unwrap_or(trimmed.len());
    let safe_end = trimmed[..byte_limit]
        .rfind('>')
        .map(|p| p + 1)
        .unwrap_or(byte_limit);
    let mut truncated = trimmed[..safe_end].to_string();
    truncated.push_str(HTML_TRUNCATION_SUFFIX);
    Some(reattach_preview_metadata(truncated))
}

#[cfg(test)]
mod tests {
    use super::{
        app_cleanup_policy_matches, apply_cleanup_rules, attach_rich_image_fallback,
        attach_rich_named_formats, build_entry_preview, collapse_preview_whitespace,
        derive_rich_text_content, extract_animated_image_data_url_from_html,
        extract_animated_image_data_url_from_text, extract_first_image_data_url_from_html,
        infer_rich_html_from_plain_text, normalize_clipboard_plain_text,
        parse_app_cleanup_policies, parse_cf_html, parse_cleanup_rules,
        split_rich_html_and_image_fallback, split_rich_html_and_named_formats,
        truncate_html_for_preview, AppCleanupPolicy, HTML_TRUNCATION_SUFFIX,
    };
    use base64::Engine;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_test_png_file(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("tiez_clip_utils_{}_{}", std::process::id(), unique));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4////fwAJ+wP9KobjigAAAABJRU5ErkJggg==")
            .unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    fn cleanup_test_path(path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }

    fn file_url_for(path: &Path) -> String {
        let raw = path.to_string_lossy().replace('\\', "/");
        if raw.starts_with('/') {
            format!("file://{}", raw)
        } else {
            format!("file:///{}", raw)
        }
    }

    #[test]
    fn rich_text_preview_prefers_readable_html_text() {
        let html = "<table><tr><td>Alpha</td><td>Beta</td></tr><tr><td>Gamma</td><td>Delta</td></tr></table>";
        let preview = build_entry_preview("rich_text", "table border=0 cellpadding=0", Some(html));

        assert_eq!(preview, "Alpha Beta Gamma Delta");
    }

    #[test]
    fn rich_text_preview_hides_markup_only_plain_text() {
        let preview = build_entry_preview(
            "rich_text",
            "table border=0 cellpadding=0 cellspacing=0 width=288",
            None,
        );

        assert_eq!(preview, "[Rich Text Content]");
    }

    #[test]
    fn rich_text_preview_strips_office_style_definition_noise() {
        let html = concat!(
            "Normal 0 false false false EN-US ZH-CN X-NONE ",
            "/* Style Definitions */ ",
            "table.MsoNormalTable {mso-style-name:普通表格; mso-style-noshow:yes;} ",
            "<table><tr><td>学院意见</td><td>通过</td></tr></table>"
        );

        let preview = build_entry_preview("rich_text", html, Some(html));

        assert_eq!(preview, "学院意见 通过");
    }

    #[test]
    fn rich_text_content_prefers_renderable_html_over_wps_plain_text_noise() {
        let text =
            "1 1 1 1 MicrosoftInternetExplorer4 0 2 DocumentNotSpecified 7.8 磅 Normal 0 顶顶顶顶";
        let html = "<html><head><meta charset=\"utf-8\"><style>body{font-family:\"Times New Roman\";}</style></head><body><p>顶顶顶顶</p></body></html>";

        let content = derive_rich_text_content(text, Some(html));

        assert_eq!(content, "顶顶顶顶");
    }

    #[test]
    fn rich_text_preview_ignores_wps_body_metadata_prefix() {
        let html = "<html><body>1 1 1 1 MicrosoftInternetExplorer4 0 2 DocumentNotSpecified 7.8 磅 Normal 0 <span>顶顶顶顶</span></body></html>";

        let preview = build_entry_preview("rich_text", html, Some(html));

        assert_eq!(preview, "顶顶顶顶");
    }

    #[test]
    fn rich_text_content_preserves_obsidian_callout_markdown() {
        let text = "> [!note]- Important\n> Keep the markdown callout syntax";
        let html =
            "<blockquote><p>Important</p><p>Keep the markdown callout syntax</p></blockquote>";

        let content = derive_rich_text_content(text, Some(html));

        assert_eq!(content, text);
    }

    #[test]
    fn infer_rich_html_from_plain_text_promotes_wps_table_fragment() {
        let text =
            "table border=0 cellpadding=0 cellspacing=0><tr><td>学院意见</td><td>通过</td></tr>";

        let html = infer_rich_html_from_plain_text(
            text,
            "WPS Office",
            Some("C:\\Program Files\\Kingsoft\\wps.exe"),
        )
        .expect("wps html-ish text should promote to rich html");

        assert!(html.starts_with("<table"));
        assert!(html.contains("<td>学院意见</td>"));
        assert_eq!(
            collapse_preview_whitespace(&derive_rich_text_content(text, Some(&html))),
            "学院意见 通过"
        );
    }

    #[test]
    fn infer_rich_html_from_plain_text_keeps_html_source_from_editor_as_code() {
        let text = "<div class=\"note\">hello</div>";

        let html = infer_rich_html_from_plain_text(
            text,
            "Visual Studio Code",
            Some("C:\\Program Files\\Microsoft VS Code\\Code.exe"),
        );

        assert!(html.is_none());
    }

    #[test]
    fn table_html_preview_keeps_valid_table_markup() {
        let row = "<tr><td>WPS</td><td>Preview</td><td>Cell</td></tr>";
        let html = format!(
            "<table border=0 cellpadding=0 cellspacing=0 style='border-collapse:collapse'>{}</table>",
            row.repeat(120)
        );

        let truncated = truncate_html_for_preview(&html).expect("table preview should exist");

        assert!(truncated.starts_with("<table"));
        assert!(truncated.contains("WPS"));
        assert!(truncated.ends_with("</table>"));
    }

    #[test]
    fn mixed_html_preview_keeps_text_context_instead_of_images_only() {
        let html = format!(
            "<div class='card'><img src='https://example.com/card.jpg' alt='cover' /><h2>巴林牵头 阿拉伯国家在理会试图推动武力破局</h2><p>{}</p></div>",
            "后续描述".repeat(2000)
        );

        let truncated = truncate_html_for_preview(&html).expect("mixed html preview should exist");

        assert!(truncated.contains("<img"));
        assert!(truncated.contains("巴林牵头"));
        assert!(!truncated.starts_with("<div style=\"display:flex;flex-wrap:wrap;gap:4px;\">"));
    }

    #[test]
    fn truncated_html_preview_keeps_rich_image_fallback_marker() {
        let base_html = format!("<div><p>GIF 标题</p><p>{}</p></div>", "内容".repeat(3000));
        let html = attach_rich_image_fallback(
            &base_html,
            "data:image/gif;base64,R0lGODlhAQABAPAAAP///wAAACH5BAAAAAAALAAAAAABAAEAAAICRAEAOw==",
        );

        let truncated = truncate_html_for_preview(&html).expect("html preview should exist");
        let (cleaned, fallback) = split_rich_html_and_image_fallback(&truncated);

        assert!(cleaned.contains("GIF 标题"));
        assert_eq!(
            fallback.as_deref(),
            Some("data:image/gif;base64,R0lGODlhAQABAPAAAP///wAAACH5BAAAAAAALAAAAAABAAEAAAICRAEAOw==")
        );
    }

    #[test]
    fn html_preview_prefers_renderable_body_over_leading_head_noise() {
        let html = format!(
            "<html><head><style>{}</style></head><body><div><p>真正可见的网页内容</p></div></body></html>",
            "x".repeat(7000)
        );

        let truncated = truncate_html_for_preview(&html).expect("html preview should exist");

        assert!(truncated.contains("真正可见的网页内容"));
        assert_ne!(truncated.trim(), HTML_TRUNCATION_SUFFIX);
    }

    #[test]
    fn rich_named_formats_round_trip_without_touching_html() {
        let html = "<table><tr><td>A</td><td>B</td></tr></table>";
        let formats = vec![
            crate::infrastructure::windows_api::win_clipboard::NamedClipboardFormat {
                name: "Rich Text Format".to_string(),
                data: b"{\\rtf1\\ansi A\\tab B}".to_vec(),
            },
            crate::infrastructure::windows_api::win_clipboard::NamedClipboardFormat {
                name: "Biff8".to_string(),
                data: vec![1, 2, 3, 4],
            },
        ];

        let tagged = attach_rich_named_formats(html, &formats);
        let (cleaned, restored) = split_rich_html_and_named_formats(&tagged);

        assert_eq!(cleaned, html);
        assert_eq!(restored, formats);
    }

    #[test]
    fn rich_named_formats_and_image_fallback_can_coexist() {
        let html = "<table><tr><td>Excel</td></tr></table>";
        let html = attach_rich_image_fallback(html, "data:image/png;base64,AAAA");
        let formats = vec![
            crate::infrastructure::windows_api::win_clipboard::NamedClipboardFormat {
                name: "Biff12".to_string(),
                data: vec![9, 8, 7],
            },
        ];

        let tagged = attach_rich_named_formats(&html, &formats);
        let (without_formats, restored_formats) = split_rich_html_and_named_formats(&tagged);
        let (cleaned, restored_image) = split_rich_html_and_image_fallback(&without_formats);

        assert_eq!(restored_formats, formats);
        assert_eq!(
            restored_image.as_deref(),
            Some("data:image/png;base64,AAAA")
        );
        assert_eq!(cleaned, "<table><tr><td>Excel</td></tr></table>");
    }

    #[test]
    fn extract_animated_image_data_url_from_html_prefers_data_gif() {
        let gif_data_url =
            "data:image/gif;base64,R0lGODlhAQABAPAAAP///wAAACH5BAAAAAAALAAAAAABAAEAAAICRAEAOw==";
        let html = format!(r#"<div><img src="{gif_data_url}" alt="gif" /></div>"#);

        let extracted = extract_animated_image_data_url_from_html(&html);

        assert_eq!(extracted.as_deref(), Some(gif_data_url));
    }

    #[test]
    fn extract_animated_image_data_url_from_html_ignores_static_png() {
        let html = r#"<div><img src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAUA" /></div>"#;

        let extracted = extract_animated_image_data_url_from_html(html);

        assert!(extracted.is_none());
    }

    #[test]
    fn extract_first_image_data_url_from_html_accepts_static_png_data_url() {
        let png_data_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAUA";
        let html = format!(r#"<div><img src="{png_data_url}" alt="png" /></div>"#);

        let extracted = extract_first_image_data_url_from_html(&html);

        assert_eq!(extracted.as_deref(), Some(png_data_url));
    }

    #[test]
    fn extract_first_image_data_url_from_html_reads_local_file_url() {
        let path = create_test_png_file("rich_local.png");
        let html = format!(
            r#"<div><img src="{}?v=1#preview" /></div>"#,
            file_url_for(&path)
        );

        let extracted = extract_first_image_data_url_from_html(&html);

        assert!(extracted
            .as_deref()
            .map(|value| value.starts_with("data:image/png;base64,"))
            .unwrap_or(false));

        cleanup_test_path(&path);
    }

    #[test]
    fn extract_animated_image_data_url_from_text_accepts_direct_gif_data_url() {
        let gif_data_url =
            "data:image/gif;base64,R0lGODlhAQABAPAAAP///wAAACH5BAAAAAAALAAAAAABAAEAAAICRAEAOw==";

        let extracted = extract_animated_image_data_url_from_text(gif_data_url);

        assert_eq!(extracted.as_deref(), Some(gif_data_url));
    }

    #[test]
    fn parse_cf_html_repairs_missing_opening_bracket() {
        let raw = b"Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n<!--StartFragment-->table border=0 cellpadding=0 cellspacing=0><tr><td>A</td></tr><!--EndFragment-->";
        let parsed = parse_cf_html(raw).expect("cf_html should parse");

        assert!(parsed.starts_with("<table"));
        assert!(parsed.contains("<td>A</td>"));
    }

    #[test]
    fn parse_cf_html_handles_fragment_offsets_without_line_break_separator() {
        let raw = b"Version:0.9\r\nStartHTML:0000000105\r\nEndHTML:0000000189\r\nStartFragment:0000000141EndFragment:0000000173\r\n<!--StartFragment--><p>Hello</p><!--EndFragment-->";
        let parsed = parse_cf_html(raw)
            .expect("cf_html should parse from markers when offsets are malformed");

        assert!(!parsed.contains("StartHTML:"));
        assert!(!parsed.contains("StartFragment:"));
        assert!(parsed.contains("<p>Hello</p>"));
    }

    #[test]
    fn parse_cf_html_does_not_return_raw_header_when_only_fragment_like_payload_survives() {
        let raw = b"Version:0.9\r\nStartHTML:0000000105\r\nEndHTML:0000000829\r\nStartFragment:0000000141EndFragment:0000000793\r\ntable border=0 cellpadding=0 cellspacing=0><tr><td>A</td></tr>";
        let parsed = parse_cf_html(raw).expect("cf_html should recover fragment-like payload");

        assert!(parsed.starts_with("<table"), "parsed={parsed:?}");
        assert!(parsed.contains("<td>A</td>"));
        assert!(!parsed.contains("Version:0.9"));
        assert!(!parsed.contains("StartHTML:"));
    }

    #[test]
    fn normalize_clipboard_plain_text_strips_cf_html_header_prefix() {
        let text = "Version:0.9 StartHTML:0000000105 EndHTML:0000000829 StartFragment:0000000141 EndFragment:0000000793 ddd";

        let normalized = normalize_clipboard_plain_text(text);

        assert_eq!(normalized, "ddd");
    }

    #[test]
    fn text_preview_drops_cf_html_header_noise_for_plain_text_items() {
        let text = "Version:0.9 StartHTML:0000000105 EndHTML:0000000829 StartFragment:0000000141 EndFragment:0000000793 ddd";

        let preview = build_entry_preview("text", text, None);

        assert_eq!(preview, "ddd");
    }

    #[test]
    fn cleanup_rules_parse_and_apply_replacements() {
        let rules = parse_cleanup_rules(
            r"(?i)token\s*:\s*\S+ => token: [REDACTED]
\b1[3-9]\d{9}\b => [PHONE]",
        );

        let cleaned = apply_cleanup_rules("token: abc123 13812345678", &rules);

        assert_eq!(cleaned, "token: [REDACTED] [PHONE]");
    }

    #[test]
    fn app_cleanup_policy_parse_filters_disabled_or_unbound_items() {
        let policies = parse_app_cleanup_policies(
            r#"[
                {"id":"1","enabled":true,"appName":"WeChat","contentTypes":["text"]},
                {"id":"2","enabled":false,"appName":"Slack","contentTypes":["text"]},
                {"id":"3","enabled":true,"contentTypes":["text"]}
            ]"#,
        );

        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].id, "1");
    }

    #[test]
    fn app_cleanup_policy_match_prefers_path_and_respects_content_type() {
        let policy = AppCleanupPolicy {
            id: "1".to_string(),
            enabled: true,
            app_name: "WeChat".to_string(),
            app_path: "C:\\Program Files\\Tencent\\WeChat.exe".to_string(),
            action: "ignore".to_string(),
            content_types: vec!["text".to_string(), "url".to_string()],
            cleanup_rules: String::new(),
        };

        assert!(app_cleanup_policy_matches(
            &policy,
            "Different Name",
            Some("C:\\Program Files\\Tencent\\WeChat.exe"),
            "text",
        ));
        assert!(!app_cleanup_policy_matches(
            &policy,
            "WeChat",
            Some("C:\\Program Files\\Tencent\\WeChat.exe"),
            "image",
        ));
    }

    #[test]
    fn app_cleanup_policy_match_accepts_executable_name_variant() {
        let policy = AppCleanupPolicy {
            id: "1".to_string(),
            enabled: true,
            app_name: "Codex".to_string(),
            app_path: String::new(),
            action: "clean".to_string(),
            content_types: vec!["text".to_string()],
            cleanup_rules: String::new(),
        };

        assert!(app_cleanup_policy_matches(
            &policy,
            "Codex.exe",
            Some(
                "C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.305.950.0_x64__2p2nqsd0c76g0\\app\\Codex.exe",
            ),
            "text",
        ));
    }

    #[test]
    fn app_cleanup_policy_match_accepts_windows_app_id_variant() {
        let policy = AppCleanupPolicy {
            id: "1".to_string(),
            enabled: true,
            app_name: "Codex".to_string(),
            app_path: "OpenAI.Codex_2p2nqsd0c76g0!App".to_string(),
            action: "clean".to_string(),
            content_types: vec!["text".to_string()],
            cleanup_rules: String::new(),
        };

        assert!(app_cleanup_policy_matches(
            &policy,
            "Codex.exe",
            Some(
                "C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.305.950.0_x64__2p2nqsd0c76g0\\app\\Codex.exe",
            ),
            "text",
        ));
    }
}

pub fn detect_content_type(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("www.") || trimmed.contains("://") && trimmed.split("://").next().map_or(false, |s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')) {
        return "url".to_string();
    }

    let mut score = 0;
    let keywords = [
        "import ",
        "const ",
        "let ",
        "var ",
        "function ",
        "class ",
        "pub fn ",
        "impl ",
        "#include",
        "package ",
        "interface ",
        "namespace ",
        "void ",
        "return ",
        "if (",
        "for (",
        "while (",
        "=>",
    ];

    for k in keywords {
        if text.contains(k) {
            score += 1;
        }
    }

    if text.contains(";") {
        score += 1;
    }
    if text.contains("{") && text.contains("}") {
        score += 1;
    }
    if text.contains("</") && text.contains(">") {
        score += 2;
    }

    if score >= 2 {
        return "code".to_string();
    }

    if trimmed.starts_with("{")
        && trimmed.ends_with("}")
        && text.contains(":")
        && text.contains("\"")
    {
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

    if text.len() > 5000 || text.starts_with("data:") {
        return false;
    }

    let has_kind = |k: &str| kinds.iter().any(|t| t == k);

    if has_kind("url") {
        let re = URL_RE.get_or_init(|| Regex::new(r"(?i)(?:[a-zA-Z][a-zA-Z0-9+\-.]*://|www\.)\S+").unwrap());
        if re.is_match(text) { return true; }
    }
    if has_kind("phone") {
        let re = PHONE_RE.get_or_init(|| {
            Regex::new(r"(?:\+?86)?[-\s\(]*1[3-9]\d{1}[-\s\)]*\d{4}[-\s]*\d{4}").unwrap()
        });
        if re.is_match(text) {
            return true;
        }
    }
    if has_kind("idcard") {
        let re = IDCARD_RE.get_or_init(|| {
            Regex::new(
                r"\b[1-9]\d{5}[1-9]\d{3}((0\d)|(1[0-2]))(([0|1|2]\d)|3[0-1])\d{3}([0-9Xx])\b",
            )
            .unwrap()
        });
        if re.is_match(text) {
            return true;
        }
    }
    if has_kind("email") {
        let re = EMAIL_RE
            .get_or_init(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap());
        if re.is_match(text) {
            return true;
        }
    }
    if has_kind("secret") {
        let re = SECRET_RE.get_or_init(|| Regex::new(r"(?ix)((?:sk|pk|ghp|gho|github_pat|AIza|AKIA|ya29)[-_][\w\-]{20,}|(?:password|secret|api[_-]?key|access[_-]?key|token|bearer)[\s:=]+[\w\-]{16,})").unwrap());
        if re.is_match(text) {
            return true;
        }
    }
    if has_kind("password") {
        if text.len() >= 8 && text.len() <= 64 && !text.contains(' ') && !text.contains('\n') {
            let has_upper = text.chars().any(|c| c.is_uppercase());
            let has_lower = text.chars().any(|c| c.is_lowercase());
            let has_digit = text.chars().any(|c| c.is_numeric());
            let has_special = text.chars().any(|c| !c.is_alphanumeric());
            if has_upper && has_lower && has_digit && has_special {
                return true;
            }
        }
    }

    for rule in custom_rules {
        if let Ok(re) = Regex::new(rule) {
            if re.is_match(text) {
                return true;
            }
        }
    }
    false
}

pub fn parse_cleanup_rules(raw_rules: &str) -> Vec<(Regex, String)> {
    raw_rules
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (pattern, replacement) = line.split_once("=>")?;
            let pattern = pattern.trim();
            if pattern.is_empty() {
                return None;
            }

            let replacement = replacement
                .trim()
                .replace(r"\n", "\n")
                .replace(r"\r", "\r")
                .replace(r"\t", "\t");

            Regex::new(pattern).ok().map(|regex| (regex, replacement))
        })
        .collect()
}

pub fn apply_cleanup_rules(text: &str, rules: &[(Regex, String)]) -> String {
    rules
        .iter()
        .fold(text.to_string(), |acc, (regex, replacement)| {
            regex.replace_all(&acc, replacement.as_str()).into_owned()
        })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCleanupPolicy {
    #[cfg_attr(not(test), allow(dead_code))]
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_policy_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub app_name: String,
    #[serde(default)]
    pub app_path: String,
    #[serde(default = "default_policy_action")]
    pub action: String,
    #[serde(default = "default_policy_content_types")]
    pub content_types: Vec<String>,
    #[serde(default)]
    pub cleanup_rules: String,
}

fn default_policy_enabled() -> bool {
    true
}

fn default_policy_action() -> String {
    "clean".to_string()
}

fn default_policy_content_types() -> Vec<String> {
    vec![
        "text".to_string(),
        "code".to_string(),
        "url".to_string(),
        "rich_text".to_string(),
        "image".to_string(),
        "file".to_string(),
        "video".to_string(),
    ]
}

fn normalize_executable_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let segment = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed).trim();
    if segment.is_empty() {
        return None;
    }

    let lower = segment.to_ascii_lowercase();
    let normalized = lower.strip_suffix(".exe").unwrap_or(&lower).trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

fn executable_name_matches(left: &str, right: &str) -> bool {
    match (
        normalize_executable_name(left),
        normalize_executable_name(right),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn app_id_matches_process_path(app_id: &str, process_path: &str) -> bool {
    let trimmed_app_id = app_id.trim();
    let trimmed_process_path = process_path.trim();
    if trimmed_app_id.is_empty() || trimmed_process_path.is_empty() || !trimmed_app_id.contains('!')
    {
        return false;
    }

    let package_family = trimmed_app_id.split('!').next().unwrap_or("").trim();
    let Some((package_name, publisher_id)) = package_family.rsplit_once('_') else {
        return false;
    };

    let normalized_path = trimmed_process_path.replace('/', "\\").to_ascii_lowercase();
    let package_name = package_name.trim().to_ascii_lowercase();
    let publisher_id = publisher_id.trim().to_ascii_lowercase();

    !package_name.is_empty()
        && !publisher_id.is_empty()
        && normalized_path.contains(&package_name)
        && normalized_path.contains(&publisher_id)
}

pub fn parse_app_cleanup_policies(raw_policies: &str) -> Vec<AppCleanupPolicy> {
    serde_json::from_str::<Vec<AppCleanupPolicy>>(raw_policies)
        .unwrap_or_default()
        .into_iter()
        .filter(|policy| {
            policy.enabled
                && (!policy.app_path.trim().is_empty() || !policy.app_name.trim().is_empty())
        })
        .collect()
}

pub fn app_cleanup_policy_matches(
    policy: &AppCleanupPolicy,
    source_app: &str,
    source_app_path: Option<&str>,
    content_type: &str,
) -> bool {
    let allowed = if policy.action.eq_ignore_ascii_case("ignore") {
        // If we are ignoring an app, we should be aggressive in matching unless types are specifically filtered
        policy.content_types.is_empty()
            || policy
                .content_types
                .iter()
                .any(|kind| kind.eq_ignore_ascii_case(content_type))
    } else {
        !policy.content_types.is_empty()
            && policy
                .content_types
                .iter()
                .any(|kind| kind.eq_ignore_ascii_case(content_type))
    };
    if !allowed {
        return false;
    }

    let source_app = source_app.trim();
    let source_app_path = source_app_path.unwrap_or("").trim();
    let policy_path = policy.app_path.trim();
    if !policy_path.is_empty() && !source_app_path.is_empty() {
        if policy_path.len() >= 2 && policy_path.starts_with('/') && policy_path.ends_with('/') {
            let re_str = &policy_path[1..policy_path.len() - 1];
            if let Ok(re) = Regex::new(re_str) {
                if re.is_match(source_app_path) {
                    return true;
                }
            }
        }
        if policy_path.eq_ignore_ascii_case(source_app_path) {
            return true;
        }
        if executable_name_matches(policy_path, source_app_path)
            || app_id_matches_process_path(policy_path, source_app_path)
        {
            return true;
        }
    }

    let policy_name = policy.app_name.trim();
    if !policy_name.is_empty() {
        if policy_name.len() >= 2 && policy_name.starts_with('/') && policy_name.ends_with('/') {
            let re_str = &policy_name[1..policy_name.len() - 1];
            if let Ok(re) = Regex::new(re_str) {
                if re.is_match(source_app) {
                    return true;
                }
            }
        }
        if policy_name.eq_ignore_ascii_case(source_app) {
            return true;
        }
        if executable_name_matches(policy_name, source_app)
            || executable_name_matches(policy_name, source_app_path)
        {
            return true;
        }
    }

    if !policy_path.is_empty()
        && !source_app.is_empty()
        && executable_name_matches(policy_path, source_app)
    {
        return true;
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

        let is_local = src.starts_with("file://")
            || (src.len() > 2
                && src.chars().nth(1) == Some(':')
                && (src.chars().nth(2) == Some('\\') || src.chars().nth(2) == Some('/')));

        if is_local {
            let path_str = if src.starts_with("file://") {
                let raw_path = src.trim_start_matches("file://");
                if raw_path.starts_with('/') && raw_path.chars().nth(2) == Some(':') {
                    &raw_path[1..]
                } else {
                    raw_path
                }
            } else {
                src
            };

            let decoded_path = decode(path_str)
                .map(|p| p.into_owned())
                .unwrap_or(path_str.to_string());
            let clean_path = decoded_path
                .split('?')
                .next()
                .unwrap_or(&decoded_path)
                .split('#')
                .next()
                .unwrap_or(&decoded_path);

            let path = std::path::Path::new(clean_path);
            if path.exists() {
                if let Ok(data) = std::fs::read(path) {
                    let ext = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("png")
                        .to_lowercase();
                    let mime = match ext.as_str() {
                        "jpg" | "jpeg" => "image/jpeg",
                        "gif" => "image/gif",
                        "webp" => "image/webp",
                        "bmp" => "image/bmp",
                        "svg" => "image/svg+xml",
                        _ => "image/png",
                    };
                    let b64 = general_purpose::STANDARD.encode(&data);
                    return format!(
                        "{}{}{}",
                        prefix,
                        format!("data:{};base64,{}", mime, b64),
                        suffix
                    );
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
    })
    .to_string()
}

pub fn process_local_images_in_html(html: &str, data_dir: &std::path::Path) -> String {
    let attachments_dir = data_dir.join("attachments");
    if !attachments_dir.exists() {
        let _ = std::fs::create_dir_all(&attachments_dir);
    }

    let re = match Regex::new(r#"(<img\s+[^>]*src=["'])([^"']+)(["'][^>]*>)"#) {
        Ok(r) => r,
        Err(_) => return html.to_string(),
    };

    re.replace_all(html, |caps: &regex::Captures| {
        let prefix = &caps[1];
        let src = &caps[2];
        let suffix = &caps[3];

        let is_local = src.starts_with("file://")
            || (src.len() > 2
                && src.chars().nth(1) == Some(':')
                && (src.chars().nth(2) == Some('\\') || src.chars().nth(2) == Some('/')));

        if is_local {
            let path_str = if src.starts_with("file://") {
                let raw_path = src.trim_start_matches("file://");
                if raw_path.starts_with('/') && raw_path.chars().nth(2) == Some(':') {
                    &raw_path[1..]
                } else {
                    raw_path
                }
            } else {
                src
            };

            let decoded_path = decode(path_str)
                .map(|p| p.into_owned())
                .unwrap_or(path_str.to_string());
            let clean_path = decoded_path
                .split('?')
                .next()
                .unwrap_or(&decoded_path)
                .split('#')
                .next()
                .unwrap_or(&decoded_path);
            let path = std::path::Path::new(clean_path);

            if path.starts_with(&attachments_dir) {
                return format!("{}{}{}", prefix, src, suffix);
            }

            if path.exists() {
                if let Ok(data) = std::fs::read(path) {
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    data.hash(&mut hasher);
                    let hash = hasher.finish();

                    let ext = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("png")
                        .to_lowercase();
                    let new_filename = format!("img_{:x}.{}", hash, ext);
                    let new_path = attachments_dir.join(&new_filename);

                    if !new_path.exists() {
                        let _ = std::fs::write(&new_path, &data);
                    }

                    let new_src = new_path.to_string_lossy().replace('\\', "/");
                    let final_src = if new_src.starts_with('/') {
                        format!("file://{}", new_src)
                    } else {
                        format!("file:///{}", new_src)
                    };
                    return format!("{}{}{}", prefix, final_src, suffix);
                }
            }
        }

        if let Some(remote_url) = normalize_remote_img_url(src) {
            if let Some((bytes, ext)) = fetch_remote_image(&remote_url) {
                if let Some(file_src) =
                    save_image_bytes_to_attachments(&bytes, ext, &attachments_dir)
                {
                    return format!("{}{}{}", prefix, file_src, suffix);
                }
            }
        }
        format!("{}{}{}", prefix, src, suffix)
    })
    .to_string()
}

pub fn parse_cf_html(raw: &[u8]) -> Option<String> {
    if raw.is_empty() {
        return None;
    }

    enum HtmlEncoding {
        Utf8,
        Utf16Le,
    }

    let detect_encoding = |data: &[u8]| -> HtmlEncoding {
        if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xFE {
            return HtmlEncoding::Utf16Le;
        }
        // Heuristic for UTF-16LE
        if data.len() >= 4 && data[1] == 0 && data[3] == 0 {
            return HtmlEncoding::Utf16Le;
        }
        HtmlEncoding::Utf8
    };

    let encoding = detect_encoding(raw);
    let raw_str = match encoding {
        HtmlEncoding::Utf8 => String::from_utf8_lossy(raw).to_string(),
        HtmlEncoding::Utf16Le => {
            let u16_buf: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16_buf)
        }
    };

    let parse_offset = |key: &str| -> Option<usize> {
        let idx = raw_str.find(key)?;
        let val_start = idx + key.len();
        let val_str: String = raw_str[val_start..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || c.is_whitespace())
            .collect();
        val_str.trim().parse::<usize>().ok()
    };

    let start_html = parse_offset("StartHTML:");
    let end_html = parse_offset("EndHTML:");
    let start_frag = parse_offset("StartFragment:");
    let end_frag = parse_offset("EndFragment:");

    // Prefer the full HTML range to preserve document-wide styles (CSS)
    let (s, e, is_full_doc) = if let (Some(s_h), Some(e_h)) = (start_html, end_html) {
        (s_h, e_h, true)
    } else if let (Some(s_f), Some(e_f)) = (start_frag, end_frag) {
        (s_f, e_f, false)
    } else {
        (0, 0, false)
    };

    if s < e {
        let content = match encoding {
            HtmlEncoding::Utf8 => {
                if e <= raw_str.len() {
                    Some(raw_str[s..e].to_string())
                } else {
                    None
                }
            }
            HtmlEncoding::Utf16Le => {
                if e <= raw.len() {
                    let u16_buf: Vec<u16> = raw[s..e]
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    Some(String::from_utf16_lossy(&u16_buf))
                } else {
                    None
                }
            }
        };

        if let Some(c) = content {
            if is_full_doc {
                return Some(c);
            } else {
                return Some(repair_html_fragment(&c));
            }
        }
    }

    // Fallback search for fragments if offsets failed or produced invalid results
    let start_marker = "<!--StartFragment-->";
    let end_marker = "<!--EndFragment-->";
    if let Some(s_idx) = raw_str.find(start_marker) {
        let after_s = s_idx + start_marker.len();
        if let Some(e_idx) = raw_str[after_s..].find(end_marker) {
            return Some(repair_html_fragment(&raw_str[after_s..after_s + e_idx]));
        }
    }

    // Last resort heuristics
    if raw_str.contains("Version:") {
        let mut in_header = true;
        let mut cleaned_lines = Vec::new();
        for line in raw_str.lines() {
            let trimmed = line.trim();
            if in_header {
                let lower = trimmed.to_ascii_lowercase();
                let is_header_key = lower.starts_with("version:")
                    || lower.starts_with("starthtml:")
                    || lower.starts_with("endhtml:")
                    || lower.starts_with("startfragment:")
                    || lower.starts_with("endfragment:")
                    || lower.starts_with("sourceurl:");

                if is_header_key || trimmed.is_empty() {
                    continue;
                }
                in_header = false;
            }
            cleaned_lines.push(line);
        }

        let cleaned = cleaned_lines.join("\n").trim().to_string();
        if looks_like_html_fragment_shallow(&cleaned) {
            return Some(repair_html_fragment(&cleaned));
        }

        if let Some(first_bracket) = raw_str.find('<') {
            let potential = &raw_str[first_bracket..];
            if looks_like_html_fragment_shallow(potential) {
                return Some(repair_html_fragment(potential));
            }
        }
    }

    if looks_like_html_fragment_shallow(&raw_str) {
        return Some(repair_html_fragment(&raw_str));
    }

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
