use rusqlite::Connection;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::domain::models::ClipboardEntry;
use crate::infrastructure::encryption;
use crate::database::{
    calc_text_hash, calc_image_hash, 
    ENCRYPT_PREFIX, save_image_to_file,
    is_text_type, has_sensitive_tag
};
use rusqlite::params;
use crate::infrastructure::repository::settings_repo::SqliteSettingsRepository;
use urlencoding::decode;

const RICH_IMAGE_FALLBACK_PREFIX: &str = "<!--TIEZ_RICH_IMAGE:";
const RICH_IMAGE_FALLBACK_SUFFIX: &str = "-->";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn is_syncable_content_type(content_type: &str) -> bool {
    matches!(content_type, "text" | "code" | "url" | "rich_text" | "image")
}


pub trait ClipboardRepository {
    fn save(&self, entry: &ClipboardEntry, data_dir: Option<&std::path::Path>) -> Result<i64, String>;
    fn get_history(&self, limit: i32, offset: i32, content_type: Option<&str>) -> Result<Vec<ClipboardEntry>, String>;
    fn search(&self, query: &str, limit: i32) -> Result<Vec<ClipboardEntry>, String>;
    fn delete(&self, id: i64, data_dir: Option<&std::path::Path>) -> Result<(), String>;
    fn clear(&self, data_dir: Option<&std::path::Path>) -> Result<(), String>;
    fn get_count(&self) -> Result<i64, String>;
    fn increment_use_count(&self, id: i64) -> Result<(), String>;
    fn touch_entry(&self, id: i64, timestamp: i64) -> Result<(), String>;
    fn toggle_pin(&self, id: i64, is_pinned: bool) -> Result<(), String>;
    fn update_pinned_order(&self, orders: Vec<(i64, i64)>) -> Result<(), String>;
    fn get_entry_by_id(&self, id: i64) -> Result<Option<ClipboardEntry>, String>;
    fn get_entry_by_content(&self, content: &str, content_type: Option<&str>) -> Result<Option<i64>, String>;
    fn update_entry_content(&self, id: i64, content: &str, preview: &str) -> Result<(), String>;
    fn get_entry_content(&self, id: i64) -> Result<Option<String>, String>;
    fn get_entry_content_full(&self, id: i64) -> Result<Option<(String, String)>, String>;
    fn get_entry_content_with_html(&self, id: i64) -> Result<Option<(String, String, Option<String>)>, String>;
}

pub struct SqliteClipboardRepository {
    conn: Arc<Mutex<Connection>>,
}


