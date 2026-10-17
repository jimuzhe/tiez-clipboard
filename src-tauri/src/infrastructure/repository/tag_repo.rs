use crate::domain::models::ClipboardEntry;
use crate::infrastructure::encryption;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

pub trait TagRepository {
    fn set_color(&self, name: &str, color: Option<String>) -> Result<(), String>;
    fn get_colors(&self) -> Result<HashMap<String, String>, String>;
    fn get_all_with_counts(&self) -> Result<HashMap<String, i32>, String>;
    fn create(&self, name: &str) -> Result<(), String>;
    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), String>;
    fn delete_globally(&self, name: &str) -> Result<(), String>;
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

    fn maybe_decrypt_text(&self, value: &str) -> rusqlite::Result<String> {
        encryption::decrypt_stored_value(value).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))))
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
            "UPDATE clipboard_history SET tags = ?, tags_updated_at = ? WHERE id = ?",
            params![tags_json, chrono::Utc::now().timestamp_millis(), entry_id],
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
            )
            .map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "UPDATE saved_tags SET color = NULL WHERE name = ?1",
                params![name],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn get_colors(&self) -> Result<HashMap<String, String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT name, color FROM saved_tags WHERE color IS NOT NULL AND color != ''")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;

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
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?))
            })
            .map_err(|e| e.to_string())?;

        let mut tag_counts: HashMap<String, i32> = HashMap::new();
        for row in rows {
            if let Ok((tag, count)) = row {
                tag_counts.insert(tag, count);
            }
        }

        // Also include saved tags with 0 count if not present
        let mut stmt_saved = conn
            .prepare("SELECT name FROM saved_tags")
            .map_err(|e| e.to_string())?;
        let saved_rows = stmt_saved
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;

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
        )
        .map_err(|e| e.to_string())?;
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
        )
        .map_err(|e| e.to_string())?;

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
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "DELETE FROM entry_tags WHERE entry_id = ? AND tag = ?",
                params![id, old_name],
            )
            .map_err(|e| e.to_string())?;
            Self::refresh_entry_tags_json(&conn, id)?;
        }
        Ok(())
    }

    fn delete_globally(&self, name: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let transaction = conn.unchecked_transaction().map_err(|e| e.to_string())?;

        // Remove from saved_tags (also drops its colour).
        transaction
            .execute("DELETE FROM saved_tags WHERE name = ?", params![name])
            .map_err(|e| e.to_string())?;

        // Deleting a tag only takes the tag off its entries; the entries stay.
        // (It used to delete every unpinned entry carrying the tag.)
        let ids: Vec<i64> = {
            let mut stmt = transaction
                .prepare("SELECT entry_id FROM entry_tags WHERE tag = ?")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![name], |row| row.get(0))
                .map_err(|e| e.to_string())?;
            rows.filter_map(Result::ok).collect()
        };

        let clipboard_repo =
            crate::infrastructure::repository::clipboard_repo::SqliteClipboardRepository::new(self.conn.clone());
        let removes_sensitive = crate::database::has_sensitive_tag(&[name.to_string()]);
        for id in ids {
            transaction
                .execute("DELETE FROM entry_tags WHERE entry_id = ?1 AND tag = ?2", params![id, name])
                .map_err(|e| e.to_string())?;
            // Also bumps tags_updated_at, so sync carries the removal.
            Self::refresh_entry_tags_json(&transaction, id)?;

            // Same rule as editing an entry's tags: with no sensitive tag left
            // the entry is stored in plain text again.
            if removes_sensitive {
                let remaining: Vec<String> = {
                    let mut stmt = transaction
                        .prepare("SELECT tag FROM entry_tags WHERE entry_id = ?")
                        .map_err(|e| e.to_string())?;
                    let rows = stmt
                        .query_map(params![id], |row| row.get::<_, String>(0))
                        .map_err(|e| e.to_string())?;
                    rows.filter_map(Result::ok).collect()
                };
                if !crate::database::has_sensitive_tag(&remaining) {
                    if let Err(e) = clipboard_repo.decrypt_entry_with_conn(&transaction, id) {
                        // Not decryptable on this device: keep the ciphertext
                        // rather than failing the whole tag deletion.
                        eprintln!("[tag_repo] entry {id} stays encrypted after tag delete: {e}");
                    }
                }
            }
        }
        transaction.commit().map_err(|e| e.to_string())
    }

    fn get_entries_by_tag(&self, tag: &str) -> Result<Vec<ClipboardEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT ch.id, ch.content_type, ch.content, ch.html_content, ch.source_app, ch.sort_at, ch.preview, ch.is_pinned, ch.tags, ch.use_count, ch.is_external, ch.pinned_order, ch.source_app_path, ch.note, ch.created_at, ch.last_used_at, ch.sort_at
             FROM clipboard_history ch
             INNER JOIN entry_tags et ON ch.id = et.entry_id
             WHERE et.tag = ? 
             ORDER BY ch.is_pinned DESC, ch.pinned_order DESC, ch.sort_at DESC",
        ).map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([tag], |row| {
                let tags_str: String = row.get(8).unwrap_or_else(|_| "[]".to_string());
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                let content_raw: String = row.get(2)?;
                let html_raw: Option<String> = row.get(3).ok();
                let preview_raw: String = row.get(6)?;
                let content = self.maybe_decrypt_text(&content_raw)?;
                let preview = self.maybe_decrypt_text(&preview_raw)?;
                let html_content = html_raw.map(|v| self.maybe_decrypt_text(&v)).transpose()?;

                Ok(ClipboardEntry {
                    id: row.get(0)?,
                    content_type: row.get(1)?,
                    content,
                    html_content,
                    source_app: row.get(4)?,
                    timestamp: row.get(5)?,
                    created_at: row.get(14).unwrap_or_else(|_| row.get(5).unwrap_or(0)),
                    last_used_at: row.get(15).unwrap_or(0),
                    sort_at: row.get(16).unwrap_or_else(|_| row.get(5).unwrap_or(0)),
                    preview,
                    is_pinned: row.get::<_, i32>(7)? == 1,
                    tags,
                    use_count: row.get(9).unwrap_or(0),
                    is_external: row.get::<_, i32>(10)? == 1,
                    pinned_order: row.get(11).unwrap_or(0),
                    source_app_path: row.get(12).unwrap_or(None),
                    note: row.get::<_, String>(13).unwrap_or_default(),
                    file_preview_exists: true, // simplified
                })
            })
            .map_err(|e| e.to_string())?;

        let mut history = Vec::new();
        for row in rows {
            match row {
                Ok(entry) => history.push(entry),
                // maybe_decrypt_text's error: this device can't decrypt the
                // entry. Skip it rather than hide the whole tag's list.
                Err(rusqlite::Error::ToSqlConversionFailure(e)) => {
                    eprintln!(">>> [TAGS] skipping unreadable entry: {e}");
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(history)
    }

    fn update_entry_tags(&self, id: i64, tags: Vec<String>) -> Result<(), String> {
        let mut guard = self.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.transaction().map_err(|e| e.to_string())?;
        let repo = crate::infrastructure::repository::clipboard_repo::SqliteClipboardRepository::new(self.conn.clone());
        repo.validate_entry_with_conn(&conn, id)?;
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
        if crate::database::has_sensitive_tag(&cleaned) {
            repo.encrypt_entry_with_conn(&conn, id)?;
        } else {
            repo.decrypt_entry_with_conn(&conn, id)?;
        }
        conn.execute(
            "UPDATE clipboard_history SET tags = ?, tags_updated_at = ? WHERE id = ?",
            params![tags_json, chrono::Utc::now().timestamp_millis(), id],
        )
        .map_err(|e| e.to_string())?;
        conn.commit().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::repository::migrations::run_migrations;

    fn repo_with(fixture: &str) -> SqliteTagRepository {
        let conn = Connection::open_in_memory().expect("open database");
        run_migrations(&conn).expect("migrate database");
        conn.execute_batch(fixture).expect("insert fixture");
        // entry_tags mirrors the tags JSON column.
        let rows: Vec<(i64, String)> = conn
            .prepare("SELECT id, tags FROM clipboard_history")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for (id, json) in rows {
            for tag in serde_json::from_str::<Vec<String>>(&json).unwrap() {
                conn.execute(
                    "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                    params![id, tag],
                )
                .unwrap();
            }
        }
        SqliteTagRepository::new(Arc::new(Mutex::new(conn)))
    }

    fn tags_of(repo: &SqliteTagRepository, id: i64) -> Vec<String> {
        let conn = repo.conn.lock().unwrap();
        let json: String = conn
            .query_row("SELECT tags FROM clipboard_history WHERE id = ?", [id], |r| r.get(0))
            .unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn deleting_a_tag_keeps_its_entries() {
        let repo = repo_with(
            r#"INSERT INTO saved_tags (name, color) VALUES ('work', '#f00');
               INSERT INTO clipboard_history
                   (id, content_type, content, source_app, timestamp, sort_at, preview, tags, is_pinned)
               VALUES
                   (1, 'text', 'unpinned', 'App', 1, 1, 'unpinned', '["other","work"]', 0),
                   (2, 'text', 'pinned', 'App', 2, 2, 'pinned', '["work"]', 1),
                   (3, 'image', '/tmp/a.png', 'App', 3, 3, '', '["work"]', 0);"#,
        );

        repo.delete_globally("work").expect("delete tag");

        let count: i64 = repo
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM clipboard_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3, "entries must not be deleted with the tag");
        assert_eq!(tags_of(&repo, 1), vec!["other".to_string()]);
        assert!(tags_of(&repo, 2).is_empty());
        assert!(tags_of(&repo, 3).is_empty());
        let counts = repo.get_all_with_counts().unwrap();
        assert!(!counts.contains_key("work"));
        assert_eq!(counts.get("other"), Some(&1));
        assert!(!repo.get_colors().unwrap().contains_key("work"));
    }

    #[test]
    fn deleting_the_last_sensitive_tag_stores_the_entry_in_plain_text() {
        let cipher = encryption::encrypt_for_storage("secret").expect("encrypt");
        let repo = repo_with(&format!(
            "INSERT INTO clipboard_history
                 (id, content_type, content, source_app, timestamp, sort_at, preview, tags)
             VALUES
                 (1, 'text', '{cipher}', 'App', 1, 1, '{cipher}', '[\"password\"]'),
                 (2, 'text', '{cipher}', 'App', 2, 2, '{cipher}', '[\"password\",\"sensitive\"]');"
        ));

        repo.delete_globally("password").expect("delete tag");

        let conn = repo.conn.lock().unwrap();
        let content = |id: i64| -> String {
            conn.query_row("SELECT content FROM clipboard_history WHERE id = ?", [id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(content(1), "secret");
        assert!(encryption::is_encrypted_value(&content(2)), "still tagged sensitive");
    }

    #[test]
    fn entries_by_tag_skip_unreadable_rows() {
        let corrupt = format!("{}{}", crate::database::ENCRYPT_PREFIX, "A".repeat(48));
        let repo = repo_with(&format!(
            "INSERT INTO clipboard_history
                 (id, content_type, content, source_app, timestamp, sort_at, preview, tags)
             VALUES
                 (1, 'text', 'kept', 'App', 1, 1, 'kept', '[\"sensitive\"]'),
                 (2, 'text', '{corrupt}', 'App', 2, 2, '{corrupt}', '[\"sensitive\"]');"
        ));
        let entries = repo.get_entries_by_tag("sensitive").expect("list still loads");
        assert_eq!(entries.iter().map(|e| e.id).collect::<Vec<_>>(), vec![1]);
    }
}
