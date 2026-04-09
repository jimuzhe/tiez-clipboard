use crate::app::commands::file_cmd::{image_ext_from_mime, save_emoji_favorite_bytes_to_dir};
use crate::database::DbState;
use crate::domain::models::ClipboardEntry;
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use base64::Engine;
use regex::Regex;
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::sleep;
use urlencoding::decode;

const DEFAULT_INTERVAL_SECS: u64 = 120;
const MIN_INTERVAL_SECS: u64 = 5;
const MAX_INTERVAL_SECS: u64 = 3600;
const DEFAULT_SNAPSHOT_INTERVAL_MIN: i64 = 720;
const MIN_SNAPSHOT_INTERVAL_MIN: i64 = 5;
const MAX_SNAPSHOT_INTERVAL_MIN: i64 = 1440;
const SYNC_FETCH_PAGE_SIZE: i32 = 1000;
const DEFAULT_WEBDAV_BASE_PATH: &str = "tiez-sync";
const MAX_REMOTE_SNAPSHOTS: usize = 24;
const MAX_INLINE_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const RICH_IMAGE_FALLBACK_PREFIX: &str = "<!--TIEZ_RICH_IMAGE:";
const RICH_IMAGE_FALLBACK_SUFFIX: &str = "-->";
const WEBDAV_OP_BATCH_SIZE: usize = 400;
const EMOJI_FAVORITES_SETTING_KEY: &str = "app.emoji_favorites";
const CLOUD_SYNC_WEBDAV_LOCAL_SEQ_KEY: &str = "cloud_sync_webdav_local_seq";
const CLOUD_SYNC_WEBDAV_OP_CURSOR_MAP_KEY: &str = "cloud_sync_webdav_op_cursor_map";
const CLOUD_SYNC_WEBDAV_BLOB_CACHE_KEY: &str = "cloud_sync_webdav_blob_cache";
const CLOUD_SYNC_WEBDAV_LAST_SNAPSHOT_PUSH_AT_KEY: &str = "cloud_sync_webdav_last_snapshot_push_at";
const CLOUD_SYNC_WEBDAV_LAST_SNAPSHOT_PULL_AT_KEY: &str = "cloud_sync_webdav_last_snapshot_pull_at";
const CLOUD_SYNC_WEBDAV_LAST_HEAD_REBUILD_AT_KEY: &str = "cloud_sync_webdav_last_head_rebuild_at";
const BLOB_KIND_IMAGE: &str = "image";
const BLOB_KIND_CONTENT: &str = "content";
const BLOB_KIND_HTML: &str = "html";
const BLOB_THRESHOLD_CONTENT: usize = 12 * 1024;
const BLOB_THRESHOLD_HTML: usize = 24 * 1024;
const WEBDAV_REQUEST_TIMEOUT_SECS: u64 = 45;
const WEBDAV_MAX_RETRIES: usize = 3;
const WEBDAV_JSON_READ_RETRIES: usize = 3;
const WEBDAV_RETRY_BASE_DELAY_MS: u64 = 600;
const WEBDAV_HEAD_REBUILD_INTERVAL_SECS: i64 = 5 * 60;
const WEBDAV_HEAD_FILENAME: &str = "head.json";
const WEBDAV_BLOB_CACHE_MAX_ENTRIES: usize = 5000;

static CLOUD_SYNC_TASK_ACTIVE: AtomicBool = AtomicBool::new(false);
static CLOUD_SYNC_REQUESTED: AtomicBool = AtomicBool::new(false);
static CLOUD_SYNC_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
static CLOUD_SYNC_LAST_SYNC_AT: AtomicI64 = AtomicI64::new(0);
static LAST_PUSHED_EMOJI_HASH: AtomicI64 = AtomicI64::new(0);
static CLOUD_SYNC_BACKOFF_UNTIL: AtomicI64 = AtomicI64::new(0);


// 用于记录在本次运行中，哪些 WebDAV 目录已经确认存在，避免重复发网络请求
static WEBDAV_KNOWN_DIRS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloudSyncProvider {
    #[allow(dead_code)]
    Http,
    WebDav,
}

impl CloudSyncProvider {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::WebDav => "webdav",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncStatus {
    pub state: String, // disabled | idle | syncing | error
    pub running: bool,
    pub last_sync_at: Option<i64>,
    pub last_error: Option<String>,
    pub uploaded_items: usize,
    pub received_items: usize,
}

#[derive(Debug, Clone)]
struct CloudSyncConfig {
    enabled: bool,
    auto_sync: bool,
    provider: CloudSyncProvider,
    base_url: String,
    api_key: String,
    device_id: String,
    interval_secs: u64,
    snapshot_interval_secs: i64,
    cursor: i64,
    webdav_url: String,
    webdav_username: String,
    webdav_password: String,
    webdav_base_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudSyncItem {
    pub content_type: String,
    pub content: String,
    #[serde(default)]
    pub content_hash: i64,
    #[serde(default)]
    pub deleted_at: i64,
    #[serde(default)]
    pub html_content: Option<String>,
    #[serde(default)]
    pub content_blob_hash: Option<String>,
    #[serde(default)]
    pub html_blob_hash: Option<String>,
    pub source_app: String,
    pub timestamp: i64,
    pub preview: String,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub use_count: i32,
    #[serde(default)]
    pub pinned_order: i64,
}

#[derive(Debug, Serialize)]
struct CloudSyncRequest {
    device_id: String,
    cursor: i64,
    entries: Vec<CloudSyncItem>,
}

#[derive(Debug, Deserialize)]
struct CloudSyncResponse {
    #[serde(default)]
    cursor: Option<i64>,
    #[serde(default)]
    entries: Vec<CloudSyncItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WebDavDeviceSnapshot {
    device_id: String,
    updated_at: i64,
    #[serde(default)]
    latest_op_seq: i64,
    entries: Vec<CloudSyncItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WebDavSettingsSnapshot {
    device_id: String,
    updated_at: i64,
    settings: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct WebDavPaths {
    devices_path: String,
    settings_path: String,
    ops_path: String,
    head_path: String,
    blobs_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WebDavOpsBatch {
    device_id: String,
    seq: i64,
    updated_at: i64,
    entries: Vec<CloudSyncItem>,
}

#[derive(Debug, Clone)]
struct WebDavOpRef {
    device_id: String,
    seq: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct WebDavDeviceHead {
    #[serde(default)]
    latest_op_seq: i64,
    #[serde(default)]
    snapshot_updated_at: i64,
    #[serde(default)]
    snapshot_op_seq: i64,
    #[serde(default)]
    settings_updated_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct WebDavSyncHead {
    #[serde(default)]
    updated_at: i64,
    #[serde(default)]
    devices: BTreeMap<String, WebDavDeviceHead>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn status_store() -> &'static Mutex<CloudSyncStatus> {
    static STORE: OnceLock<Mutex<CloudSyncStatus>> = OnceLock::new();
    STORE.get_or_init(|| {
        Mutex::new(CloudSyncStatus {
            state: "disabled".to_string(),
            running: false,
            last_sync_at: None,
            last_error: None,
            uploaded_items: 0,
            received_items: 0,
        })
    })
}

fn sync_run_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn disabled_status() -> CloudSyncStatus {
    CloudSyncStatus {
        state: "disabled".to_string(),
        running: false,
        last_sync_at: None,
        last_error: None,
        uploaded_items: 0,
        received_items: 0,
    }
}

fn cloud_sync_cancel_requested() -> bool {
    CLOUD_SYNC_CANCEL_REQUESTED.load(Ordering::Relaxed)
}

fn emit_status(app: Option<&AppHandle>, mut next: CloudSyncStatus) {
    if next.last_sync_at.is_none() {
        let ts = CLOUD_SYNC_LAST_SYNC_AT.load(Ordering::Relaxed);
        if ts > 0 {
            next.last_sync_at = Some(ts);
        }
    }
    if let Ok(mut guard) = status_store().lock() {
        *guard = next.clone();
    }
    if let Some(handle) = app {
        let _ = handle.emit("cloud-sync-status", next);
    }
}

fn parse_interval_secs(raw: Option<String>) -> u64 {
    let parsed = raw
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_SECS);
    parsed.clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS)
}

fn parse_snapshot_interval_secs(raw: Option<String>) -> i64 {
    let parsed_min = raw
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_SNAPSHOT_INTERVAL_MIN)
        .clamp(MIN_SNAPSHOT_INTERVAL_MIN, MAX_SNAPSHOT_INTERVAL_MIN);
    parsed_min.saturating_mul(60)
}

fn normalize_webdav_base_path(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        DEFAULT_WEBDAV_BASE_PATH.to_string()
    } else {
        trimmed.to_string()
    }
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn get_blob_path(base_blobs: &str, kind: &str, hash: &str) -> String {
    let prefix = if hash.len() >= 2 { &hash[0..2] } else { "xx" };
    format!("{}/{}/{}_{}.blob", base_blobs, prefix, kind, hash)
}

fn blob_cache_storage_key(cfg: &CloudSyncConfig, relative_path: &str) -> String {
    format!(
        "{}|{}|{}",
        cfg.webdav_url.trim_end_matches('/'),
        normalize_webdav_base_path(&cfg.webdav_base_path),
        relative_path
    )
}

fn get_config(app: &AppHandle) -> Option<CloudSyncConfig> {
    let db_state = app.try_state::<DbState>()?;
    let enabled = db_state
        .settings_repo
        .get("cloud_sync_enabled")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    let auto_sync = db_state
        .settings_repo
        .get("cloud_sync_auto")
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true);

    // HTTP provider is intentionally disabled for now.
    // TODO: Restore provider switching after a real HTTP sync service is available.
    let provider = CloudSyncProvider::WebDav;

    let base_url = db_state
        .settings_repo
        .get("cloud_sync_server")
        .ok()
        .flatten()
        .unwrap_or_default()
        .trim()
        .to_string();

    let api_key = db_state
        .settings_repo
        .get("cloud_sync_api_key")
        .ok()
        .flatten()
        .unwrap_or_default();

    let cursor = db_state
        .settings_repo
        .get("cloud_sync_cursor")
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    let interval_secs = parse_interval_secs(
        db_state
            .settings_repo
            .get("cloud_sync_interval_sec")
            .ok()
            .flatten(),
    );
    let snapshot_interval_secs = parse_snapshot_interval_secs(
        db_state
            .settings_repo
            .get("cloud_sync_snapshot_interval_min")
            .ok()
            .flatten(),
    );

    let stored_device_id = db_state.settings_repo.get("app.anon_id").ok().flatten();
    let device_id = stored_device_id
        .as_deref()
        .and_then(crate::app::system::normalize_anon_id)
        .unwrap_or_else(
            || crate::app::system::build_anon_id(&crate::app::system::get_machine_id()),
        );
    let should_persist_device_id = stored_device_id
        .as_deref()
        .map(|value| value.trim() != device_id)
        .unwrap_or(true);
    let did_migrate_device_id = stored_device_id
        .as_deref()
        .map(|value| value.trim() != device_id)
        .unwrap_or(false);

    if should_persist_device_id {
        let _ = db_state.settings_repo.set("app.anon_id", &device_id);
    }
    if did_migrate_device_id {
        if let Ok(conn) = db_state.conn.lock() {
            let _ = conn.execute("DELETE FROM cloud_sync_local_index", []);
        }
    }

    let webdav_url = db_state
        .settings_repo
        .get("cloud_sync_webdav_url")
        .ok()
        .flatten()
        .unwrap_or_default()
        .trim()
        .to_string();
    let webdav_username = db_state
        .settings_repo
        .get("cloud_sync_webdav_username")
        .ok()
        .flatten()
        .unwrap_or_default();
    let webdav_password = db_state
        .settings_repo
        .get("cloud_sync_webdav_password")
        .ok()
        .flatten()
        .unwrap_or_default();
    let webdav_base_path = normalize_webdav_base_path(
        &db_state
            .settings_repo
            .get("cloud_sync_webdav_base_path")
            .ok()
            .flatten()
            .unwrap_or_else(|| DEFAULT_WEBDAV_BASE_PATH.to_string()),
    );

    Some(CloudSyncConfig {
        enabled,
        auto_sync,
        provider,
        base_url: base_url.clone(),
        api_key: api_key.clone(),
        device_id,
        interval_secs,
        snapshot_interval_secs,
        cursor,
        webdav_url: if webdav_url.is_empty() {
            base_url.clone()
        } else {
            webdav_url
        },
        webdav_username,
        webdav_password: if webdav_password.trim().is_empty() {
            api_key
        } else {
            webdav_password
        },
        webdav_base_path,
    })
}

fn is_syncable_content_type(content_type: &str) -> bool {
    matches!(
        content_type,
        "text" | "code" | "url" | "rich_text" | "image" | "emoji_sync"
    )
}

fn is_setting_sync_eligible(key: &str) -> bool {
    !matches!(
        key,
        "app.anon_id"
            | "app.emoji_favorites"
            | "app.last_ping_date"
            | "app.window_width"
            | "app.window_height"
            | "app.tag_manager_size"
            | "cloud_sync_enabled"
            | "cloud_sync_auto"
            | "cloud_sync_provider"
            | "cloud_sync_server"
            | "cloud_sync_api_key"
            | "cloud_sync_interval_sec"
            | "cloud_sync_snapshot_interval_min"
            | "cloud_sync_cursor"
            | "cloud_sync_webdav_url"
            | "cloud_sync_webdav_username"
            | "cloud_sync_webdav_password"
            | "cloud_sync_webdav_base_path"
            | "cloud_sync_webdav_local_seq"
            | "cloud_sync_webdav_op_cursor_map"
            | "cloud_sync_webdav_blob_cache"
            | "cloud_sync_webdav_last_snapshot_push_at"
            | "cloud_sync_webdav_last_snapshot_pull_at"
            | "cloud_sync_webdav_last_head_rebuild_at"
            | "cloud_sync_settings_applied_at"
    )
}

fn to_data_url_from_path(path: &str) -> Option<String> {
    let file_path = Path::new(path);
    if !file_path.exists() || !file_path.is_file() {
        return None;
    }

    let bytes = std::fs::read(file_path).ok()?;
    if bytes.is_empty() || bytes.len() > MAX_INLINE_IMAGE_BYTES {
        return None;
    }

    let mime = mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    let payload = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{};base64,{}", mime, payload))
}

fn rewrite_rich_fallback_payload_to_data_url(html: &str) -> String {
    let Some(start) = html.rfind(RICH_IMAGE_FALLBACK_PREFIX) else {
        return html.to_string();
    };
    let marker_start = start + RICH_IMAGE_FALLBACK_PREFIX.len();
    let Some(end_rel) = html[marker_start..].find(RICH_IMAGE_FALLBACK_SUFFIX) else {
        return html.to_string();
    };

    let marker_end = marker_start + end_rel;
    let payload = html[marker_start..marker_end].trim();
    if payload.is_empty()
        || payload.starts_with("data:image/")
        || payload.starts_with("http://asset.localhost/")
        || payload.starts_with("https://asset.localhost/")
    {
        return html.to_string();
    }

    let Some(data_url) = to_data_url_from_path(payload) else {
        return html.to_string();
    };

    format!(
        "{}{}{}",
        &html[..marker_start],
        data_url,
        &html[marker_end..]
    )
}

fn rich_html_resource_path_to_data_url(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty()
        || value.starts_with("data:")
        || value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("//")
        || value.starts_with("asset:")
        || value.starts_with("tauri:")
        || value.starts_with("blob:")
    {
        return None;
    }

    let path_raw = if value.starts_with("file://") {
        value.trim_start_matches("file://")
    } else {
        value
    };

    let path_without_drive_prefix =
        if path_raw.starts_with('/') && path_raw.chars().nth(2) == Some(':') {
            &path_raw[1..]
        } else {
            path_raw
        };

    let decoded_path = decode(path_without_drive_prefix)
        .map(|p| p.into_owned())
        .unwrap_or_else(|_| path_without_drive_prefix.to_string());
    let clean_path = decoded_path
        .split('?')
        .next()
        .unwrap_or(&decoded_path)
        .split('#')
        .next()
        .unwrap_or(&decoded_path)
        .trim();

    if clean_path.is_empty() {
        return None;
    }

    let is_absolute = clean_path.starts_with('/')
        || (clean_path.len() >= 3
            && clean_path.as_bytes()[1] == b':'
            && (clean_path.as_bytes()[2] == b'\\' || clean_path.as_bytes()[2] == b'/'));
    if !is_absolute {
        return None;
    }

    to_data_url_from_path(clean_path)
}

fn rewrite_rich_html_image_sources_to_data_url(html: &str) -> String {
    static IMG_SRC_RE: OnceLock<Regex> = OnceLock::new();
    let re = IMG_SRC_RE
        .get_or_init(|| Regex::new(r#"(?is)(<img\b[^>]*\bsrc=["'])([^"']+)(["'][^>]*>)"#).unwrap());

    re.replace_all(html, |caps: &regex::Captures| {
        let prefix = &caps[1];
        let src = &caps[2];
        let suffix = &caps[3];

        if let Some(data_url) = rich_html_resource_path_to_data_url(src) {
            format!("{}{}{}", prefix, data_url, suffix)
        } else {
            caps[0].to_string()
        }
    })
    .into_owned()
}

fn rewrite_rich_html_resources_for_sync(html: &str) -> String {
    let with_inline_images = rewrite_rich_html_image_sources_to_data_url(html);
    rewrite_rich_fallback_payload_to_data_url(&with_inline_images)
}

fn encode_emoji_favorites_setting(raw: &str) -> Option<String> {
    let paths: Vec<String> = serde_json::from_str(raw).ok()?;
    let encoded: Vec<String> = paths
        .into_iter()
        .filter_map(|path| to_data_url_from_path(path.trim()))
        .collect();
    serde_json::to_string(&encoded).ok()
}

fn materialize_emoji_favorite_paths(app: &AppHandle, raw: &str) -> AppResult<Vec<String>> {
    let items: Vec<String> = serde_json::from_str(raw)
        .map_err(|e| AppError::Validation(format!("invalid emoji favorites payload: {}", e)))?;
    let data_dir = get_app_data_dir(app)
        .ok_or_else(|| AppError::Internal("App data dir unavailable".to_string()))?;
    let mut saved_paths: Vec<String> = Vec::new();

    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with("data:") {
            let path = Path::new(trimmed);
            if path.is_file() {
                saved_paths.push(trimmed.to_string());
            }
            continue;
        }

        let (mime, bytes) = decode_data_url(trimmed)?;
        if bytes.is_empty() || bytes.len() > MAX_INLINE_IMAGE_BYTES {
            continue;
        }
        let ext = image_ext_from_mime(mime).ok_or_else(|| {
            AppError::Validation(format!("unsupported emoji mime type: {}", mime))
        })?;
        let path = save_emoji_favorite_bytes_to_dir(&data_dir, &bytes, ext)?;
        saved_paths.push(path);
    }

    saved_paths.sort();
    saved_paths.dedup();
    Ok(saved_paths)
}

fn decode_data_url(data_url: &str) -> AppResult<(&str, Vec<u8>)> {
    let Some(header_and_payload) = data_url.strip_prefix("data:") else {
        return Err(AppError::Validation("invalid data url".to_string()));
    };
    let Some((meta, payload)) = header_and_payload.split_once(',') else {
        return Err(AppError::Validation("invalid data url payload".to_string()));
    };
    if !meta.contains(";base64") {
        return Err(AppError::Validation(
            "unsupported data url encoding".to_string(),
        ));
    }
    let mime = meta.split(';').next().unwrap_or("").trim();
    if mime.is_empty() {
        return Err(AppError::Validation("missing mime type".to_string()));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| AppError::Validation(format!("invalid base64 payload: {}", e)))?;
    Ok((mime, bytes))
}

fn image_mime_from_bytes(bytes: &[u8]) -> Option<&'static str> {
    match image::guess_format(bytes).ok()? {
        image::ImageFormat::Png => Some("image/png"),
        image::ImageFormat::Jpeg => Some("image/jpeg"),
        image::ImageFormat::Gif => Some("image/gif"),
        image::ImageFormat::WebP => Some("image/webp"),
        image::ImageFormat::Bmp => Some("image/bmp"),
        _ => None,
    }
}

fn image_data_url_from_blob_bytes(bytes: &[u8]) -> Option<String> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        let trimmed = text.trim();
        if trimmed.starts_with("data:image/") {
            return Some(trimmed.to_string());
        }
    }