impl SqliteClipboardRepository {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn encrypt_entry_with_conn(&self, conn: &Connection, id: i64) -> Result<(), String> {
        let (content_raw, preview_raw, html_raw, content_type, content_hash): (String, String, Option<String>, String, i64) =
            conn.query_row(
                "SELECT content, preview, html_content, content_type, content_hash FROM clipboard_history WHERE id = ?",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2).ok(), row.get(3)?, row.get(4)?)),
            ).map_err(|e| e.to_string())?;

        let already_encrypted = content_raw.starts_with(ENCRYPT_PREFIX)
            && preview_raw.starts_with(ENCRYPT_PREFIX)
            && html_raw
                .as_ref()
                .map(|h| h.starts_with(ENCRYPT_PREFIX))
                .unwrap_or(true);
        if already_encrypted {
            return Ok(());
        }

        let content_plain = self.maybe_decrypt_text(&content_raw);
        let preview_plain = self.maybe_decrypt_text(&preview_raw);
        let html_plain = html_raw.map(|h| self.maybe_decrypt_text(&h));

        let content_enc = self.maybe_encrypt_text(&content_plain);
        let preview_enc = self.maybe_encrypt_text(&preview_plain);
        let html_enc = html_plain.as_ref().map(|h| self.maybe_encrypt_text(h));
        let new_hash = if is_text_type(&content_type) {
            calc_text_hash(&content_plain) as i64
        } else {
            content_hash
        };

        conn.execute(
            "UPDATE clipboard_history SET content = ?, preview = ?, html_content = ?, content_hash = ? WHERE id = ?",
            params![content_enc, preview_enc, html_enc, new_hash, id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn decrypt_entry_with_conn(&self, conn: &Connection, id: i64) -> Result<(), String> {
        let (content_raw, preview_raw, html_raw, content_type, content_hash): (String, String, Option<String>, String, i64) =
            conn.query_row(
                "SELECT content, preview, html_content, content_type, content_hash FROM clipboard_history WHERE id = ?",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2).ok(), row.get(3)?, row.get(4)?)),
            ).map_err(|e| e.to_string())?;

        let any_encrypted = content_raw.starts_with(ENCRYPT_PREFIX)
            || preview_raw.starts_with(ENCRYPT_PREFIX)
            || html_raw
                .as_ref()
                .map(|h| h.starts_with(ENCRYPT_PREFIX))
                .unwrap_or(false);
        if !any_encrypted {
            return Ok(());
        }

        let content_plain = self.maybe_decrypt_text(&content_raw);
        let preview_plain = self.maybe_decrypt_text(&preview_raw);
        let html_plain = html_raw.map(|h| self.maybe_decrypt_text(&h));
        let new_hash = if is_text_type(&content_type) {
            calc_text_hash(&content_plain) as i64
        } else {
            content_hash
        };

        conn.execute(
            "UPDATE clipboard_history SET content = ?, preview = ?, html_content = ?, content_hash = ? WHERE id = ?",
            params![content_plain, preview_plain, html_plain, new_hash, id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn sync_entry_tags_with_conn(
        &self,
        conn: &Connection,
        entry_id: i64,
        tags: &[String],
    ) -> Result<(), String> {
        conn.execute("DELETE FROM entry_tags WHERE entry_id = ?", params![entry_id])
            .map_err(|e| e.to_string())?;
        for tag in tags {
            let clean = tag.trim();
            if clean.is_empty() {
                continue;
            }
            conn.execute(
                "INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                params![entry_id, clean],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn upsert_tombstone_with_conn(
        &self,
        conn: &Connection,
        content_type: &str,
        content_hash: i64,
        deleted_at: i64,
    ) -> Result<(), String> {
        if !is_syncable_content_type(content_type) || content_hash == 0 {
            return Ok(());
        }

        conn.execute(
            "INSERT INTO cloud_sync_tombstones (content_type, content_hash, deleted_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(content_type, content_hash)
             DO UPDATE SET deleted_at = MAX(cloud_sync_tombstones.deleted_at, excluded.deleted_at)",
            params![content_type, content_hash, deleted_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn clear_tombstone_with_conn(
        &self,
        conn: &Connection,
        content_type: &str,
        content_hash: i64,
    ) -> Result<(), String> {
        if !is_syncable_content_type(content_type) || content_hash == 0 {
            return Ok(());
        }

        conn.execute(
            "DELETE FROM cloud_sync_tombstones WHERE content_type = ?1 AND content_hash = ?2",
            params![content_type, content_hash],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn maybe_encrypt_text(&self, value: &str) -> String {
        #[cfg(not(feature = "portable"))]
        {
            if value.starts_with(ENCRYPT_PREFIX) {
                return value.to_string();
            }
            encryption::encrypt_value(value).unwrap_or_else(|| value.to_string())
        }
        #[cfg(feature = "portable")]
        {
            value.to_string()
        }
    }

    fn maybe_decrypt_text(&self, value: &str) -> String {
        if value.starts_with(ENCRYPT_PREFIX) {
            encryption::decrypt_value(value).unwrap_or_else(|| value.to_string())
        } else {
            value.to_string()
        }
    }

    fn extract_rich_image_fallback_payload(html: &str) -> Option<String> {
        if let Some(start) = html.rfind(RICH_IMAGE_FALLBACK_PREFIX) {
            let marker_start = start + RICH_IMAGE_FALLBACK_PREFIX.len();
            if let Some(end_rel) = html[marker_start..].find(RICH_IMAGE_FALLBACK_SUFFIX) {
                let marker_end = marker_start + end_rel;
                let payload = html[marker_start..marker_end].trim();
                if !payload.is_empty() {
                    return Some(payload.to_string());
                }
            }
        }
        None
    }

    fn fallback_payload_to_path(payload: &str) -> Option<PathBuf> {
        let value = payload.trim();
        if value.is_empty() || value.starts_with("data:image/") {
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

        if decoded_path.is_empty() {
            None
        } else {
            Some(PathBuf::from(decoded_path))
        }
    }

    fn collect_attachment_paths_for_cleanup(
        &self,
        content_raw: &str,
        html_raw: Option<&str>,
        is_external: bool,
        attachments_dir: &std::path::Path,
    ) -> Vec<PathBuf> {
        let mut paths = HashSet::new();

        if is_external {
            let content_path = PathBuf::from(self.maybe_decrypt_text(content_raw));
            if content_path.starts_with(attachments_dir) {
                paths.insert(content_path);
            }
        }

        if let Some(html_raw_value) = html_raw {
            let html = self.maybe_decrypt_text(html_raw_value);
            if let Some(payload) = Self::extract_rich_image_fallback_payload(&html) {
                if let Some(path) = Self::fallback_payload_to_path(&payload) {
                    if path.starts_with(attachments_dir) {
                        paths.insert(path);
                    }
                }
            }
        }

        paths.into_iter().collect()
    }

    pub fn save_with_conn(&self, conn: &Connection, entry: &ClipboardEntry, data_dir: Option<&std::path::Path>) -> Result<i64, String> {
        // Encrypt only when explicitly marked as sensitive
        let should_encrypt = has_sensitive_tag(&entry.tags);

        let mut final_content = entry.content.clone();
        let mut final_is_external = entry.is_external;

        // Externalize image if possible
        if entry.content_type == "image" && entry.content.starts_with("data:image/") {
            if let Some(dir) = data_dir {
                if let Some(path) = save_image_to_file(&entry.content, dir) {
                    final_content = path;
                    final_is_external = true;
                }
            }
        }

        let calculated_hash = if entry.content_type == "image" {
            if entry.content.starts_with("data:") {
                calc_image_hash(&entry.content).unwrap_or(0)
            } else {
                if let Ok(img) = image::open(&entry.content) {
                     let thumb = img.resize_exact(32, 32, image::imageops::FilterType::Nearest);
                     use std::collections::hash_map::DefaultHasher;
                     use std::hash::{Hash, Hasher};
                     let mut hasher = DefaultHasher::new();
                     thumb.as_bytes().hash(&mut hasher);
                     hasher.finish() as i64
                } else {
                    0
                }
            }
        } else {
                calc_text_hash(&final_content) as i64
        };

        // Re-adding an item should clear an older delete tombstone for the same fingerprint.
        let _ = self.clear_tombstone_with_conn(conn, &entry.content_type, calculated_hash);

        let (content, preview, content_hash, html_content) = if should_encrypt {
            let encrypted_content = self.maybe_encrypt_text(&final_content);
            let encrypted_preview = self.maybe_encrypt_text(&entry.preview);
            let encrypted_html = entry
                .html_content
                .as_ref()
                .map(|html| self.maybe_encrypt_text(html));
            (encrypted_content, encrypted_preview, calculated_hash, encrypted_html)
        } else {
            (final_content, entry.preview.clone(), calculated_hash, entry.html_content.clone())
        };

        let mut seen: HashSet<String> = HashSet::new();
        let mut cleaned_tags: Vec<String> = Vec::new();
        for tag in &entry.tags {
            let t = tag.trim();
            if t.is_empty() {
                continue;
            }
            let t_owned = t.to_string();
            if seen.insert(t_owned.clone()) {
                cleaned_tags.push(t_owned);
            }
        }

        if entry.id > 0 {
            // Update existing entry (Move to top logic)
            conn.execute(
                "UPDATE clipboard_history SET 
                    content_type = ?1, 
                    content = ?2, 
                    html_content = ?3, 
                    source_app = ?4, 
                    timestamp = ?5, 
                    preview = ?6, 
                    content_hash = ?7, 
                    tags = ?8, 
                    is_external = ?9,
                    source_app_path = ?10,
                    use_count = use_count + 1
                 WHERE id = ?11",
                params![
                    entry.content_type,
                    content,
                    html_content,
                    entry.source_app,
                    entry.timestamp,
                    preview,
                    content_hash,
                    serde_json::to_string(&cleaned_tags).unwrap_or_else(|_| "[]".to_string()),
                    if final_is_external { 1 } else { 0 },
                    entry.source_app_path.as_deref(),
                    entry.id
                ],
            ).map_err(|e| e.to_string())?;
            self.sync_entry_tags_with_conn(conn, entry.id, &cleaned_tags)?;
            Ok(entry.id)
        } else {
            // Insert new entry
            conn.execute(
                "INSERT INTO clipboard_history (content_type, content, html_content, source_app, timestamp, preview, is_pinned, content_hash, tags, is_external, pinned_order, source_app_path) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    entry.content_type,
                    content,
                    html_content,
                    entry.source_app,
                    entry.timestamp,
                    preview,
                    if entry.is_pinned { 1 } else { 0 },
                    content_hash,
                    serde_json::to_string(&cleaned_tags).unwrap_or_else(|_| "[]".to_string()),
                    if final_is_external { 1 } else { 0 },
                    entry.pinned_order,
                    entry.source_app_path.as_deref()
                ],
            ).map_err(|e| e.to_string())?;

            let new_id = conn.last_insert_rowid();
            self.sync_entry_tags_with_conn(conn, new_id, &cleaned_tags)?;
            Ok(new_id)
        }
    }

    pub fn delete_with_conn(&self, conn: &Connection, id: i64, data_dir: Option<&std::path::Path>) -> Result<(), String> {
        let mut tombstone: Option<(String, i64)> = None;
        // Check for external files to delete
        if let Some(dir) = data_dir {
             let attachments_dir = dir.join("attachments");
             let mut stmt = conn
                 .prepare("SELECT content, html_content, is_external, content_type, content_hash FROM clipboard_history WHERE id = ?")
                 .map_err(|e| e.to_string())?;
             
             if let Ok(entry) = stmt.query_row([id], |row| {
                 let content_raw: String = row.get(0)?;
                 let html_raw: Option<String> = row.get(1).ok();
                 let is_ext: i32 = row.get(2)?;
                 let content_type: String = row.get(3)?;
                 let content_hash: i64 = row.get(4)?;
                 Ok((content_raw, html_raw, is_ext == 1, content_type, content_hash))
             }) {
                 let files_to_remove = self.collect_attachment_paths_for_cleanup(
                     &entry.0,
                     entry.1.as_deref(),
                     entry.2,
                     &attachments_dir,
                 );
                 for path in files_to_remove {
                     if path.exists() {
                         let _ = std::fs::remove_file(path);
                     }
                 }
                 tombstone = Some((entry.3, entry.4));
             }
        } else {
            let mut stmt = conn
                .prepare("SELECT content_type, content_hash FROM clipboard_history WHERE id = ?")
                .map_err(|e| e.to_string())?;
            if let Ok(entry) = stmt.query_row([id], |row| {
                let content_type: String = row.get(0)?;
                let content_hash: i64 = row.get(1)?;
                Ok((content_type, content_hash))
            }) {
                tombstone = Some(entry);
            }
        }

        if let Some((content_type, content_hash)) = tombstone {
            let _ = self.upsert_tombstone_with_conn(conn, &content_type, content_hash, now_ms());
        }

        conn.execute("DELETE FROM clipboard_history WHERE id = ?", [id]).map_err(|e| e.to_string())?;
        let _ = conn.execute("DELETE FROM entry_tags WHERE entry_id = ?", params![id]);
        Ok(())
    }

    pub fn delete_metadata_with_conn(&self, conn: &Connection, id: i64) -> Result<(), String> {
        conn.execute("DELETE FROM clipboard_history WHERE id = ?", params![id]).map_err(|e| e.to_string())?;
        let _ = conn.execute("DELETE FROM entry_tags WHERE entry_id = ?", params![id]);
        Ok(())
    }

    pub fn find_by_content_with_conn(&self, conn: &Connection, content: &str, content_type: Option<&str>) -> Result<Option<i64>, String> {
        if content_type == Some("image") {
            if let Some(hash) = calc_image_hash(content) {
                let mut stmt = conn.prepare(
                    "SELECT id FROM clipboard_history \
                     WHERE (content_type = 'image' AND content_hash = ?) OR content = ?",
                ).map_err(|e| e.to_string())?;
                let mut rows = stmt.query(params![hash, content]).map_err(|e| e.to_string())?;
                if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                    return Ok(Some(row.get(0).map_err(|e| e.to_string())?));
                }
                return Ok(None);
            }
        }

        let hash = calc_text_hash(content) as i64;
        
        if let Some(ct) = content_type {
            let mut stmt = conn.prepare(
                "SELECT id FROM clipboard_history \
                 WHERE (content_type = ? AND content_hash = ?) OR (content_type = ? AND content = ?)",
            ).map_err(|e| e.to_string())?;
            let mut rows = stmt.query(params![ct, hash, ct, content]).map_err(|e| e.to_string())?;
            if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                Ok(Some(row.get(0).map_err(|e| e.to_string())?))
            } else {
                Ok(None)
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id FROM clipboard_history \
                 WHERE ((content_type IN ('text', 'rich_text', 'code', 'url')) AND content_hash = ?) OR content = ?",
            ).map_err(|e| e.to_string())?;
            let mut rows = stmt.query(params![hash, content]).map_err(|e| e.to_string())?;
            if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                Ok(Some(row.get(0).map_err(|e| e.to_string())?))
            } else {
                Ok(None)
            }
        }
    }

    pub fn enforce_limit_with_conn(&self, conn: &Connection, data_dir: Option<&std::path::Path>) -> Result<Vec<i64>, String> {
        // Check if storage limit is enabled
        if let Ok(Some(limit_enabled_str)) = SqliteSettingsRepository::get_raw(conn, "app.persistent_limit_enabled") {
            if limit_enabled_str == "false" {
                return Ok(Vec::new());
            }
        }
        
        // Get the storage limit
        if let Ok(Some(limit_str)) = SqliteSettingsRepository::get_raw(conn, "app.persistent_limit") {
            if let Ok(limit) = limit_str.parse::<i32>() {
                // Count non-pinned entries that have no tags
                let count: i32 = conn.query_row(
                    "SELECT COUNT(*) FROM clipboard_history WHERE is_pinned = 0 AND (tags = '[]' OR tags IS NULL)",
                    [],
                    |row| row.get(0)
                ).map_err(|e| e.to_string())?;
                
                if count > limit {
                    // First, get the IDs that will be deleted
                    let to_delete = count - limit;
                    let deleted_ids: Vec<i64> = {
                        let mut stmt = conn.prepare(
                            "SELECT id FROM clipboard_history 
                             WHERE is_pinned = 0 AND (tags = '[]' OR tags IS NULL)
                             ORDER BY timestamp ASC 
                             LIMIT ?"
                        ).map_err(|e| e.to_string())?;
                        
                        let rows = stmt.query_map([to_delete], |row| row.get(0)).map_err(|e| e.to_string())?;
                        rows.filter_map(|r| r.ok()).collect()
                    };
                    // Actually delete records (and files if needed)
                    for id in &deleted_ids {
                        let _ = self.delete_with_conn(conn, *id, data_dir);
                    }
                    return Ok(deleted_ids);
                }
            }
        }
        
        Ok(Vec::new())
    }
    pub fn toggle_pin_with_conn(&self, conn: &Connection, id: i64, is_pinned: bool) -> Result<(), String> {
        if is_pinned {
            // Set pinned_order to max + 1 so it appears at top
            conn.execute(
                "UPDATE clipboard_history 
                 SET is_pinned = 1, 
                     pinned_order = (SELECT COALESCE(MAX(pinned_order), 0) + 1 FROM clipboard_history WHERE is_pinned = 1) 
                 WHERE id = ?",
                params![id],
            ).map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "UPDATE clipboard_history SET is_pinned = 0, pinned_order = 0 WHERE id = ?",
                params![id],
            ).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn update_pinned_order_with_conn(&self, conn: &Connection, orders: Vec<(i64, i64)>) -> Result<(), String> {
        for (id, order) in orders {
            conn.execute(
                "UPDATE clipboard_history SET pinned_order = ? WHERE id = ?",
                params![order, id],
            ).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn get_entry_by_id_with_conn(&self, conn: &Connection, id: i64) -> Result<Option<ClipboardEntry>, String> {
        let mut stmt = conn.prepare(
            "SELECT id, content_type, content, html_content, source_app, timestamp, preview, is_pinned, tags, use_count, is_external, pinned_order, source_app_path 
             FROM clipboard_history 
             WHERE id = ? 
             LIMIT 1",
        ).map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let tags_str: String = row.get(8).unwrap_or_else(|_| "[]".to_string());
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
    
            let content_raw: String = row.get(2).map_err(|e| e.to_string())?;
            let html_raw: Option<String> = row.get(3).map_err(|e| e.to_string()).unwrap_or(None);
            let preview_raw: String = row.get(6).map_err(|e| e.to_string())?;
            let content = self.maybe_decrypt_text(&content_raw);
            let preview = self.maybe_decrypt_text(&preview_raw);
            let html_content = html_raw.map(|v| self.maybe_decrypt_text(&v));
    
            Ok(Some(ClipboardEntry {
                id: row.get(0).map_err(|e| e.to_string())?,
                content_type: row.get(1).map_err(|e| e.to_string())?,
                content,
                html_content,
                source_app: row.get(4).map_err(|e| e.to_string())?,
                timestamp: row.get(5).map_err(|e| e.to_string())?,
                preview,
                is_pinned: row.get::<_, i32>(7).map_err(|e| e.to_string())? == 1,
                tags,
                use_count: row.get(9).unwrap_or(0),
                is_external: row.get::<_, i32>(10).unwrap_or(0) == 1,
                pinned_order: row.get(11).unwrap_or(0),
                source_app_path: row.get(12).unwrap_or(None),
                file_preview_exists: true,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn update_entry_content_with_conn(
        &self,
        conn: &Connection,
        id: i64,
        content: &str,
        preview: &str,
    ) -> Result<(), String> {
        let (old_content_raw, content_type, tags_json) = conn
            .query_row(
                "SELECT content, content_type, tags FROM clipboard_history WHERE id = ?",
                params![id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
            )
            .map_err(|e| e.to_string())?;

        let old_content = self.maybe_decrypt_text(&old_content_raw);
        if old_content == content {
            return Ok(());
        }

        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let should_encrypt = has_sensitive_tag(&tags);

        if is_text_type(&content_type) {
            let hash = calc_text_hash(content) as i64;
            let new_type = if content_type == "rich_text" { "text" } else { &content_type };
            if should_encrypt {
                let encrypted_content = self.maybe_encrypt_text(content);
                let encrypted_preview = self.maybe_encrypt_text(preview);
                conn.execute(
                    "UPDATE clipboard_history SET content = ?, preview = ?, content_hash = ?, html_content = NULL, content_type = ? WHERE id = ?",
                    params![encrypted_content, encrypted_preview, hash, new_type, id],
                ).map_err(|e| e.to_string())?;
            } else {
                conn.execute(
                    "UPDATE clipboard_history SET content = ?, preview = ?, content_hash = ?, html_content = NULL, content_type = ? WHERE id = ?",
                    params![content, preview, hash, new_type, id],
                ).map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
        if should_encrypt {
            let encrypted_content = self.maybe_encrypt_text(content);
            let encrypted_preview = self.maybe_encrypt_text(preview);
            conn.execute(
                "UPDATE clipboard_history SET content = ?, preview = ?, html_content = NULL WHERE id = ?",
                params![encrypted_content, encrypted_preview, id],
            ).map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "UPDATE clipboard_history SET content = ?, preview = ?, html_content = NULL WHERE id = ?",
                params![content, preview, id],
            ).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn get_entry_content_full_with_conn(&self, conn: &Connection, id: i64) -> Result<Option<(String, String)>, String> {
        let mut stmt =
            conn.prepare("SELECT content, content_type FROM clipboard_history WHERE id = ?").map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let content: String = row.get(0).map_err(|e| e.to_string())?;
            let content_type: String = row.get(1).map_err(|e| e.to_string())?;
            Ok(Some((self.maybe_decrypt_text(&content), content_type)))
        } else {
            Ok(None)
        }
    }

    pub fn get_entry_content_with_html_with_conn(
        &self,
        conn: &Connection,
        id: i64,
    ) -> Result<Option<(String, String, Option<String>)>, String> {
        let mut stmt = conn.prepare(
            "SELECT content, content_type, html_content FROM clipboard_history WHERE id = ?",
        ).map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let content: String = row.get(0).map_err(|e| e.to_string())?;
            let content_type: String = row.get(1).map_err(|e| e.to_string())?;
            let html_raw: Option<String> = row.get(2).map_err(|e| e.to_string()).unwrap_or(None);
            let html_content = html_raw.map(|v| self.maybe_decrypt_text(&v));
            Ok(Some((self.maybe_decrypt_text(&content), content_type, html_content)))
        } else {
            Ok(None)
        }
    }
}

impl ClipboardRepository for SqliteClipboardRepository {
    fn save(&self, entry: &ClipboardEntry, data_dir: Option<&std::path::Path>) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.save_with_conn(&conn, entry, data_dir)
    }

    fn get_history(&self, limit: i32, offset: i32, content_type: Option<&str>) -> Result<Vec<ClipboardEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let map_row = |row: &rusqlite::Row| {
            let tags_str: String = row.get(8).unwrap_or_else(|_| "[]".to_string());
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            let content_type: String = row.get(1)?;
            let content_raw: String = row.get(2)?;
            let html_raw: Option<String> = row.get(3).ok();
            let preview_raw: String = row.get(6)?;
            let content = self.maybe_decrypt_text(&content_raw);
            let preview = self.maybe_decrypt_text(&preview_raw);
            let html_content = html_raw.as_ref().map(|v| self.maybe_decrypt_text(v));

            Ok((
                ClipboardEntry {
                    id: row.get(0)?,
                    content_type,
                    content,
                    html_content,
                    source_app: row.get(4)?,
                    timestamp: row.get(5)?,
                    preview,
                    is_pinned: row.get::<_, i32>(7)? == 1,
                    tags,
                    use_count: row.get(9).unwrap_or(0),
                    is_external: row.get::<_, i32>(10)? == 1,
                    pinned_order: row.get(11).unwrap_or(0),
                    source_app_path: row.get(12).unwrap_or(None),
                    file_preview_exists: {
                        let is_ext = row.get::<_, i32>(10)? == 1;
                        if is_ext {
                            let c: String = self.maybe_decrypt_text(&row.get::<_, String>(2)?);
                            std::path::Path::new(&c).exists()
                        } else {
                            true
                        }
                    },
                },
                content_raw,
                preview_raw,
                html_raw,
            ))
        };

        let mut mapped_rows = Vec::new();
        if let Some(ct) = content_type {
            let mut stmt = conn.prepare(
                "SELECT id, content_type, content, html_content, source_app, timestamp, preview, is_pinned, tags, use_count, is_external, pinned_order, source_app_path 
                 FROM clipboard_history 
                 WHERE content_type = ? 
                 ORDER BY is_pinned DESC, pinned_order DESC, timestamp DESC, id DESC 
                 LIMIT ? OFFSET ?",
            ).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![ct, limit, offset], map_row)
                .map_err(|e| e.to_string())?;
            for row in rows {
                mapped_rows.push(row.map_err(|e| e.to_string())?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, content_type, content, html_content, source_app, timestamp, preview, is_pinned, tags, use_count, is_external, pinned_order, source_app_path 
                 FROM clipboard_history 
                 ORDER BY is_pinned DESC, pinned_order DESC, timestamp DESC, id DESC 
                 LIMIT ? OFFSET ?",
            ).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([limit, offset], map_row)
                .map_err(|e| e.to_string())?;
            for row in rows {
                mapped_rows.push(row.map_err(|e| e.to_string())?);
            }
        }

        let mut history = Vec::new();
        for (entry, content_raw, preview_raw, html_raw) in mapped_rows {
            #[cfg(not(feature = "portable"))]
            {
                let is_sensitive = has_sensitive_tag(&entry.tags);
                let content_encrypted = content_raw.starts_with(ENCRYPT_PREFIX);
                let preview_encrypted = preview_raw.starts_with(ENCRYPT_PREFIX);
                let html_encrypted = html_raw
                    .as_ref()
                    .map(|h| h.starts_with(ENCRYPT_PREFIX))
                    .unwrap_or(false);
                let html_needs_encrypt = html_raw
                    .as_ref()
                    .map(|h| !h.starts_with(ENCRYPT_PREFIX))
                    .unwrap_or(false);

                if is_sensitive && (!content_encrypted || !preview_encrypted || html_needs_encrypt) {
                    let _ = self.encrypt_entry_with_conn(&conn, entry.id);
                } else if !is_sensitive && (content_encrypted || preview_encrypted || html_encrypted) {
                    let _ = self.decrypt_entry_with_conn(&conn, entry.id);
                }
            }

            history.push(entry);
        }
        Ok(history)
    }

    fn search(&self, query: &str, limit: i32) -> Result<Vec<ClipboardEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        
        let term = query.trim().to_lowercase();
        if term.is_empty() { return Ok(Vec::new()); }

        #[cfg(feature = "portable")]
        {
            // Portable version: Data is NOT encrypted, use conventional SQL LIKE search (fastest)
            let mut stmt = conn.prepare(
                "SELECT DISTINCT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.timestamp, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path 
                 FROM clipboard_history ch
                 LEFT JOIN entry_tags et ON ch.id = et.entry_id
                 WHERE ch.content LIKE '%' || ? || '%' 
                    OR ch.source_app LIKE '%' || ? || '%' 
                    OR et.tag LIKE '%' || ? || '%'
                 ORDER BY ch.timestamp DESC 
                 LIMIT ?",
            ).map_err(|e| e.to_string())?;

            let rows = stmt.query_map(params![term, term, term, limit], |row| {
                 let tags_str: String = row.get::<_, String>(8).unwrap_or_else(|_| "[]".to_string());
                 Ok(ClipboardEntry {
                    id: row.get(0)?,
                    content_type: row.get(1)?,
                    content: row.get(2)?,
                    html_content: row.get(3).ok(),
                    source_app: row.get(4)?,
                    timestamp: row.get(5)?,
                    preview: row.get(6)?,
                    is_pinned: row.get::<_, i32>(7)? == 1,
                    tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                    use_count: row.get(9).unwrap_or(0),
                    is_external: row.get::<_, i32>(10)? == 1,
                    pinned_order: row.get(11).unwrap_or(0),
                    source_app_path: row.get(12).unwrap_or(None),
                    file_preview_exists: true // Simplified for search
                 })
            }).map_err(|e| e.to_string())?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| e.to_string())?);
            }
            Ok(results)
        }

        #[cfg(not(feature = "portable"))]
        {
            let mut results: Vec<ClipboardEntry> = Vec::new();
            let mut seen: HashSet<i64> = HashSet::new();

            let sensitive_tags_sql = {
                let tags = crate::database::SENSITIVE_TAGS;
                let parts: Vec<String> = tags
                    .iter()
                    .map(|t| format!("'{}'", t.replace('\'', "''")))
                    .collect();
                format!("({})", parts.join(","))
            };

            // 1) SQL search for non-sensitive (plaintext) entries
            let sql_non_sensitive = format!(
                "SELECT DISTINCT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.timestamp, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path 
                 FROM clipboard_history ch
                 LEFT JOIN entry_tags et ON ch.id = et.entry_id
                 WHERE NOT EXISTS (
                     SELECT 1 FROM entry_tags se 
                     WHERE se.entry_id = ch.id 
                       AND se.tag COLLATE NOCASE IN {}
                 )
                   AND (
                     ch.content LIKE '%' || ?1 || '%' 
                     OR ch.source_app LIKE '%' || ?1 || '%' 
                     OR et.tag LIKE '%' || ?1 || '%'
                   )
                 ORDER BY ch.timestamp DESC, ch.id DESC
                 LIMIT ?2",
                sensitive_tags_sql
            );

            let mut stmt = conn.prepare(&sql_non_sensitive).map_err(|e| e.to_string())?;
            let rows = stmt.query_map(params![term, limit], |row| {
                let tags_str: String = row.get(8).unwrap_or_else(|_| "[]".to_string());
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                let content_raw: String = row.get(2)?;
                let preview_raw: String = row.get(6)?;
                let html_raw: Option<String> = row.get(3).ok();
                let content = self.maybe_decrypt_text(&content_raw);
                let preview = self.maybe_decrypt_text(&preview_raw);
                let html_content = html_raw.map(|v| self.maybe_decrypt_text(&v));

                Ok(ClipboardEntry {
                    id: row.get(0)?,
                    content_type: row.get(1)?,
                    content,
                    html_content,
                    source_app: row.get(4)?,
                    timestamp: row.get(5)?,
                    preview,
                    is_pinned: row.get::<_, i32>(7)? == 1,
                    tags,
                    use_count: row.get(9).unwrap_or(0),
                    is_external: row.get::<_, i32>(10)? == 1,
                    pinned_order: row.get(11).unwrap_or(0),
                    source_app_path: row.get(12).unwrap_or(None),
                    file_preview_exists: true,
                })
            }).map_err(|e| e.to_string())?;

            for row in rows {
                if let Ok(entry) = row {
                    if seen.insert(entry.id) {
                        results.push(entry);
                    }
                }
            }

            // 2) Decrypt-scan sensitive or encrypted entries (only if needed)
            if results.len() < limit as usize {
                let mut cursor_ts = i64::MAX;
                let mut cursor_id = i64::MAX;
                let batch_size = 500;
                let enc_like = format!("{}%", ENCRYPT_PREFIX);
                let sql_sensitive = format!(
                    "SELECT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.timestamp, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path 
                     FROM clipboard_history ch
                     WHERE (
                         EXISTS (
                             SELECT 1 FROM entry_tags se 
                             WHERE se.entry_id = ch.id 
                               AND se.tag COLLATE NOCASE IN {}
                         )
                         OR ch.content LIKE ?1 
                         OR ch.preview LIKE ?1 
                         OR ch.html_content LIKE ?1
                     )
                       AND ((ch.timestamp < ?2) OR (ch.timestamp = ?2 AND ch.id < ?3))
                     ORDER BY ch.timestamp DESC, ch.id DESC
                     LIMIT ?4",
                    sensitive_tags_sql
                );

                loop {
                    let mut stmt = conn.prepare(&sql_sensitive).map_err(|e| e.to_string())?;
                    let rows = stmt.query_map(
                        params![enc_like, cursor_ts, cursor_id, batch_size],
                        |row| {
                            let tags_str: String = row.get(8).unwrap_or_else(|_| "[]".to_string());
                            Ok(ClipboardEntry {
                                id: row.get(0)?,
                                content_type: row.get(1)?,
                                content: row.get(2)?, // Encrypted
                                html_content: row.get(3).ok(),
                                source_app: row.get(4)?,
                                timestamp: row.get(5)?,
                                preview: row.get(6)?, // Encrypted
                                is_pinned: row.get::<_, i32>(7)? == 1,
                                tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                                use_count: row.get(9).unwrap_or(0),
                                is_external: row.get::<_, i32>(10)? == 1,
                                pinned_order: row.get(11).unwrap_or(0),
                                source_app_path: row.get(12).unwrap_or(None),
                                file_preview_exists: true,
                            })
                        },
                    ).map_err(|e| e.to_string())?;

                    let mut batch: Vec<ClipboardEntry> = Vec::new();
                    for row in rows {
                        if let Ok(mut entry) = row {
                            entry.content = self.maybe_decrypt_text(&entry.content);
                            entry.preview = self.maybe_decrypt_text(&entry.preview);
                            if let Some(html) = entry.html_content.take() {
                                entry.html_content = Some(self.maybe_decrypt_text(&html));
                            }
                            batch.push(entry);
                        }
                    }

                    if batch.is_empty() {
                        break;
                    }

                    for entry in batch.iter() {
                        let matches = entry.content.to_lowercase().contains(&term)
                            || entry.source_app.to_lowercase().contains(&term)
                            || entry
                                .tags
                                .iter()
                                .any(|t| t.to_lowercase().contains(&term));

                        if matches && seen.insert(entry.id) {
                            results.push(entry.clone());
                            if results.len() >= limit as usize {
                                break;
                            }
                        }
                    }

                    if results.len() >= limit as usize {
                        break;
                    }

                    if let Some(last) = batch.last() {
                        cursor_ts = last.timestamp;
                        cursor_id = last.id;
                    } else {
                        break;
                    }
                }
            }

            results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then(b.id.cmp(&a.id)));
            if results.len() > limit as usize {
                results.truncate(limit as usize);
            }
            Ok(results)
        }
    }

    fn delete(&self, id: i64, data_dir: Option<&std::path::Path>) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.delete_with_conn(&conn, id, data_dir)
    }

    fn clear(&self, data_dir: Option<&std::path::Path>) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        // Get IDs of unpinned items without tags.
        let mut stmt = conn.prepare(
            "SELECT id FROM clipboard_history 
             WHERE is_pinned = 0 
               AND NOT EXISTS (SELECT 1 FROM entry_tags WHERE entry_id = clipboard_history.id)",
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0)).map_err(|e| e.to_string())?;
        let ids: Vec<i64> = rows.filter_map(Result::ok).collect();

        // Delete one-by-one so tombstones are recorded for cloud deletion sync.
        for id in &ids {
            self.delete_with_conn(&conn, *id, data_dir)?;
        }

        // VACUUM to reclaim space
        let _ = conn.execute_batch("VACUUM;");
        Ok(())
    }

    fn get_count(&self) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM clipboard_history").map_err(|e| e.to_string())?;
        let count: i64 = stmt.query_row([], |row| row.get(0)).map_err(|e| e.to_string())?;
        Ok(count)
    }

    fn increment_use_count(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE clipboard_history SET use_count = use_count + 1 WHERE id = ?",
            params![id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn touch_entry(&self, id: i64, timestamp: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE clipboard_history SET timestamp = ? WHERE id = ?",
            params![timestamp, id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn toggle_pin(&self, id: i64, is_pinned: bool) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.toggle_pin_with_conn(&conn, id, is_pinned)
    }

    fn update_pinned_order(&self, orders: Vec<(i64, i64)>) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|e| e.to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        self.update_pinned_order_with_conn(&tx, orders)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn get_entry_by_id(&self, id: i64) -> Result<Option<ClipboardEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.get_entry_by_id_with_conn(&conn, id)
    }

    fn get_entry_by_content(&self, content: &str, content_type: Option<&str>) -> Result<Option<i64>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.find_by_content_with_conn(&conn, content, content_type)
    }

    fn update_entry_content(&self, id: i64, content: &str, preview: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.update_entry_content_with_conn(&conn, id, content, preview)
    }

    fn get_entry_content(&self, id: i64) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT content FROM clipboard_history WHERE id = ?").map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let content: String = row.get(0).map_err(|e| e.to_string())?;
            Ok(Some(self.maybe_decrypt_text(&content)))
        } else {
            Ok(None)
        }
    }

    fn get_entry_content_full(&self, id: i64) -> Result<Option<(String, String)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.get_entry_content_full_with_conn(&conn, id)
    }

    fn get_entry_content_with_html(&self, id: i64) -> Result<Option<(String, String, Option<String>)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.get_entry_content_with_html_with_conn(&conn, id)
    }
}
