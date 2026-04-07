use crate::database::is_sensitive_key;
use crate::infrastructure::encryption::{self, ENCRYPT_PREFIX};
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub trait SettingsRepository {
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn get_all(&self) -> Result<HashMap<String, String>>;
    fn clear(&self) -> Result<()>;
}

pub struct SqliteSettingsRepository {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSettingsRepository {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn get_raw(conn: &Connection, key: &str) -> Result<Option<String>> {
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?")?;
        let mut rows = stmt.query(params![key])?;

        if let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            if is_sensitive_key(key) && value.starts_with(ENCRYPT_PREFIX) {
                return Ok(Some(encryption::decrypt_value(&value).unwrap_or(value)));
            }
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    fn maybe_encrypt(&self, key: &str, value: &str) -> String {
        #[cfg(feature = "portable")]
        let _ = key;
        #[cfg(not(feature = "portable"))]
        {
            if is_sensitive_key(key) && !value.starts_with(ENCRYPT_PREFIX) {
                return encryption::encrypt_value(value).unwrap_or_else(|| value.to_string());
            }
        }
        value.to_string()
    }

    fn maybe_decrypt(&self, key: &str, value: &str) -> String {
        if is_sensitive_key(key) && value.starts_with(ENCRYPT_PREFIX) {
            return encryption::decrypt_value(value).unwrap_or_else(|| value.to_string());
        }
        value.to_string()
    }
}

impl SettingsRepository for SqliteSettingsRepository {
    fn set(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let final_value = self.maybe_encrypt(key, value);

        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)",
            params![key, final_value],
        )?;
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?")?;
        let mut rows = stmt.query(params![key])?;

        if let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            let decrypted = self.maybe_decrypt(key, &value);

            // Auto-migrate to encrypted if it was plaintext and is sensitive
            #[cfg(not(feature = "portable"))]
            {
                if is_sensitive_key(key) && !value.starts_with(ENCRYPT_PREFIX) {
                    let _ = conn.execute(
                        "UPDATE settings SET value = ? WHERE key = ?",
                        params![self.maybe_encrypt(key, &decrypted), key],
                    );
                }
            }

            Ok(Some(decrypted))
        } else {
            Ok(None)
        }
    }

    fn get_all(&self) -> Result<HashMap<String, String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut settings = HashMap::new();
        for row in rows {
            let (key, value) = row?;
            let decrypted = self.maybe_decrypt(&key, &value);

            // Auto-migrate to encrypted if it was plaintext and is sensitive
            #[cfg(not(feature = "portable"))]
            {
                if is_sensitive_key(&key) && !value.starts_with(ENCRYPT_PREFIX) {
                    let _ = conn.execute(
                        "UPDATE settings SET value = ? WHERE key = ?",
                        params![self.maybe_encrypt(&key, &decrypted), &key],
                    );
                }
            }

            settings.insert(key, decrypted);
        }
        Ok(settings)
    }

    fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM settings", [])?;
        // Note: seed_defaults should probably be called by the caller or we move it here
        Ok(())
    }
}
