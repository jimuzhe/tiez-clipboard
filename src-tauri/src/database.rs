use rusqlite::{Connection, Result};

use base64::Engine;

pub use crate::infrastructure::encryption::{self, ENCRYPT_PREFIX};

pub use crate::domain::models::ClipboardEntry;
use crate::infrastructure::repository::clipboard_repo::SqliteClipboardRepository;
use crate::infrastructure::repository::settings_repo::SqliteSettingsRepository;
use crate::infrastructure::repository::tag_repo::SqliteTagRepository;
use std::sync::{Arc, Mutex};

pub struct DbState {
    pub conn: Arc<Mutex<Connection>>,
    pub repo: SqliteClipboardRepository,
    pub settings_repo: SqliteSettingsRepository,
    pub tag_repo: SqliteTagRepository,
}

const SENSITIVE_KEYS: &[&str] = &[
    "mqtt_password",
    "mqtt_username",
    "ai_profiles",
    "cloud_sync_api_key",
    "cloud_sync_webdav_password",
    "e2e_key",
];

pub const SENSITIVE_TAGS: &[&str] = &["sensitive", "密码", "password"];

pub fn is_sensitive_key(key: &str) -> bool {
    SENSITIVE_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key))
}

/// `app.sound_volume` as 0-1. Windows 0.3.x stored 0-100 (and can still sync
/// such a value in), so anything above 1 is read as a percentage.
pub fn sound_volume_from_setting(value: &str) -> Option<f64> {
    let volume = value.trim().parse::<f64>().ok().filter(|v| v.is_finite())?;
    let volume = if volume > 1.0 { volume.min(100.0) / 100.0 } else { volume };
    Some(volume.max(0.0))
}

pub fn has_sensitive_tag(tags: &[String]) -> bool {
    tags.iter()
        .any(|t| SENSITIVE_TAGS.iter().any(|s| s.eq_ignore_ascii_case(t)))
}

pub fn is_text_type(content_type: &str) -> bool {
    matches!(content_type, "text" | "code" | "url" | "rich_text")
}

fn normalize_text(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn calc_text_hash(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let normalized = normalize_text(content);
    let mut hasher = crate::infrastructure::stable_hash::StableHasher::new();
    normalized.hash(&mut hasher);
    hasher.finish()
}

/// Hash of raw bytes that stays the same across Rust releases (stored hashes
/// and attachment names depend on it; see infrastructure::stable_hash).
pub fn stable_bytes_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = crate::infrastructure::stable_hash::StableHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// File extension from an image's magic bytes.
pub fn image_extension_from_magic(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"GIF8") {
        Some("gif")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"\xFF\xD8\xFF") {
        Some("jpg")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(b"BM") {
        Some("bmp")
    } else {
        None
    }
}

/// Content-addressed attachment (`img_<hash>.<ext>` in `dir`). Written to a
/// temp file and renamed into place; an existing file of the wrong size (a
/// write cut short by a crash or a full disk) is rewritten instead of being
/// reused for good.
pub fn write_attachment(dir: &std::path::Path, bytes: &[u8], ext: &str) -> Option<std::path::PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let name = format!("img_{:x}.{}", stable_bytes_hash(bytes), ext);
    let path = dir.join(&name);
    let intact = std::fs::metadata(&path)
        .map(|meta| meta.len() == bytes.len() as u64)
        .unwrap_or(false);
    if !intact {
        let temp = dir.join(format!(".{}.{}.tmp", name, std::process::id()));
        std::fs::write(&temp, bytes).ok()?;
        if std::fs::rename(&temp, &path).is_err() {
            let _ = std::fs::remove_file(&temp);
            return None;
        }
    }
    Some(path)
}

