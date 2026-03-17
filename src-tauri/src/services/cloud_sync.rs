use crate::database::DbState;
use crate::domain::models::ClipboardEntry;
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use crate::app::commands::file_cmd::{
    image_ext_from_mime,
    list_emoji_favorite_paths_in_dir,
    save_emoji_favorite_bytes_to_dir,
};
use base64::Engine;
use regex::Regex;
use reqwest::{Client, Method, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::sleep;

const DEFAULT_INTERVAL_SECS: u64 = 120;
const MIN_INTERVAL_SECS: u64 = 30;
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
const CLOUD_SYNC_WEBDAV_LAST_SNAPSHOT_PUSH_AT_KEY: &str = "cloud_sync_webdav_last_snapshot_push_at";
const CLOUD_SYNC_WEBDAV_LAST_SNAPSHOT_PULL_AT_KEY: &str = "cloud_sync_webdav_last_snapshot_pull_at";

static CLOUD_SYNC_TASK_ACTIVE: AtomicBool = AtomicBool::new(false);
static CLOUD_SYNC_RUN_ACTIVE: AtomicBool = AtomicBool::new(false);
static CLOUD_SYNC_REQUESTED: AtomicBool = AtomicBool::new(false);
static CLOUD_SYNC_LAST_SYNC_AT: AtomicI64 = AtomicI64::new(0);
static LAST_PUSHED_EMOJI_HASH: AtomicI64 = AtomicI64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloudSyncProvider {
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
    content_type: String,
    content: String,
    #[serde(default)]
    content_hash: i64,
    #[serde(default)]
    deleted_at: i64,
    #[serde(default)]
    html_content: Option<String>,
    source_app: String,
    timestamp: i64,
    preview: String,
    #[serde(default)]
    is_pinned: bool,
    #[serde(default)]
    pinned_order: i64,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    use_count: i32,
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

fn active_sync_status_snapshot() -> CloudSyncStatus {
    let current = get_cloud_sync_status();
    CloudSyncStatus {
        state: "syncing".to_string(),
        running: true,
        last_sync_at: current.last_sync_at,
        last_error: None,
        uploaded_items: current.uploaded_items,
        received_items: current.received_items,
    }
}

fn parse_interval_secs(raw: Option<String>) -> u64 {
    let parsed = raw.and_then(|v| v.parse::<u64>().ok()).unwrap_or(DEFAULT_INTERVAL_SECS);
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

    let stored_anon_id = db_state.settings_repo.get("app.anon_id").unwrap_or(None);
    let device_id = stored_anon_id
        .as_deref()
        .and_then(crate::app::system::normalize_anon_id)
        .unwrap_or_else(|| {
            let machine_id = crate::app::system::get_machine_id();
            crate::app::system::build_anon_id(&machine_id)
        });

    if stored_anon_id
        .as_deref()
        .map(|v| v.trim() != device_id)
        .unwrap_or(true)
    {
        let _ = db_state.settings_repo.set("app.anon_id", &device_id);
        let _ = db_state
            .settings_repo
            .set(CLOUD_SYNC_WEBDAV_LOCAL_SEQ_KEY, "0");
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
    matches!(content_type, "text" | "code" | "url" | "rich_text" | "image" | "emoji_sync")
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
            | "cloud_sync_webdav_last_snapshot_push_at"
            | "cloud_sync_webdav_last_snapshot_pull_at"
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

fn encode_emoji_favorites_setting(raw: &str) -> Option<String> {
    let paths: Vec<String> = serde_json::from_str(raw).ok()?;
    let encoded: Vec<String> = paths
        .into_iter()
        .filter_map(|path| to_data_url_from_path(path.trim()))
        .collect();
    serde_json::to_string(&encoded).ok()
}

fn decode_data_url(data_url: &str) -> AppResult<(&str, Vec<u8>)> {
    let Some(header_and_payload) = data_url.strip_prefix("data:") else {
        return Err(AppError::Validation("invalid data url".to_string()));
    };
    let Some((meta, payload)) = header_and_payload.split_once(',') else {
        return Err(AppError::Validation("invalid data url payload".to_string()));
    };
    if !meta.contains(";base64") {
        return Err(AppError::Validation("unsupported data url encoding".to_string()));
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

fn decode_emoji_favorites_setting(app: &AppHandle, raw: &str) -> AppResult<String> {
    let encoded_items: Vec<String> = serde_json::from_str(raw)
        .map_err(|e| AppError::Validation(format!("invalid emoji favorites payload: {}", e)))?;
    let data_dir = get_app_data_dir(app)
        .ok_or_else(|| AppError::Internal("App data dir unavailable".to_string()))?;
    let mut saved_paths: Vec<String> = Vec::new();

    for item in encoded_items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with("data:") {
            saved_paths.push(trimmed.to_string());
            continue;
        }

        let (mime, bytes) = decode_data_url(trimmed)?;
        if bytes.is_empty() || bytes.len() > MAX_INLINE_IMAGE_BYTES {
            continue;
        }
        let ext = image_ext_from_mime(mime)
            .ok_or_else(|| AppError::Validation(format!("unsupported emoji mime type: {}", mime)))?;
        let path = save_emoji_favorite_bytes_to_dir(&data_dir, &bytes, ext)?;
        saved_paths.push(path);
    }

    serde_json::to_string(&saved_paths)
        .map_err(|e| AppError::Internal(format!("serialize emoji favorites failed: {}", e)))
}

fn existing_emoji_favorite_set(raw: &str) -> std::collections::HashSet<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty() && Path::new(path).is_file())
        .collect()
}

fn stable_emoji_favorites_json(paths: std::collections::HashSet<String>) -> String {
    let mut items: Vec<String> = paths.into_iter().collect();
    items.sort();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

fn local_emoji_favorite_set(app: &AppHandle, raw: &str) -> std::collections::HashSet<String> {
    let mut paths = existing_emoji_favorite_set(raw);
    if let Some(data_dir) = get_app_data_dir(app) {
        if let Ok(extra_paths) = list_emoji_favorite_paths_in_dir(&data_dir) {
            paths.extend(extra_paths);
        }
    }
    paths
}

fn emoji_sync_payload_hash(payload: &str) -> i64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    payload.hash(&mut hasher);
    hasher.finish() as i64
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
            item.html_content = Some(rewrite_rich_fallback_payload_to_data_url(html));
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
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
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
        .and_then(|db| db.settings_repo.get(CLOUD_SYNC_WEBDAV_OP_CURSOR_MAP_KEY).ok().flatten())
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
                source_app: e.source_app,
                timestamp: e.timestamp,
                preview: e.preview,
                is_pinned: e.is_pinned,
                pinned_order: e.pinned_order,
                tags: e.tags,
                use_count: e.use_count,
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
                source_app: "sync".to_string(),
                timestamp: row.get(2)?,
                preview: String::new(),
                is_pinned: false,
                pinned_order: 0,
                tags: Vec::new(),
                use_count: 0,
            })
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|e| AppError::Internal(e.to_string()))?);
    }
    Ok(items)
}

