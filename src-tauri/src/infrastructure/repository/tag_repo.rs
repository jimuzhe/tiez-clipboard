use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use std::collections::{HashMap, HashSet};
use crate::domain::models::ClipboardEntry;
use crate::database::ENCRYPT_PREFIX;
use crate::infrastructure::encryption;

pub trait TagRepository {
    fn set_color(&self, name: &str, color: Option<String>) -> Result<(), String>;
    fn get_colors(&self) -> Result<HashMap<String, String>, String>;
    fn get_all_with_counts(&self) -> Result<HashMap<String, i32>, String>;
    fn create(&self, name: &str) -> Result<(), String>;
    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), String>;
    fn delete_globally(&self, name: &str, data_dir: Option<&std::path::Path>) -> Result<(), String>;
    fn get_entries_by_tag(&self, tag: &str) -> Result<Vec<ClipboardEntry>, String>;
    fn update_entry_tags(&self, id: i64, tags: Vec<String>) -> Result<(), String>;
}

pub struct SqliteTagRepository {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteTagRepository {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    fn maybe_decrypt_text(&self, value: &str) -> String {
        if value.starts_with(ENCRYPT_PREFIX) {
            encryption::decrypt_value(value).unwrap_or_else(|| value.to_string())
        } else {
            value.to_string()
        }
    }