/// Stable cross-platform fingerprint used by the cloud-sync wire protocol.
///
/// Keep this algorithm in sync with `tiez-mobile/src/lib/sync.ts`. Unlike
/// `DefaultHasher`, FNV-1a 32-bit is deterministic across Rust/JavaScript and
/// stays inside JavaScript's exact integer range.
pub fn calc_sync_content_hash(content_type: &str, content: &str) -> i64 {
    let owned_bytes;
    let bytes: &[u8] = if content_type == "image" {
        let trimmed = content.trim();
        if !trimmed.starts_with("data:") && std::path::Path::new(trimmed).is_file() {
            owned_bytes = std::fs::read(trimmed).unwrap_or_else(|_| trimmed.as_bytes().to_vec());
            &owned_bytes
        } else if let Some((_, payload)) = trimmed.split_once(',') {
            use base64::Engine;
            owned_bytes = base64::engine::general_purpose::STANDARD
                .decode(payload.replace('\r', "").replace('\n', ""))
                .unwrap_or_else(|_| trimmed.as_bytes().to_vec());
            &owned_bytes
        } else {
            trimmed.as_bytes()
        }
    } else {
        owned_bytes = normalize_text(content).into_bytes();
        &owned_bytes
    };

    let mut hash = 2_166_136_261_u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    i64::from(if hash == 0 { 1 } else { hash })
}

pub fn calc_image_hash_from_bytes(bytes: &[u8]) -> Option<i64> {
    Some(stable_bytes_hash(bytes) as i64)
}

pub fn calc_image_hash(base64_data: &str) -> Option<i64> {
    let trimmed = base64_data.trim();
    if !trimmed.starts_with("data:") && (trimmed.starts_with('/') || trimmed.contains(":\\")) {
        let bytes = std::fs::read(trimmed).ok()?;
        return calc_image_hash_from_bytes(&bytes);
    }
    let parts: Vec<&str> = trimmed.splitn(2, ',').collect();
    let payload = if parts.len() == 2 { parts[1] } else { trimmed };
    let payload_clean = payload.replace("\r", "").replace("\n", "");
    if payload_clean.trim().is_empty() {
        return None;
    }

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload_clean.trim())
        .ok()?;

    calc_image_hash_from_bytes(&bytes)
}

pub fn init_db(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;

    // Performance and space pragmas
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA auto_vacuum = FULL;
        PRAGMA busy_timeout = 5000;
        PRAGMA mmap_size = 268435456;
        PRAGMA temp_store = MEMORY;
        PRAGMA cache_size = -64000;
    ",
    )?;

    // Run migrations
    crate::infrastructure::repository::migrations::run_migrations(&conn)?;

    // Initialize default settings
    seed_defaults(&conn)?;

    Ok(conn)
}

// save_entry removed (migrated to repository)

pub fn save_image_bytes_to_file_with_extension(
    image_bytes: &[u8],
    data_dir: &std::path::Path,
    extension: &str,
) -> Option<String> {
    // The bytes decide the extension: a GIF from a data URL labelled PNG, or a
    // BMP, used to be saved as .png.
    let extension = image_extension_from_magic(image_bytes).unwrap_or(
        match extension.trim().to_ascii_lowercase().as_str() {
            "gif" => "gif",
            "jpg" | "jpeg" => "jpg",
            "webp" => "webp",
            "bmp" => "bmp",
            _ => "png",
        },
    );
    write_attachment(&data_dir.join("attachments"), image_bytes, extension)
        .map(|path| path.to_string_lossy().to_string())
}

pub fn save_image_bytes_to_file(png_bytes: &[u8], data_dir: &std::path::Path) -> Option<String> {
    save_image_bytes_to_file_with_extension(png_bytes, data_dir, "png")
}

pub fn save_image_to_file(data_url: &str, data_dir: &std::path::Path) -> Option<String> {
    let parts: Vec<&str> = data_url.splitn(2, ',').collect();
    if parts.len() < 2 {
        return None;
    }

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(parts[1])
        .ok()?;

    save_image_bytes_to_file(&decoded, data_dir)
}

// get_history removed (migrated to repository)

// search_history removed (migrated to repository)

// get_important_items removed (migrated to repository)

// delete_entry_db, delete_entry, enforce_storage_limit, clear_history removed (migrated to repository)

/// Defaults that changed after release: (key, new installs, existing installs).
/// Upgraders keep what they had, including keys they never saved, whose
/// behaviour was the old default (e.g. the window followed the mouse).
pub(crate) const CHANGED_DEFAULTS: &[(&str, &str, &str)] = &[
    ("app.persistent", "true", "false"),
    ("app.arrow_key_selection", "true", "false"),
    ("app.move_to_top_after_paste", "false", "true"),
    ("app.privacy_protection", "false", "true"),
    ("app.follow_mouse", "false", "true"),
];

/// Set on a fresh database; startup turns on launch at login once, then
/// removes it (app/setup.rs). Existing installs keep their login item as is.
pub const AUTOSTART_PENDING_KEY: &str = "app.autostart_pending";