fn apply_remote_changes(app: &AppHandle, remote_items: &[CloudSyncItem]) -> AppResult<usize> {
    if remote_items.is_empty() {
        return Ok(0);
    }

    let mut applied = 0usize;
    let mut emoji_payloads: Vec<String> = Vec::new();
    let mut normal_items: Vec<&CloudSyncItem> = Vec::new();

    for item in remote_items {
        if item.content_type == "emoji_sync" {
            emoji_payloads.push(item.content.clone());
            continue;
        }
        if is_syncable_content_type(&item.content_type) {
            normal_items.push(item);
        }
    }

    for payload in emoji_payloads {
        if let Err(e) = merge_remote_emojis(app, &payload) {
            println!("Error merging remote emojis: {}", e);
        }
        applied += 1;
    }

    if normal_items.is_empty() {
        return Ok(applied);
    }

    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    let app_data_dir = get_app_data_dir(app);

    for item in normal_items {
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
            let tombstone_ts = item.deleted_at.max(item.timestamp);
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
            let preview = if item.preview.trim().is_empty() {
                item.content.chars().take(200).collect::<String>()
            } else {
                item.preview.clone()
            };
            let updated = update_existing_entry_from_sync(
                &conn,
                id,
                item,
                effective_timestamp,
                &preview,
            )?;
            if remote_hash != 0 {
                let _ = conn.execute(
                    "DELETE FROM cloud_sync_tombstones
                     WHERE content_type = ?1 AND content_hash = ?2 AND deleted_at <= ?3",
                    rusqlite::params![item.content_type, remote_hash, effective_timestamp],
                );
            }
            if updated {
                applied += 1;
            }
            continue;
        }

        let preview = if item.preview.trim().is_empty() {
            item.content.chars().take(200).collect::<String>()
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
            is_external: false,
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
                rusqlite::params![item.content_type, remote_hash, entry.timestamp],
            );
        }
        applied += 1;
    }

    Ok(applied)
}

