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
];

pub const SENSITIVE_TAGS: &[&str] = &[
    "sensitive",
    "密码",
];

pub fn is_sensitive_key(key: &str) -> bool {
    SENSITIVE_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key))
}

pub fn has_sensitive_tag(tags: &[String]) -> bool {
    tags.iter().any(|t| {
        SENSITIVE_TAGS
            .iter()
            .any(|s| s.eq_ignore_ascii_case(t))
    })
}

pub fn is_text_type(content_type: &str) -> bool {
    matches!(content_type, "text" | "code" | "url" | "rich_text")
}

fn normalize_text(content: &str) -> String {
    content.trim().replace("\r\n", "\n")
}

pub fn calc_text_hash(content: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let normalized = normalize_text(content);
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    hasher.finish()
}

pub fn calc_image_hash(base64_data: &str) -> Option<i64> {
    let parts: Vec<&str> = base64_data.splitn(2, ',').collect();
    let payload = if parts.len() == 2 { parts[1] } else { base64_data };
    let payload_clean = payload.replace("\r", "").replace("\n", "");
    
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(payload_clean.trim()).ok()?;

    let img = image::load_from_memory(&decoded).ok()?;
    let thumb = img.resize_exact(32, 32, image::imageops::FilterType::Nearest);
    
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    thumb.as_bytes().hash(&mut hasher);
    Some(hasher.finish() as i64)
}

pub fn init_db(path: &str) -> Result<Connection> {
    fn init_db_once(path: &str) -> Result<Connection> {
        let conn = Connection::open(path)?;
        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA auto_vacuum = FULL;
        ")?;
        crate::infrastructure::repository::migrations::run_migrations(&conn)?;
        seed_defaults(&conn)?;
        Ok(conn)
    }

    fn is_disk_io_error(err: &rusqlite::Error) -> bool {
        err.to_string().to_ascii_lowercase().contains("disk i/o error")
    }

    fn quarantine_corrupted_db(path: &str) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let db_path = std::path::PathBuf::from(path);
        if !db_path.exists() {
            return;
        }
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let base = db_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("clipboard");
        let ext = db_path.extension().and_then(|s| s.to_str()).unwrap_or("db");
        let mut backup = db_path.clone();
        backup.set_file_name(format!("{base}.corrupt-{ts}.{ext}"));
        let _ = std::fs::rename(&db_path, &backup);

        let mut wal_path = db_path.clone();
        wal_path.set_file_name(format!(
            "{}-wal",
            db_path.file_name().and_then(|s| s.to_str()).unwrap_or("clipboard.db")
        ));
        if wal_path.exists() {
            let mut wal_backup = backup.clone();
            wal_backup.set_file_name(format!(
                "{}-wal",
                backup.file_name().and_then(|s| s.to_str()).unwrap_or("clipboard.corrupt.db")
            ));
            let _ = std::fs::rename(&wal_path, &wal_backup);
        }

        let mut shm_path = db_path.clone();
        shm_path.set_file_name(format!(
            "{}-shm",
            db_path.file_name().and_then(|s| s.to_str()).unwrap_or("clipboard.db")
        ));
        if shm_path.exists() {
            let mut shm_backup = backup.clone();
            shm_backup.set_file_name(format!(
                "{}-shm",
                backup.file_name().and_then(|s| s.to_str()).unwrap_or("clipboard.corrupt.db")
            ));
            let _ = std::fs::rename(&shm_path, &shm_backup);
        }
    }

    match init_db_once(path) {
        Ok(conn) => Ok(conn),
        Err(err) if is_disk_io_error(&err) => {
            quarantine_corrupted_db(path);
            init_db_once(path)
        }
        Err(err) => Err(err),
    }
}

// save_entry removed (migrated to repository)

pub fn save_image_to_file(data_url: &str, data_dir: &std::path::Path) -> Option<String> {
    use std::io::Write;
    let parts: Vec<&str> = data_url.splitn(2, ',').collect();
    if parts.len() < 2 { return None; }
    
    let decoded = base64::engine::general_purpose::STANDARD.decode(parts[1]).ok()?;
    
    let attachments_dir = data_dir.join("attachments");
    if !attachments_dir.exists() {
        let _ = std::fs::create_dir_all(&attachments_dir);
    }
    
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    decoded.hash(&mut hasher);
    let hash = hasher.finish();
    
    let file_name = format!("img_{:x}.png", hash);
    let file_path = attachments_dir.join(&file_name);
    
    if !file_path.exists() {
        let mut file = std::fs::File::create(&file_path).ok()?;
        file.write_all(&decoded).ok()?;
    }
    
    Some(file_path.to_string_lossy().to_string())
}

// get_history removed (migrated to repository)

// search_history removed (migrated to repository)

// get_important_items removed (migrated to repository)

// delete_entry_db, delete_entry, enforce_storage_limit, clear_history removed (migrated to repository)

pub fn seed_defaults(conn: &Connection) -> Result<()> {
    // App settings
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.theme', 'mica')",
        [],
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
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.persistent', 'false')",
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
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.move_to_top_after_paste', 'true')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.privacy_protection', 'true')",
        [],
    );
    let _ = conn.execute("INSERT OR IGNORE INTO settings (key, value) VALUES ('app.privacy_protection_kinds', 'phone,idcard,email,secret')", []);
    let _ = conn.execute("INSERT OR IGNORE INTO settings (key, value) VALUES ('app.privacy_protection_custom_rules', '')", []);
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.use_win_v_shortcut', 'false')",
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
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.rich_paste_hotkey', 'Ctrl+Shift+Z')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.search_hotkey', 'Alt+F')",
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
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.hide_tray_icon', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.edge_docking', 'false')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.follow_mouse', 'true')",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.arrow_key_selection', 'false')",
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
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.autostart', 'true')",
        [],
    );
    let _ = conn.execute("INSERT OR IGNORE INTO settings (key, value) VALUES ('app.win_clipboard_disabled', 'false')", []);
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

    // Paste method setting: "shift_insert" (default) or "ctrl_v"
    let _ = conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('app.paste_method', 'shift_insert')",
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
    use crate::infrastructure::repository::clipboard_repo::{SqliteClipboardRepository, ClipboardRepository};
    use crate::infrastructure::repository::settings_repo::{SqliteSettingsRepository, SettingsRepository};

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
                use_count INTEGER NOT NULL DEFAULT 0,
                is_external INTEGER NOT NULL DEFAULT 0,
                pinned_order INTEGER NOT NULL DEFAULT 0
            )",
            [],
        ).unwrap();
        conn.execute(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        ).unwrap();
        conn.execute(
            "CREATE TABLE entry_tags (
                entry_id INTEGER NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY (entry_id, tag)
            )",
            [],
        ).unwrap();
        conn
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
            source_app_path: Some("C:\\TestApp.exe".to_string()),
            timestamp: 123456789,
            preview: "Hello...".to_string(),
            is_pinned: false,
            tags: vec![],
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
}