pub fn seed_defaults(conn: &Connection) -> Result<()> {
    // Every release has seeded app.hotkey, so only a new (or reset) database
    // lacks it. (Migrations may already have written other keys.)
    let fresh: bool = conn.query_row(
        "SELECT NOT EXISTS (SELECT 1 FROM settings WHERE key = 'app.hotkey')",
        [],
        |r| r.get(0),
    )?;
    for (key, new_install, existing) in CHANGED_DEFAULTS {
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
            [*key, if fresh { *new_install } else { *existing }],
        )?;
    }
    if fresh {
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, 'true')",
            [AUTOSTART_PENDING_KEY],
        )?;
    }

    // App settings
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.theme', ?1)",
        [crate::build_config::default_theme_id()],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.color_mode', 'system')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.show_app_border', 'true')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.capture_files', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.capture_rich_text', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.rich_text_snapshot_preview', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.deduplicate', 'true')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.silent_start', 'true')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.delete_after_paste', 'false')",
        [],
    );
    let _ = conn.execute("INSERT OR IGNORE INTO settings (key, value) VALUES ('app.privacy_protection_kinds', 'phone,idcard,email,secret')", []);
    let _ = conn.execute("INSERT OR IGNORE INTO settings (key, value) VALUES ('app.privacy_protection_custom_rules', '')", []);
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.cleanup_rules', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.app_cleanup_policies', '[]')",
        [],
    );

    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.sequential_mode', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.sequential_hotkey', 'Alt+V')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.plain_paste_hotkey', 'Alt+Shift+V')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.search_hotkey', 'Alt+F')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.quick_paste_modifier', 'disabled')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.sound_enabled', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.sound_paste_enabled', 'true')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.sound_volume', '1.0')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.hide_tray_icon', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.hide_dock_icon', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.edge_docking', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.last_edge_dock', 'none')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.light_mode_enabled', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.light_mode_idle_minutes', '15')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.window_pinned', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.hotkey', 'Alt+C')",
        [],
    );

    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.custom_background', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.surface_opacity', '50')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.notice_v028_shown', 'true')",
        [],
    );

    // File transfer settings
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('file_transfer_auto_close', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('file_transfer_auto_copy', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('file_server_enabled', 'false')",
        [],
    );
    let _ = conn.execute(
        // 12345 is NetBus's port, so port scans name TieZ a trojan (#157).
        // Only new installs get the new one; saved ports (and phones that
        // bookmarked them) stay as they are.
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('file_server_port', '28462')",
        [],
    );

    // MQTT settings
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_port', '443')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_enabled', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_server', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_username', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_password', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_topic', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_protocol', 'wss://')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_ssl', 'true')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_client_id', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_ws_path', '/mqtt')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('mqtt_notification_enabled', 'true')",
        [],
    );
    let _ = conn.execute(
        "UPDATE settings
         SET value = ''
         WHERE key = 'mqtt_server'
           AND value = 'tiez.name666.top'
           AND COALESCE((SELECT value FROM settings WHERE key = 'mqtt_enabled'), 'false') = 'false'",
        [],
    );
    let _ = conn.execute(
        "UPDATE settings
         SET value = ''
         WHERE key = 'mqtt_username'
           AND value = 'tiezpublic'
           AND COALESCE((SELECT value FROM settings WHERE key = 'mqtt_enabled'), 'false') = 'false'",
        [],
    );
    let _ = conn.execute(
        "UPDATE settings
         SET value = ''
         WHERE key = 'mqtt_password'
           AND value = 'tiezmessage'
           AND COALESCE((SELECT value FROM settings WHERE key = 'mqtt_enabled'), 'false') = 'false'",
        [],
    );

    // Cloud sync settings
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_enabled', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_auto', 'true')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_provider', 'webdav')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_server', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_api_key', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_interval_sec', '120')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_snapshot_interval_min', '720')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_cursor', '0')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_settings_applied_at', '0')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_webdav_url', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_webdav_username', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_webdav_password', '')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_webdav_base_path', 'tiez-sync')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_content_prefs', '{\"text\":true,\"image\":true,\"file_path\":true,\"emoji\":true}')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_webdav_local_seq', '0')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_webdav_op_cursor_map', '{}')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_webdav_blob_cache', '{}')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_webdav_last_snapshot_push_at', '0')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_webdav_last_snapshot_pull_at', '0')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('cloud_sync_webdav_last_head_rebuild_at', '0')",
        [],
    );

    // AI settings
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('ai_enabled', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('ai_target_lang', 'zh')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('ai_enable_thinking', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('ai_thinking_budget', '1024')",
        [],
    );

    // Storage limit settings
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.persistent_limit_enabled', 'true')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.persistent_limit', '500')",
        [],
    );

    Ok(())
}