fn update_existing_entry_from_sync(
    conn: &rusqlite::Connection,
    id: i64,
    item: &CloudSyncItem,
    effective_timestamp: i64,
    preview: &str,
) -> AppResult<bool> {
    let current = conn
        .query_row(
            "SELECT timestamp, preview, source_app, is_pinned, pinned_order, tags, use_count, source_app_path
             FROM clipboard_history
             WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)? == 1,
                    row.get::<_, i64>(4).unwrap_or(0),
                    row.get::<_, String>(5).unwrap_or_else(|_| "[]".to_string()),
                    row.get::<_, i32>(6).unwrap_or(0),
                    row.get::<_, Option<String>>(7).unwrap_or(None),
                ))
            },
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let incoming_tags_json =
        serde_json::to_string(&item.tags).unwrap_or_else(|_| "[]".to_string());
    let next_timestamp = current.0.max(effective_timestamp);
    let next_source_app_path = if current.2 == item.source_app {
        current.7.clone()
    } else {
        None
    };

    let changed = next_timestamp != current.0
        || current.1 != preview
        || current.2 != item.source_app
        || current.3 != item.is_pinned
        || current.4 != item.pinned_order
        || current.5 != incoming_tags_json
        || current.6 != item.use_count;

    if !changed {
        return Ok(false);
    }

    conn.execute(
        "UPDATE clipboard_history
         SET timestamp = ?1,
             preview = ?2,
             source_app = ?3,
             is_pinned = ?4,
             pinned_order = ?5,
             tags = ?6,
             use_count = ?7,
             source_app_path = ?8
         WHERE id = ?9",
        rusqlite::params![
            next_timestamp,
            preview,
            item.source_app,
            if item.is_pinned { 1 } else { 0 },
            item.pinned_order,
            incoming_tags_json,
            item.use_count,
            next_source_app_path,
            id
        ],
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;

    conn.execute(
        "DELETE FROM entry_tags WHERE entry_id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;
    for tag in &item.tags {
        let clean = tag.trim();
        if clean.is_empty() {
            continue;
        }
        conn.execute(
            "INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
            rusqlite::params![id, clean],
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    Ok(true)
}

fn cloud_sync_target_ready(cfg: &CloudSyncConfig) -> bool {
    match cfg.provider {
        CloudSyncProvider::Http => !cfg.base_url.trim().is_empty(),
        CloudSyncProvider::WebDav => !cfg.webdav_url.trim().is_empty(),
    }
}

fn build_http_client() -> AppResult<Client> {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Network(e.to_string()))
}

fn webdav_with_auth(req: RequestBuilder, cfg: &CloudSyncConfig) -> RequestBuilder {
    if cfg.webdav_username.trim().is_empty() {
        req
    } else {
        req.basic_auth(cfg.webdav_username.trim(), Some(cfg.webdav_password.trim()))
    }
}

fn webdav_url_for(cfg: &CloudSyncConfig, relative_path: &str) -> String {
    format!(
        "{}/{}",
        cfg.webdav_url.trim_end_matches('/'),
        relative_path.trim_start_matches('/')
    )
}

async fn mkcol_if_needed(
    client: &Client,
    cfg: &CloudSyncConfig,
    relative_path: &str,
) -> AppResult<()> {
    let method = Method::from_bytes(b"MKCOL")
        .map_err(|e| AppError::Internal(format!("invalid MKCOL method: {}", e)))?;
    let url = webdav_url_for(cfg, relative_path);
    let resp = webdav_with_auth(client.request(method, &url), cfg)
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;

    let code = resp.status().as_u16();
    if resp.status().is_success() || code == 405 || code == 301 || code == 302 {
        return Ok(());
    }

    let text = resp.text().await.unwrap_or_default();
    Err(AppError::Network(format!(
        "webdav MKCOL failed: {} {}",
        code, text
    )))
}

async fn ensure_webdav_directories(client: &Client, cfg: &CloudSyncConfig) -> AppResult<WebDavPaths> {
    let base = normalize_webdav_base_path(&cfg.webdav_base_path);
    let mut current = String::new();

    for segment in base.split('/').filter(|s| !s.is_empty()) {
        current = if current.is_empty() {
            segment.to_string()
        } else {
            format!("{}/{}", current, segment)
        };
        mkcol_if_needed(client, cfg, &current).await?;
    }

    let devices_path = if current.is_empty() {
        "devices".to_string()
    } else {
        format!("{}/devices", current)
    };
    let settings_path = if current.is_empty() {
        "settings".to_string()
    } else {
        format!("{}/settings", current)
    };
    let ops_path = if current.is_empty() {
        "ops".to_string()
    } else {
        format!("{}/ops", current)
    };
    mkcol_if_needed(client, cfg, &devices_path).await?;
    mkcol_if_needed(client, cfg, &settings_path).await?;
    mkcol_if_needed(client, cfg, &ops_path).await?;
    Ok(WebDavPaths {
        devices_path,
        settings_path,
        ops_path,
    })
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
    local_items: &[CloudSyncItem],
) -> AppResult<()> {
    let snapshot = WebDavDeviceSnapshot {
        device_id: cfg.device_id.clone(),
        updated_at: now_ms(),
        entries: local_items.to_vec(),
    };
    let body = serde_json::to_vec(&snapshot)
        .map_err(|e| AppError::Internal(format!("serialize snapshot failed: {}", e)))?;

    let relative = format!("{}/{}.json", devices_path.trim_end_matches('/'), cfg.device_id);
    let url = webdav_url_for(cfg, &relative);
    let resp = webdav_with_auth(
        client
            .put(&url)
            .header("Content-Type", "application/json")
            .body(body),
        cfg,
    )
    .send()
    .await
    .map_err(|e| AppError::Network(e.to_string()))?;

    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Err(AppError::Network(format!(
        "webdav PUT snapshot failed: {} {}",
        status, text
    )))
}

async fn list_webdav_snapshot_ids(
    client: &Client,
    cfg: &CloudSyncConfig,
    devices_path: &str,
) -> AppResult<Vec<String>> {
    let method = Method::from_bytes(b"PROPFIND")
        .map_err(|e| AppError::Internal(format!("invalid PROPFIND method: {}", e)))?;
    let url = webdav_url_for(cfg, devices_path);
    let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:getlastmodified />
  </d:prop>
</d:propfind>"#;

    let resp = webdav_with_auth(
        client
            .request(method, &url)
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(body),
        cfg,
    )
    .send()
    .await
    .map_err(|e| AppError::Network(e.to_string()))?;

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
    let resp = webdav_with_auth(client.get(&url), cfg)
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Network(format!(
            "webdav GET snapshot failed: {} {}",
            status, text
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    let parsed = serde_json::from_slice::<WebDavDeviceSnapshot>(&bytes)
        .map_err(|e| AppError::Network(format!("parse snapshot json failed: {}", e)))?;
    Ok(Some(parsed))
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
    let url = webdav_url_for(cfg, &relative);
    let resp = webdav_with_auth(
        client
            .put(&url)
            .header("Content-Type", "application/json")
            .body(body),
        cfg,
    )
    .send()
    .await
    .map_err(|e| AppError::Network(e.to_string()))?;
    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Err(AppError::Network(format!(
        "webdav PUT ops batch failed: {} {}",
        status, text
    )))
}

fn parse_webdav_op_refs(xml: &str) -> Vec<WebDavOpRef> {
    let Ok(re_href) = Regex::new(r"(?is)<[^>]*href[^>]*>\s*([^<]+)\s*</[^>]*href>") else {
        return Vec::new();
    };
    let Ok(re_file) = Regex::new(r"^(.+)__(\d+)\.json$") else {
        return Vec::new();
    };

    let mut refs: HashMap<String, WebDavOpRef> = HashMap::new();
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
        refs.entry(dedup_key).or_insert(WebDavOpRef { device_id, seq });
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
    let url = webdav_url_for(cfg, ops_path);
    let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:getlastmodified />
  </d:prop>
</d:propfind>"#;

    let resp = webdav_with_auth(
        client
            .request(method, &url)
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(body),
        cfg,
    )
    .send()
    .await
    .map_err(|e| AppError::Network(e.to_string()))?;

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
    let resp = webdav_with_auth(client.get(&url), cfg)
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Network(format!(
            "webdav GET ops batch failed: {} {}",
            status, text
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    let parsed = serde_json::from_slice::<WebDavOpsBatch>(&bytes)
        .map_err(|e| AppError::Network(format!("parse ops batch json failed: {}", e)))?;
    Ok(Some(parsed))
}

fn collect_syncable_settings(app: &AppHandle) -> AppResult<HashMap<String, String>> {
    let db_state = app
        .try_state::<DbState>()
        .ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    let mut map = db_state.settings_repo.get_all().map_err(AppError::from)?;
    map.retain(|k, _| is_setting_sync_eligible(k));

    let merged_emoji_json = {
        let current = map
            .get(EMOJI_FAVORITES_SETTING_KEY)
            .cloned()
            .unwrap_or_default();
        stable_emoji_favorites_json(local_emoji_favorite_set(app, &current))
    };
    if merged_emoji_json != "[]" {
        if merged_emoji_json
            != map
                .get(EMOJI_FAVORITES_SETTING_KEY)
                .cloned()
                .unwrap_or_default()
        {
            let _ = db_state
                .settings_repo
                .set(EMOJI_FAVORITES_SETTING_KEY, &merged_emoji_json);
        }
        if let Some(encoded) = encode_emoji_favorites_setting(&merged_emoji_json) {
            map.insert(EMOJI_FAVORITES_SETTING_KEY.to_string(), encoded);
        }
    } else {
        map.remove(EMOJI_FAVORITES_SETTING_KEY);
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
            let decoded = decode_emoji_favorites_setting(app, value)?;
            let mut merged = local_emoji_favorite_set(
                app,
                current.get(key).map(String::as_str).unwrap_or_default(),
            );
            merged.extend(existing_emoji_favorite_set(&decoded));
            stable_emoji_favorites_json(merged)
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
    let relative = format!("{}/{}.json", settings_path.trim_end_matches('/'), cfg.device_id);
    let url = webdav_url_for(cfg, &relative);
    let resp = webdav_with_auth(
        client
            .put(&url)
            .header("Content-Type", "application/json")
            .body(body),
        cfg,
    )
    .send()
    .await
    .map_err(|e| AppError::Network(e.to_string()))?;
    if resp.status().is_success() {
        return Ok(local_settings);
    }
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Err(AppError::Network(format!(
        "webdav PUT settings snapshot failed: {} {}",
        status, text
    )))
}

async fn fetch_webdav_settings_snapshot(
    client: &Client,
    cfg: &CloudSyncConfig,
    settings_path: &str,
    device_id: &str,
) -> AppResult<Option<WebDavSettingsSnapshot>> {
    let relative = format!("{}/{}.json", settings_path.trim_end_matches('/'), device_id);
    let url = webdav_url_for(cfg, &relative);
    let resp = webdav_with_auth(client.get(&url), cfg)
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Network(format!(
            "webdav GET settings snapshot failed: {} {}",
            status, text
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    let parsed = serde_json::from_slice::<WebDavSettingsSnapshot>(&bytes)
        .map_err(|e| AppError::Network(format!("parse settings snapshot json failed: {}", e)))?;
    Ok(Some(parsed))
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
        if crate::app::system::is_same_device_id(&device_id, &cfg.device_id) {
            continue;
        }
        if let Some(snapshot) = fetch_webdav_settings_snapshot(client, cfg, settings_path, &device_id).await?
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
        .set("cloud_sync_settings_applied_at", &snapshot.updated_at.to_string())
        .map_err(AppError::from)?;
    Ok(changed)
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
) -> AppResult<usize> {
    let refs = list_webdav_op_refs(client, cfg, ops_path).await?;
    if refs.is_empty() {
        return Ok(0);
    }

    let mut cursor_map = load_webdav_op_cursor_map(app);
    let mut received = 0usize;
    for op_ref in refs {
        if crate::app::system::is_same_device_id(&op_ref.device_id, &cfg.device_id) {
            continue;
        }
        let last_seq = cursor_map.get(&op_ref.device_id).copied().unwrap_or(0);
        if op_ref.seq <= last_seq {
            continue;
        }

        if let Some(batch) = fetch_webdav_ops_batch(client, cfg, ops_path, &op_ref).await? {
            if batch.device_id != op_ref.device_id {
                continue;
            }
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
    let endpoint = format!("{}/api/v1/clipboard/sync", cfg.base_url.trim_end_matches('/'));
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
    let local_max = local_items.iter().map(|x| x.timestamp).max().unwrap_or(cfg.cursor);
    let remote_max = body.entries.iter().map(|x| x.timestamp).max().unwrap_or(cfg.cursor);
    let next_cursor = body.cursor.unwrap_or(cfg.cursor).max(local_max).max(remote_max);

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

async fn sync_once_webdav(app: &AppHandle, cfg: &CloudSyncConfig) -> AppResult<CloudSyncStatus> {
    let now = now_ms();
    let local_items = collect_local_syncable_items(app)?;
    let (delta_items, collapsed_index) = collect_local_incremental_items(app, &local_items)?;
    let client = build_http_client()?;
    let paths = ensure_webdav_directories(&client, cfg).await?;

    let mut uploaded_items = 0usize;
    if !delta_items.is_empty() {
        let mut next_seq = get_local_webdav_op_seq(app);
        for chunk in delta_items.chunks(WEBDAV_OP_BATCH_SIZE) {
            next_seq = next_seq.saturating_add(1);
            upload_webdav_ops_batch(&client, cfg, &paths.ops_path, next_seq, chunk).await?;
        }
        set_local_webdav_op_seq(app, next_seq);
        replace_local_sync_index(app, &collapsed_index)?;
        uploaded_items += delta_items.len();
    }



    let mut received_items = pull_remote_webdav_ops(app, &client, cfg, &paths.ops_path).await?;

    // Incremental Emoji Sync check
    if let Ok(emoji_op) = check_and_create_emoji_sync_op(app) {
        if let Some(op) = emoji_op {
            let next_seq = get_local_webdav_op_seq(app).saturating_add(1);
            upload_webdav_ops_batch(&client, cfg, &paths.ops_path, next_seq, &[op]).await?;
            set_local_webdav_op_seq(app, next_seq);
            uploaded_items += 1;
        }
    }

    let cursor_map = load_webdav_op_cursor_map(app);

    if should_pull_webdav_snapshot(app, now, !cursor_map.is_empty(), cfg.snapshot_interval_secs) {
        let mut remote_items: Vec<CloudSyncItem> = Vec::new();
        let ids = list_webdav_snapshot_ids(&client, cfg, &paths.devices_path).await?;
        for device_id in ids.into_iter().take(MAX_REMOTE_SNAPSHOTS) {
            if crate::app::system::is_same_device_id(&device_id, &cfg.device_id) {
                continue;
            }
            if let Some(snapshot) =
                fetch_webdav_snapshot(&client, cfg, &paths.devices_path, &device_id).await?
            {
                remote_items.extend(snapshot.entries);
            }
        }
        remote_items.sort_by_key(|item| item.timestamp);
        received_items += apply_remote_changes(app, &remote_items)?;

        // Also pull remote settings when pulling snapshots
        let settings_changed = pull_remote_settings_snapshot(app, &client, cfg, &paths.settings_path).await?;
        received_items += settings_changed;

        set_setting_i64(app, CLOUD_SYNC_WEBDAV_LAST_SNAPSHOT_PULL_AT_KEY, now);
    }

    if should_push_webdav_snapshot(app, now, cfg.snapshot_interval_secs) {
        upload_webdav_snapshot(&client, cfg, &paths.devices_path, &local_items).await?;
        uploaded_items += local_items.len();

        // Also push local settings when pushing snapshots
        let local_settings = upload_webdav_settings_snapshot(app, &client, cfg, &paths.settings_path).await?;
        uploaded_items += local_settings.len();

        set_setting_i64(app, CLOUD_SYNC_WEBDAV_LAST_SNAPSHOT_PUSH_AT_KEY, now);
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

async fn sync_once_inner(app: &AppHandle, cfg: &CloudSyncConfig) -> AppResult<CloudSyncStatus> {
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
        let msg = if cfg.provider == CloudSyncProvider::Http {
            "cloud_sync_server is empty".to_string()
        } else {
            "cloud_sync_webdav_url is empty".to_string()
        };
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
        CloudSyncProvider::WebDav => sync_once_webdav(app, cfg).await,
    };

    match result {
        Ok(status) => {
            emit_status(Some(app), status.clone());
            Ok(status)
        }
        Err(err) => {
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

async fn sync_once(app: &AppHandle, cfg: &CloudSyncConfig) -> AppResult<CloudSyncStatus> {
    if CLOUD_SYNC_RUN_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        CLOUD_SYNC_REQUESTED.store(true, Ordering::Relaxed);
        let status = active_sync_status_snapshot();
        emit_status(Some(app), status.clone());
        return Ok(status);
    }

    let _guard = CloudSyncRunGuard;
    sync_once_inner(app, cfg).await
}

struct CloudSyncTaskGuard;

impl Drop for CloudSyncTaskGuard {
    fn drop(&mut self) {
        CLOUD_SYNC_TASK_ACTIVE.store(false, Ordering::Relaxed);
    }
}

struct CloudSyncRunGuard;

impl Drop for CloudSyncRunGuard {
    fn drop(&mut self) {
        CLOUD_SYNC_RUN_ACTIVE.store(false, Ordering::Release);
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

            if !cfg.enabled || !cloud_sync_target_ready(&cfg) {
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
            } else if cfg.auto_sync || requested {
                if let Err(e) = sync_once(&app, &cfg).await {
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
                let interval = cfg.interval_secs.clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS);
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
    start_cloud_sync_client(app);
    CLOUD_SYNC_REQUESTED.store(true, Ordering::Relaxed);
}

pub async fn cloud_sync_now(app: AppHandle) -> AppResult<CloudSyncStatus> {
    let cfg = get_config(&app).ok_or_else(|| AppError::Internal("DB state unavailable".to_string()))?;
    sync_once(&app, &cfg).await
}

fn check_and_create_emoji_sync_op(app: &AppHandle) -> AppResult<Option<CloudSyncItem>> {
    let db_state = app.try_state::<DbState>().ok_or_else(|| AppError::Internal("DB unavailable".to_string()))?;
    let emoji_json = db_state.settings_repo.get(EMOJI_FAVORITES_SETTING_KEY).ok().flatten().unwrap_or_default();
    let merged_emoji_json = stable_emoji_favorites_json(local_emoji_favorite_set(app, &emoji_json));

    if merged_emoji_json != emoji_json {
        db_state
            .settings_repo
            .set(EMOJI_FAVORITES_SETTING_KEY, &merged_emoji_json)
            .map_err(AppError::from)?;
    }

    if merged_emoji_json.trim().is_empty() || merged_emoji_json == "[]" {
        return Ok(None);
    }

    let encoded_emoji_json = encode_emoji_favorites_setting(&merged_emoji_json).unwrap_or_else(|| "[]".to_string());
    if encoded_emoji_json.trim().is_empty() || encoded_emoji_json == "[]" {
        return Ok(None);
    }

    let current_hash = emoji_sync_payload_hash(&encoded_emoji_json);

    if current_hash == LAST_PUSHED_EMOJI_HASH.load(Ordering::Relaxed) {
        return Ok(None);
    }

    LAST_PUSHED_EMOJI_HASH.store(current_hash, Ordering::Relaxed);

    Ok(Some(CloudSyncItem {
        content_type: "emoji_sync".to_string(),
        content: encoded_emoji_json,
        content_hash: current_hash,
        deleted_at: 0,
        html_content: None,
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
    let db_state = app.try_state::<DbState>().ok_or_else(|| AppError::Internal("DB unavailable".to_string()))?;
    let local_json = db_state.settings_repo.get(EMOJI_FAVORITES_SETTING_KEY).ok().flatten().unwrap_or_default();

    let mut local_set = local_emoji_favorite_set(app, &local_json);
    let decoded_remote_json = decode_emoji_favorites_setting(app, remote_json)?;
    let remote_set = existing_emoji_favorite_set(&decoded_remote_json);

    for emoji in remote_set {
        local_set.insert(emoji);
    }

    let new_json = stable_emoji_favorites_json(local_set);
    if new_json != local_json {
        db_state.settings_repo.set(EMOJI_FAVORITES_SETTING_KEY, &new_json).map_err(AppError::from)?;

        let payload = encode_emoji_favorites_setting(&new_json).unwrap_or_else(|| "[]".to_string());
        LAST_PUSHED_EMOJI_HASH.store(emoji_sync_payload_hash(&payload), Ordering::Relaxed);

        let _ = app.emit("settings-changed", ());
    }

    Ok(())
}