    let mime = image_mime_from_bytes(bytes)?;
    let payload = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{};base64,{}", mime, payload))
}

fn decode_emoji_favorites_setting(app: &AppHandle, raw: &str) -> AppResult<String> {
    let saved_paths = materialize_emoji_favorite_paths(app, raw)?;
    serde_json::to_string(&saved_paths)
        .map_err(|e| AppError::Internal(format!("serialize emoji favorites failed: {}", e)))
}

fn normalize_item_for_sync(mut item: CloudSyncItem) -> Option<CloudSyncItem> {
    if item.deleted_at > 0 {
        return Some(item);
    }

    if item.content_type == "image" && !item.content.starts_with("data:image/") {
        item.content = to_data_url_from_path(&item.content)?;
    }

    if item.content_type == "rich_text" {
        if let Some(html) = item.html_content.as_ref() {
            item.html_content = Some(rewrite_rich_html_resources_for_sync(html));
        }
    }

    Some(item)
}

fn compute_sync_content_hash(content_type: &str, content: &str) -> i64 {
    match content_type {
        "image" => crate::database::calc_image_hash(content).unwrap_or(0),
        "text" | "code" | "url" | "rich_text" => crate::database::calc_text_hash(content) as i64,
        _ => 0,
    }
}

fn resolved_content_hash(item: &CloudSyncItem) -> i64 {
    if item.content_hash != 0 {
        item.content_hash
    } else {
        compute_sync_content_hash(&item.content_type, &item.content)
    }
}

fn sync_key_for_item(item: &CloudSyncItem) -> Option<String> {
    let hash = resolved_content_hash(item);
    if hash == 0 {
        return None;
    }
    Some(format!("{}:{}", item.content_type, hash))
}

fn sync_digest_for_item(item: &CloudSyncItem) -> String {
    let tags_json = serde_json::to_string(&item.tags).unwrap_or_else(|_| "[]".to_string());
    let html_hash = item
        .html_content
        .as_ref()
        .map(|v| crate::database::calc_text_hash(v))
        .unwrap_or(0);
    let preview_hash = crate::database::calc_text_hash(&item.preview);
    let source_hash = crate::database::calc_text_hash(&item.source_app);
    let meta = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        resolved_content_hash(item),
        item.timestamp,
        item.deleted_at,
        item.is_pinned,
        item.pinned_order,
        item.use_count,
        html_hash,
        preview_hash,
        source_hash,
        crate::database::calc_text_hash(&tags_json)
    );
    crate::database::calc_text_hash(&meta).to_string()
}

async fn process_items_blobs_before_push(
    client: &reqwest::Client,
    cfg: &CloudSyncConfig,
    blobs_path: &str,
    blob_cache: &mut HashMap<String, i64>,
    items: &mut [CloudSyncItem],
) -> AppResult<()> {
    for item in items {
        if item.deleted_at > 0 {
            continue;
        }

        if item.content_type == "image" {
            if !item.content.starts_with("data:image/") {
                item.content = to_data_url_from_path(&item.content).ok_or_else(|| {
                    AppError::Internal("convert image path to data url failed".to_string())
                })?;
            }
            if !item.content.is_empty() {
                let relative_hash = sha256_hex(item.content.as_bytes());
                let relative = get_blob_path(blobs_path, BLOB_KIND_IMAGE, &relative_hash);
                let cache_key = blob_cache_storage_key(cfg, &relative);
                if !blob_cache.contains_key(&cache_key) {
                    upload_webdav_blob(
                        client,
                        cfg,
                        blobs_path,
                        BLOB_KIND_IMAGE,
                        item.content.as_bytes(),
                    )
                    .await?;
                }
                blob_cache.insert(cache_key, now_ms());
                let hash = relative_hash;
                item.content_blob_hash = Some(hash);
                item.content = String::new();
            }
        } else {
            let bytes = item.content.as_bytes();
            if bytes.len() > BLOB_THRESHOLD_CONTENT {
                let relative_hash = sha256_hex(bytes);
                let relative = get_blob_path(blobs_path, BLOB_KIND_CONTENT, &relative_hash);
                let cache_key = blob_cache_storage_key(cfg, &relative);
                if !blob_cache.contains_key(&cache_key) {
                    upload_webdav_blob(client, cfg, blobs_path, BLOB_KIND_CONTENT, bytes).await?;
                }
                blob_cache.insert(cache_key, now_ms());
                let hash = relative_hash;
                item.content_blob_hash = Some(hash);
                item.content = String::new();
            }
            if let Some(html) = item.html_content.as_ref() {
                let hbytes = html.as_bytes();
                if hbytes.len() > BLOB_THRESHOLD_HTML {
                    let relative_hash = sha256_hex(hbytes);
                    let relative = get_blob_path(blobs_path, BLOB_KIND_HTML, &relative_hash);
                    let cache_key = blob_cache_storage_key(cfg, &relative);
                    if !blob_cache.contains_key(&cache_key) {
                        upload_webdav_blob(client, cfg, blobs_path, BLOB_KIND_HTML, hbytes).await?;
                    }
                    blob_cache.insert(cache_key, now_ms());
                    let hash = relative_hash;
                    item.html_blob_hash = Some(hash);
                    item.html_content = None;
                }
            }
        }
    }
    Ok(())
}

async fn enrich_item_blobs_after_pull(
    app: &tauri::AppHandle,
    client: &reqwest::Client,
    cfg: &CloudSyncConfig,
    blobs_path: &str,
    items: &mut [CloudSyncItem],
) -> AppResult<()> {
    for item in items {
        if let Some(hash) = item.content_blob_hash.as_ref() {
            let kind = if item.content_type == "image" {
                BLOB_KIND_IMAGE
            } else {
                BLOB_KIND_CONTENT
            };
            let bytes = download_webdav_blob(client, cfg, blobs_path, kind, hash).await?;
            if item.content_type == "image" {
                let data_url = image_data_url_from_blob_bytes(&bytes).ok_or_else(|| {
                    AppError::Validation(format!("unsupported image blob payload: {}", hash))
                })?;
                if let Some(data_dir) = get_app_data_dir(app) {
                    if let Some(path) = crate::database::save_image_to_file(&data_url, &data_dir) {
                        item.content = path;
                    } else {
                        item.content = data_url;
                    }
                } else {
                    item.content = data_url;
                }
            } else {
                item.content = String::from_utf8(bytes).unwrap_or_default();
            }
        }

        if let Some(hash) = item.html_blob_hash.as_ref() {
            let bytes = download_webdav_blob(client, cfg, blobs_path, BLOB_KIND_HTML, hash).await?;
            item.html_content = Some(String::from_utf8(bytes).unwrap_or_default());
        }
    }
    Ok(())
}

fn collapse_items_by_sync_key(items: &[CloudSyncItem]) -> BTreeMap<String, CloudSyncItem> {
    let mut map: BTreeMap<String, CloudSyncItem> = BTreeMap::new();
    for item in items {
        let Some(key) = sync_key_for_item(item) else {
            continue;
        };
        let mut normalized = item.clone();
        normalized.content_hash = resolved_content_hash(item);

        let replace = map
            .get(&key)
            .map(|old| normalized.timestamp >= old.timestamp)
            .unwrap_or(true);
        if replace {
            map.insert(key, normalized);
        }
    }
    map
}

fn load_local_sync_index(app: &AppHandle) -> AppResult<HashMap<String, String>> {
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut stmt = conn
        .prepare("SELECT sync_key, digest FROM cloud_sync_local_index")
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut index = HashMap::new();
    for row in rows {
        let (k, v) = row.map_err(|e| AppError::Internal(e.to_string()))?;
        index.insert(k, v);
    }
    Ok(index)
}

fn replace_local_sync_index(
    app: &AppHandle,
    collapsed: &BTreeMap<String, CloudSyncItem>,
) -> AppResult<()> {
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    let mut conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    tx.execute("DELETE FROM cloud_sync_local_index", [])
        .map_err(|e| AppError::Internal(e.to_string()))?;
    for (sync_key, item) in collapsed {
        let digest = sync_digest_for_item(item);
        tx.execute(
            "INSERT INTO cloud_sync_local_index (sync_key, digest) VALUES (?1, ?2)",
            rusqlite::params![sync_key, digest],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
    }
    tx.commit().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(())
}

fn collect_local_incremental_items(
    app: &AppHandle,
    local_items: &[CloudSyncItem],
) -> AppResult<(Vec<CloudSyncItem>, BTreeMap<String, CloudSyncItem>)> {
    let collapsed = collapse_items_by_sync_key(local_items);
    let prev_index = load_local_sync_index(app)?;

    let mut deltas = Vec::new();
    for (sync_key, item) in &collapsed {
        let digest = sync_digest_for_item(item);
        let changed = prev_index
            .get(sync_key)
            .map(|old| old != &digest)
            .unwrap_or(true);
        if changed {
            deltas.push(item.clone());
        }
    }

    deltas.sort_by_key(|item| item.timestamp);
    Ok((deltas, collapsed))
}

fn get_setting_i64(app: &AppHandle, key: &str, default: i64) -> i64 {
    app.try_state::<DbState>()
        .and_then(|db| db.settings_repo.get(key).ok().flatten())
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
}

fn set_setting_i64(app: &AppHandle, key: &str, value: i64) {
    if let Some(db_state) = app.try_state::<DbState>() {
        let _ = db_state.settings_repo.set(key, &value.to_string());
    }
}

fn get_local_webdav_op_seq(app: &AppHandle) -> i64 {
    get_setting_i64(app, CLOUD_SYNC_WEBDAV_LOCAL_SEQ_KEY, 0)
}

fn set_local_webdav_op_seq(app: &AppHandle, seq: i64) {
    set_setting_i64(app, CLOUD_SYNC_WEBDAV_LOCAL_SEQ_KEY, seq);
}

fn load_webdav_op_cursor_map(app: &AppHandle) -> HashMap<String, i64> {
    let raw = app
        .try_state::<DbState>()
        .and_then(|db| {
            db.settings_repo
                .get(CLOUD_SYNC_WEBDAV_OP_CURSOR_MAP_KEY)
                .ok()
                .flatten()
        })
        .unwrap_or_default();
    if raw.trim().is_empty() {
        return HashMap::new();
    }
    serde_json::from_str::<HashMap<String, i64>>(&raw).unwrap_or_default()
}

fn save_webdav_op_cursor_map(app: &AppHandle, map: &HashMap<String, i64>) {
    if let Some(db_state) = app.try_state::<DbState>() {
        let payload = serde_json::to_string(map).unwrap_or_else(|_| "{}".to_string());
        let _ = db_state
            .settings_repo
            .set(CLOUD_SYNC_WEBDAV_OP_CURSOR_MAP_KEY, &payload);
    }
}

fn load_webdav_blob_cache(app: &AppHandle) -> HashMap<String, i64> {
    let raw = app
        .try_state::<DbState>()
        .and_then(|db| {
            db.settings_repo
                .get(CLOUD_SYNC_WEBDAV_BLOB_CACHE_KEY)
                .ok()
                .flatten()
        })
        .unwrap_or_default();
    if raw.trim().is_empty() {
        return HashMap::new();
    }
    serde_json::from_str::<HashMap<String, i64>>(&raw).unwrap_or_default()
}

fn save_webdav_blob_cache(app: &AppHandle, cache: &HashMap<String, i64>) {
    if let Some(db_state) = app.try_state::<DbState>() {
        let mut entries: Vec<(String, i64)> = cache.iter().map(|(k, v)| (k.clone(), *v)).collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        entries.truncate(WEBDAV_BLOB_CACHE_MAX_ENTRIES);
        let payload = serde_json::to_string(&entries.into_iter().collect::<HashMap<_, _>>())
            .unwrap_or_else(|_| "{}".to_string());
        let _ = db_state
            .settings_repo
            .set(CLOUD_SYNC_WEBDAV_BLOB_CACHE_KEY, &payload);
    }
}

fn get_app_data_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    let state = app.try_state::<crate::app_state::AppDataDir>()?;
    let guard = state.0.lock().ok()?;
    Some(guard.clone())
}

fn collect_local_syncable_items(app: &AppHandle) -> AppResult<Vec<CloudSyncItem>> {
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;

    let mut entries: Vec<ClipboardEntry> = Vec::new();
    let mut offset: i32 = 0;

    loop {
        let batch = db_state
            .repo
            .get_history(SYNC_FETCH_PAGE_SIZE, offset, None)
            .map_err(AppError::Internal)?;

        if batch.is_empty() {
            break;
        }

        let fetched = batch.len() as i32;
        entries.extend(
            batch
                .into_iter()
                .filter(|e| is_syncable_content_type(&e.content_type)),
        );
        offset = offset.saturating_add(fetched);
        if fetched < SYNC_FETCH_PAGE_SIZE {
            break;
        }
    }

    let mut items: Vec<CloudSyncItem> = entries
        .into_iter()
        .filter_map(|e| {
            let normalized = normalize_item_for_sync(CloudSyncItem {
                content_type: e.content_type,
                content: e.content,
                content_hash: 0,
                deleted_at: 0,
                html_content: e.html_content,
                content_blob_hash: None,
                html_blob_hash: None,
                source_app: e.source_app,
                timestamp: e.timestamp,
                preview: e.preview,
                is_pinned: e.is_pinned,
                tags: e.tags,
                use_count: e.use_count,
                pinned_order: e.pinned_order,
            })?;
            let mut item = normalized;
            item.content_hash = compute_sync_content_hash(&item.content_type, &item.content);
            Some(item)
        })
        .collect();

    let mut tombstones = collect_local_tombstones(app)?;
    items.append(&mut tombstones);
    items.sort_by_key(|e| e.timestamp);
    Ok(items)
}

fn collect_local_changes(app: &AppHandle, cursor: i64) -> AppResult<Vec<CloudSyncItem>> {
    let mut items = collect_local_syncable_items(app)?;
    items.retain(|e| e.timestamp > cursor);
    Ok(items)
}

fn collect_local_tombstones(app: &AppHandle) -> AppResult<Vec<CloudSyncItem>> {
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut stmt = conn
        .prepare(
            "SELECT content_type, content_hash, deleted_at
             FROM cloud_sync_tombstones
             ORDER BY deleted_at ASC",
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CloudSyncItem {
                content_type: row.get(0)?,
                content: String::new(),
                content_hash: row.get(1)?,
                deleted_at: row.get(2)?,
                html_content: None,
                content_blob_hash: None,
                html_blob_hash: None,
                source_app: "sync".to_string(),
                timestamp: row.get(2)?,
                preview: String::new(),
                is_pinned: false,
                tags: Vec::new(),
                use_count: 0,
                pinned_order: 0,
            })
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|e| AppError::Internal(e.to_string()))?);
    }
    Ok(items)
}

