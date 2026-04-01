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
const TEXT_PREVIEW_MAX_CHARS: usize = 500;
const TEXT_PREVIEW_TRUNCATED_CHARS: usize = TEXT_PREVIEW_MAX_CHARS - 3;
const RICH_TEXT_PREVIEW_FALLBACK: &str = "[Rich Text Content]";
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

fn collapse_preview_whitespace(text: &str) -> String {
    static WHITESPACE_RE: OnceLock<Regex> = OnceLock::new();

    let normalized = text.replace("\r\n", "\n").replace('\r', "\n").replace('\n', " ");
    WHITESPACE_RE
        .get_or_init(|| Regex::new(r"\s+").unwrap())
        .replace_all(&normalized, " ")
        .trim()
        .to_string()
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
            if !lines.last().map(|last: &String| last.is_empty()).unwrap_or(false) {
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

    let lower = normalized.to_ascii_lowercase();
    if !(lower.contains("microsoftinternetexplorer") || lower.contains("documentnotspecified")) {
        return normalized;
    }

    let stripped = OFFICE_METADATA_PREFIX_RE
        .get_or_init(|| {
            Regex::new(
                r"(?is)^\s*(?:(?:\d+|false|true|[a-z]{2}(?:-[a-z]{2})?|x-none|normal|documentnotspecified|microsoftinternetexplorer\d*|[\d.]+(?:pt|px|磅))\s+)+"
            )
            .unwrap()
        })
        .replace(&normalized, "")
        .trim()
        .to_string();

    if stripped.is_empty() {
        normalized
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
        .get_or_init(|| Regex::new(r"(?is)<(table|p|div|span|img|a|ul|ol|li|blockquote|pre|h[1-6])\b").unwrap())
        .find(&processed)
    {
        let prefix = &processed[..renderable_match.start()];
        if is_office_style_definition_text(prefix) {
            processed = processed[renderable_match.start()..].to_string();
        }
    }

    processed.trim().to_string()
}

fn looks_like_html_fragment(text: &str) -> bool {
    let repaired = strip_office_preview_noise(text);
    let trimmed = repaired.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('<') {
        return true;
    }

    let lower = trimmed.to_ascii_lowercase();
    [
        "table ",
        "tbody",
        "thead",
        "tfoot",
        "tr ",
        "td ",
        "th ",
        "col ",
        "colgroup",
        "div ",
        "span ",
        "p ",
        "meta ",
        "style ",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
        || (lower.contains("cellpadding=") && lower.contains("cellspacing="))
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
            Regex::new(r"(?is)</?(?:br|p|div|li|tr|td|th|table|h[1-6]|section|article|ul|ol)\b[^>]*>")
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

pub fn derive_rich_text_content(content: &str, html_content: Option<&str>) -> String {
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

    sanitize_rich_text_plain_text(content)
}

pub fn build_entry_preview(content_type: &str, content: &str, html_content: Option<&str>) -> String {
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
        collapse_preview_whitespace(content)
    };

    if preview_text.chars().count() > TEXT_PREVIEW_MAX_CHARS {
        let preview_text: String = preview_text.chars().take(TEXT_PREVIEW_TRUNCATED_CHARS).collect();
        format!("{}...", preview_text)
    } else {
        preview_text
    }
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
    let repaired = repair_html_fragment(html);
    if repaired.trim().is_empty() {
        return None;
    }

    if repaired.chars().count() <= HTML_PREVIEW_MAX_CHARS {
        return Some(repaired);
    }

    let trimmed = repaired.trim();
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
            // Office/WPS table fragments may omit explicit </tr> tags. Returning the
            // intact table fragment is safer than chopping through markup and showing
            // raw HTML text in the list preview.
            return Some(slice.to_string());
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

        return Some(out);
    }

    Some(truncate_chars_with_suffix(trimmed, HTML_PREVIEW_MAX_CHARS, HTML_TRUNCATION_SUFFIX))
}

#[cfg(test)]
mod tests {
    use super::{build_entry_preview, derive_rich_text_content, parse_cf_html, truncate_html_for_preview};

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
        let text = "1 1 1 1 MicrosoftInternetExplorer4 0 2 DocumentNotSpecified 7.8 磅 Normal 0 顶顶顶顶";
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
    fn parse_cf_html_repairs_missing_opening_bracket() {
        let raw = b"Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n<!--StartFragment-->table border=0 cellpadding=0 cellspacing=0><tr><td>A</td></tr><!--EndFragment-->";
        let parsed = parse_cf_html(raw).expect("cf_html should parse");

        assert!(parsed.starts_with("<table"));
        assert!(parsed.contains("<td>A</td>"));
    }
}

pub fn detect_content_type(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("http") || trimmed.starts_with("www.") {
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

    if text.len() > 5000 || text.starts_with("data:") { return false; }

    let has_kind = |k: &str| kinds.iter().any(|t| t == k);

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
                repair_html_fragment(&fragment)
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
            return Some(repair_html_fragment(fragment));
        }
    }
    if looks_like_html_fragment(&raw_text) {
        return Some(repair_html_fragment(&raw_text));
    }
    None
}