    fn refresh_entry_tags_json(conn: &Connection, entry_id: i64) -> Result<(), String> {
        let mut stmt = conn
            .prepare("SELECT tag FROM entry_tags WHERE entry_id = ? ORDER BY tag")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![entry_id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;

        let mut tags: Vec<String> = Vec::new();
        for row in rows {
            if let Ok(tag) = row {
                if !tag.trim().is_empty() {
                    tags.push(tag);
                }
            }
        }

        let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE clipboard_history SET tags = ? WHERE id = ?",
            params![tags_json, entry_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl TagRepository for SqliteTagRepository {
    fn set_color(&self, name: &str, color: Option<String>) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        if let Some(c) = color {
            conn.execute(
                "INSERT INTO saved_tags (name, color) VALUES (?1, ?2) 
                 ON CONFLICT(name) DO UPDATE SET color = ?2",
                params![name, c],
            ).map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "UPDATE saved_tags SET color = NULL WHERE name = ?1",
                params![name],
            ).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn get_colors(&self) -> Result<HashMap<String, String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT name, color FROM saved_tags WHERE color IS NOT NULL AND color != ''").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).map_err(|e| e.to_string())?;

        let mut map = HashMap::new();
        for row in rows {
            if let Ok((name, color)) = row {
                map.insert(name, color);
            }
        }
        Ok(map)
    }

    fn get_all_with_counts(&self) -> Result<HashMap<String, i32>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT tag, COUNT(*) FROM entry_tags GROUP BY tag")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)))
            .map_err(|e| e.to_string())?;

        let mut tag_counts: HashMap<String, i32> = HashMap::new();
        for row in rows {
            if let Ok((tag, count)) = row {
                tag_counts.insert(tag, count);
            }
        }

        // Also include saved tags with 0 count if not present
        let mut stmt_saved = conn.prepare("SELECT name FROM saved_tags").map_err(|e| e.to_string())?;
        let saved_rows = stmt_saved.query_map([], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| e.to_string())?;

        for row in saved_rows {
            if let Ok(name) = row {
                tag_counts.entry(name).or_insert(0);
            }
        }

        Ok(tag_counts)
    }

    fn create(&self, name: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR IGNORE INTO saved_tags (name) VALUES (?)",
            params![name],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        
        // Update saved_tags table: merge color info if exists
        let old_color: Option<String> = conn
            .query_row(
                "SELECT color FROM saved_tags WHERE name = ?",
                params![old_name],
                |row| row.get(0),
            )
            .ok();

        conn.execute(
            "INSERT OR IGNORE INTO saved_tags (name, color) VALUES (?1, ?2)",
            params![new_name, old_color],
        ).map_err(|e| e.to_string())?;

        let _ = conn.execute("DELETE FROM saved_tags WHERE name = ?", params![old_name]);

        // Update entry_tags and refresh JSON cache
        let mut stmt = conn
            .prepare("SELECT entry_id FROM entry_tags WHERE tag = ?")
            .map_err(|e| e.to_string())?;
        let ids: Vec<i64> = stmt
            .query_map(params![old_name], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();

        for id in ids {
            conn.execute(
                "INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                params![id, new_name],
            ).map_err(|e| e.to_string())?;
            conn.execute(
                "DELETE FROM entry_tags WHERE entry_id = ? AND tag = ?",
                params![id, old_name],
            ).map_err(|e| e.to_string())?;
            Self::refresh_entry_tags_json(&conn, id)?;
        }
        Ok(())
    }

    fn delete_globally(&self, name: &str, data_dir: Option<&std::path::Path>) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        
        // Remove from saved_tags
        let _ = conn.execute("DELETE FROM saved_tags WHERE name = ?", params![name]);

        // Delete all entries that carry this tag
        let mut stmt = conn
            .prepare("SELECT entry_id FROM entry_tags WHERE tag = ?")
            .map_err(|e| e.to_string())?;
        let ids: Vec<i64> = stmt
            .query_map(params![name], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        
        for id in ids {
            if let Some(dir) = data_dir {
                let attachments_dir = dir.join("attachments");
                let mut stmt_content = conn.prepare("SELECT content, is_external FROM clipboard_history WHERE id = ?").map_err(|e| e.to_string())?;
                
                if let Ok(entry) = stmt_content.query_row([id], |row| {
                    let content_raw: String = row.get(0)?;
                    let is_ext: i32 = row.get(1)?;
                    Ok((content_raw, is_ext == 1))
                }) {
                    if entry.1 { // is_external
                        let content_path = self.maybe_decrypt_text(&entry.0);
                        let path = std::path::Path::new(&content_path);
                        if path.starts_with(&attachments_dir) && path.exists() {
                            let _ = std::fs::remove_file(path);
                        }
                    }
                }
            }
            let _ = conn.execute("DELETE FROM entry_tags WHERE entry_id = ?", params![id]);
            let _ = conn.execute("DELETE FROM clipboard_history WHERE id = ?", params![id]);
        }
        Ok(())
    }

    fn get_entries_by_tag(&self, tag: &str) -> Result<Vec<ClipboardEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.timestamp, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path 
             FROM clipboard_history ch
             INNER JOIN entry_tags et ON ch.id = et.entry_id
             WHERE et.tag = ? 
             ORDER BY ch.is_pinned DESC, ch.pinned_order DESC, ch.timestamp DESC",
        ).map_err(|e| e.to_string())?;
        
        let rows = stmt.query_map([tag], |row| {
            let tags_str: String = row.get(8).unwrap_or_else(|_| "[]".to_string());
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            let content_raw: String = row.get(2)?;
            let html_raw: Option<String> = row.get(3).ok();
            let preview_raw: String = row.get(6)?;
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
                file_preview_exists: true, // simplified
            })
        }).map_err(|e| e.to_string())?;

        let mut history = Vec::new();
        for row in rows {
            if let Ok(entry) = row {
                history.push(entry);
            }
        }
        Ok(history)
    }

    fn update_entry_tags(&self, id: i64, tags: Vec<String>) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut seen: HashSet<String> = HashSet::new();
        let mut cleaned: Vec<String> = Vec::new();
        for tag in tags {
            let t = tag.trim();
            if t.is_empty() {
                continue;
            }
            let t_owned = t.to_string();
            if seen.insert(t_owned.clone()) {
                cleaned.push(t_owned);
            }
        }

        conn.execute("DELETE FROM entry_tags WHERE entry_id = ?", params![id])
            .map_err(|e| e.to_string())?;
        for tag in &cleaned {
            conn.execute(
                "INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                params![id, tag],
            )
            .map_err(|e| e.to_string())?;
        }

        let tags_json = serde_json::to_string(&cleaned).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE clipboard_history SET tags = ? WHERE id = ?",
            params![tags_json, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