fn update_existing_entry_from_sync(
    conn: &rusqlite::Connection,
    id: i64,
    item: &CloudSyncItem,
    effective_timestamp: i64,
) -> AppResult<bool> {
    let (local_timestamp, local_is_pinned, local_pinned_order, local_preview, local_source_app, local_use_count, local_tags_json, local_source_app_path, local_is_external): (i64, bool, i64, String, String, i32, String, Option<String>, bool) = conn
        .query_row(
            "SELECT timestamp, is_pinned, pinned_order, preview, source_app, use_count, tags, source_app_path, is_external FROM clipboard_history WHERE id = ?",
            rusqlite::params![id],
            |row| Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7).unwrap_or(None),
                row.get(8).unwrap_or(false),
            )),
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut changed = false;
    let mut timestamp = local_timestamp;
    let mut is_pinned = local_is_pinned;
    let mut pinned_order = local_pinned_order;
    let mut preview = local_preview;
    let mut source_app = local_source_app;
    let mut use_count = local_use_count;
    let mut tags_json = local_tags_json.clone();
    let mut source_app_path = local_source_app_path;

    if effective_timestamp > local_timestamp {
        timestamp = effective_timestamp;
        changed = true;
    }
    if item.is_pinned != local_is_pinned {
        is_pinned = item.is_pinned;
        changed = true;
    }
    if item.pinned_order != local_pinned_order {
        pinned_order = item.pinned_order;
        changed = true;
    }
    if !item.preview.is_empty() && item.preview != preview {
        preview = item.preview.clone();
        changed = true;
    }
    if item.source_app != "sync" && item.source_app != source_app && !item.source_app.is_empty() {
        source_app = item.source_app.clone();
        source_app_path = None;
        changed = true;
    }
    if item.use_count > local_use_count {
        use_count = item.use_count;
        changed = true;
    }
    let remote_tags_json = serde_json::to_string(&item.tags).unwrap_or_else(|_| "[]".to_string());
    if remote_tags_json != tags_json {
        tags_json = remote_tags_json;
        changed = true;
    }
    let remote_is_external =
        item.content_type == "image" || item.content_type == "file" || item.content_type == "video";
    if remote_is_external != local_is_external {
        changed = true;
    }

    if changed {
        conn.execute(
            "UPDATE clipboard_history SET 
                timestamp = ?, 
                is_pinned = ?, 
                pinned_order = ?, 
                preview = ?, 
                source_app = ?, 
                use_count = ?, 
                tags = ?,
                source_app_path = ?,
                is_external = ?
             WHERE id = ?",
            rusqlite::params![
                timestamp,
                is_pinned,
                pinned_order,
                preview,
                source_app,
                use_count,
                tags_json,
                source_app_path,
                if remote_is_external { 1 } else { 0 },
                id
            ],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

        if tags_json != local_tags_json {
            conn.execute(
                "DELETE FROM entry_tags WHERE entry_id = ?",
                rusqlite::params![id],
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;
            for tag in &item.tags {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?, ?)",
                    rusqlite::params![id, tag],
                );
            }
        }
    }

    Ok(changed)
}