// Migrated to repositories: toggle_pin, update_pinned_order, get_entry_by_content,
// update_entry_content, insert_entry, get_entry_content, get_entry_content_full,
// get_entry_content_with_html, get_entry_by_id, update_entry_tags, get_all_tags,
// create_tag, rename_tag, delete_tag_globally, get_entries_by_tag, set_tag_color, get_tag_colors

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::repository::clipboard_repo::{
        ClipboardRepository, ClipboardSortMode, SqliteClipboardRepository,
    };
    use crate::infrastructure::repository::settings_repo::{
        SettingsRepository, SqliteSettingsRepository,
    };

    // 辅助函数：创建一个内存中的临时测试数据库
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // 调用你的 init_db 逻辑（手动执行部分关键建表语句）
        conn.execute(
            "CREATE TABLE clipboard_history (
                id INTEGER PRIMARY KEY,
                content_type TEXT NOT NULL,
                content TEXT NOT NULL,
                html_content TEXT,
                source_app TEXT NOT NULL,
                source_app_path TEXT,
                timestamp INTEGER NOT NULL,
                preview TEXT NOT NULL,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                content_hash INTEGER NOT NULL DEFAULT 0,
                tags TEXT NOT NULL DEFAULT '[]',
                note TEXT NOT NULL DEFAULT '',
                use_count INTEGER NOT NULL DEFAULT 0,
                item_hotkey TEXT NOT NULL DEFAULT '',
                item_hotkey_global INTEGER NOT NULL DEFAULT 0,
                last_used_at INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                sort_at INTEGER NOT NULL DEFAULT 0,
                move_to_group_hotkey TEXT NOT NULL DEFAULT '',
                move_to_group_hotkey_global INTEGER NOT NULL DEFAULT 0,
                is_external INTEGER NOT NULL DEFAULT 0,
                pinned_order INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE entry_tags (
                entry_id INTEGER NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY (entry_id, tag)
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE cloud_sync_tombstones (
                content_type TEXT NOT NULL,
                content_hash INTEGER NOT NULL,
                deleted_at INTEGER NOT NULL,
                PRIMARY KEY (content_type, content_hash)
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE cloud_sync_local_index (
                sync_key TEXT PRIMARY KEY,
                digest TEXT NOT NULL
            )",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn sound_volume_reads_legacy_percentages() {
        assert_eq!(sound_volume_from_setting("0.4"), Some(0.4));
        assert_eq!(sound_volume_from_setting("70"), Some(0.7));
        assert_eq!(sound_volume_from_setting("250"), Some(1.0));
        assert_eq!(sound_volume_from_setting("0"), Some(0.0));
        assert_eq!(sound_volume_from_setting("loud"), None);
    }

    #[test]
    fn test_save_and_get_history() {
        let conn = setup_test_db();

        let entry = ClipboardEntry {
            id: 0,
            content_type: "text".to_string(),
            content: "Hello Integration Test".to_string(),
            html_content: None,
            source_app: "TestApp".to_string(),
            source_app_path: Some("/Applications/TestApp.app".to_string()),
            timestamp: 123456789,
            created_at: 123456789,
            last_used_at: 0,
            sort_at: 123456789,
            preview: "Hello...".to_string(),
            is_pinned: false,
            tags: vec![],
            note: String::new(),
            use_count: 0,
            is_external: false,
            pinned_order: 0,
            file_preview_exists: true,
        };

        let conn_arc = Arc::new(Mutex::new(conn));
        let repo = SqliteClipboardRepository::new(conn_arc);

        // 1. 测试保存
        let id = repo.save(&entry, None).expect("保存失败");
        assert!(id > 0);

        // 2. 测试获取
        let history = repo.get_history(10, 0, None).expect("获取历史失败");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "Hello Integration Test");
        assert_eq!(history[0].source_app, "TestApp");
        assert_eq!(history[0].created_at, 123456789);
        assert_eq!(history[0].sort_at, 123456789);
        assert_eq!(history[0].last_used_at, 0);

        repo.increment_use_count(id).expect("更新使用时间失败");
        let used = repo.get_history(10, 0, None).expect("重新获取历史失败");
        assert!(used[0].last_used_at > 0);
        assert_eq!(used[0].created_at, 123456789);
    }

    #[test]
    fn test_history_sort_modes_keep_pinned_items_first() {
        let conn = setup_test_db();
        for (content, is_pinned, created_at, last_used_at, sort_at, use_count, pinned_order) in [
            ("pinned-activity", 1, 100, 20, 50, 1, 1),
            ("pinned-manual", 1, 200, 10, 40, 5, 2),
            ("normal-used", 0, 300, 30, 60, 10, 99),
            ("normal-created", 0, 400, 0, 70, 0, 0),
        ] {
            conn.execute(
                "INSERT INTO clipboard_history (
                    content_type, content, source_app, timestamp, preview, is_pinned,
                    created_at, last_used_at, sort_at, use_count, pinned_order
                 ) VALUES ('text', ?1, 'TestApp', ?2, ?1, ?3, ?4, ?5, ?2, ?6, ?7)",
                rusqlite::params![
                    content,
                    sort_at,
                    is_pinned,
                    created_at,
                    last_used_at,
                    use_count,
                    pinned_order
                ],
            )
            .unwrap();
        }

        let repo = SqliteClipboardRepository::new(Arc::new(Mutex::new(conn)));
        let contents_for = |mode| {
            repo.get_history_sorted(10, 0, None, mode)
                .unwrap()
                .into_iter()
                .map(|entry| entry.content)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            contents_for(ClipboardSortMode::Activity),
            [
                "pinned-manual",
                "pinned-activity",
                "normal-created",
                "normal-used"
            ]
        );
        assert_eq!(
            contents_for(ClipboardSortMode::Created),
            [
                "pinned-manual",
                "pinned-activity",
                "normal-created",
                "normal-used"
            ]
        );
        assert_eq!(
            contents_for(ClipboardSortMode::LastUsed),
            [
                "pinned-manual",
                "pinned-activity",
                "normal-used",
                "normal-created"
            ]
        );
        assert_eq!(
            contents_for(ClipboardSortMode::Usage),
            [
                "pinned-manual",
                "pinned-activity",
                "normal-used",
                "normal-created"
            ]
        );
        assert_eq!(
            contents_for(ClipboardSortMode::Manual),
            [
                "pinned-manual",
                "pinned-activity",
                "normal-created",
                "normal-used"
            ]
        );
        // The plain text paste hotkey: newest copy first, pins have no say.
        assert_eq!(
            contents_for(ClipboardSortMode::Recent),
            [
                "normal-created",
                "normal-used",
                "pinned-activity",
                "pinned-manual"
            ]
        );
    }

    #[test]
    fn test_settings_persistence() {
        let conn = setup_test_db();
        let conn_arc = Arc::new(Mutex::new(conn));
        let repo = SqliteSettingsRepository::new(conn_arc);

        // 测试设置保存
        repo.set("test_key", "test_value").unwrap();

        // 测试设置读取
        let val = repo.get("test_key").unwrap();
        assert_eq!(val, Some("test_value".to_string()));
    }

    #[test]
    fn sync_hash_is_stable_and_normalizes_line_endings() {
        assert_eq!(calc_sync_content_hash("text", "hello"), 1_335_831_723);
        assert_eq!(
            calc_sync_content_hash("text", "line 1\r\nline 2"),
            calc_sync_content_hash("text", "line 1\nline 2")
        );
    }

    #[test]
    fn image_sync_hash_matches_file_and_data_url_representations() {
        let path = std::env::temp_dir().join(format!("tiez-sync-hash-{}.png", std::process::id()));
        let bytes = [0_u8, 1, 2, 3, 250, 251, 252];
        std::fs::write(&path, bytes).expect("write hash fixture");
        let data_url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );

        assert_eq!(
            calc_sync_content_hash("image", &path.to_string_lossy()),
            calc_sync_content_hash("image", &data_url)
        );

        let _ = std::fs::remove_file(path);
    }
}
