use crate::database::is_sensitive_key;
use crate::infrastructure::encryption::{self, ENCRYPT_PREFIX};
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const LEGACY_PLAIN_PREFIX: &str = "plain:";

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

    fn strip_plain_prefixes<'a>(mut value: &'a str) -> &'a str {
        while let Some(stripped) = value.strip_prefix(LEGACY_PLAIN_PREFIX) {
            value = stripped;
        }
        value
    }

    fn encrypted_payload<'a>(value: &'a str) -> Option<&'a str> {
        let normalized = Self::strip_plain_prefixes(value);
        if normalized.starts_with(ENCRYPT_PREFIX) {
            Some(normalized)
        } else {
            None
        }
    }

    fn should_try_decrypt(key: &str, value: &str) -> bool {
        Self::encrypted_payload(value).is_some()
            && (is_sensitive_key(key) || key.eq_ignore_ascii_case("mqtt_username"))
    }

    fn try_decrypt_legacy_or_sensitive(key: &str, value: &str) -> Option<String> {
        if !Self::should_try_decrypt(key, value) {
            return None;
        }

        let mut current = value.to_string();
        let mut changed = false;

        for _ in 0..4 {
            let stripped = Self::strip_plain_prefixes(&current).to_string();
            if stripped != current {
                current = stripped;
                changed = true;
            }

            if !current.starts_with(ENCRYPT_PREFIX) {
                break;
            }

            let decrypted = encryption::decrypt_value(&current)?;
            current = decrypted;
            changed = true;
        }

        let final_value = Self::strip_plain_prefixes(&current).to_string();
        if final_value != current {
            current = final_value;
            changed = true;
        }

        if changed && !current.starts_with(ENCRYPT_PREFIX) {
            Some(current)
        } else {
            None
        }
    }

    pub fn get_raw(conn: &Connection, key: &str) -> Result<Option<String>> {
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?")?;
        let mut rows = stmt.query(params![key])?;

        if let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            if let Some(decrypted) = Self::try_decrypt_legacy_or_sensitive(key, &value) {
                return Ok(Some(decrypted));
            }
            if Self::should_try_decrypt(key, &value) {
                return Ok(Some(String::new()));
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
        if let Some(decrypted) = Self::try_decrypt_legacy_or_sensitive(key, value) {
            return decrypted;
        }
        if Self::should_try_decrypt(key, value) {
            return String::new();
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

            // Auto-migrate to encrypted if it was plaintext and is sensitive.
            // Also migrate legacy encrypted mqtt_username back to plaintext.
            #[cfg(not(feature = "portable"))]
            {
                if is_sensitive_key(key) && !value.starts_with(ENCRYPT_PREFIX) {
                    let _ = conn.execute(
                        "UPDATE settings SET value = ? WHERE key = ?",
                        params![self.maybe_encrypt(key, &decrypted), key],
                    );
                } else if key.eq_ignore_ascii_case("mqtt_username")
                    && Self::encrypted_payload(&value).is_some()
                    && !decrypted.is_empty()
                {
                    let _ = conn.execute(
                        "UPDATE settings SET value = ? WHERE key = ?",
                        params![&decrypted, key],
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

            // Auto-migrate to encrypted if it was plaintext and is sensitive.
            // Also migrate legacy encrypted mqtt_username back to plaintext.
            #[cfg(not(feature = "portable"))]
            {
                if is_sensitive_key(&key) && !value.starts_with(ENCRYPT_PREFIX) {
                    let _ = conn.execute(
                        "UPDATE settings SET value = ? WHERE key = ?",
                        params![self.maybe_encrypt(&key, &decrypted), &key],
                    );
                } else if key.eq_ignore_ascii_case("mqtt_username")
                    && Self::encrypted_payload(&value).is_some()
                    && !decrypted.is_empty()
                {
                    let _ = conn.execute(
                        "UPDATE settings SET value = ? WHERE key = ?",
                        params![&decrypted, &key],
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