fn apply_remote_changes(app: &AppHandle, remote_items: &[CloudSyncItem]) -> AppResult<usize> {
    if remote_items.is_empty() {
        return Ok(0);
    }

    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    let mut applied = 0usize;
    let app_data_dir = get_app_data_dir(app);
    for item in remote_items {
        if item.content_type == "emoji_sync" {
            if let Err(e) = merge_remote_emojis(app, &item.content) {
                println!("Error merging remote emojis: {}", e);
            }
            applied += 1;
            continue;
        }

        if !is_syncable_content_type(&item.content_type) {
            continue;
        }

        let conn = db_state
            .conn
            .lock()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let effective_timestamp = if item.timestamp > 0 {
            item.timestamp
        } else {
            now_ms()
        };

        let remote_hash = if item.content_hash != 0 {
            item.content_hash
        } else {
            compute_sync_content_hash(&item.content_type, &item.content)
        };

        if item.deleted_at > 0 {
            if remote_hash == 0 {
                continue;
            }
            let tombstone_ts = item.deleted_at.max(effective_timestamp);
            let _ = conn.execute(
                "INSERT INTO cloud_sync_tombstones (content_type, content_hash, deleted_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(content_type, content_hash)
                 DO UPDATE SET deleted_at = MAX(cloud_sync_tombstones.deleted_at, excluded.deleted_at)",
                rusqlite::params![item.content_type, remote_hash, tombstone_ts],
            );

            let mut stmt = conn
                .prepare(
                    "SELECT id FROM clipboard_history
                     WHERE content_type = ?1 AND content_hash = ?2",
                )
                .map_err(|e| AppError::Internal(e.to_string()))?;
            let rows = stmt
                .query_map(rusqlite::params![item.content_type, remote_hash], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|e| AppError::Internal(e.to_string()))?;
            for row in rows {
                let id = row.map_err(|e| AppError::Internal(e.to_string()))?;
                db_state
                    .repo
                    .delete_with_conn(&conn, id, app_data_dir.as_deref())
                    .map_err(AppError::Internal)?;
                applied += 1;
            }
            continue;
        }

        if item.content.trim().is_empty() {
            continue;
        }

        if remote_hash != 0 {
            let tombstone_deleted_at = conn
                .query_row(
                    "SELECT deleted_at FROM cloud_sync_tombstones WHERE content_type = ?1 AND content_hash = ?2 LIMIT 1",
                    rusqlite::params![item.content_type, remote_hash],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0);
            if tombstone_deleted_at >= effective_timestamp.max(item.deleted_at) {
                continue;
            }
        }

        let existing = db_state
            .repo
            .find_by_content_with_conn(&conn, &item.content, Some(&item.content_type))
            .map_err(AppError::Internal)?;

        if let Some(id) = existing {
            if update_existing_entry_from_sync(&conn, id, item, effective_timestamp)? {
                applied += 1;
            }
            if remote_hash != 0 {
                let _ = conn.execute(
                    "DELETE FROM cloud_sync_tombstones
                     WHERE content_type = ?1 AND content_hash = ?2 AND deleted_at <= ?3",
                    rusqlite::params![item.content_type, remote_hash, effective_timestamp],
                );
            }
            continue;
        }

        let preview = if item.preview.trim().is_empty() {
            if item.content_type == "image" {
                "[Image Content]".to_string()
            } else {
                item.content.chars().take(200).collect::<String>()
            }
        } else {
            item.preview.clone()
        };

        let entry = ClipboardEntry {
            id: 0,
            content_type: item.content_type.clone(),
            content: item.content.clone(),
            html_content: item.html_content.clone(),
            source_app: item.source_app.clone(),
            source_app_path: None,
            timestamp: effective_timestamp,
            preview,
            is_pinned: item.is_pinned,
            tags: item.tags.clone(),
            use_count: item.use_count,
            is_external: item.content_type == "image"
                || item.content_type == "file"
                || item.content_type == "video",
            pinned_order: item.pinned_order,
            file_preview_exists: true,
        };

        db_state
            .repo
            .save_with_conn(&conn, &entry, app_data_dir.as_deref())
            .map_err(AppError::Internal)?;
        if remote_hash != 0 {
            let _ = conn.execute(
                "DELETE FROM cloud_sync_tombstones
                 WHERE content_type = ?1 AND content_hash = ?2 AND deleted_at <= ?3",
                rusqlite::params![item.content_type, remote_hash, effective_timestamp],
            );
        }
        applied += 1;
    }

    Ok(applied)
}

fn cloud_sync_target_ready(cfg: &CloudSyncConfig) -> bool {
    match cfg.provider {
        CloudSyncProvider::Http => !cfg.base_url.trim().is_empty(),
        CloudSyncProvider::WebDav => !cfg.webdav_url.trim().is_empty(),
    }
}

fn cloud_sync_target_not_ready_message(cfg: &CloudSyncConfig) -> String {
    match cfg.provider {
        CloudSyncProvider::Http => "cloud_sync_server is empty".to_string(),
        CloudSyncProvider::WebDav => "cloud_sync_webdav_url is empty".to_string(),
    }
}

fn build_http_client() -> AppResult<Client> {
    Client::builder()
        .timeout(Duration::from_secs(WEBDAV_REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Network(e.to_string()))
}

fn webdav_retry_delay(attempt: usize) -> Duration {
    let factor = 1u64 << attempt.min(4);
    Duration::from_millis(WEBDAV_RETRY_BASE_DELAY_MS.saturating_mul(factor))
}

fn is_retryable_webdav_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::REQUEST_TIMEOUT
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::GATEWAY_TIMEOUT
    )
}

fn check_webdav_status_for_backoff(status: StatusCode) {
    if matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE
    ) {
        // 进入 5 分钟冷却期，避免激怒坚果云导致封禁时间被无限延长
        let cooldown = now_ms() + 300 * 1000;
        CLOUD_SYNC_BACKOFF_UNTIL.store(cooldown, Ordering::Relaxed);
    }
}

async fn webdav_send_with_retry<F>(mut build_request: F) -> AppResult<Response>
where
    F: FnMut() -> RequestBuilder,
{
    let mut last_error = None;

    for attempt in 0..=WEBDAV_MAX_RETRIES {
        match build_request().send().await {
            Ok(resp) => {
                check_webdav_status_for_backoff(resp.status());
                if is_retryable_webdav_status(resp.status()) && attempt < WEBDAV_MAX_RETRIES {
                    last_error = Some(format!("transient WebDAV status {}", resp.status()));
                    sleep(webdav_retry_delay(attempt)).await;
                    continue;
                }
                return Ok(resp);
            }
            Err(err) => {
                last_error = Some(err.to_string());
                if attempt < WEBDAV_MAX_RETRIES {
                    sleep(webdav_retry_delay(attempt)).await;
                    continue;
                }
            }
        }
    }

    Err(AppError::Network(
        last_error.unwrap_or_else(|| "webdav request failed".to_string()),
    ))
}

fn webdav_with_auth(req: RequestBuilder, cfg: &CloudSyncConfig) -> RequestBuilder {
    if cfg.webdav_username.trim().is_empty() {
        req
    } else {
        req.basic_auth(cfg.webdav_username.trim(), Some(cfg.webdav_password.trim()))
    }
}

fn encode_webdav_relative_path(relative_path: &str, collection: bool) -> String {
    let mut encoded = relative_path
        .replace('\\', "/")
        .split('/')
        .filter_map(|segment| {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(urlencoding::encode(trimmed).into_owned())
            }
        })
        .collect::<Vec<_>>()
        .join("/");

    if collection && !encoded.is_empty() {
        encoded.push('/');
    }

    encoded
}

fn webdav_resource_url_for(cfg: &CloudSyncConfig, relative_path: &str) -> String {
    let encoded = encode_webdav_relative_path(relative_path, false);
    if encoded.is_empty() {
        cfg.webdav_url.trim_end_matches('/').to_string()
    } else {
        format!("{}/{}", cfg.webdav_url.trim_end_matches('/'), encoded)
    }
}

fn webdav_collection_url_for(cfg: &CloudSyncConfig, relative_path: &str) -> String {
    let encoded = encode_webdav_relative_path(relative_path, true);
    if encoded.is_empty() {
        format!("{}/", cfg.webdav_url.trim_end_matches('/'))
    } else {
        format!("{}/{}", cfg.webdav_url.trim_end_matches('/'), encoded)
    }
}

fn webdav_url_for(cfg: &CloudSyncConfig, relative_path: &str) -> String {
    webdav_resource_url_for(cfg, relative_path)
}

async fn webdav_collection_exists(
    client: &Client,
    cfg: &CloudSyncConfig,
    relative_path: &str,
) -> AppResult<bool> {
    let method = Method::from_bytes(b"PROPFIND")
        .map_err(|e| AppError::Internal(format!("invalid PROPFIND method: {}", e)))?;
    let url = webdav_collection_url_for(cfg, relative_path);
    let resp = webdav_send_with_retry(|| {
        webdav_with_auth(
            client
                .request(method.clone(), &url)
                .header("Depth", "0")
                .header("Content-Type", "application/xml; charset=utf-8"),
            cfg,
        )
    })
    .await?;

    Ok(resp.status().is_success() || resp.status().as_u16() == 207)
}

async fn mkcol_if_needed(
    client: &Client,
    cfg: &CloudSyncConfig,
    relative_path: &str,
) -> AppResult<()> {
    // 1. 生成唯一的缓存 Key（URL + 相对路径）
    let cache_key = format!("{}:{}", cfg.webdav_url, relative_path);
    // 2. 检查缓存中是否已经记录过该目录
    {
        let cache = WEBDAV_KNOWN_DIRS.get_or_init(|| Mutex::new(HashSet::new()));
        if cache.lock().unwrap().contains(&cache_key) {
            // 如果已在缓存中，直接返回成功，不产生任何网络请求
            return Ok(());
        }
    }

    let method = Method::from_bytes(b"MKCOL")
        .map_err(|e| AppError::Internal(format!("invalid MKCOL method: {}", e)))?;
    let url = webdav_collection_url_for(cfg, relative_path);
    let resp =
        webdav_send_with_retry(|| webdav_with_auth(client.request(method.clone(), &url), cfg))
            .await?;

    let code = resp.status().as_u16();
    if resp.status().is_success() {
        // 创建成功，写入缓存
        let cache = WEBDAV_KNOWN_DIRS.get_or_init(|| Mutex::new(HashSet::new()));
        cache.lock().unwrap().insert(cache_key);
        return Ok(());
    }

    if matches!(code, 301 | 302 | 307 | 308 | 405 | 409)
        && webdav_collection_exists(client, cfg, relative_path).await?
    {
        // 如果服务器反馈目录已存在 (405) 或者发生冲突 (409)，同样记录到缓存中
        let cache = WEBDAV_KNOWN_DIRS.get_or_init(|| Mutex::new(HashSet::new()));
        cache.lock().unwrap().insert(cache_key);
        return Ok(());
    }

    let text = resp.text().await.unwrap_or_default();
    Err(AppError::Network(format!(
        "webdav MKCOL failed for {}: {} {}",
        url, code, text
    )))
}

async fn delete_webdav_resource_if_exists(
    client: &Client,
    cfg: &CloudSyncConfig,
    relative_path: &str,
) -> AppResult<()> {
    let url = webdav_url_for(cfg, relative_path);
    let resp = webdav_send_with_retry(|| webdav_with_auth(client.delete(&url), cfg)).await?;

    if resp.status().is_success() || resp.status().as_u16() == 404 {
        return Ok(());
    }

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Err(AppError::Network(format!(
        "webdav DELETE cleanup failed for {}: {} {}",
        url, status, text
    )))
}

async fn move_webdav_resource(
    client: &Client,
    cfg: &CloudSyncConfig,
    from_relative: &str,
    to_relative: &str,
) -> AppResult<bool> {
    let from_url = webdav_url_for(cfg, from_relative);
    let destination = webdav_url_for(cfg, to_relative);
    let resp = webdav_send_with_retry(|| {
        let method = Method::from_bytes(b"MOVE").expect("MOVE is a valid HTTP method");
        webdav_with_auth(
            client
                .request(method, &from_url)
                .header("Destination", destination.clone())
                .header("Overwrite", "T"),
            cfg,
        )
    })
    .await?;

    if resp.status().is_success() {
        return Ok(true);
    }

    if matches!(resp.status().as_u16(), 405 | 409 | 412 | 501) {
        return Ok(false);
    }

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Err(AppError::Network(format!(
        "webdav MOVE publish failed for {} -> {}: {} {}",
        from_url, destination, status, text
    )))
}

