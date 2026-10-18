use crate::database::{
    calc_image_hash, calc_image_hash_from_bytes, calc_sync_content_hash, calc_text_hash,
    has_sensitive_tag, is_text_type, save_image_bytes_to_file_with_extension, save_image_to_file,
    ENCRYPT_PREFIX,
};
use crate::domain::models::ClipboardEntry;
use crate::infrastructure::encryption;
use crate::infrastructure::repository::settings_repo::SqliteSettingsRepository;
use crate::services::clipboard::entry_matches_search;
use rusqlite::params;
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use urlencoding::decode;

const RICH_IMAGE_FALLBACK_PREFIX: &str = "<!--TIEZ_RICH_IMAGE:";
const RICH_IMAGE_FALLBACK_SUFFIX: &str = "-->";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ClipboardSortMode {
    #[default]
    Activity,
    Created,
    LastUsed,
    Usage,
    Manual,
    /// Newest copy first, pinned or not. Not a list setting: the plain text
    /// paste hotkey uses it so a pin at the top of the list never wins.
    Recent,
}

impl ClipboardSortMode {
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("created") => Self::Created,
            Some("last_used") => Self::LastUsed,
            Some("usage") => Self::Usage,
            Some("manual") => Self::Manual,
            Some(_) => Self::Activity,
            None => Self::Manual,
        }
    }

    fn order_clause(self) -> &'static str {
        match self {
            Self::Activity => {
                " ORDER BY is_pinned DESC, CASE WHEN is_pinned = 1 THEN pinned_order ELSE sort_at END DESC, sort_at DESC, id DESC LIMIT ? OFFSET ?"
            }
            Self::Created => {
                " ORDER BY is_pinned DESC, CASE WHEN is_pinned = 1 THEN pinned_order WHEN created_at > 0 THEN created_at ELSE sort_at END DESC, sort_at DESC, id DESC LIMIT ? OFFSET ?"
            }
            Self::LastUsed => {
                " ORDER BY is_pinned DESC, CASE WHEN is_pinned = 1 THEN pinned_order ELSE last_used_at END DESC, sort_at DESC, id DESC LIMIT ? OFFSET ?"
            }
            Self::Usage => {
                " ORDER BY is_pinned DESC, CASE WHEN is_pinned = 1 THEN pinned_order ELSE use_count END DESC, sort_at DESC, id DESC LIMIT ? OFFSET ?"
            }
            Self::Manual => {
                " ORDER BY is_pinned DESC, CASE WHEN is_pinned = 1 THEN pinned_order ELSE 0 END DESC, sort_at DESC, id DESC LIMIT ? OFFSET ?"
            }
            Self::Recent => " ORDER BY sort_at DESC, id DESC LIMIT ? OFFSET ?",
        }
    }

    pub fn compare(self, a: &ClipboardEntry, b: &ClipboardEntry) -> std::cmp::Ordering {
        if self == Self::Recent {
            return b.sort_at.cmp(&a.sort_at).then_with(|| b.id.cmp(&a.id));
        }
        let pinned_comparison = b.is_pinned.cmp(&a.is_pinned);
        if pinned_comparison != std::cmp::Ordering::Equal {
            return pinned_comparison;
        }
        if a.is_pinned {
            return b
                .pinned_order
                .cmp(&a.pinned_order)
                .then_with(|| b.sort_at.cmp(&a.sort_at))
                .then_with(|| b.id.cmp(&a.id));
        }

        let by_selected_mode = match self {
            Self::Activity => b.sort_at.cmp(&a.sort_at),
            Self::Created => {
                let a_created_at = if a.created_at > 0 {
                    a.created_at
                } else {
                    a.timestamp
                };
                let b_created_at = if b.created_at > 0 {
                    b.created_at
                } else {
                    b.timestamp
                };
                b_created_at.cmp(&a_created_at)
            }
            Self::LastUsed => b.last_used_at.cmp(&a.last_used_at),
            Self::Usage => b.use_count.cmp(&a.use_count),
            Self::Manual | Self::Recent => b.sort_at.cmp(&a.sort_at),
        };

        by_selected_mode
            .then_with(|| b.sort_at.cmp(&a.sort_at))
            .then_with(|| b.id.cmp(&a.id))
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn is_syncable_content_type(content_type: &str) -> bool {
    matches!(
        content_type,
        "text" | "code" | "url" | "rich_text" | "image" | "file" | "video" | "emoji_sync"
    )
}

pub trait ClipboardRepository {
    fn save(
        &self,
        entry: &ClipboardEntry,
        data_dir: Option<&std::path::Path>,
    ) -> Result<i64, String>;
    fn get_history(
        &self,
        limit: i32,
        offset: i32,
        content_type: Option<&str>,
    ) -> Result<Vec<ClipboardEntry>, String>;
    fn get_history_sorted(
        &self,
        limit: i32,
        offset: i32,
        content_type: Option<&str>,
        sort_mode: ClipboardSortMode,
    ) -> Result<Vec<ClipboardEntry>, String> {
        let _ = sort_mode;
        self.get_history(limit, offset, content_type)
    }
    fn get_history_since(
        &self,
        since_timestamp: i64,
        limit: i32,
        offset: i32,
        content_type: Option<&str>,
    ) -> Result<Vec<ClipboardEntry>, String>;
    fn search(
        &self,
        query: &str,
        limit: i32,
        tag_only: bool,
        note_only: bool,
    ) -> Result<Vec<ClipboardEntry>, String>;
    fn delete(&self, id: i64, data_dir: Option<&std::path::Path>) -> Result<(), String>;
    fn clear(&self, data_dir: Option<&std::path::Path>) -> Result<(), String>;
    fn get_count(&self) -> Result<i64, String>;
    fn increment_use_count(&self, id: i64) -> Result<(), String>;
    fn touch_entry(&self, id: i64, timestamp: i64) -> Result<(), String>;
    fn toggle_pin(&self, id: i64, is_pinned: bool) -> Result<(), String>;
    fn update_pinned_order(&self, orders: Vec<(i64, i64)>) -> Result<(), String>;
    fn get_entry_by_id(&self, id: i64) -> Result<Option<ClipboardEntry>, String>;
    fn get_entry_by_content(
        &self,
        content: &str,
        content_type: Option<&str>,
    ) -> Result<Option<i64>, String>;
    fn update_entry_content(&self, id: i64, content: &str, preview: &str) -> Result<(), String>;
    fn update_entry_note(&self, id: i64, note: &str) -> Result<(), String>;
    fn get_entry_content(&self, id: i64) -> Result<Option<String>, String>;
    fn get_entry_content_full(&self, id: i64) -> Result<Option<(String, String)>, String>;
    fn get_entry_content_with_html(
        &self,
        id: i64,
    ) -> Result<Option<(String, String, Option<String>)>, String>;
}

pub struct SqliteClipboardRepository {
    conn: Arc<Mutex<Connection>>,
}

struct RawHistoryRow {
    id: i64,
    content_type: String,
    content_raw: String,
    html_raw: Option<String>,
    preview_raw: String,
    source_app: String,
    timestamp: i64,
    created_at: i64,
    last_used_at: i64,
    sort_at: i64,
    is_pinned: bool,
    tags: Vec<String>,
    use_count: i32,
    is_external: bool,
    pinned_order: i64,
    source_app_path: Option<String>,
    note: String,
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

        let content_plain = self.maybe_decrypt_text(&content_raw)?;
        let preview_plain = self.maybe_decrypt_text(&preview_raw)?;
        let html_plain = html_raw.map(|h| self.maybe_decrypt_text(&h)).transpose()?;

        let content_enc = self.maybe_encrypt_text(&content_plain)?;
        let preview_enc = self.maybe_encrypt_text(&preview_plain)?;
        let html_enc = html_plain.as_ref().map(|h| self.maybe_encrypt_text(h)).transpose()?;
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

        let any_encrypted = encryption::is_encrypted_value(&content_raw)
            || encryption::is_encrypted_value(&preview_raw)
            || html_raw
                .as_ref()
                .map(|h| encryption::is_encrypted_value(h))
                .unwrap_or(false);
        if !any_encrypted {
            return Ok(());
        }

        let content_plain = self.maybe_decrypt_text(&content_raw)?;
        let preview_plain = self.maybe_decrypt_text(&preview_raw)?;
        let html_plain = html_raw.map(|h| self.maybe_decrypt_text(&h)).transpose()?;
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
        conn.execute(
            "DELETE FROM entry_tags WHERE entry_id = ?",
            params![entry_id],
        )
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

    fn maybe_encrypt_text(&self, value: &str) -> Result<String, String> {
        encryption::encrypt_for_storage(value)
    }

    fn maybe_decrypt_text(&self, value: &str) -> Result<String, String> {
        encryption::decrypt_stored_value(value)
    }

    pub(crate) fn validate_entry_with_conn(&self, conn: &Connection, id: i64) -> Result<(), String> {
        let (content, preview, html): (String, String, Option<String>) = conn.query_row(
            "SELECT content, preview, html_content FROM clipboard_history WHERE id = ?",
            [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).map_err(|e| e.to_string())?;
        self.maybe_decrypt_text(&content)?;
        self.maybe_decrypt_text(&preview)?;
        html.as_deref().map(|v| self.maybe_decrypt_text(v)).transpose()?;
        Ok(())
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
    ) -> Result<Vec<PathBuf>, String> {
        let mut paths = HashSet::new();

        if is_external {
            let content_path = PathBuf::from(self.maybe_decrypt_text(content_raw)?);
            if content_path.starts_with(attachments_dir) {
                paths.insert(content_path);
            }
        }

        if let Some(html_raw_value) = html_raw {
            let html = self.maybe_decrypt_text(html_raw_value)?;
            if let Some(payload) = Self::extract_rich_image_fallback_payload(&html) {
                if let Some(path) = Self::fallback_payload_to_path(&payload) {
                    if path.starts_with(attachments_dir) {
                        paths.insert(path);
                    }
                }
            }
        }

        Ok(paths.into_iter().collect())
    }

    fn parse_raw_history_row(row: &rusqlite::Row<'_>) -> Result<RawHistoryRow, rusqlite::Error> {
        let tags_str: String = row.get(8).unwrap_or_else(|_| "[]".to_string());
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        Ok(RawHistoryRow {
            id: row.get(0)?,
            content_type: row.get(1)?,
            content_raw: row.get(2)?,
            html_raw: row.get(3).ok(),
            preview_raw: row.get(6)?,
            source_app: row.get(4)?,
            timestamp: row.get(5)?,
            is_pinned: row.get::<_, i32>(7)? == 1,
            tags,
            use_count: row.get(9).unwrap_or(0),
            is_external: row.get::<_, i32>(10)? == 1,
            pinned_order: row.get(11).unwrap_or(0),
            source_app_path: row.get(12).unwrap_or(None),
            note: row.get::<_, String>(13).unwrap_or_default(),
            created_at: row.get(14).unwrap_or_else(|_| row.get(5).unwrap_or(0)),
            last_used_at: row.get(15).unwrap_or(0),
            sort_at: row.get(16).unwrap_or_else(|_| row.get(5).unwrap_or(0)),
        })
    }

    fn decrypt_history_row(&self, raw: RawHistoryRow) -> Result<ClipboardEntry, String> {
        Ok(ClipboardEntry {
            id: raw.id,
            content_type: raw.content_type,
            content: self.maybe_decrypt_text(&raw.content_raw)?,
            html_content: raw
                .html_raw
                .as_ref()
                .map(|value| self.maybe_decrypt_text(value)).transpose()?,
            source_app: raw.source_app,
            timestamp: raw.timestamp,
            created_at: raw.created_at,
            last_used_at: raw.last_used_at,
            sort_at: raw.sort_at,
            preview: self.maybe_decrypt_text(&raw.preview_raw)?,
            is_pinned: raw.is_pinned,
            tags: raw.tags,
            use_count: raw.use_count,
            is_external: raw.is_external,
            pinned_order: raw.pinned_order,
            source_app_path: raw.source_app_path,
            note: raw.note,
            file_preview_exists: true,
        })
    }

    fn fetch_history_rows(
        conn: &Connection,
        since_timestamp: Option<i64>,
        limit: i32,
        offset: i32,
        content_type: Option<&str>,
        sort_mode: ClipboardSortMode,
    ) -> Result<Vec<RawHistoryRow>, String> {
        let select = "SELECT id, content_type, content, html_content, source_app, sort_at, preview, is_pinned, tags, use_count, is_external, pinned_order, source_app_path, note, created_at, last_used_at, sort_at
             FROM clipboard_history";
        let order = sort_mode.order_clause();

        let mut mapped_rows = Vec::new();
        match (content_type, since_timestamp) {
            (Some(ct), Some(since)) => {
                let sql = format!("{select} WHERE content_type = ?1 AND sort_at > ?2{order}");
                let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(
                        params![ct, since, limit, offset],
                        Self::parse_raw_history_row,
                    )
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    mapped_rows.push(row.map_err(|e| e.to_string())?);
                }
            }
            (Some(ct), None) => {
                let sql = format!("{select} WHERE content_type = ?1{order}");
                let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![ct, limit, offset], Self::parse_raw_history_row)
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    mapped_rows.push(row.map_err(|e| e.to_string())?);
                }
            }
            (None, Some(since)) => {
                let sql = format!("{select} WHERE sort_at > ?1{order}");
                let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![since, limit, offset], Self::parse_raw_history_row)
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    mapped_rows.push(row.map_err(|e| e.to_string())?);
                }
            }
            (None, None) => {
                let sql = format!("{select}{order}");
                let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], Self::parse_raw_history_row)
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    mapped_rows.push(row.map_err(|e| e.to_string())?);
                }
            }
        }
        Ok(mapped_rows)
    }

    fn load_history(
        &self,
        since_timestamp: Option<i64>,
        limit: i32,
        offset: i32,
        content_type: Option<&str>,
        sort_mode: ClipboardSortMode,
    ) -> Result<Vec<ClipboardEntry>, String> {
        let raw_rows = {
            let conn = self.conn.lock().map_err(|e| e.to_string())?;
            Self::fetch_history_rows(
                &conn,
                since_timestamp,
                limit,
                offset,
                content_type,
                sort_mode,
            )?
        };
        raw_rows
            .into_iter()
            .map(|raw| self.decrypt_history_row(raw))
            .collect()
    }

    pub fn save_with_conn(
        &self,
        conn: &Connection,
        entry: &ClipboardEntry,
        data_dir: Option<&std::path::Path>,
        precomputed_content_hash: Option<i64>,
    ) -> Result<i64, String> {
        if entry.id > 0 { self.validate_entry_with_conn(conn, entry.id)?; }
        let mut entry = entry.clone();
        entry.content = self.maybe_decrypt_text(&entry.content)?;
        entry.preview = self.maybe_decrypt_text(&entry.preview)?;
        entry.html_content = entry.html_content.as_deref().map(|v| self.maybe_decrypt_text(v)).transpose()?;
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

        // A session-only image (persistence off) sits under session/, which is
        // swept once no session entry shows it. Pinning or tagging moves the
        // entry here, so give the database its own copy in attachments/.
        if entry.content_type == "image" && !final_content.starts_with("data:") {
            if let Some(dir) = data_dir {
                let path = std::path::Path::new(&final_content);
                if path.starts_with(dir.join("session")) {
                    let extension = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("png");
                    if let Some(copied) = std::fs::read(path).ok().and_then(|bytes| {
                        save_image_bytes_to_file_with_extension(&bytes, dir, extension)
                    }) {
                        final_content = copied;
                        final_is_external = true;
                    }
                }
            }
        }

        let calculated_hash = precomputed_content_hash.unwrap_or_else(|| {
            if entry.content_type == "image" {
                if entry.content.starts_with("data:") {
                    calc_image_hash(&entry.content).unwrap_or(0)
                } else if let Ok(bytes) = std::fs::read(&entry.content) {
                    calc_image_hash_from_bytes(&bytes).unwrap_or(0)
                } else {
                    0
                }
            } else {
                calc_text_hash(&final_content) as i64
            }
        });

        let (content, preview, content_hash, html_content) = if should_encrypt {
            let encrypted_content = self.maybe_encrypt_text(&final_content)?;
            let encrypted_preview = self.maybe_encrypt_text(&entry.preview)?;
            let encrypted_html = entry
                .html_content
                .as_ref()
                .map(|html| self.maybe_encrypt_text(html)).transpose()?;
            (
                encrypted_content,
                encrypted_preview,
                calculated_hash,
                encrypted_html,
            )
        } else {
            (
                final_content.clone(),
                entry.preview.clone(),
                calculated_hash,
                entry.html_content.clone(),
            )
        };

        // Re-adding an item should clear an older delete tombstone for the same fingerprint.
        self.clear_tombstone_with_conn(conn, &entry.content_type, calculated_hash)?;
        let sync_hash = calc_sync_content_hash(&entry.content_type, &final_content);
        if sync_hash != calculated_hash {
            self.clear_tombstone_with_conn(conn, &entry.content_type, sync_hash)?;
        }

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
        let created_at = if entry.created_at > 0 {
            entry.created_at
        } else {
            entry.timestamp
        };
        let sort_at = if entry.sort_at > 0 {
            entry.sort_at
        } else {
            entry.timestamp
        };

        if entry.id > 0 {
            // Update existing entry (Move to top logic)
            conn.execute(
                "UPDATE clipboard_history SET 
                    content_type = ?1, 
                    content = ?2, 
                    html_content = ?3, 
                    source_app = ?4, 
                    timestamp = ?5, 
                    sort_at = ?5,
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
                    sort_at,
                    preview,
                    content_hash,
                    serde_json::to_string(&cleaned_tags).unwrap_or_else(|_| "[]".to_string()),
                    if final_is_external { 1 } else { 0 },
                    entry.source_app_path.as_deref(),
                    entry.id
                ],
            )
            .map_err(|e| e.to_string())?;
            self.sync_entry_tags_with_conn(conn, entry.id, &cleaned_tags)?;
            Ok(entry.id)
        } else {
            // Insert new entry
            conn.execute(
                "INSERT INTO clipboard_history (content_type, content, html_content, source_app, timestamp, created_at, last_used_at, sort_at, preview, is_pinned, content_hash, tags, is_external, pinned_order, source_app_path, note, use_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    entry.content_type,
                    content,
                    html_content,
                    entry.source_app,
                    sort_at,
                    created_at,
                    entry.last_used_at,
                    sort_at,
                    preview,
                    if entry.is_pinned { 1 } else { 0 },
                    content_hash,
                    serde_json::to_string(&cleaned_tags).unwrap_or_else(|_| "[]".to_string()),
                    if final_is_external { 1 } else { 0 },
                    entry.pinned_order,
                    entry.source_app_path.as_deref(),
                    entry.note,
                    entry.use_count
                ],
            ).map_err(|e| e.to_string())?;

            let new_id = conn.last_insert_rowid();
            self.sync_entry_tags_with_conn(conn, new_id, &cleaned_tags)?;
            Ok(new_id)
        }
    }

    /// Delete one entry and remove its attachment files that no other entry
    /// uses. Inside a larger transaction prefer delete_collecting_files and
    /// remove the files only after the commit.
    pub fn delete_with_conn(
        &self,
        conn: &Connection,
        id: i64,
        data_dir: Option<&std::path::Path>,
    ) -> Result<(), String> {
        let candidates = self.delete_collecting_files(conn, id, data_dir)?;
        remove_attachment_files(self.unreferenced_files(conn, candidates));
        Ok(())
    }

    /// Delete an entry that was just pasted ("delete after paste"). The paste
    /// may still point at its file (a file reference on Windows, resolved by the
    /// target app later, or a second Ctrl+V), so the file is removed a couple of
    /// minutes later, and only if no entry uses it by then (the same image may
    /// have been copied again meanwhile).
    pub fn delete_after_paste(&self, id: i64, data_dir: Option<&std::path::Path>) -> Result<(), String> {
        let candidates = {
            let conn = self.conn.lock().map_err(|e| e.to_string())?;
            self.delete_collecting_files(&conn, id, data_dir)?
        };
        if candidates.is_empty() {
            return Ok(());
        }
        let repo = Self::new(self.conn.clone());
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(120));
            let unused = match repo.conn.lock() {
                Ok(conn) => repo.unreferenced_files(&conn, candidates),
                Err(_) => return,
            };
            remove_attachment_files(unused);
        });
        Ok(())
    }

    /// Delete one entry (row, tags, tombstone) and return its attachment files
    /// without touching them: callers filter them with unreferenced_files once
    /// all rows are gone and delete them after committing. Checking each file
    /// against every other row on every delete made clearing N entries O(N²),
    /// and deleting before the commit lost files when the clear then failed.
    pub fn delete_collecting_files(
        &self,
        conn: &Connection,
        id: i64,
        data_dir: Option<&std::path::Path>,
    ) -> Result<Vec<std::path::PathBuf>, String> {
        self.validate_entry_with_conn(conn, id)?;
        let mut tombstone: Option<(String, i64)> = None;
        let mut candidates = Vec::new();
        // Check for external files to delete
        if let Some(dir) = data_dir {
            let attachments_dir = dir.join("attachments");
            let mut stmt = conn
                 .prepare("SELECT content, html_content, is_external, content_type FROM clipboard_history WHERE id = ?")
                 .map_err(|e| e.to_string())?;

            if let Ok(entry) = stmt.query_row([id], |row| {
                let content_raw: String = row.get(0)?;
                let html_raw: Option<String> = row.get(1).ok();
                let is_ext: i32 = row.get(2)?;
                let content_type: String = row.get(3)?;
                Ok((content_raw, html_raw, is_ext == 1, content_type))
            }) {
                let files_to_remove = self.collect_attachment_paths_for_cleanup(
                    &entry.0,
                    entry.1.as_deref(),
                    entry.2,
                    &attachments_dir,
                )?;
                // Fingerprint image bytes before removing managed attachments.
                // Hashing the path after deletion would create a tombstone that
                // cannot match the byte-based hash uploaded to mobile devices.
                let content = self.maybe_decrypt_text(&entry.0)?;
                tombstone = Some((entry.3.clone(), calc_sync_content_hash(&entry.3, &content)));
                candidates.extend(files_to_remove.into_iter().filter(|path| path.exists()));
            }
        } else {
            let mut stmt = conn
                .prepare("SELECT content_type, content FROM clipboard_history WHERE id = ?")
                .map_err(|e| e.to_string())?;
            if let Ok(entry) = stmt.query_row([id], |row| {
                let content_type: String = row.get(0)?;
                let content: String = row.get(1)?;
                Ok((content_type, content))
            }) {
                let content = self.maybe_decrypt_text(&entry.1)?;
                tombstone = Some((entry.0.clone(), calc_sync_content_hash(&entry.0, &content)));
            }
        }

        if let Some((content_type, content_hash)) = tombstone {
            self.upsert_tombstone_with_conn(conn, &content_type, content_hash, now_ms())?;
        }

        conn.execute("DELETE FROM clipboard_history WHERE id = ?", [id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM entry_tags WHERE entry_id = ?", params![id]).map_err(|e| e.to_string())?;
        Ok(candidates)
    }

    /// The candidates no remaining entry refers to, from one pass over the
    /// table. Rows must already be deleted. On any read error nothing is
    /// returned: keeping a file is safe, deleting a used one is not.
    pub fn unreferenced_files(
        &self,
        conn: &Connection,
        candidates: Vec<std::path::PathBuf>,
    ) -> Vec<std::path::PathBuf> {
        let mut pending: Vec<(std::path::PathBuf, String, String)> = candidates
            .into_iter()
            .filter_map(|path| {
                let name = path.file_name()?.to_str()?.to_string();
                let full = path.to_string_lossy().to_string();
                Some((path, full, name))
            })
            .collect();
        pending.sort_by(|a, b| a.1.cmp(&b.1));
        pending.dedup_by(|a, b| a.1 == b.1);
        if pending.is_empty() {
            return Vec::new();
        }
        let Ok(mut stmt) = conn.prepare("SELECT content, html_content, is_external FROM clipboard_history") else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1).ok().flatten(),
                row.get::<_, i32>(2).unwrap_or(0) == 1,
            ))
        }) else {
            return Vec::new();
        };
        for row in rows {
            let Ok((content_raw, html_raw, is_external)) = row else {
                return Vec::new();
            };
            if is_external {
                let Ok(content) = self.maybe_decrypt_text(&content_raw) else {
                    return Vec::new();
                };
                pending.retain(|(_, full, name)| !(content == *full || content.ends_with(name.as_str())));
            }
            if let Some(html_raw) = html_raw {
                let Ok(html) = self.maybe_decrypt_text(&html_raw) else {
                    return Vec::new();
                };
                pending.retain(|(_, full, name)| !(html.contains(name.as_str()) || html.contains(full.as_str())));
            }
            if pending.is_empty() {
                break;
            }
        }
        pending.into_iter().map(|(path, _, _)| path).collect()
    }

    pub fn delete_metadata_with_conn(&self, conn: &Connection, id: i64) -> Result<(), String> {
        self.validate_entry_with_conn(conn, id)?;
        conn.execute("DELETE FROM clipboard_history WHERE id = ?", params![id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM entry_tags WHERE entry_id = ?", params![id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Return every textual candidate so a different rich-text format cannot
    /// hide a later match. Callers verify decrypted content and HTML.
    pub fn find_text_candidates_with_conn(&self, conn: &Connection, content: &str) -> Result<Vec<i64>, String> {
        let hash = calc_text_hash(content) as i64;
        let mut stmt = conn.prepare(
            "SELECT id FROM clipboard_history WHERE content_type IN ('text','code','url','rich_text') AND (content_hash = ?1 OR content = ?2) ORDER BY sort_at DESC, id DESC"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![hash, content], |row| row.get(0)).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<i64>, _>>().map_err(|e| e.to_string())
    }

    pub fn find_by_content_with_conn(
        &self,
        conn: &Connection,
        content: &str,
        content_type: Option<&str>,
    ) -> Result<Option<i64>, String> {
        if content_type == Some("image") {
            if let Some(hash) = calc_image_hash(content) {
                let mut stmt = conn
                    .prepare(
                        "SELECT id FROM clipboard_history \
                     WHERE (content_type = 'image' AND content_hash = ?) OR content = ?",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query(params![hash, content])
                    .map_err(|e| e.to_string())?;
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
            let mut rows = stmt
                .query(params![ct, hash, ct, content])
                .map_err(|e| e.to_string())?;
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
            let mut rows = stmt
                .query(params![hash, content])
                .map_err(|e| e.to_string())?;
            if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                Ok(Some(row.get(0).map_err(|e| e.to_string())?))
            } else {
                Ok(None)
            }
        }
    }

    pub fn enforce_limit_with_conn(
        &self,
        conn: &Connection,
        data_dir: Option<&std::path::Path>,
    ) -> Result<Vec<i64>, String> {
        // Check if storage limit is enabled
        if let Ok(Some(limit_enabled_str)) =
            SqliteSettingsRepository::get_raw(conn, "app.persistent_limit_enabled")
        {
            if limit_enabled_str == "false" {
                return Ok(Vec::new());
            }
        }

        // Get the storage limit
        if let Ok(Some(limit_str)) = SqliteSettingsRepository::get_raw(conn, "app.persistent_limit")
        {
            if let Ok(limit) = limit_str.parse::<i32>() {
                // Count non-pinned entries
                let count: i32 = conn.query_row(
                    "SELECT COUNT(*) FROM clipboard_history WHERE is_pinned = 0 AND (tags = '[]' OR tags IS NULL)",
                    [],
                    |row| row.get(0)
                ).map_err(|e| e.to_string())?;

                if count > limit {
                    // First, get the IDs that will be deleted
                    let to_delete = count - limit;
                    let deleted_ids: Vec<i64> = {
                        let mut stmt = conn
                            .prepare(
                                "SELECT id FROM clipboard_history 
                             WHERE is_pinned = 0 AND (tags = '[]' OR tags IS NULL)
                             ORDER BY sort_at ASC
                             LIMIT ?",
                            )
                            .map_err(|e| e.to_string())?;

                        let rows = stmt
                            .query_map([to_delete], |row| row.get(0))
                            .map_err(|e| e.to_string())?;
                        rows.filter_map(|r| r.ok()).collect()
                    };
                    // Actually delete records (and files if needed)
                    let mut candidates = Vec::new();
                    for id in &deleted_ids {
                        if let Ok(files) = self.delete_collecting_files(conn, *id, data_dir) {
                            candidates.extend(files);
                        }
                    }
                    remove_attachment_files(self.unreferenced_files(conn, candidates));
                    return Ok(deleted_ids);
                }
            }
        }

        Ok(Vec::new())
    }
    pub fn toggle_pin_with_conn(
        &self,
        conn: &Connection,
        id: i64,
        is_pinned: bool,
    ) -> Result<(), String> {
        self.validate_entry_with_conn(conn, id)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        if is_pinned {
            // Set pinned_order to max + 1 so it appears at top
            conn.execute(
                "UPDATE clipboard_history 
                 SET is_pinned = 1, 
                     pinned_order = (SELECT COALESCE(MAX(pinned_order), 0) + 1 FROM clipboard_history WHERE is_pinned = 1),
                     timestamp = ?2,
                     sort_at = ?2
                 WHERE id = ?1",
                params![id, now],
            ).map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "UPDATE clipboard_history SET is_pinned = 0, pinned_order = 0, timestamp = ?2, sort_at = ?2 WHERE id = ?1",
                params![id, now],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn update_pinned_order_with_conn(
        &self,
        conn: &Connection,
        orders: Vec<(i64, i64)>,
    ) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        for (id, order) in orders {
            conn.execute(
                "UPDATE clipboard_history SET pinned_order = ?, timestamp = ?, sort_at = ? WHERE id = ?",
                params![order, now, now, id],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn get_entry_by_id_with_conn(
        &self,
        conn: &Connection,
        id: i64,
    ) -> Result<Option<ClipboardEntry>, String> {
        let mut stmt = conn.prepare(
            "SELECT id, content_type, content, html_content, source_app, sort_at, preview, is_pinned, tags, use_count, is_external, pinned_order, source_app_path, note, created_at, last_used_at, sort_at
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
            let content = self.maybe_decrypt_text(&content_raw)?;
            let preview = self.maybe_decrypt_text(&preview_raw)?;
            let html_content = html_raw.map(|v| self.maybe_decrypt_text(&v)).transpose()?;

            Ok(Some(ClipboardEntry {
                id: row.get(0).map_err(|e| e.to_string())?,
                content_type: row.get(1).map_err(|e| e.to_string())?,
                content,
                html_content,
                source_app: row.get(4).map_err(|e| e.to_string())?,
                timestamp: row.get(5).map_err(|e| e.to_string())?,
                created_at: row.get(14).unwrap_or_else(|_| row.get(5).unwrap_or(0)),
                last_used_at: row.get(15).unwrap_or(0),
                sort_at: row.get(16).unwrap_or_else(|_| row.get(5).unwrap_or(0)),
                preview,
                is_pinned: row.get::<_, i32>(7).map_err(|e| e.to_string())? == 1,
                tags,
                use_count: row.get(9).unwrap_or(0),
                is_external: row.get::<_, i32>(10).unwrap_or(0) == 1,
                pinned_order: row.get(11).unwrap_or(0),
                source_app_path: row.get(12).unwrap_or(None),
                note: row.get::<_, String>(13).unwrap_or_default(),
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
        self.validate_entry_with_conn(conn, id)?;
        let content = self.maybe_decrypt_text(content)?;
        let preview = self.maybe_decrypt_text(preview)?;
        let content = content.as_str();
        let preview = preview.as_str();
        let (old_content_raw, content_type, tags_json, has_html) = conn
            .query_row(
                "SELECT content, content_type, tags, (html_content IS NOT NULL) FROM clipboard_history WHERE id = ?",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, bool>(3)?,
                    ))
                },
            )
            .map_err(|e| e.to_string())?;

        let old_content = self.maybe_decrypt_text(&old_content_raw)?;
        // Procceed if content changed, OR if content is same but we need to transition away from rich text/clear HTML
        if old_content == content && content_type != "rich_text" && !has_html {
            return Ok(());
        }

        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let should_encrypt = has_sensitive_tag(&tags);

        if is_text_type(&content_type) {
            let hash = calc_text_hash(content) as i64;
            let new_type = if content_type == "rich_text" {
                "text"
            } else {
                &content_type
            };
            if should_encrypt {
                let encrypted_content = self.maybe_encrypt_text(content)?;
                let encrypted_preview = self.maybe_encrypt_text(preview)?;
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
            let encrypted_content = self.maybe_encrypt_text(content)?;
            let encrypted_preview = self.maybe_encrypt_text(preview)?;
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

    pub fn get_entry_content_full_with_conn(
        &self,
        conn: &Connection,
        id: i64,
    ) -> Result<Option<(String, String)>, String> {
        self.validate_entry_with_conn(conn, id)?;
        let mut stmt = conn
            .prepare("SELECT content, content_type FROM clipboard_history WHERE id = ?")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let content: String = row.get(0).map_err(|e| e.to_string())?;
            let content_type: String = row.get(1).map_err(|e| e.to_string())?;
            Ok(Some((self.maybe_decrypt_text(&content)?, content_type)))
        } else {
            Ok(None)
        }
    }

    pub fn get_entry_content_with_html_with_conn(
        &self,
        conn: &Connection,
        id: i64,
    ) -> Result<Option<(String, String, Option<String>)>, String> {
        self.validate_entry_with_conn(conn, id)?;
        let mut stmt = conn
            .prepare(
                "SELECT content, content_type, html_content FROM clipboard_history WHERE id = ?",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let content: String = row.get(0).map_err(|e| e.to_string())?;
            let content_type: String = row.get(1).map_err(|e| e.to_string())?;
            let html_raw: Option<String> = row.get(2).map_err(|e| e.to_string()).unwrap_or(None);
            let html_content = html_raw.map(|v| self.maybe_decrypt_text(&v)).transpose()?;
            Ok(Some((
                self.maybe_decrypt_text(&content)?,
                content_type,
                html_content,
            )))
        } else {
            Ok(None)
        }
    }
}

impl ClipboardRepository for SqliteClipboardRepository {
    fn save(
        &self,
        entry: &ClipboardEntry,
        data_dir: Option<&std::path::Path>,
    ) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.save_with_conn(&conn, entry, data_dir, None)
    }

    fn get_history(
        &self,
        limit: i32,
        offset: i32,
        content_type: Option<&str>,
    ) -> Result<Vec<ClipboardEntry>, String> {
        self.load_history(None, limit, offset, content_type, ClipboardSortMode::Manual)
    }

    fn get_history_sorted(
        &self,
        limit: i32,
        offset: i32,
        content_type: Option<&str>,
        sort_mode: ClipboardSortMode,
    ) -> Result<Vec<ClipboardEntry>, String> {
        self.load_history(None, limit, offset, content_type, sort_mode)
    }

    fn get_history_since(
        &self,
        since_timestamp: i64,
        limit: i32,
        offset: i32,
        content_type: Option<&str>,
    ) -> Result<Vec<ClipboardEntry>, String> {
        self.load_history(
            Some(since_timestamp),
            limit,
            offset,
            content_type,
            ClipboardSortMode::Manual,
        )
    }

    fn search(
        &self,
        query: &str,
        limit: i32,
        tag_only: bool,
        note_only: bool,
    ) -> Result<Vec<ClipboardEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        let term = query.trim().to_lowercase();
        if !note_only && term.is_empty() {
            return Ok(Vec::new());
        }

        #[cfg(feature = "portable")]
        {
            // Portable version: Data is NOT encrypted, use conventional SQL LIKE search (fastest)
            let sql = if tag_only {
                "SELECT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.sort_at, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path, ch.note, ch.created_at, ch.last_used_at, ch.sort_at
                 FROM clipboard_history ch
                 WHERE EXISTS (
                    SELECT 1 FROM entry_tags et
                    WHERE et.entry_id = ch.id AND et.tag COLLATE NOCASE = ?1
                 )
                 ORDER BY ch.sort_at DESC
                 LIMIT ?2"
            } else if note_only {
                if term.is_empty() {
                    "SELECT id, content_type, content, html_content, source_app, sort_at, preview, is_pinned, tags, use_count, is_external, pinned_order, source_app_path, note, created_at, last_used_at, sort_at
                     FROM clipboard_history
                     WHERE TRIM(note) != ''
                     ORDER BY sort_at DESC
                     LIMIT ?1"
                } else {
                    "SELECT id, content_type, content, html_content, source_app, sort_at, preview, is_pinned, tags, use_count, is_external, pinned_order, source_app_path, note, created_at, last_used_at, sort_at
                     FROM clipboard_history
                     WHERE note LIKE '%' || ?1 || '%'
                     ORDER BY sort_at DESC
                     LIMIT ?2"
                }
            } else {
                "SELECT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.sort_at, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path, ch.note, ch.created_at, ch.last_used_at, ch.sort_at
                 FROM clipboard_history ch
                 WHERE (
                    (ch.content_type IN ('text','code','url','rich_text')
                     AND (ch.content LIKE '%' || ?1 || '%' OR ch.preview LIKE '%' || ?1 || '%'))
                    OR ch.source_app LIKE '%' || ?1 || '%'
                    OR ch.note LIKE '%' || ?1 || '%'
                    OR EXISTS (
                        SELECT 1 FROM entry_tags et
                        WHERE et.entry_id = ch.id AND et.tag LIKE '%' || ?1 || '%'
                    )
                 )
                 ORDER BY ch.sort_at DESC
                 LIMIT ?2"
            };

            let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;

            let map_row = Self::parse_raw_history_row;

            let rows = if note_only && term.is_empty() {
                stmt.query_map(params![limit], map_row)
            } else {
                stmt.query_map(params![term, limit], map_row)
            }
            .map_err(|e| e.to_string())?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| e.to_string())?);
            }
            drop(stmt);
            drop(conn);
            results.into_iter().map(|raw| self.decrypt_history_row(raw)).collect()
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
            let sql_non_sensitive = if tag_only {
                format!(
                    "SELECT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.sort_at, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path, ch.note, ch.created_at, ch.last_used_at, ch.sort_at
                     FROM clipboard_history ch
                     WHERE NOT EXISTS (
                         SELECT 1 FROM entry_tags se
                         WHERE se.entry_id = ch.id
                           AND se.tag COLLATE NOCASE IN {}
                     )
                       AND EXISTS (
                           SELECT 1 FROM entry_tags et
                           WHERE et.entry_id = ch.id AND et.tag COLLATE NOCASE = ?1
                       )
                     ORDER BY ch.sort_at DESC, ch.id DESC
                     LIMIT ?2",
                    sensitive_tags_sql
                )
            } else if note_only {
                if term.is_empty() {
                    format!(
                        "SELECT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.sort_at, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path, ch.note, ch.created_at, ch.last_used_at, ch.sort_at
                         FROM clipboard_history ch
                         WHERE NOT EXISTS (
                             SELECT 1 FROM entry_tags se
                             WHERE se.entry_id = ch.id
                               AND se.tag COLLATE NOCASE IN {}
                         )
                           AND TRIM(ch.note) != ''
                         ORDER BY ch.sort_at DESC, ch.id DESC
                         LIMIT ?1",
                        sensitive_tags_sql
                    )
                } else {
                    format!(
                        "SELECT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.sort_at, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path, ch.note, ch.created_at, ch.last_used_at, ch.sort_at
                         FROM clipboard_history ch
                         WHERE NOT EXISTS (
                             SELECT 1 FROM entry_tags se
                             WHERE se.entry_id = ch.id
                               AND se.tag COLLATE NOCASE IN {}
                         )
                           AND ch.note LIKE '%' || ?1 || '%'
                         ORDER BY ch.sort_at DESC, ch.id DESC
                         LIMIT ?2",
                        sensitive_tags_sql
                    )
                }
            } else {
                format!(
                    "SELECT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.sort_at, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path, ch.note, ch.created_at, ch.last_used_at, ch.sort_at
                     FROM clipboard_history ch
                     WHERE NOT EXISTS (
                         SELECT 1 FROM entry_tags se
                         WHERE se.entry_id = ch.id
                           AND se.tag COLLATE NOCASE IN {}
                     )
                       AND (
                         (ch.content_type IN ('text','code','url','rich_text')
                          AND (ch.content LIKE '%' || ?1 || '%' OR ch.preview LIKE '%' || ?1 || '%'))
                         OR ch.source_app LIKE '%' || ?1 || '%'
                         OR ch.note LIKE '%' || ?1 || '%'
                         OR EXISTS (
                             SELECT 1 FROM entry_tags et
                             WHERE et.entry_id = ch.id AND et.tag LIKE '%' || ?1 || '%'
                         )
                       )
                     ORDER BY ch.sort_at DESC, ch.id DESC
                     LIMIT ?2",
                    sensitive_tags_sql
                )
            };

            let mut stmt = conn
                .prepare(&sql_non_sensitive)
                .map_err(|e| e.to_string())?;
            let map_row = Self::parse_raw_history_row;

            let rows = if note_only && term.is_empty() {
                stmt.query_map(params![limit], map_row)
            } else {
                stmt.query_map(params![term, limit], map_row)
            }
            .map_err(|e| e.to_string())?;

            let raw_rows = rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
            drop(stmt);
            drop(conn);
            for raw in raw_rows {
                let entry = self.decrypt_history_row(raw)?;
                if seen.insert(entry.id) { results.push(entry); }
            }

            // 2) Decrypt-scan sensitive or encrypted entries (only if needed)
            if results.len() < limit as usize {
                let mut cursor_ts = i64::MAX;
                let mut cursor_id = i64::MAX;
                let batch_size = 500;
                let enc_like = format!("{}%", ENCRYPT_PREFIX);
                let legacy_like = format!("{}%", encryption::LEGACY_PLAIN_PREFIX);
                let sql_sensitive = format!(
                    "SELECT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.sort_at, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path, ch.note, ch.created_at, ch.last_used_at, ch.sort_at
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
                         OR ch.content LIKE ?2
                         OR ch.preview LIKE ?2
                         OR ch.html_content LIKE ?2
                     )
                       AND ((ch.sort_at < ?3) OR (ch.sort_at = ?3 AND ch.id < ?4))
                     ORDER BY ch.sort_at DESC, ch.id DESC
                     LIMIT ?5",
                    sensitive_tags_sql
                );

                loop {
                    let mut batch = {
                        let conn = self.conn.lock().map_err(|e| e.to_string())?;
                        let mut stmt = conn.prepare(&sql_sensitive).map_err(|e| e.to_string())?;
                        let rows = stmt
                            .query_map(
                                params![enc_like, legacy_like, cursor_ts, cursor_id, batch_size],
                                |row| {
                                    let tags_str: String =
                                        row.get(8).unwrap_or_else(|_| "[]".to_string());
                                    Ok(ClipboardEntry {
                                        id: row.get(0)?,
                                        content_type: row.get(1)?,
                                        content: row.get(2)?, // Encrypted
                                        html_content: row.get(3).ok(),
                                        source_app: row.get(4)?,
                                        timestamp: row.get(5)?,
                                        created_at: row
                                            .get(14)
                                            .unwrap_or_else(|_| row.get(5).unwrap_or(0)),
                                        last_used_at: row.get(15).unwrap_or(0),
                                        sort_at: row
                                            .get(16)
                                            .unwrap_or_else(|_| row.get(5).unwrap_or(0)),
                                        preview: row.get(6)?, // Encrypted
                                        is_pinned: row.get::<_, i32>(7)? == 1,
                                        tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                                        note: row.get::<_, String>(13).unwrap_or_default(),
                                        use_count: row.get(9).unwrap_or(0),
                                        is_external: row.get::<_, i32>(10)? == 1,
                                        pinned_order: row.get(11).unwrap_or(0),
                                        source_app_path: row.get(12).unwrap_or(None),
                                        file_preview_exists: true,
                                    })
                                },
                            )
                            .map_err(|e| e.to_string())?;

                        let mut raw_batch = Vec::new();
                        for row in rows {
                            raw_batch.push(row.map_err(|e| e.to_string())?);
                        }
                        raw_batch
                    };

                    if batch.is_empty() {
                        break;
                    }

                    // Keychain/decryption work can be noticeably slower than the
                    // SQL read. Do it after releasing the global DB mutex so new
                    // clipboard captures are not blocked by a long search.
                    for entry in &mut batch {
                        entry.content = self.maybe_decrypt_text(&entry.content)?;
                        entry.preview = self.maybe_decrypt_text(&entry.preview)?;
                        if let Some(html) = entry.html_content.take() {
                            entry.html_content = Some(self.maybe_decrypt_text(&html)?);
                        }
                    }

                    for entry in batch.iter() {
                        if entry_matches_search(entry, &term, tag_only, note_only)
                            && seen.insert(entry.id)
                        {
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
                        cursor_ts = last.sort_at;
                        cursor_id = last.id;
                    } else {
                        break;
                    }
                }
            }

            results.sort_by(|a, b| b.sort_at.cmp(&a.sort_at).then(b.id.cmp(&a.id)));
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
        let transaction = conn.unchecked_transaction().map_err(|e| e.to_string())?;

        // Get IDs of unpinned items without tags.
        let ids = {
            let mut stmt = transaction
                .prepare(
                    "SELECT id FROM clipboard_history
             WHERE is_pinned = 0
               AND NOT EXISTS (SELECT 1 FROM entry_tags WHERE entry_id = clipboard_history.id)",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, i64>(0))
                .map_err(|e| e.to_string())?;
            rows.filter_map(Result::ok).collect::<Vec<i64>>()
        };

        // Keep per-entry cleanup/tombstones, but commit them as one transaction.
        // The previous implementation performed one durable SQLite commit per
        // entry and then VACUUMed synchronously, freezing the app on large lists.
        let mut candidates = Vec::new();
        for id in &ids {
            candidates.extend(self.delete_collecting_files(&transaction, *id, data_dir)?);
        }
        let unused = self.unreferenced_files(&transaction, candidates);
        transaction.commit().map_err(|e| e.to_string())?;
        // Files go only once the rows are really gone.
        remove_attachment_files(unused);
        Ok(())
    }

    fn get_count(&self) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM clipboard_history")
            .map_err(|e| e.to_string())?;
        let count: i64 = stmt
            .query_row([], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        Ok(count)
    }

    fn increment_use_count(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.validate_entry_with_conn(&conn, id)?;
        conn.execute(
            "UPDATE clipboard_history
             SET use_count = use_count + 1, last_used_at = ?1
             WHERE id = ?2",
            params![now_ms(), id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn touch_entry(&self, id: i64, timestamp: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.validate_entry_with_conn(&conn, id)?;
        conn.execute(
            "UPDATE clipboard_history SET timestamp = ?1, sort_at = ?1 WHERE id = ?2",
            params![timestamp, id],
        )
        .map_err(|e| e.to_string())?;
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

    fn get_entry_by_content(
        &self,
        content: &str,
        content_type: Option<&str>,
    ) -> Result<Option<i64>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.find_by_content_with_conn(&conn, content, content_type)
    }

    fn update_entry_content(&self, id: i64, content: &str, preview: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.update_entry_content_with_conn(&conn, id, content, preview)
    }

    fn update_entry_note(&self, id: i64, note: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.validate_entry_with_conn(&conn, id)?;
        let cleaned = note.trim();
        conn.execute(
            "UPDATE clipboard_history SET note = ?1 WHERE id = ?2",
            params![cleaned, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn get_entry_content(&self, id: i64) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.validate_entry_with_conn(&conn, id)?;
        let mut stmt = conn
            .prepare("SELECT content FROM clipboard_history WHERE id = ?")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let content: String = row.get(0).map_err(|e| e.to_string())?;
            Ok(Some(self.maybe_decrypt_text(&content)?))
        } else {
            Ok(None)
        }
    }

    fn get_entry_content_full(&self, id: i64) -> Result<Option<(String, String)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.get_entry_content_full_with_conn(&conn, id)
    }

    fn get_entry_content_with_html(
        &self,
        id: i64,
    ) -> Result<Option<(String, String, Option<String>)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        self.get_entry_content_with_html_with_conn(&conn, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::repository::migrations::run_migrations;

    fn search_repository(fixture: &str) -> SqliteClipboardRepository {
        let conn = Connection::open_in_memory().expect("open database");
        run_migrations(&conn).expect("migrate database");
        conn.execute_batch(fixture).expect("insert search fixture");
        let repo = SqliteClipboardRepository::new(Arc::new(Mutex::new(conn)));
        {
            let conn = repo.conn.lock().unwrap();
            let tags = conn
                .prepare("SELECT id, tags FROM clipboard_history")
                .unwrap()
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            for (id, json) in tags {
                repo.sync_entry_tags_with_conn(
                    &conn,
                    id,
                    &serde_json::from_str::<Vec<String>>(&json).unwrap(),
                )
                .unwrap();
            }
        }
        repo
    }

    #[test]
    fn text_dedup_candidates_include_other_types_and_all_rich_variants() {
        let repo = search_repository("INSERT INTO clipboard_history (id, content_type, content, timestamp, sort_at, source_app, preview) VALUES (1,'rich_text','same',1,1,'test',''), (2,'rich_text','same',2,2,'test',''), (3,'code','same',3,3,'test',''), (4,'file','same',4,4,'test','');");
        let conn = repo.conn.lock().unwrap();
        conn.execute("UPDATE clipboard_history SET content_hash = ?1", params![calc_text_hash("same") as i64]).unwrap();
        assert_eq!(repo.find_text_candidates_with_conn(&conn, "same").unwrap(), vec![3,2,1]);
        assert_eq!(repo.find_text_candidates_with_conn(&conn, "different").unwrap(), Vec::<i64>::new());
    }

    fn search_ids(
        repo: &SqliteClipboardRepository,
        query: &str,
        limit: i32,
        tag_only: bool,
        note_only: bool,
    ) -> Vec<i64> {
        repo.search(query, limit, tag_only, note_only)
            .expect("search history")
            .into_iter()
            .map(|entry| entry.id)
            .collect()
    }

    const GENERAL_FIXTURE: &str = r#"
        INSERT INTO clipboard_history
            (id, content_type, content, source_app, timestamp, sort_at, preview, tags, note)
        VALUES
            (1, 'text', 'needle content', 'App', 100, 100, '', '["needle-a","needle-b","other"]', ''),
            (2, 'text', 'other', 'App', 90, 90, '', '["needle-a","needle-b"]', ''),
            (3, 'text', 'untagged needle', 'App', 80, 80, '', '[]', ''),
            (4, 'image', 'needle', 'App', 75, 75, 'needle', '[]', ''),
            (5, 'image', 'other', 'Needle App', 70, 70, '', '[]', ''),
            (6, 'text', 'other', 'App', 60, 60, '', '[]', 'needle note'),
            (7, 'rich_text', 'other', 'App', 50, 50, 'needle preview', '[]', '');
    "#;

    #[test]
    fn saving_a_session_image_copies_it_out_of_session_storage() {
        let data_dir = std::env::temp_dir().join(format!("tiez-session-save-{}", std::process::id()));
        let session_dir = data_dir.join("session").join("attachments");
        std::fs::create_dir_all(&session_dir).unwrap();
        let png = b"\x89PNG\r\n\x1a\nnot really an image";
        let session_file = session_dir.join("img_1.png");
        std::fs::write(&session_file, png).unwrap();

        let repo = search_repository("");
        let entry: ClipboardEntry = serde_json::from_value(serde_json::json!({
            "id": -1,
            "content_type": "image",
            "content": session_file.to_string_lossy(),
            "source_app": "App",
            "timestamp": 1,
            "preview": "",
            "is_pinned": true,
            "is_external": true,
        }))
        .unwrap();
        let id = {
            let conn = repo.conn.lock().unwrap();
            repo.save_with_conn(&conn, &entry, Some(&data_dir), None).unwrap()
        };
        let stored: String = repo
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT content FROM clipboard_history WHERE id = ?", [id], |row| row.get(0))
            .unwrap();
        let stored = std::path::Path::new(&stored);
        assert!(stored.starts_with(data_dir.join("attachments")), "{stored:?}");
        assert_eq!(std::fs::read(stored).unwrap(), png);

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn search_preserves_matching_fields_order_and_limit_without_duplicate_tag_rows() {
        let repo = search_repository(GENERAL_FIXTURE);
        assert_eq!(
            search_ids(&repo, " NeEdLe ", 20, false, false),
            vec![1, 2, 3, 5, 6, 7]
        );
        assert_eq!(search_ids(&repo, "needle", 2, false, false), vec![1, 2]);
        assert_eq!(
            search_ids(&repo, "needle", 0, false, false),
            Vec::<i64>::new()
        );
    }

    #[test]
    fn tag_search_keeps_case_insensitive_exact_matching_including_literal_wildcards() {
        let repo = search_repository(
            r#"
            INSERT INTO clipboard_history
                (id, content_type, content, source_app, timestamp, sort_at, preview, tags)
            VALUES
                (1, 'text', 'other', 'App', 100, 100, '', '["Work","WORK","work-extra"]'),
                (2, 'text', 'other', 'App', 90, 90, '', '["work"]'),
                (3, 'text', 'work', 'App', 80, 80, '', '["work-extra"]'),
                (4, 'text', 'other', 'App', 70, 70, '', '["work%"]');
        "#,
        );
        assert_eq!(search_ids(&repo, " WoRk ", 20, true, false), vec![1, 2]);
        assert_eq!(search_ids(&repo, "work", 1, true, false), vec![1]);
        assert_eq!(search_ids(&repo, "work%", 20, true, false), vec![4]);
        assert_eq!(search_ids(&repo, "wor", 20, true, false), Vec::<i64>::new());
    }

    #[test]
    fn note_search_and_empty_query_behavior_are_preserved() {
        let repo = search_repository(GENERAL_FIXTURE);
        assert_eq!(search_ids(&repo, "needle", 20, false, true), vec![6]);
        assert_eq!(search_ids(&repo, " ", 20, false, true), vec![6]);
        assert!(search_ids(&repo, " ", 20, false, false).is_empty());
        assert!(search_ids(&repo, " ", 20, true, false).is_empty());
    }

    #[cfg(not(feature = "portable"))]
    #[test]
    fn search_still_scans_sensitive_records_and_decodes_legacy_storage() {
        // Legacy storage exercises decryption without accessing the system keychain.
        let repo = search_repository(
            r#"
            INSERT INTO clipboard_history
                (id, content_type, content, source_app, timestamp, sort_at, preview, tags, note)
            VALUES
                (1, 'text', 'plain:hidden needle', 'App', 100, 100, 'plain:hidden preview', '["password","project"]', 'needle note'),
                (2, 'text', 'needle', 'App', 90, 90, '', '["project"]', '');
        "#,
        );
        let results = repo.search("needle", 20, false, false).unwrap();
        assert_eq!(
            results.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(results[0].content, "hidden needle");
        assert_eq!(results[0].preview, "hidden preview");
        assert_eq!(search_ids(&repo, "project", 20, true, false), vec![1, 2]);
        assert_eq!(search_ids(&repo, "password", 20, true, false), vec![1]);
        assert_eq!(
            search_ids(&repo, "proj", 20, true, false),
            Vec::<i64>::new()
        );
        assert_eq!(search_ids(&repo, "needle", 20, false, true), vec![1]);
        assert_eq!(search_ids(&repo, "", 20, false, true), vec![1]);
    }
}

/// Remove attachment files (and their cached thumbnails).
pub fn remove_attachment_files(paths: Vec<std::path::PathBuf>) {
    for path in paths {
        crate::app::commands::thumbnail_cmd::remove_cached_thumbnails(&path);
        let _ = std::fs::remove_file(&path);
    }
}