async fn upload_webdav_bytes_resource(
    client: &Client,
    cfg: &CloudSyncConfig,
    relative_path: &str,
    body: Vec<u8>,
    content_type: &str,
    label: &str,
) -> AppResult<()> {
    async fn upload_target(
        client: &Client,
        cfg: &CloudSyncConfig,
        url: &str,
        payload: &[u8],
        content_type: &str,
        label: &str,
    ) -> AppResult<()> {
        let url_owned = url.to_string();
        let payload_owned = payload.to_vec();
        let content_type_owned = content_type.to_string();

        let resp = webdav_send_with_retry(|| {
            webdav_with_auth(
                client
                    .put(&url_owned)
                    .header("Content-Type", &content_type_owned)
                    .body(payload_owned.clone()),
                cfg,
            )
        })
        .await?;

        if resp.status().is_success() {
            return Ok(());
        }

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(AppError::Network(format!(
            "webdav PUT {} failed: {} {}",
            label, status, text
        )))
    }

    let final_url = webdav_url_for(cfg, relative_path);
    let temp_relative = format!(
        "{}.uploading.{}.{}.tmp",
        relative_path.trim_end_matches('/'),
        cfg.device_id,
        now_ms()
    );
    let temp_url = webdav_url_for(cfg, &temp_relative);

    upload_target(client, cfg, &temp_url, &body, content_type, label).await?;

    match move_webdav_resource(client, cfg, &temp_relative, relative_path).await {
        Ok(true) => Ok(()),
        Ok(false) => {
            let fallback = upload_target(client, cfg, &final_url, &body, content_type, label).await;
            let _ = delete_webdav_resource_if_exists(client, cfg, &temp_relative).await;
            fallback
        }
        Err(err) => {
            let _ = delete_webdav_resource_if_exists(client, cfg, &temp_relative).await;
            Err(err)
        }
    }
}

async fn upload_webdav_json_resource(
    client: &Client,
    cfg: &CloudSyncConfig,
    relative_path: &str,
    body: Vec<u8>,
    label: &str,
) -> AppResult<()> {
    upload_webdav_bytes_resource(client, cfg, relative_path, body, "application/json", label).await
}

async fn fetch_webdav_json_resource<T, F>(
    mut make_request: F,
    missing_status: u16,
    fetch_error_label: &str,
    parse_error_label: &str,
) -> AppResult<Option<T>>
where
    T: for<'de> Deserialize<'de>,
    F: FnMut() -> RequestBuilder,
{
    for attempt in 0..=WEBDAV_JSON_READ_RETRIES {
        let resp = webdav_send_with_retry(|| make_request()).await?;

        let status_code = resp.status().as_u16();
        if status_code == missing_status {
            return Ok(None);
        }
        
        // 兼容坚果云：如果父目录不存在，GET 可能返回 409 Conflict (AncestorsNotFound)
        if status_code == 409 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Network(format!(
                "{}: {} {}",
                fetch_error_label, status, text
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Network(e.to_string()))?;
        match serde_json::from_slice::<T>(&bytes) {
            Ok(parsed) => return Ok(Some(parsed)),
            Err(err)
                if matches!(err.classify(), serde_json::error::Category::Eof)
                    && attempt < WEBDAV_JSON_READ_RETRIES =>
            {
                sleep(webdav_retry_delay(attempt)).await;
            }
            Err(err) => return Err(AppError::Network(format!("{}: {}", parse_error_label, err))),
        }
    }

    Err(AppError::Network(format!(
        "{}: exhausted retries",
        parse_error_label
    )))
}

async fn ensure_webdav_directories(
    client: &Client,
    cfg: &CloudSyncConfig,
) -> AppResult<WebDavPaths> {
    let base = normalize_webdav_base_path(&cfg.webdav_base_path);
    let mut current = String::new();

    let paths = WebDavPaths {
        devices_path: if base.is_empty() {
            "devices".into()
        } else {
            format!("{}/devices", base)
        },
        settings_path: if base.is_empty() {
            "settings".into()
        } else {
            format!("{}/settings", base)
        },
        ops_path: if base.is_empty() {
            "ops".into()
        } else {
            format!("{}/ops", base)
        },
        head_path: if base.is_empty() {
            WEBDAV_HEAD_FILENAME.into()
        } else {
            format!("{}/{}", base, WEBDAV_HEAD_FILENAME)
        },
        blobs_path: if base.is_empty() {
            "blobs".into()
        } else {
            format!("{}/blobs", base)
        },
    };

    // 注意：不再使用全局静态标识 WEBDAV_ROOT_INITIALIZED 来跳过初始化，
    // 因为这会导致在切换 WebDAV 配置时无法正确为新地址创建目录。
    // 性能优化现在完全依赖 WEBDAV_KNOWN_DIRS 缓存。

    for segment in base.split('/').filter(|s| !s.is_empty()) {
        current = if current.is_empty() {
            segment.to_string()
        } else {
            format!("{}/{}", current, segment)
        };
        mkcol_if_needed(client, cfg, &current).await?;
    }

    mkcol_if_needed(client, cfg, &paths.devices_path).await?;
    mkcol_if_needed(client, cfg, &paths.settings_path).await?;
    mkcol_if_needed(client, cfg, &paths.ops_path).await?;
    mkcol_if_needed(client, cfg, &paths.blobs_path).await?;


    Ok(paths)
}

async fn upload_webdav_blob(
    client: &Client,
    cfg: &CloudSyncConfig,
    base_blobs: &str,
    kind: &str,
    data: &[u8],
) -> AppResult<String> {
    let hash = sha256_hex(data);
    let prefix = if hash.len() >= 2 { &hash[0..2] } else { "xx" };
    let prefix_path = format!("{}/{}", base_blobs, prefix);
    mkcol_if_needed(client, cfg, &prefix_path).await?;

    let blob_file_path = get_blob_path(base_blobs, kind, &hash);
    upload_webdav_bytes_resource(
        client,
        cfg,
        &blob_file_path,
        data.to_vec(),
        "application/octet-stream",
        "blob",
    )
    .await?;
    Ok(hash)
}

async fn download_webdav_blob(
    client: &Client,
    cfg: &CloudSyncConfig,
    base_blobs: &str,
    kind: &str,
    hash: &str,
) -> AppResult<Vec<u8>> {
    let blob_file_path = get_blob_path(base_blobs, kind, hash);
    let url = webdav_url_for(cfg, &blob_file_path);
    let resp = webdav_with_auth(client.get(&url), cfg)
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(AppError::Network(format!(
            "webdav GET blob failed: {} ({})",
            resp.status(),
            blob_file_path
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    Ok(bytes.to_vec())
}

fn parse_webdav_snapshot_ids(xml: &str) -> Vec<String> {
    let Ok(re) = Regex::new(r"(?is)<[^>]*href[^>]*>\s*([^<]+)\s*</[^>]*href>") else {
        return Vec::new();
    };

    let mut ids = Vec::new();
    for caps in re.captures_iter(xml) {
        let Some(raw_match) = caps.get(1) else {
            continue;
        };
        let raw_href = raw_match.as_str().trim();
        if raw_href.is_empty() {
            continue;
        }

        let decoded_href = urlencoding::decode(raw_href)
            .map(|v| v.into_owned())
            .unwrap_or_else(|_| raw_href.to_string());

        let normalized = decoded_href.trim_end_matches('/');
        let Some(file_name) = normalized.rsplit('/').next() else {
            continue;
        };

        let Some(device_id) = file_name.strip_suffix(".json") else {
            continue;
        };
        if device_id.is_empty() {
            continue;
        }
        if ids.iter().any(|existing| existing == device_id) {
            continue;
        }
        ids.push(device_id.to_string());
    }
    ids
}

async fn upload_webdav_snapshot(
    client: &Client,
    cfg: &CloudSyncConfig,
    devices_path: &str,
    latest_op_seq: i64,
    local_items: &[CloudSyncItem],
) -> AppResult<()> {
    let snapshot = WebDavDeviceSnapshot {
        device_id: cfg.device_id.clone(),
        updated_at: now_ms(),
        latest_op_seq,
        entries: local_items.to_vec(),
    };
    let body = serde_json::to_vec(&snapshot)
        .map_err(|e| AppError::Internal(format!("serialize snapshot failed: {}", e)))?;

    let relative = format!(
        "{}/{}.json",
        devices_path.trim_end_matches('/'),
        cfg.device_id
    );
    upload_webdav_json_resource(client, cfg, &relative, body, "snapshot").await
}

async fn list_webdav_snapshot_ids(
    client: &Client,
    cfg: &CloudSyncConfig,
    devices_path: &str,
) -> AppResult<Vec<String>> {
    let method = Method::from_bytes(b"PROPFIND")
        .map_err(|e| AppError::Internal(format!("invalid PROPFIND method: {}", e)))?;
    let url = webdav_collection_url_for(cfg, devices_path);
    let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:getlastmodified />
  </d:prop>
</d:propfind>"#;

    let resp = webdav_send_with_retry(|| {
        webdav_with_auth(
            client
                .request(method.clone(), &url)
                .header("Depth", "1")
                .header("Content-Type", "application/xml; charset=utf-8")
                .body(body.to_string()),
            cfg,
        )
    })
    .await?;

    let status = resp.status();
    if !status.is_success() && status.as_u16() != 207 {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Network(format!(
            "webdav PROPFIND failed: {} {}",
            status, text
        )));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    Ok(parse_webdav_snapshot_ids(&text))
}

async fn fetch_webdav_snapshot(
    client: &Client,
    cfg: &CloudSyncConfig,
    devices_path: &str,
    device_id: &str,
) -> AppResult<Option<WebDavDeviceSnapshot>> {
    let relative = format!("{}/{}.json", devices_path.trim_end_matches('/'), device_id);
    let url = webdav_url_for(cfg, &relative);
    fetch_webdav_json_resource(
        || webdav_with_auth(client.get(&url), cfg),
        404,
        "webdav GET snapshot failed",
        "parse snapshot json failed",
    )
    .await
}

fn webdav_ops_filename(device_id: &str, seq: i64) -> String {
    format!("{}__{:020}.json", device_id, seq.max(0))
}

async fn upload_webdav_ops_batch(
    client: &Client,
    cfg: &CloudSyncConfig,
    ops_path: &str,
    seq: i64,
    entries: &[CloudSyncItem],
) -> AppResult<()> {
    let batch = WebDavOpsBatch {
        device_id: cfg.device_id.clone(),
        seq,
        updated_at: now_ms(),
        entries: entries.to_vec(),
    };
    let body = serde_json::to_vec(&batch)
        .map_err(|e| AppError::Internal(format!("serialize ops batch failed: {}", e)))?;
    let relative = format!(
        "{}/{}",
        ops_path.trim_end_matches('/'),
        webdav_ops_filename(&cfg.device_id, seq)
    );
    upload_webdav_json_resource(client, cfg, &relative, body, "ops batch").await
}

fn parse_webdav_op_refs(xml: &str) -> Vec<WebDavOpRef> {
    let Ok(re_href) = Regex::new(r"(?is)<[^>]*href[^>]*>\s*([^<]+)\s*</[^>]*href>") else {
        return Vec::new();
    };
    let Ok(re_file) = Regex::new(r"^(.+)__(\d+)\.json$") else {
        return Vec::new();
    };

    let mut refs: HashMap<String, WebDavOpRef> = HashMap::new();
    let _start = std::time::Instant::now();
    for caps in re_href.captures_iter(xml) {
        let Some(raw_match) = caps.get(1) else {
            continue;
        };
        let raw_href = raw_match.as_str().trim();
        if raw_href.is_empty() {
            continue;
        }

        let decoded_href = urlencoding::decode(raw_href)
            .map(|v| v.into_owned())
            .unwrap_or_else(|_| raw_href.to_string());
        let normalized = decoded_href.trim_end_matches('/');
        let Some(file_name) = normalized.rsplit('/').next() else {
            continue;
        };
        let Some(file_caps) = re_file.captures(file_name) else {
            continue;
        };
        let Some(device_id_match) = file_caps.get(1) else {
            continue;
        };
        let Some(seq_match) = file_caps.get(2) else {
            continue;
        };
        let Ok(seq) = seq_match.as_str().parse::<i64>() else {
            continue;
        };
        let device_id = device_id_match.as_str().to_string();
        let dedup_key = format!("{}:{}", device_id, seq);
        refs.entry(dedup_key)
            .or_insert(WebDavOpRef { device_id, seq });
    }

    let mut out: Vec<WebDavOpRef> = refs.into_values().collect();

    out.sort_by(|a, b| a.device_id.cmp(&b.device_id).then(a.seq.cmp(&b.seq)));
    out
}

async fn list_webdav_op_refs(
    client: &Client,
    cfg: &CloudSyncConfig,
    ops_path: &str,
) -> AppResult<Vec<WebDavOpRef>> {
    let method = Method::from_bytes(b"PROPFIND")
        .map_err(|e| AppError::Internal(format!("invalid PROPFIND method: {}", e)))?;
    let url = webdav_collection_url_for(cfg, ops_path);
    let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:getlastmodified />
  </d:prop>
</d:propfind>"#;

    let resp = webdav_send_with_retry(|| {
        webdav_with_auth(
            client
                .request(method.clone(), &url)
                .header("Depth", "1")
                .header("Content-Type", "application/xml; charset=utf-8")
                .body(body.to_string()),
            cfg,
        )
    })
    .await?;

    let status = resp.status();
    if !status.is_success() && status.as_u16() != 207 {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Network(format!(
            "webdav PROPFIND ops failed: {} {}",
            status, text
        )));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;

    Ok(parse_webdav_op_refs(&text))
}

async fn fetch_webdav_ops_batch(
    client: &Client,
    cfg: &CloudSyncConfig,
    ops_path: &str,
    op_ref: &WebDavOpRef,
) -> AppResult<Option<WebDavOpsBatch>> {
    let relative = format!(
        "{}/{}",
        ops_path.trim_end_matches('/'),
        webdav_ops_filename(&op_ref.device_id, op_ref.seq)
    );
    let url = webdav_url_for(cfg, &relative);

    fetch_webdav_json_resource(
        || webdav_with_auth(client.get(&url), cfg),
        404,
        "webdav GET ops batch failed",
        "parse ops batch json failed",
    )
    .await
}

fn collect_syncable_settings(app: &AppHandle) -> AppResult<HashMap<String, String>> {
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    let mut map = db_state.settings_repo.get_all().map_err(AppError::from)?;
    map.retain(|k, _| is_setting_sync_eligible(k));

    if let Some(raw) = map.get(EMOJI_FAVORITES_SETTING_KEY).cloned() {
        if let Some(encoded) = encode_emoji_favorites_setting(&raw) {
            map.insert(EMOJI_FAVORITES_SETTING_KEY.to_string(), encoded);
        }
    }

    Ok(map)
}

fn apply_synced_settings(app: &AppHandle, incoming: &HashMap<String, String>) -> AppResult<usize> {
    if incoming.is_empty() {
        return Ok(0);
    }
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    let current = db_state.settings_repo.get_all().map_err(AppError::from)?;
    let mut changed = 0usize;
    for (key, value) in incoming {
        if !is_setting_sync_eligible(key) {
            continue;
        }

        let normalized_value = if key == EMOJI_FAVORITES_SETTING_KEY {
            decode_emoji_favorites_setting(app, value)?
        } else {
            value.clone()
        };

        if current
            .get(key)
            .map(|v| v == &normalized_value)
            .unwrap_or(false)
        {
            continue;
        }
        db_state
            .settings_repo
            .set(key, &normalized_value)
            .map_err(AppError::from)?;
        changed += 1;
    }
    Ok(changed)
}

async fn upload_webdav_settings_snapshot(
    app: &AppHandle,
    client: &Client,
    cfg: &CloudSyncConfig,
    settings_path: &str,
) -> AppResult<HashMap<String, String>> {
    let local_settings = collect_syncable_settings(app)?;

    let snapshot = WebDavSettingsSnapshot {
        device_id: cfg.device_id.clone(),
        updated_at: now_ms(),
        settings: local_settings.clone(),
    };
    let body = serde_json::to_vec(&snapshot)
        .map_err(|e| AppError::Internal(format!("serialize settings snapshot failed: {}", e)))?;
    let relative = format!(
        "{}/{}.json",
        settings_path.trim_end_matches('/'),
        cfg.device_id
    );
    upload_webdav_json_resource(client, cfg, &relative, body, "settings snapshot").await?;
    Ok(local_settings)
}

async fn fetch_webdav_settings_snapshot(
    client: &Client,
    cfg: &CloudSyncConfig,
    settings_path: &str,
    device_id: &str,
) -> AppResult<Option<WebDavSettingsSnapshot>> {
    let relative = format!("{}/{}.json", settings_path.trim_end_matches('/'), device_id);
    let url = webdav_url_for(cfg, &relative);
    fetch_webdav_json_resource(
        || webdav_with_auth(client.get(&url), cfg),
        404,
        "webdav GET settings snapshot failed",
        "parse settings snapshot json failed",
    )
    .await
}

async fn pull_remote_settings_snapshot(
    app: &AppHandle,
    client: &Client,
    cfg: &CloudSyncConfig,
    settings_path: &str,
) -> AppResult<usize> {
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    let last_applied_ts = db_state
        .settings_repo
        .get("cloud_sync_settings_applied_at")
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    let ids = list_webdav_snapshot_ids(client, cfg, settings_path).await?;
    let mut latest: Option<WebDavSettingsSnapshot> = None;
    for device_id in ids.into_iter().take(MAX_REMOTE_SNAPSHOTS) {
        if crate::app::system::same_anon_id(&device_id, &cfg.device_id) {
            continue;
        }
        if let Some(snapshot) =
            fetch_webdav_settings_snapshot(client, cfg, settings_path, &device_id).await?
        {
            let replace = latest
                .as_ref()
                .map(|cur| snapshot.updated_at > cur.updated_at)
                .unwrap_or(true);
            if replace {
                latest = Some(snapshot);
            }
        }
    }

    let Some(snapshot) = latest else {
        return Ok(0);
    };
    if snapshot.updated_at <= last_applied_ts {
        return Ok(0);
    }

    let changed = apply_synced_settings(app, &snapshot.settings)?;
    db_state
        .settings_repo
        .set(
            "cloud_sync_settings_applied_at",
            &snapshot.updated_at.to_string(),
        )
        .map_err(AppError::from)?;
    Ok(changed)
}

fn should_rebuild_webdav_head(app: &AppHandle, now: i64) -> bool {
    let last = get_setting_i64(app, CLOUD_SYNC_WEBDAV_LAST_HEAD_REBUILD_AT_KEY, 0);
    should_run_periodic_snapshot(last, now, WEBDAV_HEAD_REBUILD_INTERVAL_SECS)
}

fn touch_webdav_head_rebuild_at(app: &AppHandle, now: i64) {
    set_setting_i64(app, CLOUD_SYNC_WEBDAV_LAST_HEAD_REBUILD_AT_KEY, now);
}

fn update_webdav_head_device<F>(head: &mut WebDavSyncHead, device_id: &str, mut update: F)
where
    F: FnMut(&mut WebDavDeviceHead),
{
    let entry = head.devices.entry(device_id.to_string()).or_default();
    update(entry);
}

async fn fetch_webdav_sync_head(
    client: &Client,
    cfg: &CloudSyncConfig,
    head_path: &str,
) -> AppResult<Option<WebDavSyncHead>> {
    let url = webdav_url_for(cfg, head_path);
    fetch_webdav_json_resource(
        || webdav_with_auth(client.get(&url), cfg),
        404,
        "webdav GET head failed",
        "parse head json failed",
    )
    .await
}

async fn upload_webdav_sync_head(
    client: &Client,
    cfg: &CloudSyncConfig,
    head_path: &str,
    head: &WebDavSyncHead,
) -> AppResult<()> {
    let body = serde_json::to_vec(head)
        .map_err(|e| AppError::Internal(format!("serialize head failed: {}", e)))?;
    upload_webdav_json_resource(client, cfg, head_path, body, "sync head").await
}

async fn rebuild_webdav_sync_head(
    client: &Client,
    cfg: &CloudSyncConfig,
    paths: &WebDavPaths,
) -> AppResult<WebDavSyncHead> {
    let mut head = WebDavSyncHead {
        updated_at: now_ms(),
        devices: BTreeMap::new(),
    };

    for op_ref in list_webdav_op_refs(client, cfg, &paths.ops_path).await? {
        update_webdav_head_device(&mut head, &op_ref.device_id, |device| {
            device.latest_op_seq = device.latest_op_seq.max(op_ref.seq);
        });
    }

    for device_id in list_webdav_snapshot_ids(client, cfg, &paths.devices_path).await? {
        let snapshot = fetch_webdav_snapshot(client, cfg, &paths.devices_path, &device_id).await?;
        let updated_at = snapshot
            .as_ref()
            .map(|snapshot| snapshot.updated_at)
            .unwrap_or(0);
        let snapshot_op_seq = snapshot
            .as_ref()
            .map(|snapshot| snapshot.latest_op_seq)
            .unwrap_or(0);
        update_webdav_head_device(&mut head, &device_id, |device| {
            device.latest_op_seq = device.latest_op_seq.max(snapshot_op_seq);
            device.snapshot_updated_at = device.snapshot_updated_at.max(updated_at);
            device.snapshot_op_seq = device.snapshot_op_seq.max(snapshot_op_seq);
        });
    }

    for device_id in list_webdav_snapshot_ids(client, cfg, &paths.settings_path).await? {
        let updated_at =
            fetch_webdav_settings_snapshot(client, cfg, &paths.settings_path, &device_id)
                .await?
                .map(|snapshot| snapshot.updated_at)
                .unwrap_or(0);
        update_webdav_head_device(&mut head, &device_id, |device| {
            device.settings_updated_at = device.settings_updated_at.max(updated_at);
        });
    }

    Ok(head)
}

async fn resolve_webdav_sync_head(
    app: &AppHandle,
    client: &Client,
    cfg: &CloudSyncConfig,
    paths: &WebDavPaths,
    now: i64,
) -> AppResult<WebDavSyncHead> {
    let fetched = fetch_webdav_sync_head(client, cfg, &paths.head_path).await?;
    let needs_rebuild = fetched.is_none() || should_rebuild_webdav_head(app, now);

    if !needs_rebuild {
        return Ok(fetched.unwrap_or_default());
    }

    match rebuild_webdav_sync_head(client, cfg, paths).await {
        Ok(mut rebuilt) => {
            rebuilt.updated_at = now_ms();
            if fetched.as_ref() != Some(&rebuilt) {
                upload_webdav_sync_head(client, cfg, &paths.head_path, &rebuilt).await?;
            }
            touch_webdav_head_rebuild_at(app, now);
            Ok(rebuilt)
        }
        Err(err) => {
            if let Some(existing) = fetched {
                Ok(existing)
            } else {
                Err(err)
            }
        }
    }
}

async fn pull_remote_webdav_ops_from_head(
    app: &AppHandle,
    client: &Client,
    cfg: &CloudSyncConfig,
    blobs_path: &str,
    ops_path: &str,
    head: &WebDavSyncHead,
) -> AppResult<(usize, bool)> {
    let mut cursor_map = load_webdav_op_cursor_map(app);
    let mut received = 0usize;
    let mut head_stale = false;

    for (device_id, device_head) in &head.devices {
        if crate::app::system::same_anon_id(device_id, &cfg.device_id) {
            continue;
        }
        if device_head.latest_op_seq <= 0 {
            continue;
        }

        let mut last_seq = cursor_map.get(device_id).copied().unwrap_or(0);
        if device_head.latest_op_seq <= last_seq {
            continue;
        }

        for seq in (last_seq + 1)..=device_head.latest_op_seq {
            if cloud_sync_cancel_requested() {
                return Ok((received, head_stale));
            }

            let op_ref = WebDavOpRef {
                device_id: device_id.clone(),
                seq,
            };
            match fetch_webdav_ops_batch(client, cfg, ops_path, &op_ref).await? {
                Some(mut batch) if batch.device_id == op_ref.device_id => {
                    enrich_item_blobs_after_pull(app, client, cfg, blobs_path, &mut batch.entries)
                        .await?;
                    received += apply_remote_changes(app, &batch.entries)?;
                    last_seq = last_seq.max(batch.seq).max(seq);
                    cursor_map.insert(device_id.clone(), last_seq);
                }
                Some(_) | None => {
                    head_stale = true;
                    break;
                }
            }
        }
    }

    save_webdav_op_cursor_map(app, &cursor_map);
    Ok((received, head_stale))
}

async fn pull_remote_webdav_snapshots_from_head(
    app: &AppHandle,
    client: &Client,
    cfg: &CloudSyncConfig,
    blobs_path: &str,
    devices_path: &str,
    head: &WebDavSyncHead,
) -> AppResult<usize> {
    let mut remote_items: Vec<CloudSyncItem> = Vec::new();
    let mut device_ids: Vec<(String, i64)> = head
        .devices
        .iter()
        .filter_map(|(device_id, device_head)| {
            if crate::app::system::same_anon_id(device_id, &cfg.device_id)
                || device_head.snapshot_updated_at <= 0
            {
                None
            } else {
                Some((device_id.clone(), device_head.snapshot_updated_at))
            }
        })
        .collect();

    device_ids.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    for (device_id, _) in device_ids.into_iter().take(MAX_REMOTE_SNAPSHOTS) {
        if cloud_sync_cancel_requested() {
            break;
        }
        if let Some(mut snapshot) =
            fetch_webdav_snapshot(client, cfg, devices_path, &device_id).await?
        {
            enrich_item_blobs_after_pull(app, client, cfg, blobs_path, &mut snapshot.entries)
                .await?;
            remote_items.extend(snapshot.entries);
        }
    }

    remote_items.sort_by_key(|item| item.timestamp);
    apply_remote_changes(app, &remote_items)
}

async fn pull_remote_settings_snapshot_from_head(
    app: &AppHandle,
    client: &Client,
    cfg: &CloudSyncConfig,
    settings_path: &str,
    head: &WebDavSyncHead,
) -> AppResult<usize> {
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    let last_applied_ts = db_state
        .settings_repo
        .get("cloud_sync_settings_applied_at")
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    let Some((device_id, latest_ts)) = head
        .devices
        .iter()
        .filter(|(device_id, device_head)| {
            !crate::app::system::same_anon_id(device_id, &cfg.device_id)
                && device_head.settings_updated_at > 0
        })
        .map(|(device_id, device_head)| (device_id.clone(), device_head.settings_updated_at))
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
    else {
        return Ok(0);
    };

    if latest_ts <= last_applied_ts {
        return Ok(0);
    }

    let Some(snapshot) =
        fetch_webdav_settings_snapshot(client, cfg, settings_path, &device_id).await?
    else {
        return Ok(0);
    };
    if snapshot.updated_at <= last_applied_ts {
        return Ok(0);
    }

    let changed = apply_synced_settings(app, &snapshot.settings)?;
    db_state
        .settings_repo
        .set(
            "cloud_sync_settings_applied_at",
            &snapshot.updated_at.to_string(),
        )
        .map_err(AppError::from)?;
    Ok(changed)
}

async fn cleanup_local_webdav_ops(
    client: &Client,
    cfg: &CloudSyncConfig,
    ops_path: &str,
    max_seq_to_delete: i64,
) -> AppResult<usize> {
    if max_seq_to_delete <= 0 {
        return Ok(0);
    }

    let refs = list_webdav_op_refs(client, cfg, ops_path).await?;
    let mut deleted = 0usize;
    for op_ref in refs {
        if !crate::app::system::same_anon_id(&op_ref.device_id, &cfg.device_id)
            || op_ref.seq > max_seq_to_delete
        {
            continue;
        }

        let relative = format!(
            "{}/{}",
            ops_path.trim_end_matches('/'),
            webdav_ops_filename(&op_ref.device_id, op_ref.seq)
        );
        delete_webdav_resource_if_exists(client, cfg, &relative).await?;
        deleted += 1;
    }

    Ok(deleted)
}

fn should_run_periodic_snapshot(last_ts: i64, now: i64, interval_secs: i64) -> bool {
    if last_ts <= 0 {
        return true;
    }
    now.saturating_sub(last_ts) >= interval_secs.saturating_mul(1000)
}

fn should_push_webdav_snapshot(app: &AppHandle, now: i64, snapshot_interval_secs: i64) -> bool {
    let last = get_setting_i64(app, CLOUD_SYNC_WEBDAV_LAST_SNAPSHOT_PUSH_AT_KEY, 0);
    should_run_periodic_snapshot(last, now, snapshot_interval_secs)
}

fn should_pull_webdav_snapshot(
    app: &AppHandle,
    now: i64,
    has_remote_op_cursor: bool,
    snapshot_interval_secs: i64,
) -> bool {
    let last = get_setting_i64(app, CLOUD_SYNC_WEBDAV_LAST_SNAPSHOT_PULL_AT_KEY, 0);
    if !has_remote_op_cursor {
        // Cold-start fallback for new peers without op cursors yet.
        return should_run_periodic_snapshot(last, now, (5 * 60).min(snapshot_interval_secs));
    }
    should_run_periodic_snapshot(last, now, snapshot_interval_secs)
}

async fn pull_remote_webdav_ops(
    app: &AppHandle,
    client: &Client,
    cfg: &CloudSyncConfig,
    ops_path: &str,
    blobs_path: &str,
) -> AppResult<usize> {
    let refs = list_webdav_op_refs(client, cfg, ops_path).await?;
    if refs.is_empty() {
        return Ok(0);
    }

    let mut cursor_map = load_webdav_op_cursor_map(app);
    let mut received = 0usize;
    let _total_refs = refs.len();
    for (_index, op_ref) in refs.into_iter().enumerate() {
        if cloud_sync_cancel_requested() {
            break;
        }
        if crate::app::system::same_anon_id(&op_ref.device_id, &cfg.device_id) {
            continue;
        }
        let last_seq = cursor_map.get(&op_ref.device_id).copied().unwrap_or(0);
        if op_ref.seq <= last_seq {
            continue;
        }

        if let Some(mut batch) = fetch_webdav_ops_batch(client, cfg, ops_path, &op_ref).await? {
            if batch.device_id != op_ref.device_id {
                continue;
            }
            if cloud_sync_cancel_requested() {
                break;
            }
            enrich_item_blobs_after_pull(app, client, cfg, blobs_path, &mut batch.entries).await?;
            received += apply_remote_changes(app, &batch.entries)?;
            let next_seq = batch.seq.max(op_ref.seq).max(last_seq);
            cursor_map.insert(op_ref.device_id.clone(), next_seq);
        }
    }
    save_webdav_op_cursor_map(app, &cursor_map);
    Ok(received)
}

async fn sync_once_http(app: &AppHandle, cfg: &CloudSyncConfig) -> AppResult<CloudSyncStatus> {
    let local_items = collect_local_changes(app, cfg.cursor)?;
    let endpoint = format!(
        "{}/api/v1/clipboard/sync",
        cfg.base_url.trim_end_matches('/')
    );
    let request = CloudSyncRequest {
        device_id: cfg.device_id.clone(),
        cursor: cfg.cursor,
        entries: local_items.clone(),
    };

    let client = build_http_client()?;
    let mut http_req = client.post(&endpoint).json(&request);
    if !cfg.api_key.trim().is_empty() {
        http_req = http_req.bearer_auth(cfg.api_key.trim());
    }

    let resp = http_req
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        let status_code = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Network(format!(
            "sync endpoint failed: {} {}",
            status_code, text
        )));
    }

    let body = resp
        .json::<CloudSyncResponse>()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;

    let received = apply_remote_changes(app, &body.entries)?;
    if received > 0 {
        let _ = app.emit("clipboard-changed", ());
    }
    let local_max = local_items
        .iter()
        .map(|x| x.timestamp)
        .max()
        .unwrap_or(cfg.cursor);
    let remote_max = body
        .entries
        .iter()
        .map(|x| x.timestamp)
        .max()
        .unwrap_or(cfg.cursor);
    let next_cursor = body
        .cursor
        .unwrap_or(cfg.cursor)
        .max(local_max)
        .max(remote_max);

    if let Some(db_state) = app.try_state::<DbState>() {
        let _ = db_state
            .settings_repo
            .set("cloud_sync_cursor", &next_cursor.to_string());
    }

    let now = now_ms();
    CLOUD_SYNC_LAST_SYNC_AT.store(now, Ordering::Relaxed);
    Ok(CloudSyncStatus {
        state: "idle".to_string(),
        running: true,
        last_sync_at: Some(now),
        last_error: None,
        uploaded_items: local_items.len(),
        received_items: received,
    })
}

async fn sync_once_webdav(
    app: &AppHandle,
    cfg: &CloudSyncConfig,
    force_snapshot: bool,
) -> AppResult<CloudSyncStatus> {
    if cloud_sync_cancel_requested() {
        return Ok(disabled_status());
    }
    let now = now_ms();
    let local_items = collect_local_syncable_items(app)?;
    let (delta_items, collapsed_index) = collect_local_incremental_items(app, &local_items)?;
    let client = build_http_client()?;
    let paths = ensure_webdav_directories(&client, cfg).await?;
    let mut sync_head = resolve_webdav_sync_head(app, &client, cfg, &paths, now).await?;
    let mut sync_head_dirty = false;
    let mut webdav_blob_cache = load_webdav_blob_cache(app);
    let should_pull_snapshot = force_snapshot
        || should_pull_webdav_snapshot(
            app,
            now,
            !load_webdav_op_cursor_map(app).is_empty(),
            cfg.snapshot_interval_secs,
        );
    let should_push_snapshot =
        force_snapshot || should_push_webdav_snapshot(app, now, cfg.snapshot_interval_secs);

    let mut uploaded_items = 0usize;
    if !delta_items.is_empty() {
        let mut next_seq = get_local_webdav_op_seq(app);
        let mut processed_delta = delta_items.clone();
        process_items_blobs_before_push(
            &client,
            cfg,
            &paths.blobs_path,
            &mut webdav_blob_cache,
            &mut processed_delta,
        )
        .await?;
        for chunk in processed_delta.chunks(WEBDAV_OP_BATCH_SIZE) {
            if cloud_sync_cancel_requested() {
                return Ok(disabled_status());
            }
            next_seq = next_seq.saturating_add(1);
            upload_webdav_ops_batch(&client, cfg, &paths.ops_path, next_seq, chunk).await?;
        }
        set_local_webdav_op_seq(app, next_seq);
        replace_local_sync_index(app, &collapsed_index)?;
        uploaded_items += delta_items.len();
        update_webdav_head_device(&mut sync_head, &cfg.device_id, |device| {
            device.latest_op_seq = device.latest_op_seq.max(next_seq);
        });
        sync_head_dirty = true;
    }

    if cloud_sync_cancel_requested() {
        return Ok(disabled_status());
    }

    let (mut received_items, head_stale) = pull_remote_webdav_ops_from_head(
        app,
        &client,
        cfg,
        &paths.blobs_path,
        &paths.ops_path,
        &sync_head,
    )
    .await?;
    if head_stale {
        let rebuilt = rebuild_webdav_sync_head(&client, cfg, &paths).await?;
        if rebuilt != sync_head {
            sync_head = rebuilt;
            sync_head.updated_at = now_ms();
            upload_webdav_sync_head(&client, cfg, &paths.head_path, &sync_head).await?;
            touch_webdav_head_rebuild_at(app, now);
        }
        received_items +=
            pull_remote_webdav_ops(app, &client, cfg, &paths.ops_path, &paths.blobs_path).await?;
        received_items += pull_remote_webdav_snapshots_from_head(
            app,
            &client,
            cfg,
            &paths.blobs_path,
            &paths.devices_path,
            &sync_head,
        )
        .await?;
    }

    // Incremental Emoji Sync check
    if let Ok(emoji_op) = check_and_create_emoji_sync_op(app) {
        if let Some(op) = emoji_op {
            let next_seq = get_local_webdav_op_seq(app).saturating_add(1);
            upload_webdav_ops_batch(&client, cfg, &paths.ops_path, next_seq, &[op]).await?;
            set_local_webdav_op_seq(app, next_seq);
            uploaded_items += 1;
            update_webdav_head_device(&mut sync_head, &cfg.device_id, |device| {
                device.latest_op_seq = device.latest_op_seq.max(next_seq);
            });
            sync_head_dirty = true;
        }
    }

    save_webdav_blob_cache(app, &webdav_blob_cache);

    if should_pull_snapshot {
        received_items += pull_remote_webdav_snapshots_from_head(
            app,
            &client,
            cfg,
            &paths.blobs_path,
            &paths.devices_path,
            &sync_head,
        )
        .await?;

        received_items += pull_remote_settings_snapshot_from_head(
            app,
            &client,
            cfg,
            &paths.settings_path,
            &sync_head,
        )
        .await?;
        set_setting_i64(app, CLOUD_SYNC_WEBDAV_LAST_SNAPSHOT_PULL_AT_KEY, now);
    }

    if should_push_snapshot {
        if cloud_sync_cancel_requested() {
            return Ok(disabled_status());
        }
        let latest_op_seq = get_local_webdav_op_seq(app);
        upload_webdav_snapshot(
            &client,
            cfg,
            &paths.devices_path,
            latest_op_seq,
            &local_items,
        )
        .await?;
        uploaded_items += local_items.len();
        update_webdav_head_device(&mut sync_head, &cfg.device_id, |device| {
            device.latest_op_seq = device.latest_op_seq.max(latest_op_seq);
            device.snapshot_updated_at = device.snapshot_updated_at.max(now_ms());
            device.snapshot_op_seq = device.snapshot_op_seq.max(latest_op_seq);
        });
        let local_settings =
            upload_webdav_settings_snapshot(app, &client, cfg, &paths.settings_path).await?;
        uploaded_items += local_settings.len();
        update_webdav_head_device(&mut sync_head, &cfg.device_id, |device| {
            device.settings_updated_at = device.settings_updated_at.max(now_ms());
        });
        sync_head_dirty = true;
        set_setting_i64(app, CLOUD_SYNC_WEBDAV_LAST_SNAPSHOT_PUSH_AT_KEY, now);
        let _ = cleanup_local_webdav_ops(&client, cfg, &paths.ops_path, latest_op_seq).await;
    }

    if sync_head_dirty {
        sync_head.updated_at = now_ms();
        upload_webdav_sync_head(&client, cfg, &paths.head_path, &sync_head).await?;
    }

    if received_items > 0 {
        let _ = app.emit("clipboard-changed", ());
    }
    CLOUD_SYNC_LAST_SYNC_AT.store(now, Ordering::Relaxed);

    if let Some(db_state) = app.try_state::<DbState>() {
        let _ = db_state
            .settings_repo
            .set("cloud_sync_cursor", &now.to_string());
    }

    Ok(CloudSyncStatus {
        state: "idle".to_string(),
        running: true,
        last_sync_at: Some(now),
        last_error: None,
        uploaded_items,
        received_items,
    })
}

async fn sync_once(
    app: &AppHandle,
    cfg: &CloudSyncConfig,
    force_snapshot: bool,
) -> AppResult<CloudSyncStatus> {
    let _run_guard = sync_run_lock().lock().await;
    if cloud_sync_cancel_requested() {
        let status = disabled_status();
        emit_status(Some(app), status.clone());
        return Ok(status);
    }

    if !cfg.enabled {
        let status = CloudSyncStatus {
            state: "disabled".to_string(),
            running: false,
            last_sync_at: None,
            last_error: None,
            uploaded_items: 0,
            received_items: 0,
        };
        emit_status(Some(app), status.clone());
        return Ok(status);
    }

    if !cloud_sync_target_ready(cfg) {
        let msg = cloud_sync_target_not_ready_message(cfg);
        let status = CloudSyncStatus {
            state: "error".to_string(),
            running: true,
            last_sync_at: None,
            last_error: Some(msg.clone()),
            uploaded_items: 0,
            received_items: 0,
        };
        emit_status(Some(app), status);
        return Err(AppError::Validation(msg));
    }

    emit_status(
        Some(app),
        CloudSyncStatus {
            state: "syncing".to_string(),
            running: true,
            last_sync_at: None,
            last_error: None,
            uploaded_items: 0,
            received_items: 0,
        },
    );

    let result = match cfg.provider {
        CloudSyncProvider::Http => sync_once_http(app, cfg).await,
        CloudSyncProvider::WebDav => sync_once_webdav(app, cfg, force_snapshot).await,
    };

    match result {
        Ok(status) => {
            emit_status(Some(app), status.clone());
            Ok(status)
        }
        Err(err) => {
            if cloud_sync_cancel_requested() {
                let status = disabled_status();
                emit_status(Some(app), status.clone());
                return Ok(status);
            }
            emit_status(
                Some(app),
                CloudSyncStatus {
                    state: "error".to_string(),
                    running: true,
                    last_sync_at: None,
                    last_error: Some(format!("[{}] {}", cfg.provider.as_str(), err)),
                    uploaded_items: 0,
                    received_items: 0,
                },
            );
            Err(err)
        }
    }
}

struct CloudSyncTaskGuard;

impl Drop for CloudSyncTaskGuard {
    fn drop(&mut self) {
        CLOUD_SYNC_TASK_ACTIVE.store(false, Ordering::Relaxed);
    }
}

pub fn get_cloud_sync_status() -> CloudSyncStatus {
    if let Ok(guard) = status_store().lock() {
        guard.clone()
    } else {
        CloudSyncStatus {
            state: "error".to_string(),
            running: false,
            last_sync_at: None,
            last_error: Some("status lock poisoned".to_string()),
            uploaded_items: 0,
            received_items: 0,
        }
    }
}

pub fn start_cloud_sync_client(app: AppHandle) {
    if CLOUD_SYNC_TASK_ACTIVE.swap(true, Ordering::Relaxed) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let _guard = CloudSyncTaskGuard;

        loop {
            let mut requested = CLOUD_SYNC_REQUESTED.swap(false, Ordering::Relaxed);
            let cfg = match get_config(&app) {
                Some(c) => c,
                None => {
                    emit_status(
                        Some(&app),
                        CloudSyncStatus {
                            state: "disabled".to_string(),
                            running: false,
                            last_sync_at: None,
                            last_error: None,
                            uploaded_items: 0,
                            received_items: 0,
                        },
                    );
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            let now = now_ms();
            let backoff_until = CLOUD_SYNC_BACKOFF_UNTIL.load(Ordering::Relaxed);
            if backoff_until > now {
                let remaining_secs = (backoff_until - now) / 1000;
                if remaining_secs > 0 {
                    emit_status(
                        Some(&app),
                        CloudSyncStatus {
                            state: "idle".to_string(),
                            running: true,
                            last_sync_at: None,
                            last_error: Some(format!(
                                "WebDAV Cooldown (JianGuoYun Rate Limit): {}s remaining",
                                remaining_secs
                            )),
                            uploaded_items: 0,
                            received_items: 0,
                        },
                    );
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }

            if !cfg.enabled || !cloud_sync_target_ready(&cfg) {
                if !cfg.enabled {
                    emit_status(
                        Some(&app),
                        CloudSyncStatus {
                            state: "disabled".to_string(),
                            running: false,
                            last_sync_at: None,
                            last_error: None,
                            uploaded_items: 0,
                            received_items: 0,
                        },
                    );
                } else {
                    emit_status(
                        Some(&app),
                        CloudSyncStatus {
                            state: "error".to_string(),
                            running: false,
                            last_sync_at: None,
                            last_error: Some(cloud_sync_target_not_ready_message(&cfg)),
                            uploaded_items: 0,
                            received_items: 0,
                        },
                    );
                }
            } else if cfg.auto_sync || requested {
                if let Err(e) = sync_once(&app, &cfg, false).await {
                    emit_status(
                        Some(&app),
                        CloudSyncStatus {
                            state: "error".to_string(),
                            running: true,
                            last_sync_at: None,
                            last_error: Some(e.to_string()),
                            uploaded_items: 0,
                            received_items: 0,
                        },
                    );
                }
            } else {
                emit_status(
                    Some(&app),
                    CloudSyncStatus {
                        state: "idle".to_string(),
                        running: true,
                        last_sync_at: None,
                        last_error: None,
                        uploaded_items: 0,
                        received_items: 0,
                    },
                );
            }

            if cfg.auto_sync {
                let interval = cfg
                    .interval_secs
                    .clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS);
                let mut elapsed = 0u64;
                while elapsed < interval {
                    requested = CLOUD_SYNC_REQUESTED.swap(false, Ordering::Relaxed);
                    if requested {
                        break;
                    }
                    sleep(Duration::from_secs(1)).await;
                    elapsed += 1;
                }
            } else {
                loop {
                    requested = CLOUD_SYNC_REQUESTED.swap(false, Ordering::Relaxed);
                    if requested {
                        break;
                    }
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });
}

pub fn restart_cloud_sync_client(app: AppHandle) {
    CLOUD_SYNC_CANCEL_REQUESTED.store(false, Ordering::Relaxed);
    start_cloud_sync_client(app);
    CLOUD_SYNC_REQUESTED.store(true, Ordering::Relaxed);
}

pub fn request_cloud_sync(app: AppHandle) {
    let Some(cfg) = get_config(&app) else {
        return;
    };
    if !cfg.enabled || !cfg.auto_sync || !cloud_sync_target_ready(&cfg) {
        return;
    }
    CLOUD_SYNC_CANCEL_REQUESTED.store(false, Ordering::Relaxed);
    start_cloud_sync_client(app);
    CLOUD_SYNC_REQUESTED.store(true, Ordering::Relaxed);
}

pub fn stop_cloud_sync_client(app: AppHandle) {
    CLOUD_SYNC_CANCEL_REQUESTED.store(true, Ordering::Relaxed);
    emit_status(Some(&app), disabled_status());
}

pub async fn cloud_sync_now(app: AppHandle) -> AppResult<CloudSyncStatus> {
    let current = get_cloud_sync_status();
    if current.state == "syncing" {
        return Ok(current);
    }
    CLOUD_SYNC_CANCEL_REQUESTED.store(false, Ordering::Relaxed);
    let cfg =
        get_config(&app).ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    sync_once(&app, &cfg, true).await
}

fn check_and_create_emoji_sync_op(app: &AppHandle) -> AppResult<Option<CloudSyncItem>> {
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB unavailable".to_string()))?;
    let emoji_json = db_state
        .settings_repo
        .get(EMOJI_FAVORITES_SETTING_KEY)
        .ok()
        .flatten()
        .unwrap_or_default();

    if emoji_json.trim().is_empty() || emoji_json == "[]" {
        return Ok(None);
    }

    let Some(sync_payload) = encode_emoji_favorites_setting(&emoji_json) else {
        return Ok(None);
    };
    if sync_payload.trim().is_empty() || sync_payload == "[]" {
        return Ok(None);
    }

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    sync_payload.hash(&mut hasher);
    let current_hash = hasher.finish() as i64;

    if current_hash == LAST_PUSHED_EMOJI_HASH.load(Ordering::Relaxed) {
        return Ok(None);
    }

    LAST_PUSHED_EMOJI_HASH.store(current_hash, Ordering::Relaxed);

    Ok(Some(CloudSyncItem {
        content_type: "emoji_sync".to_string(),
        content: sync_payload,
        content_hash: current_hash,
        deleted_at: 0,
        html_content: None,
        content_blob_hash: None,
        html_blob_hash: None,
        source_app: "TieZ".to_string(),
        timestamp: now_ms(),
        preview: "⭐ Emoji Sync".to_string(),
        is_pinned: false,
        pinned_order: 0,
        tags: vec![],
        use_count: 0,
    }))
}

fn merge_remote_emojis(app: &AppHandle, remote_json: &str) -> AppResult<()> {
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB unavailable".to_string()))?;
    let local_json = db_state
        .settings_repo
        .get(EMOJI_FAVORITES_SETTING_KEY)
        .ok()
        .flatten()
        .unwrap_or_default();

    let local_paths = if local_json.trim().is_empty() || local_json == "[]" {
        Vec::new()
    } else {
        materialize_emoji_favorite_paths(app, &local_json)?
    };
    let remote_paths = materialize_emoji_favorite_paths(app, remote_json)?;
    let normalized_local_json = serde_json::to_string(&local_paths).unwrap_or_default();

    let mut merged: std::collections::HashSet<String> = local_paths.iter().cloned().collect();
    for path in remote_paths {
        merged.insert(path);
    }

    let mut merged_paths: Vec<String> = merged.into_iter().collect();
    merged_paths.sort();

    if merged_paths != local_paths || normalized_local_json != local_json {
        let new_json = serde_json::to_string(&merged_paths).unwrap_or_default();
        db_state
            .settings_repo
            .set(EMOJI_FAVORITES_SETTING_KEY, &new_json)
            .map_err(AppError::from)?;

        // Update local hash to prevent echoing back the same data.
        let sync_payload =
            encode_emoji_favorites_setting(&new_json).unwrap_or_else(|| "[]".to_string());
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        sync_payload.hash(&mut hasher);
        LAST_PUSHED_EMOJI_HASH.store(hasher.finish() as i64, Ordering::Relaxed);

        let _ = app.emit("settings-changed", ());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_item_for_sync, rewrite_rich_html_resources_for_sync, CloudSyncItem,
        RICH_IMAGE_FALLBACK_PREFIX, RICH_IMAGE_FALLBACK_SUFFIX,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_PNG_BYTES: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9,
        251, 3, 253, 160, 164, 95, 122, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    fn make_temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tiez-cloud-sync-{name}-{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn rewrite_rich_html_resources_for_sync_inlines_local_images_and_fallbacks() {
        let dir = make_temp_dir("rich-html");
        let image_path = dir.join("inline.png");
        fs::write(&image_path, TEST_PNG_BYTES).expect("write test png");

        let image_path_str = image_path.to_string_lossy().replace('\\', "/");
        let html = format!(
            "<div><img src=\"file://{}\"></div>\n{}{}{}",
            image_path_str, RICH_IMAGE_FALLBACK_PREFIX, image_path_str, RICH_IMAGE_FALLBACK_SUFFIX
        );

        let rewritten = rewrite_rich_html_resources_for_sync(&html);

        assert!(rewritten.contains("src=\"data:image/png;base64,"));
        assert!(rewritten.contains(RICH_IMAGE_FALLBACK_PREFIX));
        assert!(rewritten.contains("data:image/png;base64,"));
        assert!(!rewritten.contains(&format!("file://{}", image_path_str)));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn normalize_item_for_sync_rewrites_rich_html_local_resources() {
        let dir = make_temp_dir("normalize-item");
        let image_path = dir.join("entry.png");
        fs::write(&image_path, TEST_PNG_BYTES).expect("write test png");

        let item = CloudSyncItem {
            content_type: "rich_text".to_string(),
            content: "hello".to_string(),
            content_hash: 0,
            deleted_at: 0,
            html_content: Some(format!(
                "<p>Hello</p><img src=\"{}\">",
                image_path.to_string_lossy()
            )),
            content_blob_hash: None,
            html_blob_hash: None,
            source_app: "Test".to_string(),
            timestamp: 1,
            preview: "hello".to_string(),
            is_pinned: false,
            tags: vec![],
            use_count: 0,
            pinned_order: 0,
        };

        let normalized = normalize_item_for_sync(item).expect("normalized item");
        let html = normalized.html_content.expect("html content");

        assert!(html.contains("src=\"data:image/png;base64,"));
        assert!(!html.contains("entry.png"));

        let _ = fs::remove_dir_all(dir);
    }
}
